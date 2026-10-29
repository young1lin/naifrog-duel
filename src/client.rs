//! Desktop online client: the login, rooms and seat resume of `web/network.js`, for the native
//! game. A worker thread owns the HTTP login and the WebSocket; the renderer reads a `RemoteView`
//! every frame, exactly as the browser build reads the page's view, and sends only controls.
//! There is no TLS here: the desktop reaches plain `http://IP:port` servers, and an `https://`
//! server needs the browser. The key and resume token stay in memory and are never printed.
//! The status line is kept as a `Status` and worded in the game's language when read.
use crate::{
    controls::TRIG,
    i18n::Lang,
    net::{
        ClientMessage, INPUT_MASK, PROTOCOL_VERSION, RemoteView, ServerMessage, Snapshot,
        valid_name,
    },
};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};
use tungstenite::{
    HandshakeError, Message, WebSocket, client::IntoClientRequest, http::HeaderValue,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a socket read waits before queued controls are sent.
const READ_SLICE: Duration = Duration::from_millis(4);
const IDLE_SLICE: Duration = Duration::from_millis(50);
const MAX_MESSAGE: usize = 16384;
const MAX_RESPONSE: u64 = 64 * 1024;
/// Resume attempts after an unexpected drop, 1 s apart growing to 4 s, as the page does:
/// about 5 minutes, as long as the server keeps a dropped seat.
const RETRIES: u32 = 80;
/// Hits kept between two renderer polls.
const MAX_HITS: usize = 32;

/// A server address as a browser's address bar shows it: `http://host[:port]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
    origin: String,
    authority: String,
    host: String,
    port: u16,
}

/// Why `Server::parse` refused an address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerProblem {
    /// The desktop speaks plain HTTP only.
    Https,
    /// Not `http://host[:port]`: the address as typed.
    Malformed(String),
}

impl ServerProblem {
    pub fn text(&self, lang: Lang) -> String {
        match (self, lang) {
            (Self::Https, Lang::Zh) => "https:// 服务器请用浏览器：桌面版只支持 http://".into(),
            (Self::Https, Lang::En) => {
                "HTTPS SERVERS NEED THE BROWSER: THE DESKTOP SPEAKS PLAIN HTTP ONLY".into()
            }
            (Self::Malformed(url), Lang::Zh) => {
                format!("服务器地址应形如 http://IP:端口，而不是“{url}”")
            }
            (Self::Malformed(url), Lang::En) => {
                format!("SERVER MUST LOOK LIKE http://IP:PORT, NOT {url:?}")
            }
        }
    }
}

impl Server {
    pub fn parse(url: &str) -> Result<Self, ServerProblem> {
        let lower = url.trim().trim_end_matches('/').to_ascii_lowercase();
        if lower.starts_with("https://") {
            return Err(ServerProblem::Https);
        }
        let bad = || ServerProblem::Malformed(url.into());
        let rest = lower.strip_prefix("http://").ok_or_else(bad)?;
        if rest.is_empty() || rest.contains(['/', '?', '#', '@', ' ']) {
            return Err(bad());
        }
        let (host, port) = if let Some(inner) = rest.strip_prefix('[') {
            let (host, after) = inner.split_once(']').ok_or_else(bad)?;
            (host, after.strip_prefix(':'))
        } else {
            match rest.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (rest, None),
            }
        };
        let port = match port {
            Some(p) => p.parse::<u16>().ok().filter(|p| *p > 0).ok_or_else(bad)?,
            None => 80,
        };
        if host.is_empty() {
            return Err(bad());
        }
        let bracketed = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        let authority = format!("{bracketed}:{port}");
        // The browser drops the default port from its origin, and the server compares exactly.
        let origin = if port == 80 {
            format!("http://{bracketed}")
        } else {
            format!("http://{authority}")
        };
        Ok(Self {
            origin,
            authority,
            host: host.into(),
            port,
        })
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }
    fn connect(&self) -> io::Result<TcpStream> {
        let addrs: Vec<SocketAddr> = (self.host.as_str(), self.port).to_socket_addrs()?.collect();
        let mut last = io::Error::new(io::ErrorKind::NotFound, "no address");
        for addr in addrs {
            match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                Ok(stream) => {
                    stream.set_read_timeout(Some(IO_TIMEOUT))?;
                    stream.set_write_timeout(Some(IO_TIMEOUT))?;
                    stream.set_nodelay(true)?;
                    return Ok(stream);
                }
                Err(error) => last = error,
            }
        }
        Err(last)
    }
}

/// A room code as typed: case, spaces and the middle dash do not matter.
pub fn room_code(typed: &str) -> Option<String> {
    let code: String = typed
        .chars()
        .filter(|c| !matches!(c, ' ' | '-'))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    (code.len() == 8
        && code
            .bytes()
            .all(|b| b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(&b)))
    .then_some(code)
}
/// `ABCDEFGH` as `ABCD-EFGH`.
pub fn show_code(code: &str) -> String {
    if code.len() == 8 {
        format!("{}-{}", &code[..4], &code[4..])
    } else {
        code.into()
    }
}

/// The connection's one-line state. Kept as data so a language switch rewords it at once.
#[derive(Clone, Debug, Default, PartialEq)]
enum Status {
    #[default]
    Empty,
    TypeKey,
    LoggingIn,
    LoggedIn,
    ChooseRoom,
    LogInFirst,
    WrongKey,
    TooManyTries,
    LoginRefused(u16),
    Unreachable(io::ErrorKind),
    BadCode,
    Creating,
    Joining,
    LoginExpired,
    ConnectionRefused(u16),
    /// The WebSocket library's own (English) words.
    ConnectionFailed(String),
    Seated {
        code: String,
        player: usize,
    },
    Room {
        code: String,
        player: usize,
        state: RoomState,
    },
    /// An error or close reason the server sent.
    Server(String),
    Left,
    Resuming,
    ResumeFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoomState {
    Waiting,
    AskedRematch,
    RivalAsksRematch,
    Fight,
}

impl Status {
    fn text(&self, lang: Lang, origin: &str) -> String {
        let zh = lang == Lang::Zh;
        let color = |player: usize| crate::ui::color_name(player, lang);
        match self {
            Self::Empty => String::new(),
            Self::TypeKey if zh => format!("服务器 {origin}：请输入服务口令"),
            Self::TypeKey => format!("SERVER {origin}: TYPE THE ACCESS KEY"),
            Self::LoggingIn if zh => format!("正在登录 {origin}……"),
            Self::LoggingIn => format!("LOGGING IN TO {origin} ..."),
            Self::LoggedIn => lang
                .pick(
                    "已登录。请创建房间，或输入朋友发来的匹配码",
                    "LOGGED IN. CREATE A ROOM, OR TYPE A FRIEND'S ROOM CODE",
                )
                .into(),
            Self::ChooseRoom => lang
                .pick(
                    "请创建房间，或输入朋友发来的匹配码",
                    "CREATE A ROOM, OR TYPE A FRIEND'S ROOM CODE",
                )
                .into(),
            Self::LogInFirst => lang
                .pick("请先输入服务口令登录", "LOG IN FIRST: TYPE THE ACCESS KEY")
                .into(),
            Self::WrongKey if zh => format!("口令不正确，或 {origin} 不是服务器设置的地址"),
            Self::WrongKey => {
                format!("WRONG KEY, OR {origin} IS NOT THE ADDRESS THE SERVER WAS SET UP FOR")
            }
            Self::TooManyTries => lang
                .pick("尝试次数太多，请等一分钟", "TOO MANY TRIES: WAIT A MINUTE")
                .into(),
            Self::LoginRefused(code) if zh => format!("服务器拒绝登录（HTTP {code}）"),
            Self::LoginRefused(code) => format!("THE SERVER REFUSED THE LOGIN (HTTP {code})"),
            Self::Unreachable(kind) => {
                let why = match kind {
                    io::ErrorKind::ConnectionRefused => {
                        lang.pick("连接被拒绝", "CONNECTION REFUSED")
                    }
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => {
                        lang.pick("连接超时", "TIMED OUT")
                    }
                    io::ErrorKind::NotFound => lang.pick("找不到这个地址", "ADDRESS NOT FOUND"),
                    io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::UnexpectedEof => lang.pick("连接被中断", "CONNECTION DROPPED"),
                    io::ErrorKind::InvalidData => {
                        lang.pick("对方不是 HTTP 服务器", "NOT AN HTTP RESPONSE")
                    }
                    _ => lang.pick("网络错误", "NETWORK ERROR"),
                };
                if zh {
                    format!("连不上 {origin}：{why}")
                } else {
                    format!("CANNOT REACH {origin}: {why}")
                }
            }
            Self::BadCode => lang
                .pick(
                    "匹配码为 8 位字母或数字",
                    "A ROOM CODE IS 8 LETTERS OR DIGITS",
                )
                .into(),
            Self::Creating => lang.pick("正在创建房间……", "CREATING A ROOM ...").into(),
            Self::Joining => lang.pick("正在加入房间……", "JOINING THE ROOM ...").into(),
            Self::LoginExpired => lang
                .pick(
                    "登录已失效或被拒绝，请重新输入口令",
                    "LOGIN EXPIRED OR REFUSED: TYPE THE ACCESS KEY AGAIN",
                )
                .into(),
            Self::ConnectionRefused(code) if zh => format!("服务器拒绝连接（HTTP {code}）"),
            Self::ConnectionRefused(code) => {
                format!("THE SERVER REFUSED THE CONNECTION (HTTP {code})")
            }
            Self::ConnectionFailed(_) if zh => "连接失败：请检查服务器地址与网络".into(),
            Self::ConnectionFailed(error) => format!("CONNECTION FAILED: {error}"),
            Self::Seated { code, player } if zh => format!(
                "房间 {}：你是 P{}（{}）",
                show_code(code),
                player + 1,
                color(*player)
            ),
            Self::Seated { code, player } => format!(
                "ROOM {}: YOU ARE P{} ({})",
                show_code(code),
                player + 1,
                color(*player)
            ),
            Self::Room {
                code,
                player,
                state,
            } => {
                let state = match state {
                    RoomState::Waiting => lang.pick(
                        "等待对方 / 断线暂停",
                        "WAITING FOR THE OTHER PLAYER / PAUSED",
                    ),
                    RoomState::AskedRematch => lang.pick(
                        "已请求重开，等对方确认",
                        "REMATCH ASKED: WAITING FOR THE OTHER PLAYER",
                    ),
                    RoomState::RivalAsksRematch => lang.pick(
                        "对方请求重开，按 R / Start 确认",
                        "THE OTHER PLAYER ASKS FOR A REMATCH: R / START",
                    ),
                    RoomState::Fight => lang.pick("服务器对战中", "FIGHT ON THE SERVER"),
                };
                if zh {
                    format!(
                        "房间 {} · 你是 P{}（{}） · {state}",
                        show_code(code),
                        player + 1,
                        color(*player)
                    )
                } else {
                    format!(
                        "ROOM {}  P{} {}: {state}",
                        show_code(code),
                        player + 1,
                        color(*player)
                    )
                }
            }
            Self::Server(text) => reason(lang, text),
            Self::Left => lang
                .pick(
                    "已离开；可以创建或加入另一间房",
                    "LEFT THE ROOM. CREATE OR JOIN ANOTHER ONE",
                )
                .into(),
            Self::Resuming => lang
                .pick(
                    "连接中断，正在恢复座位……",
                    "CONNECTION LOST: RESUMING THE SEAT ...",
                )
                .into(),
            Self::ResumeFailed => lang
                .pick(
                    "自动恢复失败，请重新创建或加入（断线席位保留 5 分钟）",
                    "COULD NOT RESUME. CREATE OR JOIN AGAIN (A LOST SEAT IS KEPT 5 MIN)",
                )
                .into(),
        }
    }
}

#[derive(Default)]
struct Shared {
    active: bool,
    player: Option<usize>,
    room: Option<String>,
    transport: u32,
    status: Status,
    /// The language `status` is worded in.
    lang: Lang,
    snapshot: Option<Snapshot>,
    logged_in: bool,
    busy: bool,
    /// Something the player must read happened (an error, or the room closed).
    attention: bool,
}

enum Command {
    Login(String),
    Active(bool),
    Create,
    Join(String),
    Leave,
    Input(u16, u16),
    Restart,
}

/// The handle the game keeps. Dropping it stops the worker and closes the socket.
pub struct Client {
    server: Server,
    shared: Arc<Mutex<Shared>>,
    commands: mpsc::Sender<Command>,
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

impl Client {
    /// `name` is the player-supplied display name (`--name`); anything the server
    /// would reject is dropped here so the wire never carries it.
    pub fn start(server: Server, name: Option<&str>, lang: Lang) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            status: Status::TypeKey,
            lang,
            ..Shared::default()
        }));
        let (commands, rx) = mpsc::channel();
        let worker = Worker {
            server: server.clone(),
            shared: shared.clone(),
            cookie: None,
            socket: None,
            session: None,
            name: name.and_then(valid_name),
            last_seq: None,
            round: None,
            input_seq: 0,
            retries: 0,
            reconnect_at: None,
            intentional: false,
            active: false,
        };
        thread::Builder::new()
            .name("naifrog-online".into())
            .spawn(move || worker.run(rx))
            .expect("start the online thread");
        Self {
            server,
            shared,
            commands,
        }
    }
    pub fn server(&self) -> &Server {
        &self.server
    }
    fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
    pub fn login(&self, key: &str) {
        let key = key.trim();
        let mut shared = lock(&self.shared);
        if shared.busy || key.is_empty() {
            return;
        }
        shared.busy = true;
        shared.status = Status::LoggingIn;
        drop(shared);
        self.send(Command::Login(key.into()));
    }
    /// Switch to online play. The renderer sees `active` on its next poll.
    pub fn enter(&self) {
        lock(&self.shared).active = true;
        self.send(Command::Active(true));
    }
    /// Leave online play entirely: leave the room and hand control back to local modes.
    pub fn exit(&self) {
        {
            let mut shared = lock(&self.shared);
            shared.active = false;
            shared.player = None;
            shared.room = None;
            shared.snapshot = None;
        }
        self.send(Command::Active(false));
    }
    pub fn create(&self) {
        self.send(Command::Create);
    }
    /// Join by a typed code; a malformed code only updates the status line.
    pub fn join(&self, typed: &str) {
        match room_code(typed) {
            Some(code) => self.send(Command::Join(code)),
            None => lock(&self.shared).status = Status::BadCode,
        }
    }
    pub fn leave(&self) {
        self.send(Command::Leave);
    }
    /// This tick's controls; dropped unless seated in a room with a snapshot.
    pub fn input(&self, held: u16, pressed: u16) {
        self.send(Command::Input(held, pressed));
    }
    /// Vote to restart; the server restarts once both seats voted for the same round.
    pub fn restart(&self) {
        self.send(Command::Restart);
    }
    /// What the renderer reads each frame. The snapshot is handed over once.
    pub fn poll(&self) -> RemoteView {
        let mut shared = lock(&self.shared);
        RemoteView {
            active: shared.active,
            player: shared.player,
            room: shared.room.clone(),
            transport: shared.transport,
            status: shared.status.text(shared.lang, &self.server.origin),
            snapshot: shared.snapshot.take(),
        }
    }
    pub fn logged_in(&self) -> bool {
        lock(&self.shared).logged_in
    }
    /// The status line, worded in the language last set.
    pub fn status(&self) -> String {
        let shared = lock(&self.shared);
        shared.status.text(shared.lang, &self.server.origin)
    }
    pub fn set_lang(&self, lang: Lang) {
        lock(&self.shared).lang = lang;
    }
    /// True once after an error or a closed room, so the game can show its menu.
    pub fn take_attention(&self) -> bool {
        std::mem::take(&mut lock(&self.shared).attention)
    }
}

struct Worker {
    server: Server,
    shared: Arc<Mutex<Shared>>,
    cookie: Option<String>,
    socket: Option<WebSocket<TcpStream>>,
    /// Room code and resume token of the seat to come back to after a drop.
    session: Option<(String, String)>,
    /// The validated display name sent with Create/Join; `None` takes the seat default.
    name: Option<String>,
    last_seq: Option<u64>,
    round: Option<u32>,
    input_seq: u32,
    retries: u32,
    reconnect_at: Option<Instant>,
    /// The socket is closing on purpose (left, refused, room closed): do not resume.
    intentional: bool,
    active: bool,
}

impl Worker {
    fn run(mut self, rx: mpsc::Receiver<Command>) {
        loop {
            let wait = if self.socket.is_some() {
                Duration::ZERO
            } else {
                IDLE_SLICE
            };
            match rx.recv_timeout(wait) {
                Ok(command) => {
                    self.handle(command);
                    while let Ok(command) = rx.try_recv() {
                        self.handle(command);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.leave_room();
                    return;
                }
            }
            if self.reconnect_at.is_some_and(|at| Instant::now() >= at) {
                self.reconnect_at = None;
                if let (true, Some((code, resume))) = (self.active, self.session.clone()) {
                    self.connect(ClientMessage::Join {
                        version: PROTOCOL_VERSION,
                        code,
                        resume: Some(resume),
                        name: self.name.clone(),
                    });
                }
            }
            self.read();
        }
    }
    fn shared(&self) -> MutexGuard<'_, Shared> {
        lock(&self.shared)
    }
    fn status(&self, status: Status) {
        self.shared().status = status;
    }
    fn alert(&self, status: Status) {
        let mut shared = self.shared();
        shared.status = status;
        shared.attention = true;
    }
    fn handle(&mut self, command: Command) {
        match command {
            Command::Login(key) => self.login(&key),
            Command::Active(on) => {
                if on && !self.active {
                    self.active = true;
                    self.drop_socket();
                    // Keep a line already there, such as the answer to a login sent first.
                    let mut shared = self.shared();
                    if shared.status == Status::Empty {
                        shared.status = if shared.logged_in {
                            Status::ChooseRoom
                        } else {
                            Status::TypeKey
                        };
                    }
                } else if !on && self.active {
                    self.leave_room();
                    self.active = false;
                    self.status(Status::Empty);
                }
            }
            Command::Create | Command::Join(_) if !self.shared().logged_in => {
                self.alert(Status::LogInFirst);
            }
            Command::Create => {
                self.session = None;
                self.connect(ClientMessage::Create {
                    version: PROTOCOL_VERSION,
                    name: self.name.clone(),
                });
            }
            Command::Join(code) => {
                self.session = None;
                self.connect(ClientMessage::Join {
                    version: PROTOCOL_VERSION,
                    code,
                    resume: None,
                    name: self.name.clone(),
                });
            }
            Command::Leave => {
                self.leave_room();
                self.status(Status::Left);
            }
            Command::Input(held, pressed) => {
                let seated = self.active && self.shared().player.is_some();
                if let (true, Some(round)) = (seated, self.round) {
                    self.input_seq = self.input_seq.wrapping_add(1);
                    self.write(&ClientMessage::Input {
                        seq: self.input_seq,
                        round,
                        held: held & INPUT_MASK,
                        pressed: pressed & TRIG,
                    });
                }
            }
            Command::Restart => {
                let seated = self.active && self.shared().player.is_some();
                if let (true, Some(round)) = (seated, self.round) {
                    self.write(&ClientMessage::Restart { round });
                }
            }
        }
    }
    fn login(&mut self, key: &str) {
        let body = serde_json::json!({ "access_key": key }).to_string();
        let result = self.post("/api/auth", &body);
        if let Ok((200, Some(cookie))) = &result {
            self.cookie = Some(cookie.clone());
        }
        let mut shared = self.shared();
        shared.busy = false;
        shared.status = match result {
            Ok((200, Some(_))) => {
                shared.logged_in = true;
                Status::LoggedIn
            }
            Ok((401, _)) => Status::WrongKey,
            Ok((429, _)) => Status::TooManyTries,
            Ok((status, _)) => Status::LoginRefused(status),
            Err(error) => Status::Unreachable(error.kind()),
        };
    }
    /// One HTTP/1.1 POST; returns the status and the session cookie's `name=value`.
    fn post(&self, path: &str, body: &str) -> io::Result<(u16, Option<String>)> {
        let mut stream = self.server.connect()?;
        write!(
            stream,
            "POST {path} HTTP/1.1\r\nHost: {}\r\nOrigin: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.server.authority,
            self.server.origin,
            body.len()
        )?;
        let mut response = Vec::new();
        stream.take(MAX_RESPONSE).read_to_end(&mut response)?;
        let invalid = || io::Error::new(io::ErrorKind::InvalidData, "not an HTTP response");
        let end = response
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(invalid)?;
        let head = std::str::from_utf8(&response[..end]).map_err(|_| invalid())?;
        let mut lines = head.split("\r\n");
        let status = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .ok_or_else(invalid)?;
        let cookie = lines
            .filter_map(|l| l.split_once(':'))
            .filter(|(name, _)| name.trim().eq_ignore_ascii_case("set-cookie"))
            .filter_map(|(_, value)| value.split(';').next())
            .map(str::trim)
            .find(|pair| pair.contains("naifrog_session="))
            .map(String::from);
        Ok((status, cookie))
    }
    /// Forget the socket and everything seen through it. Like the page's `drop()`, the new
    /// `transport` makes the renderer drop held inputs and old snapshots.
    fn drop_socket(&mut self) {
        self.reconnect_at = None;
        if let Some(mut socket) = self.socket.take() {
            let _ = socket.close(None);
            let _ = socket.flush();
        }
        self.last_seq = None;
        self.round = None;
        let mut shared = self.shared();
        shared.transport = shared.transport.wrapping_add(1);
        shared.player = None;
        shared.room = None;
        shared.snapshot = None;
    }
    fn leave_room(&mut self) {
        self.intentional = true;
        self.write(&ClientMessage::Leave);
        self.drop_socket();
        self.session = None;
    }
    fn connect(&mut self, first: ClientMessage) {
        self.drop_socket();
        self.intentional = false;
        self.status(if matches!(first, ClientMessage::Create { .. }) {
            Status::Creating
        } else {
            Status::Joining
        });
        match self.open() {
            Ok(socket) => {
                self.socket = Some(socket);
                self.write(&first);
            }
            Err(error) => {
                self.alert(error);
                self.closed();
            }
        }
    }
    fn open(&mut self) -> Result<WebSocket<TcpStream>, Status> {
        let failed = |e: &dyn std::fmt::Display| Status::ConnectionFailed(e.to_string());
        let stream = self
            .server
            .connect()
            .map_err(|e| Status::Unreachable(e.kind()))?;
        let mut request = format!("ws://{}/api/ws", self.server.authority)
            .into_client_request()
            .map_err(|e| failed(&e))?;
        let headers = request.headers_mut();
        let value = |v: &str| HeaderValue::from_str(v).map_err(|e| failed(&e));
        headers.insert("Origin", value(&self.server.origin)?);
        if let Some(cookie) = &self.cookie {
            headers.insert("Cookie", value(cookie)?);
        }
        match tungstenite::client(request, stream) {
            Ok((socket, _)) => {
                socket
                    .get_ref()
                    .set_read_timeout(Some(READ_SLICE))
                    .map_err(|e| failed(&e))?;
                Ok(socket)
            }
            Err(HandshakeError::Failure(tungstenite::Error::Http(response)))
                if response.status() == 401 =>
            {
                self.shared().logged_in = false;
                self.session = None;
                Err(Status::LoginExpired)
            }
            Err(HandshakeError::Failure(tungstenite::Error::Http(response))) => {
                Err(Status::ConnectionRefused(response.status().as_u16()))
            }
            Err(error) => Err(failed(&error)),
        }
    }
    fn write(&mut self, message: &ClientMessage) {
        let Some(socket) = self.socket.as_mut() else {
            return;
        };
        let Ok(text) = serde_json::to_string(message) else {
            return;
        };
        if socket.send(Message::text(text)).is_err() {
            self.socket = None;
            self.closed();
        }
    }
    fn read(&mut self) {
        let Some(socket) = self.socket.as_mut() else {
            return;
        };
        match socket.read() {
            Ok(Message::Text(text)) if text.len() <= MAX_MESSAGE => {
                match serde_json::from_str::<ServerMessage>(&text) {
                    Ok(message) => self.message(message),
                    Err(_) => self.fail_protocol(),
                }
            }
            Ok(Message::Text(_) | Message::Binary(_)) => self.fail_protocol(),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => {
                self.socket = None;
                self.closed();
            }
        }
    }
    fn fail_protocol(&mut self) {
        self.drop_socket();
        self.closed();
    }
    fn message(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Welcome {
                code,
                player,
                resume,
            } => {
                let valid = player < 2
                    && room_code(&code).as_deref() == Some(code.as_str())
                    && resume.len() == 64
                    && resume.bytes().all(|b| b.is_ascii_hexdigit());
                if !valid {
                    self.fail_protocol();
                    return;
                }
                self.input_seq = 0;
                self.retries = 0;
                let mut shared = self.shared();
                shared.player = Some(player);
                shared.room = Some(code.clone());
                shared.status = Status::Seated {
                    code: code.clone(),
                    player,
                };
                drop(shared);
                self.session = Some((code, resume));
            }
            ServerMessage::Snapshot { mut frame } => {
                if self.last_seq.is_some_and(|seq| frame.seq <= seq) {
                    return;
                }
                self.last_seq = Some(frame.seq);
                self.round = Some(frame.round);
                let mut shared = self.shared();
                // Hits from snapshots the renderer has not read yet must still show.
                if let Some(old) = shared.snapshot.take() {
                    let mut hits = old.hits;
                    hits.append(&mut frame.hits);
                    frame.hits = hits;
                }
                if frame.hits.len() > MAX_HITS {
                    let n = frame.hits.len() - MAX_HITS;
                    frame.hits.drain(..n);
                }
                if let (Some(you), Some(code)) = (shared.player, shared.room.clone()) {
                    let other = 1 - you;
                    let state = if !frame.connected.iter().all(|c| *c) {
                        RoomState::Waiting
                    } else if frame.votes[you] {
                        RoomState::AskedRematch
                    } else if frame.votes[other] {
                        RoomState::RivalAsksRematch
                    } else {
                        RoomState::Fight
                    };
                    shared.status = Status::Room {
                        code,
                        player: you,
                        state,
                    };
                }
                shared.snapshot = Some(*frame);
            }
            ServerMessage::Error { message } => {
                self.intentional = true;
                if message == "room unavailable" {
                    self.session = None;
                }
                self.alert(Status::Server(message));
            }
            ServerMessage::Closed { reason: why } => {
                self.intentional = true;
                self.session = None;
                self.alert(Status::Server(why));
            }
        }
    }
    /// The socket is gone. Resume the seat unless that was on purpose, as the page does.
    fn closed(&mut self) {
        self.shared().player = None;
        if self.intentional || self.session.is_none() || !self.active {
            return;
        }
        self.retries += 1;
        if self.retries <= RETRIES {
            self.status(Status::Resuming);
            self.reconnect_at =
                Some(Instant::now() + Duration::from_millis(1000 * u64::from(self.retries.min(4))));
        } else {
            self.session = None;
            self.alert(Status::ResumeFailed);
        }
    }
}

/// The server's reasons, as a player reads them. The Chinese follows web/network.js.
fn reason(lang: Lang, text: &str) -> String {
    let (zh, en) = match text {
        "room unavailable" => (
            "房间不存在、已满或已过期",
            "NO SUCH ROOM, OR IT IS FULL OR EXPIRED",
        ),
        "server full" => (
            "服务器房间已满，请稍后再试",
            "THE SERVER HAS NO FREE ROOM: TRY LATER",
        ),
        "already in another room" => (
            "你已在另一个房间里（另一个窗口？）",
            "YOU ARE ALREADY IN ANOTHER ROOM (ANOTHER WINDOW?)",
        ),
        "client outdated, reload the page" => (
            "游戏版本比服务器旧，请更新",
            "THIS GAME IS OLDER THAN THE SERVER: UPDATE IT",
        ),
        "connection replaced" => (
            "这个席位已在另一个窗口打开",
            "THIS SEAT WAS OPENED IN ANOTHER WINDOW",
        ),
        "server transfer budget exhausted" => (
            "服务器本小时流量额度已用完，请稍后再试",
            "THE SERVER USED UP THIS HOUR'S TRAFFIC: TRY LATER",
        ),
        "room closed" => (
            "房间已关闭（对方离开或超时）",
            "THE ROOM CLOSED (THE OTHER PLAYER LEFT OR IT TIMED OUT)",
        ),
        "request denied" => ("请求被拒绝，连接已断开", "REQUEST DENIED: DISCONNECTED"),
        other => {
            return match lang {
                Lang::Zh => format!("服务器：{other}"),
                Lang::En => format!("SERVER: {}", other.to_ascii_uppercase()),
            };
        }
    };
    lang.pick(zh, en).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_addresses_parse_like_a_browser_origin() {
        let s = Server::parse("HTTP://203.0.113.10:8080/").unwrap();
        assert_eq!(s.origin(), "http://203.0.113.10:8080");
        assert_eq!((s.authority.as_str(), s.port), ("203.0.113.10:8080", 8080));
        let s = Server::parse("http://example.com:80").unwrap();
        assert_eq!(
            (s.origin(), s.authority.as_str()),
            ("http://example.com", "example.com:80")
        );
        let s = Server::parse("http://[::1]:18080").unwrap();
        assert_eq!((s.origin(), s.host.as_str()), ("http://[::1]:18080", "::1"));
        for bad in [
            "https://example.com",
            "example.com:8080",
            "http://",
            "http://host:0",
            "http://host:99999",
            "http://host/path",
            "http://user@host",
        ] {
            assert!(Server::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(
            Server::parse("https://example.com"),
            Err(ServerProblem::Https)
        );
        assert_eq!(
            Server::parse("http://host/path")
                .unwrap_err()
                .text(Lang::Zh),
            "服务器地址应形如 http://IP:端口，而不是“http://host/path”"
        );
    }

    /// Every status line in both languages; the desktop menu draws them.
    fn statuses() -> Vec<Status> {
        let room = |state| Status::Room {
            code: "ABCDEFGH".into(),
            player: 1,
            state,
        };
        let mut all = vec![
            Status::Empty,
            Status::TypeKey,
            Status::LoggingIn,
            Status::LoggedIn,
            Status::ChooseRoom,
            Status::LogInFirst,
            Status::WrongKey,
            Status::TooManyTries,
            Status::LoginRefused(503),
            Status::BadCode,
            Status::Creating,
            Status::Joining,
            Status::LoginExpired,
            Status::ConnectionRefused(403),
            Status::ConnectionFailed("io error".into()),
            Status::Seated {
                code: "ABCDEFGH".into(),
                player: 0,
            },
            room(RoomState::Waiting),
            room(RoomState::AskedRematch),
            room(RoomState::RivalAsksRematch),
            room(RoomState::Fight),
            Status::Left,
            Status::Resuming,
            Status::ResumeFailed,
        ];
        for kind in [
            io::ErrorKind::ConnectionRefused,
            io::ErrorKind::TimedOut,
            io::ErrorKind::NotFound,
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::InvalidData,
            io::ErrorKind::PermissionDenied,
        ] {
            all.push(Status::Unreachable(kind));
        }
        for why in [
            "room unavailable",
            "server full",
            "already in another room",
            "client outdated, reload the page",
            "connection replaced",
            "server transfer budget exhausted",
            "room closed",
            "request denied",
        ] {
            all.push(Status::Server(why.into()));
        }
        all
    }

    #[test]
    fn status_lines_come_in_both_languages_and_the_canvas_draws_them() {
        let origin = "http://192.168.1.20:8080";
        for status in statuses() {
            let (zh, en) = (status.text(Lang::Zh, origin), status.text(Lang::En, origin));
            assert_eq!(zh.is_empty(), status == Status::Empty);
            if status != Status::Empty {
                assert_ne!(zh, en);
            }
            assert!(zh.chars().all(crate::i18n::drawn), "{zh}");
            assert!(en.is_ascii(), "{en}");
        }
        assert_eq!(
            Status::Server("room closed".into()).text(Lang::Zh, origin),
            "房间已关闭（对方离开或超时）"
        );
        assert_eq!(
            Status::Room {
                code: "ABCDEFGH".into(),
                player: 0,
                state: RoomState::Fight,
            }
            .text(Lang::En, origin),
            "ROOM ABCD-EFGH  P1 GOLD: FIGHT ON THE SERVER"
        );
    }

    #[test]
    fn room_codes_ignore_case_spaces_and_the_dash() {
        assert_eq!(room_code("abcd-efgh").as_deref(), Some("ABCDEFGH"));
        assert_eq!(room_code(" WXYZ 2345 ").as_deref(), Some("WXYZ2345"));
        assert_eq!(room_code("ABCDEFG"), None);
        assert_eq!(room_code("ABCDEFGI"), None, "I, O, 0 and 1 are never used");
        assert_eq!(show_code("ABCDEFGH"), "ABCD-EFGH");
    }

    /// Two desktop clients against the real server in this process: log in, create, join,
    /// move, rematch votes, leave.
    #[cfg(feature = "server")]
    #[test]
    fn two_desktop_clients_play_a_room_on_the_real_server() {
        use crate::server::{Config, router};
        const KEY: &str = "0123456789abcdef0123456789abcdef";
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let origin = format!("http://{addr}");
        let config = Config::new(addr, &origin, KEY, std::env::current_dir().unwrap()).unwrap();
        runtime.spawn(async move {
            axum::serve(
                listener,
                router(config).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap();
        });
        fn wait(what: &str, mut done: impl FnMut() -> bool) {
            let end = Instant::now() + Duration::from_secs(10);
            while !done() {
                assert!(Instant::now() < end, "timed out: {what}");
                thread::sleep(Duration::from_millis(10));
            }
        }
        let server = Server::parse(&origin).unwrap();
        let (a, b) = (
            Client::start(server.clone(), Some("Desk-One"), Lang::En),
            Client::start(server, Some("desk two"), Lang::Zh),
        );
        // Entering online play right after a login keeps the login's answer on screen.
        a.login("wrong key");
        a.enter();
        wait("refused", || a.status().starts_with("WRONG KEY"));
        thread::sleep(Duration::from_millis(100));
        assert!(a.status().starts_with("WRONG KEY"), "{}", a.status());
        assert!(!a.logged_in());
        a.exit();
        a.create();
        wait("not logged in", || a.take_attention());
        for c in [&a, &b] {
            c.login(KEY);
            c.enter();
        }
        wait("logins", || a.logged_in() && b.logged_in());
        a.create();
        let mut code = None;
        wait("welcome", || {
            code = a.poll().room;
            code.is_some()
        });
        let code = code.unwrap();
        b.join(&show_code(&code).to_lowercase());
        let mut last = [None, None];
        let mut snapshot = |c: &Client, i: usize| {
            let view = c.poll();
            if view.snapshot.is_some() {
                last[i] = view.snapshot.map(|s| (view.player, s));
            }
            last[i].clone()
        };
        wait("both seated", || {
            snapshot(&a, 0).is_some_and(|(p, s)| p == Some(0) && s.connected == [true, true])
                && snapshot(&b, 1).is_some_and(|(p, _)| p == Some(1))
        });
        // The server steps the fight once both seats have sent controls, as every game tick does.
        for _ in 0..5 {
            a.input(0, 0);
            b.input(0, 0);
            thread::sleep(Duration::from_millis(16));
        }
        let start = snapshot(&a, 0).unwrap().1.arena.fighters[0].x;
        wait("P1 walks right", || {
            a.input(crate::controls::RIGHT, 0);
            b.input(0, 0);
            thread::sleep(Duration::from_millis(16));
            snapshot(&b, 1).is_some_and(|(_, s)| s.arena.fighters[0].x > start)
        });
        a.restart();
        wait("vote seen", || {
            snapshot(&b, 1).is_some_and(|(_, s)| s.votes == [true, false])
        });
        a.leave();
        wait("left", || {
            a.poll().room.is_none() && a.status().starts_with("LEFT")
        });
        wait("room closed for B", || b.take_attention());
        assert!(b.status().contains("房间已关闭"), "{}", b.status());
        // A language switch rewords the line already shown.
        b.set_lang(Lang::En);
        assert!(b.status().contains("ROOM CLOSED"), "{}", b.status());
        a.exit();
        assert!(!a.poll().active);
    }
}
