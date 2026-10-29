//! Dedicated, renderer-free authoritative server with bounded in-memory resources.
//! Serve behind a TLS reverse proxy. Tokens and rooms are intentionally not persisted.

use crate::{
    combat::{Arena, Hit},
    controls::{InputMux, Source},
    net::{ClientMessage, PROTOCOL_VERSION, ServerMessage, Snapshot, valid_input, valid_name},
};
use axum::{
    Json, Router,
    body::{Body, Bytes, to_bytes},
    extract::{
        ConnectInfo, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, Weak},
    time::{Duration, Instant, SystemTime},
};
use subtle::ConstantTimeEq;
use tokio::{
    io::AsyncReadExt,
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, watch},
    time::{MissedTickBehavior, timeout},
};

const CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const MAX_WS: usize = 32;
const MAX_WS_PER_IP: usize = 4;
const MAX_WS_PER_SESSION: usize = 2;
const MAX_SESSIONS: usize = 256;
const MAX_LIMITER_IPS: usize = 1024;
const MAX_ROOM_CONFIG: usize = 256;
const INPUT_TIMEOUT: Duration = Duration::from_millis(250);
/// A room lives while a seat's connection answers the heartbeat (a ping every
/// PING_INTERVAL). A dropped seat is kept this long: enough for a tablet that switched
/// apps to reload the page (about 22 MB) on a slow link and log in again.
const RESUME_GRACE: Duration = Duration::from_secs(300);
/// Backstop for a room whose seats stay connected: the login session ends then anyway.
const ROOM_LIFETIME: Duration = SESSION_LIFETIME;
const VOTE_WINDOW: Duration = Duration::from_secs(10);
const SESSION_LIFETIME: Duration = Duration::from_secs(12 * 3600);
const SEND_TIMEOUT: Duration = Duration::from_secs(2);
const FIRST_MESSAGE_TIMEOUT: Duration = Duration::from_secs(5);
const PING_INTERVAL: Duration = Duration::from_secs(10);
const PONG_TIMEOUT: Duration = Duration::from_secs(10);
/// After our Close frame, how long and how much client data we read and discard before dropping.
const CLOSE_LINGER: Duration = Duration::from_secs(1);
const CLOSE_DRAIN_BYTES: u64 = 64 * 1024;
const MAX_PACKET: usize = 1024;
const MAX_HITS: usize = 64;
const MAX_WASM_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Concurrent streaming producers. Each holds at most two 64 KiB chunks in memory.
const WASM_PRODUCERS: usize = 4;
const FILE_PRODUCERS: usize = 64;
const PERMIT_WAIT: Duration = Duration::from_secs(10);
const WASM_DEADLINE: Duration = Duration::from_secs(120);
/// A 2 MiB texture must finish over a ~2.5 Mbit/s uplink that two devices share, six
/// connections each (about 27 KiB/s per connection): 30 s cut the stage art off mid-file.
const FILE_DEADLINE: Duration = Duration::from_secs(120);
/// WASM downloads per IP per minute. A household behind one NAT address loads two
/// devices and reloads now and then; the hourly egress budget still caps the bytes.
const WASM_PER_IP_PER_MINUTE: u32 = 6;
/// Sent with a gzip-encoded file: its decoded length, so the page can show progress.
const UNCOMPRESSED_LENGTH: &str = "x-uncompressed-length";
const CHUNK: usize = 64 * 1024;
/// Unauthenticated pages may use at most 1/16 of the shared hourly egress budget.
const PUBLIC_EGRESS_DIVISOR: u64 = 16;
/// Conservative per-message allowance for WebSocket framing.
const WS_FRAME_OVERHEAD: u64 = 16;
const BUDGET_EXHAUSTED: &str = "server transfer budget exhausted";
/// Browser caching, the content-hashed-URL scheme of mature web game engines. API answers,
/// refusals and the page itself (`/`) are never stored. The page's scripts and stylesheet
/// and game files under their plain URLs are revalidated on every load: the ETag answers
/// an unchanged file with a bodiless 304. A game file asked for under `/v/<hash>/`, where `<hash>` is that file's current
/// content fingerprint (listed by /api/manifest), is kept for a year without asking: a
/// changed file gets a new hash and so a new URL, while unchanged files keep theirs.
const NO_STORE: &str = "private, no-store";
const REVALIDATE_PAGE: &str = "no-cache";
const REVALIDATE_GAME: &str = "private, no-cache";
const IMMUTABLE_GAME: &str = "private, max-age=31536000, immutable";
/// File types under /pkg and /assets/processed that may be served at all.
const GAME_EXTENSIONS: [&str; 7] = ["js", "wasm", "webp", "png", "json", "gif", "otf"];
/// Upper bound on cached fingerprints; the two served directories hold a few dozen files.
const MAX_FINGERPRINTS: usize = 1024;
const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";

/// Validated immutable configuration; the access key is retained only as SHA-256.
#[derive(Clone)]
pub struct Config {
    bind: SocketAddr,
    origin: String,
    access_digest: [u8; 32],
    asset_root: PathBuf,
    secure_cookie: bool,
    /// Plain HTTP reachable from other machines: the key and cookies are not encrypted.
    public_http: bool,
    /// The access key is shorter than `ACCESS_KEY_MIN`: only the login rate limits guard it.
    short_key: bool,
    max_rooms: usize,
    proxy_ips: HashSet<IpAddr>,
    egress_limit: u64,
}

#[derive(Debug)]
pub struct ConfigError(&'static str);
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ConfigError {}

/// A generated access key has at least this many bytes.
const ACCESS_KEY_MIN: usize = 32;
/// The floor for a key a person picks, allowed only with `Relaxed::short_key`.
const SHORT_ACCESS_KEY_MIN: usize = 6;

/// Explicit opt-outs from the secure defaults, each behind its own environment flag.
#[derive(Clone, Copy, Debug, Default)]
pub struct Relaxed {
    /// Plain HTTP on a public address, for a server without a domain (`http://IP:port`).
    /// The access key, session cookie and game traffic then cross the network unencrypted.
    pub public_http: bool,
    /// An access key of `SHORT_ACCESS_KEY_MIN` bytes or more instead of `ACCESS_KEY_MIN`.
    pub short_key: bool,
}

impl Config {
    /// Construct a secure configuration. Plain HTTP is allowed only for loopback origins/binds.
    pub fn new(
        bind: SocketAddr,
        origin: impl Into<String>,
        access_key: impl AsRef<[u8]>,
        asset_root: impl Into<PathBuf>,
    ) -> Result<Self, ConfigError> {
        Self::new_relaxed(bind, origin, access_key, asset_root, Relaxed::default())
    }

    /// Like `new`, but with the opt-outs in `relaxed`, which must each be asked for explicitly.
    pub fn new_relaxed(
        bind: SocketAddr,
        origin: impl Into<String>,
        access_key: impl AsRef<[u8]>,
        asset_root: impl Into<PathBuf>,
        relaxed: Relaxed,
    ) -> Result<Self, ConfigError> {
        let origin = origin.into();
        let uri: Uri = origin.parse().map_err(|_| ConfigError("invalid origin"))?;
        let scheme = uri.scheme_str().ok_or(ConfigError("invalid origin"))?;
        let authority = uri.authority().ok_or(ConfigError("invalid origin"))?;
        let host = uri.host().ok_or(ConfigError("invalid origin"))?;
        if !matches!(scheme, "http" | "https")
            || authority.as_str().contains('@')
            || origin != format!("{scheme}://{authority}")
            || host != host.to_ascii_lowercase()
            || !host.is_ascii()
            || host.is_empty()
            || (scheme == "http" && uri.port_u16() == Some(80))
            || (scheme == "https" && uri.port_u16() == Some(443))
            || (authority.as_str().contains(':')
                && !authority.as_str().ends_with(']')
                && uri.port_u16().is_none())
        {
            return Err(ConfigError(
                "origin must be an exact canonical HTTP(S) origin without a path",
            ));
        }
        let host_ip = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .ok();
        let local_origin = host == "localhost" || host_ip.is_some_and(|ip| ip.is_loopback());
        let public_http = scheme == "http" && (!local_origin || !bind.ip().is_loopback());
        if public_http && !relaxed.public_http {
            return Err(ConfigError(
                "plain HTTP on a public address needs NAIFROG_ALLOW_INSECURE_PUBLIC_HTTP=1",
            ));
        }
        let key_len = access_key.as_ref().len();
        if key_len < ACCESS_KEY_MIN && !relaxed.short_key {
            return Err(ConfigError(
                "access key must contain at least 32 bytes (NAIFROG_ALLOW_SHORT_ACCESS_KEY=1 allows 6)",
            ));
        }
        if key_len < SHORT_ACCESS_KEY_MIN {
            return Err(ConfigError("access key must contain at least 6 bytes"));
        }
        let asset_root = asset_root
            .into()
            .canonicalize()
            .map_err(|_| ConfigError("invalid asset root"))?;
        if !asset_root.is_dir() {
            return Err(ConfigError("invalid asset root"));
        }
        Ok(Self {
            bind,
            secure_cookie: scheme == "https",
            public_http,
            short_key: key_len < ACCESS_KEY_MIN,
            origin,
            access_digest: digest(access_key.as_ref()),
            asset_root,
            max_rooms: 16,
            proxy_ips: HashSet::new(),
            egress_limit: 512 * 1024 * 1024,
        })
    }

    /// Set a room limit in 1..=256 (default 16).
    pub fn with_max_rooms(mut self, max_rooms: usize) -> Result<Self, ConfigError> {
        if !(1..=MAX_ROOM_CONFIG).contains(&max_rooms) {
            return Err(ConfigError("invalid room limit"));
        }
        self.max_rooms = max_rooms;
        Ok(self)
    }

    /// Trust only these exact peer IPs, never arbitrary forwarded headers or CIDR ranges.
    pub fn with_proxy_ips(
        mut self,
        ips: impl IntoIterator<Item = IpAddr>,
    ) -> Result<Self, ConfigError> {
        self.proxy_ips.clear();
        for ip in ips {
            self.proxy_ips.insert(ip);
            if self.proxy_ips.len() > 64 {
                return Err(ConfigError("too many trusted proxies"));
            }
        }
        Ok(self)
    }

    fn cookie_name(&self) -> &'static str {
        if self.secure_cookie {
            "__Host-naifrog_session"
        } else {
            "naifrog_session"
        }
    }
    pub fn bind(&self) -> SocketAddr {
        self.bind
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }
    pub fn asset_root(&self) -> &Path {
        &self.asset_root
    }
    pub fn max_rooms(&self) -> usize {
        self.max_rooms
    }

    /// Environment: NAIFROG_BIND (127.0.0.1:18080), NAIFROG_ORIGIN
    /// (http://127.0.0.1:18080), NAIFROG_ACCESS_KEY (required, >=32 bytes),
    /// NAIFROG_ASSET_ROOT (current directory), NAIFROG_MAX_ROOMS (16),
    /// NAIFROG_PROXY_IPS (comma-separated exact IPs), and
    /// NAIFROG_ALLOW_INSECURE_PUBLIC_HTTP (1 allows unencrypted HTTP on a public address,
    /// e.g. `http://IP:port` without a domain; 0 or unset refuses it), and
    /// NAIFROG_ALLOW_SHORT_ACCESS_KEY (1 accepts a key of 6..32 bytes; 0 or unset refuses it).
    pub fn from_env() -> Result<Self, ConfigError> {
        let env = |name| {
            std::env::var(name).map_err(|_| ConfigError("missing or invalid server environment"))
        };
        let bind = match std::env::var("NAIFROG_BIND") {
            Ok(value) => value
                .parse()
                .map_err(|_| ConfigError("invalid bind address"))?,
            Err(std::env::VarError::NotPresent) => "127.0.0.1:18080".parse().unwrap(),
            Err(_) => return Err(ConfigError("invalid bind address")),
        };
        let origin = match std::env::var("NAIFROG_ORIGIN") {
            Ok(value) => value,
            Err(std::env::VarError::NotPresent) => "http://127.0.0.1:18080".into(),
            Err(_) => return Err(ConfigError("invalid origin")),
        };
        let root = match std::env::var_os("NAIFROG_ASSET_ROOT") {
            Some(value) => PathBuf::from(value),
            None => std::env::current_dir().map_err(|_| ConfigError("invalid asset root"))?,
        };
        let flag = |name, error| match std::env::var(name) {
            Ok(value) if value == "1" => Ok(true),
            Ok(value) if value == "0" => Ok(false),
            Err(std::env::VarError::NotPresent) => Ok(false),
            _ => Err(ConfigError(error)),
        };
        let relaxed = Relaxed {
            public_http: flag(
                "NAIFROG_ALLOW_INSECURE_PUBLIC_HTTP",
                "NAIFROG_ALLOW_INSECURE_PUBLIC_HTTP must be 0 or 1",
            )?,
            short_key: flag(
                "NAIFROG_ALLOW_SHORT_ACCESS_KEY",
                "NAIFROG_ALLOW_SHORT_ACCESS_KEY must be 0 or 1",
            )?,
        };
        let mut config =
            Self::new_relaxed(bind, origin, env("NAIFROG_ACCESS_KEY")?, root, relaxed)?;
        match std::env::var("NAIFROG_MAX_ROOMS") {
            Ok(value) => {
                config = config.with_max_rooms(
                    value
                        .parse()
                        .map_err(|_| ConfigError("invalid room limit"))?,
                )?
            }
            Err(std::env::VarError::NotPresent) => {}
            Err(_) => return Err(ConfigError("invalid room limit")),
        }
        match std::env::var("NAIFROG_PROXY_IPS") {
            Ok(value) if !value.trim().is_empty() => {
                let ips = value
                    .split(',')
                    .map(|v| v.trim().parse::<IpAddr>())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| ConfigError("invalid trusted proxy list"))?;
                config = config.with_proxy_ips(ips)?;
            }
            Ok(_) | Err(std::env::VarError::NotPresent) => {}
            Err(_) => return Err(ConfigError("invalid trusted proxy list")),
        }
        if let Ok(value) = std::env::var("NAIFROG_EGRESS_MB_PER_HOUR") {
            let mb: u64 = value
                .parse()
                .map_err(|_| ConfigError("invalid transfer budget"))?;
            if !(64..=8192).contains(&mb) {
                return Err(ConfigError("transfer budget must be 64..8192 MiB/hour"));
            }
            config.egress_limit = mb * 1024 * 1024;
        }
        Ok(config)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn random_token() -> Result<String, ()> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| ())?;
    Ok(hex(&bytes))
}
fn random_code() -> Result<String, ()> {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).map_err(|_| ())?;
    Ok(bytes
        .iter()
        .map(|b| CODE_ALPHABET[(b & 31) as usize] as char)
        .collect())
}
fn valid_token(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_code(value: &str) -> bool {
    value.len() == 8 && value.bytes().all(|b| CODE_ALPHABET.contains(&b))
}
fn exact_origin(headers: &HeaderMap, origin: &str) -> bool {
    let mut values = headers.get_all(header::ORIGIN).iter();
    values
        .next()
        .is_some_and(|v| v.as_bytes() == origin.as_bytes())
        && values.next().is_none()
}
fn session_cookie<'a>(headers: &'a HeaderMap, cookie_name: &str) -> Option<&'a str> {
    let mut found = None;
    for value in headers.get_all(header::COOKIE) {
        for pair in value.to_str().ok()?.split(';') {
            let (name, value) = pair.trim().split_once('=')?;
            if name == cookie_name {
                if found.is_some() || !valid_token(value) {
                    return None;
                }
                found = Some(value);
            }
        }
    }
    found
}

#[derive(Clone, Copy)]
struct ClientIp(IpAddr);
struct Session {
    ip: IpAddr,
    expires: Instant,
}
struct Bucket {
    tokens: f64,
    updated: Instant,
    seen: Instant,
}
impl Bucket {
    fn new(capacity: u32, now: Instant) -> Self {
        Self {
            tokens: capacity as f64,
            updated: now,
            seen: now,
        }
    }
    fn refill(&mut self, rate: f64, capacity: u32, now: Instant) {
        self.tokens = (self.tokens
            + now.saturating_duration_since(self.updated).as_secs_f64() * rate)
            .min(capacity as f64);
        self.updated = now;
        self.seen = now;
    }
    fn take(&mut self, rate: f64, capacity: u32, now: Instant) -> bool {
        self.refill(rate, capacity, now);
        if self.tokens < 1.0 {
            false
        } else {
            self.tokens -= 1.0;
            true
        }
    }
}
// Strict rolling-minute limits. Timestamp storage is bounded by each configured
// per-IP/global cap; no user payloads or outbound frames are queued here.
struct Window {
    events: std::collections::VecDeque<Instant>,
    seen: Instant,
}
impl Window {
    fn new(now: Instant) -> Self {
        Self {
            events: std::collections::VecDeque::new(),
            seen: now,
        }
    }
    fn prune(&mut self, now: Instant) {
        while self
            .events
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= Duration::from_secs(60))
        {
            self.events.pop_front();
        }
        self.seen = now;
    }
}
struct Limiter {
    per_ip: u32,
    global_cap: u32,
    ips: HashMap<IpAddr, Window>,
    global: Window,
}
impl Limiter {
    fn new(per_ip: u32, global_cap: u32, now: Instant) -> Self {
        Self {
            per_ip,
            global_cap,
            ips: HashMap::new(),
            global: Window::new(now),
        }
    }
    fn allow(&mut self, ip: IpAddr, now: Instant) -> bool {
        if !self.ips.contains_key(&ip) && self.ips.len() >= MAX_LIMITER_IPS {
            self.ips.retain(|_, window| {
                now.saturating_duration_since(window.seen) < Duration::from_secs(60)
            });
            if self.ips.len() >= MAX_LIMITER_IPS {
                return false;
            }
        }
        self.global.prune(now);
        let local = self.ips.entry(ip).or_insert_with(|| Window::new(now));
        local.prune(now);
        if self.global.events.len() >= self.global_cap as usize
            || local.events.len() >= self.per_ip as usize
        {
            return false;
        }
        self.global.events.push_back(now);
        local.events.push_back(now);
        true
    }
}
struct Quotas {
    auth: Limiter,
    assets: Limiter,
    wasm: Limiter,
    upgrades: Limiter,
}
/// Digest of an authenticated session cookie. Rooms, seats, resume capabilities
/// and WebSocket quotas are bound to it; the raw cookie is never stored.
type SessionId = [u8; 32];
#[derive(Default)]
struct WsCounts {
    total: usize,
    ips: HashMap<IpAddr, usize>,
    sessions: HashMap<SessionId, usize>,
}
// Fixed 61 one-minute bins: conservatively includes the entire oldest minute.
// Allocation never grows with requests, IPs, rooms or snapshot frequency.
struct Egress {
    start: Instant,
    marks: [u64; 61],
    bytes: [u64; 61],
    limit: u64,
}
impl Egress {
    fn new(limit: u64, now: Instant) -> Self {
        Self {
            start: now,
            marks: [u64::MAX; 61],
            bytes: [0; 61],
            limit,
        }
    }
    fn minute(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.start).as_secs() / 60
    }
    fn used(&self, minute: u64) -> u64 {
        self.marks
            .iter()
            .zip(self.bytes)
            .filter(|(m, _)| **m <= minute && minute - **m < 61)
            .map(|(_, b)| b)
            .sum()
    }
    fn fits(&self, count: u64, now: Instant) -> bool {
        count <= self.limit.saturating_sub(self.used(self.minute(now)))
    }
    fn commit(&mut self, count: u64, now: Instant) {
        let minute = self.minute(now);
        let index = (minute % 61) as usize;
        if self.marks[index] != minute {
            self.marks[index] = minute;
            self.bytes[index] = 0;
        }
        self.bytes[index] = self.bytes[index].saturating_add(count);
    }
}
/// One budget for every HTTP file body and WebSocket frame the application sends.
/// Unauthenticated responses are additionally capped so anonymous page loads
/// cannot starve logged-in players of the shared budget.
struct EgressBudget {
    total: Egress,
    public: Egress,
}
impl EgressBudget {
    fn reserve(&mut self, count: u64, public: bool, now: Instant) -> bool {
        if !self.total.fits(count, now) || (public && !self.public.fits(count, now)) {
            return false;
        }
        self.total.commit(count, now);
        if public {
            self.public.commit(count, now);
        }
        true
    }
}
#[derive(Debug, PartialEq, Eq)]
enum JoinError {
    /// Unknown, full, expired or closed room, or a resume capability that does
    /// not match this session. Deliberately not more specific.
    Unavailable,
    /// The global room limit is reached.
    ServerFull,
    /// This session still holds a connected seat in another room.
    Busy,
    /// Create/Join with a different PROTOCOL_VERSION (stale cached page).
    Outdated,
    /// Malformed first message, randomness failure or arithmetic exhaustion.
    Internal,
}
impl JoinError {
    fn message(&self) -> &'static str {
        match self {
            Self::Unavailable => "room unavailable",
            Self::ServerFull => "server full",
            Self::Busy => "already in another room",
            Self::Outdated => "client outdated, reload the page",
            Self::Internal => "request denied",
        }
    }
}
struct AppState {
    config: Config,
    rooms: Mutex<HashMap<String, Arc<Room>>>,
    sessions: Mutex<HashMap<SessionId, Session>>,
    quotas: Mutex<Quotas>,
    ws_counts: Mutex<WsCounts>,
    admissions: Mutex<()>,
    wasm_transfers: Arc<Semaphore>,
    file_transfers: Arc<Semaphore>,
    egress: Mutex<EgressBudget>,
    fingerprints: Fingerprints,
    gzip_copies: GzipCopies,
}
// Lock order: admissions -> rooms -> room.data. sessions, quotas, ws_counts and
// egress are leaf locks. No std::sync guard is held across an await: such a
// future would not be Send, and spawning it would fail to compile.
impl AppState {
    fn new(config: Config) -> Arc<Self> {
        let now = Instant::now();
        Arc::new(Self {
            config: config.clone(),
            rooms: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            quotas: Mutex::new(Quotas {
                auth: Limiter::new(10, 120, now),
                assets: Limiter::new(120, 2048, now),
                wasm: Limiter::new(WASM_PER_IP_PER_MINUTE, 12, now),
                upgrades: Limiter::new(30, 240, now),
            }),
            ws_counts: Mutex::new(WsCounts::default()),
            admissions: Mutex::new(()),
            wasm_transfers: Arc::new(Semaphore::new(WASM_PRODUCERS)),
            file_transfers: Arc::new(Semaphore::new(FILE_PRODUCERS)),
            egress: Mutex::new(EgressBudget {
                total: Egress::new(config.egress_limit, now),
                public: Egress::new(config.egress_limit / PUBLIC_EGRESS_DIVISOR, now),
            }),
            fingerprints: Fingerprints::default(),
            gzip_copies: GzipCopies::default(),
        })
    }
    fn reserve_egress(&self, count: u64, public: bool) -> bool {
        lock(&self.egress).reserve(count, public, Instant::now())
    }
    /// Record bytes that are sent regardless of the budget (final close frames).
    fn charge_egress(&self, count: u64) {
        lock(&self.egress).total.commit(count, Instant::now());
    }
    fn client_ip(&self, peer: SocketAddr, headers: &HeaderMap) -> Option<IpAddr> {
        if !self.config.proxy_ips.contains(&peer.ip()) {
            return Some(peer.ip());
        }
        let mut values = headers.get_all("x-real-ip").iter();
        let ip = values.next()?.to_str().ok()?.parse().ok()?;
        if values.next().is_some() {
            return None;
        }
        Some(ip)
    }
    /// The authenticated session for this request, bound to the login IP.
    fn session(&self, headers: &HeaderMap, ip: IpAddr, now: Instant) -> Option<SessionId> {
        let token = session_cookie(headers, self.config.cookie_name())?;
        let id = digest(token.as_bytes());
        let mut sessions = lock(&self.sessions);
        sessions.retain(|_, session| session.expires > now);
        sessions.get(&id).is_some_and(|s| s.ip == ip).then_some(id)
    }
    fn authenticated(&self, headers: &HeaderMap, ip: IpAddr, now: Instant) -> bool {
        self.session(headers, ip, now).is_some()
    }
    fn reserve_ws(self: &Arc<Self>, ip: IpAddr, owner: SessionId) -> Option<WsPermit> {
        let mut counts = lock(&self.ws_counts);
        if counts.total >= MAX_WS
            || counts.ips.get(&ip).copied().unwrap_or(0) >= MAX_WS_PER_IP
            || counts.sessions.get(&owner).copied().unwrap_or(0) >= MAX_WS_PER_SESSION
        {
            return None;
        }
        counts.total += 1;
        *counts.ips.entry(ip).or_default() += 1;
        *counts.sessions.entry(owner).or_default() += 1;
        Some(WsPermit {
            app: Arc::downgrade(self),
            ip,
            owner,
        })
    }
    fn prune_rooms(&self, now: Instant) {
        lock(&self.rooms).retain(|_, room| {
            let expired = { lock(&room.data).expired(now) };
            if expired {
                room.close();
                false
            } else {
                true
            }
        });
    }
    fn remove_room(&self, room: &Arc<Room>) {
        room.close();
        let mut rooms = lock(&self.rooms);
        if rooms
            .get(&room.code)
            .is_some_and(|stored| Arc::ptr_eq(stored, room))
        {
            rooms.remove(&room.code);
        }
    }
    /// Per-session room quota: one room at a time. A connected seat elsewhere
    /// (for example another tab) refuses the request; seats this session left
    /// disconnected in other rooms are abandoned, exactly like an explicit Leave.
    /// Call with the admissions lock held.
    fn release_other_rooms(&self, owner: SessionId, keep: Option<&str>) -> Result<(), JoinError> {
        let mut rooms = lock(&self.rooms);
        let mut abandoned = Vec::new();
        for (code, room) in rooms.iter() {
            if keep == Some(code.as_str()) {
                continue;
            }
            let data = lock(&room.data);
            let mut held = false;
            for seat in data.slots.iter().flatten().filter(|s| s.owner == owner) {
                if seat.connected {
                    return Err(JoinError::Busy);
                }
                held = true;
            }
            if held {
                abandoned.push(code.clone());
            }
        }
        for code in abandoned {
            if let Some(room) = rooms.remove(&code) {
                room.close();
            }
        }
        Ok(())
    }
    fn create(
        self: &Arc<Self>,
        owner: SessionId,
        name: Option<&str>,
        now: Instant,
    ) -> Result<Membership, JoinError> {
        let _admission = lock(&self.admissions);
        self.prune_rooms(now);
        self.release_other_rooms(owner, None)?;
        let token = random_token().map_err(|_| JoinError::Internal)?;
        let mut rooms = lock(&self.rooms);
        if rooms.len() >= self.config.max_rooms {
            return Err(JoinError::ServerFull);
        }
        let code = (0..16)
            .find_map(|_| random_code().ok().filter(|code| !rooms.contains_key(code)))
            .ok_or(JoinError::Internal)?;
        let room = Room::new(
            code.clone(),
            digest(token.as_bytes()),
            owner,
            clean_name(name, 0),
            now,
        );
        rooms.insert(code, room.clone());
        drop(rooms);
        spawn_room(Arc::downgrade(self), Arc::downgrade(&room));
        Ok(Membership {
            updates: room.updates.subscribe(),
            room,
            player: 0,
            epoch: 1,
            resume: token,
        })
    }
    /// Join as P2, or resume a seat. A resume capability only works for the same
    /// authenticated session, the same room code and the seat it was issued for.
    fn join(
        self: &Arc<Self>,
        code: &str,
        resume: Option<&str>,
        owner: SessionId,
        name: Option<&str>,
        now: Instant,
    ) -> Result<Membership, JoinError> {
        if !valid_code(code) || resume.is_some_and(|t| !valid_token(t)) {
            return Err(JoinError::Unavailable);
        }
        let _admission = lock(&self.admissions);
        self.prune_rooms(now);
        self.release_other_rooms(owner, Some(code))?;
        let room = lock(&self.rooms)
            .get(code)
            .cloned()
            .ok_or(JoinError::Unavailable)?;
        // Reuse the capability on resume: a lost Welcome must not strand a retry.
        let new_token = match resume {
            Some(token) => token.to_owned(),
            None => random_token().map_err(|_| JoinError::Internal)?,
        };
        let (player, epoch) = {
            let mut data = lock(&room.data);
            if data.expired(now) {
                return Err(JoinError::Unavailable);
            }
            let player = if let Some(token) = resume {
                let supplied = digest(token.as_bytes());
                data.slots
                    .iter()
                    .position(|s| {
                        s.as_ref().is_some_and(|s| {
                            bool::from(s.owner.ct_eq(&owner) & s.resume_digest.ct_eq(&supplied))
                        })
                    })
                    .ok_or(JoinError::Unavailable)?
            } else if data.slots[1].is_none() {
                1
            } else {
                return Err(JoinError::Unavailable);
            };
            // A fresh valid name replaces the seat's; anything else keeps what the
            // seat had (its stored name on resume, the seat default otherwise).
            let name = name.and_then(valid_name).unwrap_or_else(|| {
                data.slots[player]
                    .as_ref()
                    .map_or_else(|| seat_name(player), |s| s.name.clone())
            });
            let epoch = data
                .attach(player, digest(new_token.as_bytes()), owner, now, name)
                .map_err(|_| JoinError::Unavailable)?;
            (player, epoch)
        };
        Ok(Membership {
            updates: room.updates.subscribe(),
            room,
            player,
            epoch,
            resume: new_token,
        })
    }
}
struct WsPermit {
    app: Weak<AppState>,
    ip: IpAddr,
    owner: SessionId,
}
impl Drop for WsPermit {
    fn drop(&mut self) {
        if let Some(app) = self.app.upgrade() {
            let mut counts = lock(&app.ws_counts);
            counts.total = counts.total.saturating_sub(1);
            if let Some(count) = counts.sessions.get_mut(&self.owner) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    counts.sessions.remove(&self.owner);
                }
            }
            if let Some(count) = counts.ips.get_mut(&self.ip) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    counts.ips.remove(&self.ip);
                }
            }
        }
    }
}

#[derive(Clone)]
enum RoomUpdate {
    // Boxed: the snapshot dwarfs the variant-less arm, and every receiver slots one.
    Frame(Box<Snapshot>),
    Closed,
}
struct Slot {
    resume_digest: [u8; 32],
    /// Authenticated session that holds this seat; resume requires the same one.
    owner: SessionId,
    /// Validated display name, `P1`/`P2` until the seat says otherwise. Names are
    /// never identity (resume does not check them) and never logged.
    name: String,
    /// False until this connection submits a valid current-round input. The
    /// simulation stays paused while either seat is loading or resuming.
    initialized: bool,
    epoch: u64,
    connected: bool,
    disconnected_at: Option<Instant>,
    input: InputMux,
    last_seq: Option<u32>,
    last_input: Option<Instant>,
    vote: Option<Instant>,
}
/// The seat's name when nobody supplied a valid one.
fn seat_name(seat: usize) -> String {
    if seat == 0 { "P1" } else { "P2" }.into()
}
/// Validate a supplied name, falling back to `seat_name`. Invalid names are
/// cleaned, not rejected: a name is decoration, never a credential.
fn clean_name(name: Option<&str>, seat: usize) -> String {
    name.and_then(valid_name).unwrap_or_else(|| seat_name(seat))
}
impl Slot {
    fn new(resume_digest: [u8; 32], owner: SessionId, epoch: u64, name: String) -> Self {
        Self {
            resume_digest,
            owner,
            name,
            initialized: false,
            epoch,
            connected: true,
            disconnected_at: None,
            input: InputMux::default(),
            last_seq: None,
            last_input: None,
            vote: None,
        }
    }
}
struct RoomData {
    arena: Arena,
    slots: [Option<Slot>; 2],
    created: Instant,
    closed: bool,
    next_epoch: u64,
    round: u32,
    snapshot_seq: u64,
    hits: Vec<Hit>,
    /// A seat dropped since the fight last ran. When both seats are current again the
    /// round counts back in (3-2-1) before continuing, unless it ended meanwhile.
    interrupted: bool,
}
impl RoomData {
    fn expired(&self, now: Instant) -> bool {
        self.closed
            || now.saturating_duration_since(self.created) >= ROOM_LIFETIME
            || self.slots.iter().flatten().any(|s| {
                s.disconnected_at
                    .is_some_and(|t| now.saturating_duration_since(t) >= RESUME_GRACE)
            })
    }
    fn connected(&self) -> [bool; 2] {
        std::array::from_fn(|i| self.slots[i].as_ref().is_some_and(|s| s.connected))
    }
    fn is_current(&self, player: usize, epoch: u64) -> bool {
        !self.closed
            && self
                .slots
                .get(player)
                .and_then(Option::as_ref)
                .is_some_and(|s| s.connected && s.epoch == epoch)
    }
    fn clear_votes(&mut self) {
        for slot in self.slots.iter_mut().flatten() {
            slot.vote = None;
        }
    }
    fn attach(
        &mut self,
        player: usize,
        resume_digest: [u8; 32],
        owner: SessionId,
        now: Instant,
        name: String,
    ) -> Result<u64, ()> {
        if self.expired(now) {
            return Err(());
        }
        self.next_epoch = self.next_epoch.checked_add(1).ok_or(())?;
        self.slots[player] = Some(Slot::new(resume_digest, owner, self.next_epoch, name));
        // Loading/resume remains paused until both seats deliver current-round input.
        self.clear_votes();
        Ok(self.next_epoch)
    }
    fn disconnect(&mut self, player: usize, epoch: u64, now: Instant) {
        if !self.is_current(player, epoch) {
            return;
        }
        let slot = self.slots[player].as_mut().unwrap();
        slot.connected = false;
        slot.disconnected_at = Some(now);
        slot.input.cancel_all();
        slot.last_input = None;
        self.interrupted = true;
        self.clear_votes();
    }
    fn expire_votes(&mut self, now: Instant) {
        for slot in self.slots.iter_mut().flatten() {
            if slot
                .vote
                .is_some_and(|t| now.saturating_duration_since(t) > VOTE_WINDOW)
            {
                slot.vote = None;
            }
        }
    }
    fn message(
        &mut self,
        player: usize,
        epoch: u64,
        message: ClientMessage,
        now: Instant,
    ) -> Result<bool, ()> {
        if !self.is_current(player, epoch) || self.expired(now) {
            return Err(());
        }
        match message {
            ClientMessage::Input {
                seq,
                round,
                held,
                pressed,
            } => {
                let slot = self.slots[player].as_mut().unwrap();
                if !valid_input(held, pressed)
                    || round > self.round
                    || slot.last_seq.is_some_and(|old| seq <= old)
                {
                    return Err(());
                }
                slot.last_seq = Some(seq);
                // In-flight pre-restart controls cannot introduce edges in the new round.
                if round < self.round {
                    return Ok(false);
                }
                slot.last_input = Some(now);
                slot.initialized = true;
                slot.input.sample_with_press(Source::Remote, held, pressed);
            }
            ClientMessage::Restart { round } => {
                if round != self.round || self.connected() != [true; 2] {
                    return Ok(false);
                }
                self.expire_votes(now);
                self.slots[player].as_mut().unwrap().vote = Some(now);
                if self.connected() == [true; 2]
                    && self.slots.iter().flatten().all(|s| s.vote.is_some())
                {
                    self.arena.restart();
                    self.round = self.round.saturating_add(1);
                    self.hits.clear();
                    for slot in self.slots.iter_mut().flatten() {
                        slot.input.restart();
                        slot.vote = None;
                    }
                }
            }
            ClientMessage::Leave => return Ok(true),
            ClientMessage::Create { .. } | ClientMessage::Join { .. } => return Err(()),
        }
        Ok(false)
    }
    fn step(&mut self, now: Instant) {
        self.expire_votes(now);
        for slot in self.slots.iter_mut().flatten() {
            if slot
                .last_input
                .is_some_and(|t| now.saturating_duration_since(t) > INPUT_TIMEOUT)
            {
                slot.input.cancel_all();
                slot.last_input = None;
            }
        }
        if !self.closed
            && self.connected() == [true; 2]
            && self.slots.iter().flatten().all(|s| s.initialized)
        {
            if self.interrupted {
                self.interrupted = false;
                // `resume_now` itself decides which phases count back in: versus, a
                // round introduction and a shown result just continue where they froze.
                self.arena.resume_now();
            }
            let input = std::array::from_fn(|i| self.slots[i].as_mut().unwrap().input.take());
            let hits = self.arena.step(input);
            self.hits.extend(
                hits.into_iter()
                    .take(MAX_HITS.saturating_sub(self.hits.len())),
            );
        }
    }
    fn snapshot(&mut self, now: Instant) -> Snapshot {
        self.snapshot_seq = self.snapshot_seq.saturating_add(1);
        Snapshot {
            seq: self.snapshot_seq,
            round: self.round,
            arena: self.arena.clone(),
            held: std::array::from_fn(|i| self.slots[i].as_ref().map_or(0, |s| s.input.held())),
            connected: self.connected(),
            votes: std::array::from_fn(|i| {
                self.slots[i].as_ref().is_some_and(|s| s.vote.is_some())
            }),
            names: std::array::from_fn(|i| {
                self.slots[i]
                    .as_ref()
                    .map_or_else(|| seat_name(i), |s| s.name.clone())
            }),
            // How much longer the server keeps a dropped seat, per seat, in whole seconds.
            // `None` while connected; clamped at zero past the grace (the room dies anyway).
            grace_left: std::array::from_fn(|i| {
                self.slots[i].as_ref().and_then(|s| {
                    s.disconnected_at.map(|t| {
                        RESUME_GRACE
                            .as_secs()
                            .saturating_sub(now.saturating_duration_since(t).as_secs())
                            as u16
                    })
                })
            }),
            hits: std::mem::take(&mut self.hits),
        }
    }
}
struct Room {
    code: String,
    data: Mutex<RoomData>,
    updates: watch::Sender<RoomUpdate>,
}
impl Room {
    fn new(
        code: String,
        resume_digest: [u8; 32],
        owner: SessionId,
        name: String,
        now: Instant,
    ) -> Arc<Self> {
        let mut data = RoomData {
            arena: Arena::default(),
            slots: [Some(Slot::new(resume_digest, owner, 1, name)), None],
            created: now,
            closed: false,
            next_epoch: 1,
            round: 0,
            snapshot_seq: 0,
            hits: Vec::new(),
            interrupted: false,
        };
        let (updates, _) = watch::channel(RoomUpdate::Frame(Box::new(data.snapshot(now))));
        Arc::new(Self {
            code,
            data: Mutex::new(data),
            updates,
        })
    }
    fn close(&self) {
        let mut data = lock(&self.data);
        if data.closed {
            return;
        }
        data.closed = true;
        for slot in data.slots.iter_mut().flatten() {
            slot.input.cancel_all();
            slot.connected = false;
            slot.vote = None;
        }
        self.updates.send_replace(RoomUpdate::Closed);
    }
}
struct Membership {
    room: Arc<Room>,
    player: usize,
    epoch: u64,
    resume: String,
    updates: watch::Receiver<RoomUpdate>,
}
impl Drop for Membership {
    fn drop(&mut self) {
        lock(&self.room.data).disconnect(self.player, self.epoch, Instant::now());
    }
}
fn spawn_room(app: Weak<AppState>, room: Weak<Room>) {
    tokio::spawn(async move {
        let mut sim = tokio::time::interval(Duration::from_nanos(1_000_000_000 / 60));
        let mut broadcast = tokio::time::interval(Duration::from_millis(50));
        sim.set_missed_tick_behavior(MissedTickBehavior::Skip);
        broadcast.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            let send_frame =
                tokio::select! { _ = sim.tick() => false, _ = broadcast.tick() => true };
            let (Some(app), Some(room)) = (app.upgrade(), room.upgrade()) else {
                break;
            };
            let now = Instant::now();
            let expired = {
                let mut data = lock(&room.data);
                if data.expired(now) {
                    true
                } else {
                    if send_frame {
                        room.updates
                            .send_replace(RoomUpdate::Frame(Box::new(data.snapshot(now))));
                    } else {
                        data.step(now);
                    }
                    false
                }
            };
            if expired {
                app.remove_room(&room);
                break;
            }
            // Both strong references are dropped before awaiting the next tick.
        }
    });
}

/// Build the HTTP router. Serve it with into_make_service_with_connect_info::<SocketAddr>().
pub fn router(config: Config) -> Router {
    routes(AppState::new(config))
}
/// Hash the game files once at startup so the first page load does not wait for it.
async fn warm_fingerprints(state: Arc<AppState>) {
    let manifest = state.manifest().await;
    for name in manifest
        .files
        .keys()
        .filter(|n| n.starts_with("pkg/") && n.ends_with(".wasm"))
    {
        let path = state.config.asset_root.join("web").join(name);
        if let Ok(target) = tokio::fs::canonicalize(path).await
            && let Some((len, _)) = file_stamp(&target).await
        {
            let _ = state.gzip_copies.file(&target, len).await;
        }
    }
}
fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/config", get(public_config))
        .route("/api/manifest", get(game_manifest))
        .route("/api/auth", post(authenticate))
        .route("/api/ws", get(upgrade))
        .fallback(serve_file)
        .layer(middleware::from_fn_with_state(state.clone(), security))
        .with_state(state)
}

/// Bind the configured address and serve until Ctrl-C. Does not configure TLS itself.
pub async fn run(config: Config) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    // The only log line: no requests, cookies, tokens or the access key are ever printed.
    eprintln!(
        "naifrog-server listening on {} for origin {}; {} rooms, {} MiB/h egress, {} trusted proxy address(es){}{}",
        listener.local_addr()?,
        config.origin,
        config.max_rooms,
        config.egress_limit / (1024 * 1024),
        config.proxy_ips.len(),
        if config.public_http {
            "; UNENCRYPTED public HTTP: the access key and cookies cross the network in plain text"
        } else {
            ""
        },
        if config.short_key {
            "; SHORT access key: only the login rate limits stand in the way of guessing it"
        } else {
            ""
        }
    );
    let state = AppState::new(config);
    tokio::spawn(warm_fingerprints(state.clone()));
    axum::serve(
        listener,
        routes(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}
fn denied(status: StatusCode) -> Response {
    (status, "request denied").into_response()
}
fn secured(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert("content-security-policy", HeaderValue::from_static(CSP));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    // File responses choose their own caching; everything else is never stored.
    headers
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static(NO_STORE));
    response
}
async fn security(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .and_then(|peer| state.client_ip(peer.0, request.headers()));
    if let Some(ip) = ip {
        request.extensions_mut().insert(ClientIp(ip));
    }
    // Reject before the upgrade extractor so unauthenticated probes always see 401.
    if request.uri().path() == "/api/ws"
        && (!exact_origin(request.headers(), &state.config.origin)
            || !ip.is_some_and(|ip| state.authenticated(request.headers(), ip, Instant::now())))
    {
        return secured(denied(StatusCode::UNAUTHORIZED));
    }
    secured(next.run(request).await)
}
/// Map a request path to (directory, relative file, protected). The four page
/// files are public so the login form can load; everything else needs a session.
fn route_file<'a>(root: &Path, path: &'a str) -> Option<(PathBuf, &'a str, bool)> {
    let web = root.join("web");
    match path {
        "/" | "/index.html" => Some((web, "index.html", false)),
        "/main.js" => Some((web, "main.js", false)),
        "/network.js" => Some((web, "network.js", false)),
        "/style.css" => Some((web, "style.css", false)),
        _ => {
            if let Some(relative) = path.strip_prefix("/pkg/") {
                Some((web.join("pkg"), relative, true))
            } else {
                path.strip_prefix("/assets/processed/")
                    .map(|relative| (root.join("assets/processed"), relative, true))
            }
        }
    }
}
fn content_type(relative: &str) -> &'static str {
    match Path::new(relative).extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("webp") => "image/webp",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("json") => "application/json",
        Some("otf") => "font/otf",
        _ => "application/octet-stream",
    }
}
/// Split a `/v/<hash>/` prefix (lowercase hex, at most 64 digits) off a request path.
fn split_version(path: &str) -> (Option<&str>, &str) {
    path.strip_prefix("/v/")
        .and_then(|rest| {
            let (version, tail) = rest.split_at(rest.find('/')?);
            (!version.is_empty()
                && version.len() <= 64
                && version
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
            .then_some((Some(version), tail))
        })
        .unwrap_or((None, path))
}
/// Whether If-None-Match names this ETag (weak or strong) or is `*`.
fn not_modified(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .any(|tag| tag == "*" || tag.strip_prefix("W/").unwrap_or(tag) == etag)
}
/// Every file request (GET or HEAD, with or without Range/If-* headers) is rate
/// counted and, for protected paths, authenticated. Range is ignored: each GET is a
/// complete 200 whose full length is charged to the egress budget before streaming
/// starts. If-None-Match naming the current ETag gets a bodiless 304 that costs no
/// egress. A WASM file goes out gzip-encoded when the client accepts gzip and a
/// matching `<file>.gz` lies beside it. Caching follows the consts at the top.
async fn serve_file(State(state): State<Arc<AppState>>, request: Request) -> Response {
    let (version, path) = split_version(request.uri().path());
    let Some((base, relative, protected)) = route_file(&state.config.asset_root, path) else {
        return denied(StatusCode::NOT_FOUND);
    };
    // Only game files are versioned; the page files keep their one address.
    if version.is_some() && !protected {
        return denied(StatusCode::NOT_FOUND);
    }
    let head = request.method() == Method::HEAD;
    if !head && request.method() != Method::GET {
        let mut response = denied(StatusCode::METHOD_NOT_ALLOWED);
        response
            .headers_mut()
            .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
        return response;
    }
    let Some(&ClientIp(ip)) = request.extensions().get::<ClientIp>() else {
        return denied(StatusCode::UNAUTHORIZED);
    };
    let now = Instant::now();
    let authenticated = state.authenticated(request.headers(), ip, now);
    if protected && !authenticated {
        return denied(StatusCode::UNAUTHORIZED);
    }
    let wasm = relative.ends_with(".wasm");
    let allowed = {
        let mut quotas = lock(&state.quotas);
        quotas.assets.allow(ip, now) && (!wasm || quotas.wasm.allow(ip, now))
    };
    if !allowed {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    }
    let Some((target, len)) =
        safe_asset_path(&state.config.asset_root, &base, relative, protected, wasm).await
    else {
        return denied(StatusCode::NOT_FOUND);
    };
    let gzip = if wasm && accepts_gzip(request.headers()) {
        state.gzip_copies.file(&target, len).await
    } else {
        None
    };
    let fingerprint = state.fingerprints.file(&target).await;
    let cache = if relative == "index.html" {
        // A stored page may be kept in the back/forward cache when the player leaves, its
        // game socket still open: the server would not see the drop, nor the rival the pause.
        NO_STORE
    } else if !protected {
        REVALIDATE_PAGE
    } else if version.is_some() && version == fingerprint.as_deref() {
        IMMUTABLE_GAME
    } else {
        REVALIDATE_GAME
    };
    // The gzip copy is a different representation of the same file: its own ETag.
    let etag =
        fingerprint.map(|hash| format!("\"{hash}{}\"", if gzip.is_some() { "-gz" } else { "" }));
    let caching = |headers: &mut HeaderMap| {
        if let Some(etag) = etag.as_deref().and_then(|e| HeaderValue::from_str(e).ok()) {
            headers.insert(header::ETAG, etag);
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
        }
        if wasm {
            headers.insert(header::VARY, HeaderValue::from_static("accept-encoding"));
        }
    };
    if etag
        .as_deref()
        .is_some_and(|etag| not_modified(request.headers(), etag))
    {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        caching(response.headers_mut());
        return response;
    }
    let (target, sent) = gzip.clone().unwrap_or((target, len));
    let body = if head {
        Body::empty()
    } else {
        let transfers = if wasm {
            &state.wasm_transfers
        } else {
            &state.file_transfers
        };
        let Ok(Ok(permit)) = timeout(PERMIT_WAIT, transfers.clone().acquire_owned()).await else {
            return denied(StatusCode::SERVICE_UNAVAILABLE);
        };
        let Ok(file) = tokio::fs::File::open(&target).await else {
            return denied(StatusCode::NOT_FOUND);
        };
        if !state.reserve_egress(sent, !authenticated) {
            return denied(StatusCode::SERVICE_UNAVAILABLE);
        }
        let deadline = if wasm { WASM_DEADLINE } else { FILE_DEADLINE };
        stream_file(file, sent, deadline, permit)
    };
    let mut response = Response::new(body);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(relative)),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(sent));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("none"));
    caching(headers);
    if gzip.is_some() {
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        headers.insert(UNCOMPRESSED_LENGTH, HeaderValue::from(len));
    }
    response
}
/// Whether Accept-Encoding lists gzip with a non-zero quality.
fn accepts_gzip(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|item| {
            let mut parts = item.split(';');
            parts
                .next()
                .is_some_and(|coding| coding.trim().eq_ignore_ascii_case("gzip"))
                && parts.all(|param| {
                    let param = param.trim();
                    !param.get(..2).is_some_and(|q| q.eq_ignore_ascii_case("q="))
                        || param[2..].trim().parse::<f32>().is_ok_and(|q| q > 0.0)
                })
        })
}
type FileStamp = (u64, SystemTime);
async fn file_stamp(path: &Path) -> Option<FileStamp> {
    let metadata = tokio::fs::metadata(path).await.ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some((metadata.len(), metadata.modified().ok()?))
}
struct GzipCheck {
    source: FileStamp,
    compressed: FileStamp,
    matches: bool,
}
/// Verify the full decoded content, including gzip CRCs, before serving a copy under
/// the plain file's fingerprint. Equal decoded lengths do not identify the program.
#[derive(Default)]
struct GzipCopies {
    entries: Mutex<HashMap<(PathBuf, PathBuf), GzipCheck>>,
    // Serialize cache misses so simultaneous first downloads cannot pile up decoders.
    validating: tokio::sync::Mutex<()>,
}
impl GzipCopies {
    async fn file(&self, target: &Path, len: u64) -> Option<(PathBuf, u64)> {
        let _validation = self.validating.lock().await;
        let mut name = target.as_os_str().to_owned();
        name.push(".gz");
        let path = tokio::fs::canonicalize(PathBuf::from(name)).await.ok()?;
        if path.parent() != target.parent() || len > MAX_WASM_BYTES {
            return None;
        }
        let source = file_stamp(target).await?;
        let compressed = file_stamp(&path).await?;
        if source.0 != len || !(18..=MAX_WASM_BYTES).contains(&compressed.0) {
            return None;
        }
        let key = (target.to_owned(), path.clone());
        if let Some(check) = lock(&self.entries).get(&key)
            && check.source == source
            && check.compressed == compressed
        {
            return check.matches.then_some((path, compressed.0));
        }
        let (plain_path, gzip_path) = key.clone();
        let matches = tokio::task::spawn_blocking(move || -> std::io::Result<bool> {
            let mut plain = std::fs::File::open(plain_path)?;
            let mut plain_hash = Sha256::new();
            if std::io::copy(
                &mut std::io::Read::take(&mut plain, len + 1),
                &mut plain_hash,
            )? != len
            {
                return Ok(false);
            }
            let decoder = flate2::read::MultiGzDecoder::new(std::fs::File::open(gzip_path)?);
            let mut decoded = std::io::Read::take(decoder, len + 1);
            let mut gzip_hash = Sha256::new();
            let decoded_len = std::io::copy(&mut decoded, &mut gzip_hash)?;
            Ok(decoded_len == len && plain_hash.finalize() == gzip_hash.finalize())
        })
        .await
        .ok()?
        .unwrap_or(false);
        // A file updated while decoding must be checked again on the next request.
        if file_stamp(target).await? != source || file_stamp(&path).await? != compressed {
            return None;
        }
        let mut entries = lock(&self.entries);
        if entries.len() >= MAX_FINGERPRINTS {
            entries.clear();
        }
        entries.insert(
            key,
            GzipCheck {
                source,
                compressed,
                matches,
            },
        );
        matches.then_some((path, compressed.0))
    }
}
/// Stream at most `len` bytes through a one-chunk channel. The producer task owns
/// the transfer permit and stops at the deadline even if the client stops reading,
/// so slow readers cannot pin permits; the short body then aborts the connection.
fn stream_file(
    mut file: tokio::fs::File,
    len: u64,
    deadline: Duration,
    permit: OwnedSemaphorePermit,
) -> Body {
    let (tx, mut rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(1);
    tokio::spawn(async move {
        let _permit = permit;
        let _ = timeout(deadline, async move {
            let mut left = len;
            while left > 0 {
                let mut chunk = vec![0; CHUNK.min(usize::try_from(left).unwrap_or(CHUNK))];
                let read = match file.read(&mut chunk).await {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(error) => {
                        let _ = tx.send(Err(error)).await;
                        break;
                    }
                };
                chunk.truncate(read);
                left -= read as u64;
                if tx.send(Ok(Bytes::from(chunk))).await.is_err() {
                    break;
                }
            }
        })
        .await;
    });
    Body::from_stream(futures_util::stream::poll_fn(move |cx| rx.poll_recv(cx)))
}
async fn safe_asset_path(
    root: &Path,
    base: &Path,
    relative: &str,
    protected: bool,
    wasm: bool,
) -> Option<(PathBuf, u64)> {
    if relative.is_empty() || relative.len() > 512 || !relative.split('/').all(safe_segment) {
        return None;
    }
    if protected && !game_file(relative) {
        return None;
    }
    let base = tokio::fs::canonicalize(base).await.ok()?;
    let target = tokio::fs::canonicalize(base.join(relative)).await.ok()?;
    if !base.starts_with(root) || !target.starts_with(&base) {
        return None;
    }
    let metadata = tokio::fs::metadata(&target).await.ok()?;
    let limit = if wasm { MAX_WASM_BYTES } else { MAX_FILE_BYTES };
    (metadata.is_file() && metadata.len() <= limit).then_some((target, metadata.len()))
}
/// One path segment a file may be served under: short, plain ASCII, not hidden.
fn safe_segment(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 128
        && !p.starts_with('.')
        && p.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
fn game_file(name: &str) -> bool {
    !name.starts_with('.')
        && Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| GAME_EXTENSIONS.contains(&e))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
/// Content fingerprints of served files, kept per path with the length and modification
/// time they were taken at, so a rebuilt file is hashed once rather than on every request.
#[derive(Default)]
struct Fingerprints(Mutex<HashMap<PathBuf, (u64, SystemTime, String)>>);
impl Fingerprints {
    /// SHA-256 of the file's content, first 80 bits in hex.
    async fn file(&self, path: &Path) -> Option<String> {
        let metadata = tokio::fs::metadata(path).await.ok()?;
        let stamp = (metadata.len(), metadata.modified().ok()?);
        if let Some((len, modified, hash)) = lock(&self.0).get(path)
            && (*len, *modified) == stamp
        {
            return Some(hash.clone());
        }
        let owned = path.to_owned();
        let hash = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
            let mut hasher = Sha256::new();
            std::io::copy(&mut std::fs::File::open(owned)?, &mut hasher)?;
            Ok(hex(&hasher.finalize()[..10]))
        })
        .await
        .ok()?
        .ok()?;
        let mut cache = lock(&self.0);
        if cache.len() >= MAX_FINGERPRINTS {
            cache.clear();
        }
        cache.insert(path.to_owned(), (stamp.0, stamp.1, hash.clone()));
        Some(hash)
    }
    /// Add every servable file directly in `dir` to `out`, keyed `<prefix><name>`.
    async fn list(&self, dir: &Path, prefix: &str, out: &mut BTreeMap<String, String>) {
        let Ok(dir) = tokio::fs::canonicalize(dir).await else {
            return;
        };
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(name) = entry.file_name().into_string()
                && safe_segment(&name)
                && game_file(&name)
                && entry.file_type().await.is_ok_and(|t| t.is_file())
                && let Some(hash) = self.file(&dir.join(&name)).await
            {
                out.insert(format!("{prefix}{name}"), hash);
            }
        }
    }
}
/// The catalog the page loads game files by: every servable file under /pkg and
/// /assets/processed with its content fingerprint. A rebuild changes only the entries of
/// files whose content changed, so the browser downloads just those again.
#[derive(Serialize)]
struct Manifest {
    files: BTreeMap<String, String>,
}
impl AppState {
    async fn manifest(&self) -> Manifest {
        let root = &self.config.asset_root;
        let mut files = BTreeMap::new();
        let fingerprints = &self.fingerprints;
        fingerprints
            .list(&root.join("web/pkg"), "pkg/", &mut files)
            .await;
        fingerprints
            .list(
                &root.join("assets/processed"),
                "assets/processed/",
                &mut files,
            )
            .await;
        Manifest { files }
    }
}
#[derive(Serialize)]
struct PublicConfig {
    multiplayer: bool,
    authenticated: bool,
    protocol: u8,
}
async fn public_config(State(state): State<Arc<AppState>>, request: Request) -> Response {
    let Some(&ClientIp(ip)) = request.extensions().get::<ClientIp>() else {
        return denied(StatusCode::UNAUTHORIZED);
    };
    let now = Instant::now();
    if !lock(&state.quotas).assets.allow(ip, now) {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    }
    Json(PublicConfig {
        multiplayer: true,
        authenticated: state.authenticated(request.headers(), ip, now),
        protocol: PROTOCOL_VERSION,
    })
    .into_response()
}
/// The game-file catalog (`Manifest`). Logged-in players only: it names the protected files.
async fn game_manifest(State(state): State<Arc<AppState>>, request: Request) -> Response {
    let Some(&ClientIp(ip)) = request.extensions().get::<ClientIp>() else {
        return denied(StatusCode::UNAUTHORIZED);
    };
    let now = Instant::now();
    if !state.authenticated(request.headers(), ip, now) {
        return denied(StatusCode::UNAUTHORIZED);
    }
    if !lock(&state.quotas).assets.allow(ip, now) {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    }
    Json(state.manifest().await).into_response()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    access_key: String,
}
/// Build the session cookie. HTTPS uses the __Host- prefix, which browsers only
/// accept with Secure, Path=/ and no Domain; the reader uses the same name.
fn session_cookie_header(config: &Config, token: &str) -> String {
    format!(
        "{}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}{}",
        config.cookie_name(),
        SESSION_LIFETIME.as_secs(),
        if config.secure_cookie { "; Secure" } else { "" }
    )
}
async fn authenticate(State(state): State<Arc<AppState>>, request: Request) -> Response {
    let Some(&ClientIp(ip)) = request.extensions().get::<ClientIp>() else {
        return denied(StatusCode::UNAUTHORIZED);
    };
    let now = Instant::now();
    if !lock(&state.quotas).auth.allow(ip, now) {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    }
    if !exact_origin(request.headers(), &state.config.origin)
        || !request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.split(';').next() == Some("application/json"))
    {
        return denied(StatusCode::UNAUTHORIZED);
    }
    let previous =
        session_cookie(request.headers(), state.config.cookie_name()).map(|t| digest(t.as_bytes()));
    let Ok(Ok(body)) = timeout(
        FIRST_MESSAGE_TIMEOUT,
        to_bytes(request.into_body(), MAX_PACKET),
    )
    .await
    else {
        return denied(StatusCode::UNAUTHORIZED);
    };
    let Ok(credentials) = serde_json::from_slice::<Credentials>(&body) else {
        return denied(StatusCode::UNAUTHORIZED);
    };
    if !bool::from(
        state
            .config
            .access_digest
            .ct_eq(&digest(credentials.access_key.as_bytes())),
    ) {
        return denied(StatusCode::UNAUTHORIZED);
    }
    let Ok(token) = random_token() else {
        return denied(StatusCode::SERVICE_UNAVAILABLE);
    };
    {
        let mut sessions = lock(&state.sessions);
        sessions.retain(|_, s| s.expires > now);
        if let Some(old) = previous
            && sessions.get(&old).is_some_and(|s| s.ip == ip)
        {
            sessions.remove(&old);
        }
        if sessions.len() >= MAX_SESSIONS {
            return denied(StatusCode::TOO_MANY_REQUESTS);
        }
        sessions.insert(
            digest(token.as_bytes()),
            Session {
                ip,
                expires: now + SESSION_LIFETIME,
            },
        );
    }
    let mut response = Json(serde_json::json!({"authenticated": true})).into_response();
    if let Ok(cookie) = HeaderValue::from_str(&session_cookie_header(&state.config, &token)) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    response
}
async fn upgrade(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
    request: Request,
) -> Response {
    let Some(&ClientIp(ip)) = request.extensions().get::<ClientIp>() else {
        return denied(StatusCode::UNAUTHORIZED);
    };
    let now = Instant::now();
    if !exact_origin(request.headers(), &state.config.origin) {
        return denied(StatusCode::UNAUTHORIZED);
    }
    let Some(owner) = state.session(request.headers(), ip, now) else {
        return denied(StatusCode::UNAUTHORIZED);
    };
    if !lock(&state.quotas).upgrades.allow(ip, now) {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    }
    let Some(permit) = state.reserve_ws(ip, owner) else {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    };
    ws.max_message_size(MAX_PACKET)
        .max_frame_size(MAX_PACKET)
        .write_buffer_size(0)
        .max_write_buffer_size(64 * 1024)
        .on_upgrade(move |socket| connection(state, socket, permit, owner))
}
fn decode_input(text: &str) -> Result<ClientMessage, ()> {
    if text.len() > MAX_PACKET {
        return Err(());
    }
    let message: ClientMessage = serde_json::from_str(text).map_err(|_| ())?;
    if matches!(message, ClientMessage::Leave) {
        // Serde internally-tagged unit variants otherwise ignore extra fields.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct StrictUnit {
            #[serde(rename = "type")]
            _kind: String,
        }
        serde_json::from_str::<StrictUnit>(text).map_err(|_| ())?;
    }
    if matches!(message, ClientMessage::Input { held, pressed, .. } if !valid_input(held, pressed))
    {
        return Err(());
    }
    Ok(message)
}
fn frame_cost(message: &Message) -> u64 {
    let payload = match message {
        Message::Text(text) => text.len(),
        Message::Binary(data) | Message::Ping(data) | Message::Pong(data) => data.len(),
        Message::Close(_) => 125,
    };
    payload as u64 + WS_FRAME_OVERHEAD
}
#[derive(PartialEq)]
enum SendError {
    /// The shared egress budget refused the frame; nothing was sent.
    Budget,
    /// The socket failed or did not drain within SEND_TIMEOUT.
    Socket,
}
/// Every outbound frame is charged to the shared egress budget first. When the
/// budget is exhausted the frame is not sent and the connection ends.
async fn send(state: &AppState, socket: &mut WebSocket, message: Message) -> Result<(), SendError> {
    if !state.reserve_egress(frame_cost(&message), false) {
        return Err(SendError::Budget);
    }
    match timeout(SEND_TIMEOUT, socket.send(message)).await {
        Ok(Ok(())) => Ok(()),
        _ => Err(SendError::Socket),
    }
}
async fn send_json(
    state: &AppState,
    socket: &mut WebSocket,
    message: &ServerMessage,
) -> Result<(), SendError> {
    let Ok(text) = serde_json::to_string(message) else {
        return Err(SendError::Socket);
    };
    send(state, socket, Message::Text(text.into())).await
}
/// End a connection after a failed send, explaining a budget refusal if possible.
async fn abort(state: &AppState, socket: &mut WebSocket, error: SendError) {
    if error == SendError::Budget {
        close_socket(state, socket, Some(BUDGET_EXHAUSTED)).await;
    }
}
/// Final error and Close frames are charged even past the budget (at most two
/// small frames per connection) so the client can show why it was disconnected.
async fn close_socket(state: &AppState, socket: &mut WebSocket, error: Option<&str>) {
    let mut frames = Vec::with_capacity(2);
    if let Some(message) = error
        && let Ok(text) = serde_json::to_string(&ServerMessage::Error {
            message: message.into(),
        })
    {
        frames.push(Message::Text(text.into()));
    }
    frames.push(Message::Close(None));
    for frame in frames {
        state.charge_egress(frame_cost(&frame));
        if !matches!(timeout(SEND_TIMEOUT, socket.send(frame)).await, Ok(Ok(()))) {
            return;
        }
    }
    // Dropping a socket with unread input makes the OS send a TCP reset, and the client
    // may then discard the frames above. Read (no replies after Close) until the client's
    // Close, within a short, bounded linger.
    let _ = timeout(CLOSE_LINGER, async {
        let mut left = CLOSE_DRAIN_BYTES;
        while let Some(Ok(message)) = socket.recv().await {
            let cost = frame_cost(&message);
            if matches!(message, Message::Close(_)) || cost >= left {
                break;
            }
            left -= cost;
        }
    })
    .await;
}
async fn connection(
    state: Arc<AppState>,
    mut socket: WebSocket,
    _permit: WsPermit,
    owner: SessionId,
) {
    let mut budget = Bucket::new(180, Instant::now());
    let first = timeout(FIRST_MESSAGE_TIMEOUT, socket.recv()).await;
    let initial = match first {
        Ok(Some(Ok(Message::Text(text)))) => decode_input(&text),
        _ => Err(()),
    };
    let membership = match initial {
        Ok(ClientMessage::Create { version, name }) if version == PROTOCOL_VERSION => {
            state.create(owner, name.as_deref(), Instant::now())
        }
        Ok(ClientMessage::Join {
            version,
            code,
            resume,
            name,
        }) if version == PROTOCOL_VERSION => state.join(
            &code,
            resume.as_deref(),
            owner,
            name.as_deref(),
            Instant::now(),
        ),
        Ok(ClientMessage::Create { .. } | ClientMessage::Join { .. }) => Err(JoinError::Outdated),
        _ => Err(JoinError::Internal),
    };
    let mut member = match membership {
        Ok(member) => member,
        Err(error) => {
            close_socket(&state, &mut socket, Some(error.message())).await;
            return;
        }
    };
    budget.take(120.0, 180, Instant::now());
    let welcome = ServerMessage::Welcome {
        code: member.room.code.clone(),
        player: member.player,
        resume: member.resume.clone(),
    };
    if let Err(error) = send_json(&state, &mut socket, &welcome).await {
        abort(&state, &mut socket, error).await;
        return;
    }
    let initial = member.updates.borrow_and_update().clone();
    if let RoomUpdate::Frame(frame) = initial
        && let Err(error) = send_json(&state, &mut socket, &ServerMessage::Snapshot { frame }).await
    {
        abort(&state, &mut socket, error).await;
        return;
    }
    let mut heartbeat =
        tokio::time::interval_at(tokio::time::Instant::now() + PING_INTERVAL, PING_INTERVAL);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut awaiting_pong: Option<(Instant, [u8; 8])> = None;
    // Room locks are only taken inside synchronous expressions and released before
    // any socket await below.
    loop {
        tokio::select! {
            received = socket.recv() => {
                let Some(Ok(message)) = received else { break; };
                let now = Instant::now();
                let current = lock(&member.room.data).is_current(member.player, member.epoch);
                if !current {
                    close_socket(&state, &mut socket, Some("connection replaced")).await;
                    break;
                }
                if !budget.take(120.0, 180, now) {
                    close_socket(&state, &mut socket, Some("request denied")).await;
                    break;
                }
                match message {
                    Message::Text(text) => {
                        let action = decode_input(&text).and_then(|msg| {
                            lock(&member.room.data).message(member.player, member.epoch, msg, now)
                        });
                        match action {
                            Ok(false) => {}
                            Ok(true) => {
                                state.remove_room(&member.room);
                                let _ = send_json(&state, &mut socket, &ServerMessage::Closed { reason: "room closed".into() }).await;
                                close_socket(&state, &mut socket, None).await;
                                break;
                            }
                            Err(()) => {
                                close_socket(&state, &mut socket, Some("request denied")).await;
                                break;
                            }
                        }
                    }
                    Message::Pong(data) => {
                        if awaiting_pong.as_ref().is_some_and(|(_, nonce)| data.as_ref() == nonce) {
                            awaiting_pong = None;
                        }
                    }
                    Message::Ping(data) => {
                        if let Err(error) = send(&state, &mut socket, Message::Pong(data)).await {
                            abort(&state, &mut socket, error).await;
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Binary(_) => {
                        close_socket(&state, &mut socket, Some("request denied")).await;
                        break;
                    }
                }
            },
            changed = member.updates.changed() => {
                if changed.is_err() { break; }
                let update = member.updates.borrow_and_update().clone();
                match update {
                    RoomUpdate::Closed => {
                        let _ = send_json(&state, &mut socket, &ServerMessage::Closed { reason: "room closed".into() }).await;
                        close_socket(&state, &mut socket, None).await;
                        break;
                    }
                    RoomUpdate::Frame(frame) => {
                        let current = lock(&member.room.data).is_current(member.player, member.epoch);
                        if !current {
                            close_socket(&state, &mut socket, Some("connection replaced")).await;
                            break;
                        }
                        if let Err(error) = send_json(&state, &mut socket, &ServerMessage::Snapshot { frame }).await {
                            abort(&state, &mut socket, error).await;
                            break;
                        }
                    }
                }
            },
            _ = heartbeat.tick() => {
                let now = Instant::now();
                if awaiting_pong.is_some_and(|(sent, _)| now.saturating_duration_since(sent) >= PONG_TIMEOUT) {
                    close_socket(&state, &mut socket, None).await;
                    break;
                }
                if awaiting_pong.is_none() {
                    let mut nonce = [0; 8];
                    if getrandom::fill(&mut nonce).is_err() { break; }
                    if let Err(error) = send(&state, &mut socket, Message::Ping(nonce.to_vec().into())).await {
                        abort(&state, &mut socket, error).await;
                        break;
                    }
                    awaiting_pong = Some((now, nonce));
                }
            },
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combat::{Move, Outcome, RESUME_TICKS, State as FightState, UNIT},
        controls::{LEFT, LIGHT, RIGHT},
    };
    use axum::body::Body;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    const KEY: &str = "test-only-access-key-32-bytes-long";
    fn config() -> Config {
        Config::new(
            "127.0.0.1:18080".parse().unwrap(),
            "http://127.0.0.1:18080",
            KEY,
            std::env::current_dir().unwrap(),
        )
        .unwrap()
    }
    const A: SessionId = [1; 32];
    const B: SessionId = [2; 32];
    const C: SessionId = [3; 32];
    /// P1 is seat 0 (epoch 1), P2 seat 1 (epoch 2). P2 has already submitted one
    /// neutral input (seq 0) so the loading gate only waits on P1.
    fn paired(now: Instant) -> Arc<Room> {
        let room = Room::new("ABCDEFGH".into(), digest(b"p1"), A, seat_name(0), now);
        {
            let mut data = lock(&room.data);
            let epoch = data.attach(1, digest(b"p2"), B, now, seat_name(1)).unwrap();
            data.message(1, epoch, input(0, 0, 0, 0), now).unwrap();
        }
        room
    }
    fn input(seq: u32, round: u32, held: u16, pressed: u16) -> ClientMessage {
        ClientMessage::Input {
            seq,
            round,
            held,
            pressed,
        }
    }
    /// Play out the round introduction with both seats neutral. The clock stays pinned so
    /// the wall-clock input timeout never fires and eats nobody's hold during the warm.
    fn warm_intro(data: &mut RoomData, now: Instant) {
        while data.arena.intro > 0 {
            data.step(now);
        }
    }

    #[test]
    fn authority_input_is_slot_scoped_and_simulation_pauses() {
        let now = Instant::now();
        let room = Room::new("ABCDEFGH".into(), digest(b"p1"), A, seat_name(0), now);
        let mut data = lock(&room.data);
        let original = data.arena.clone();
        data.message(0, 1, input(1, 0, RIGHT, 0), now).unwrap();
        assert_eq!(data.arena, original, "input alone must never mutate combat");
        data.step(now);
        assert_eq!(data.arena, original, "waiting simulation is paused");
        let epoch = data.attach(1, digest(b"p2"), B, now, seat_name(1)).unwrap();
        assert!(data.message(2, epoch, input(1, 0, RIGHT, 0), now).is_err());
        assert!(data.message(0, epoch, input(2, 0, RIGHT, 0), now).is_err());
        data.step(now);
        assert_eq!(data.arena, original, "paused until P2 finishes loading");
        data.message(1, epoch, input(1, 0, 0, 0), now).unwrap();
        data.step(now);
        assert_eq!(
            data.arena.fighters[0].x, original.fighters[0].x,
            "the round introduction eats input before the fight is live"
        );
        warm_intro(&mut data, now);
        data.message(0, 1, input(2, 0, RIGHT, 0), now).unwrap();
        data.step(now);
        assert!(data.arena.fighters[0].x > original.fighters[0].x);
        assert_eq!(data.arena.fighters[1].x, original.fighters[1].x);
        assert_eq!(data.arena.fighters[0].hp, original.fighters[0].hp);
        let paused = data.arena.clone();
        data.disconnect(1, epoch, now);
        data.step(now + Duration::from_millis(50));
        assert_eq!(data.arena, paused);
    }

    #[test]
    fn invalid_packets_and_nonmonotonic_sequences_are_rejected() {
        for packet in [
            r#"{"type":"input","seq":1,"round":0,"held":0,"pressed":0,"hp":999}"#,
            r#"{"type":"input","seq":1,"round":0,"held":0,"pressed":0,"player":1}"#,
            r#"{"type":"input","seq":1,"round":0,"held":256,"pressed":0}"#,
            r#"{"type":"input","seq":1,"round":0,"held":0,"pressed":32}"#,
            r#"{"type":"input","seq":-1,"round":0,"held":0,"pressed":0}"#,
            r#"{"type":"input","seq":1,"seq":2,"round":0,"held":0,"pressed":0}"#,
            r#"{"type":"create","version":2,"player":0}"#,
            r#"{"type":"join","version":2,"code":"ABCDEFGH","arena":{}}"#,
            r#"{"type":"restart","player":0}"#,
            r#"{"type":"leave","hp":100}"#,
            r#"{"type":"state","x":0}"#,
            "[]",
            "null",
        ] {
            assert!(
                decode_input(packet).is_err(),
                "accepted forged packet: {packet}"
            );
        }
        assert!(decode_input(&" ".repeat(MAX_PACKET + 1)).is_err());
        let now = Instant::now();
        let room = paired(now);
        let mut data = lock(&room.data);
        data.message(0, 1, input(7, 0, RIGHT, 0), now).unwrap();
        for seq in [7, 6, 0] {
            assert!(data.message(0, 1, input(seq, 0, LEFT, 0), now).is_err());
        }
        assert!(data.message(0, 1, input(8, 1, RIGHT, 0), now).is_err());
        assert!(
            data.message(
                0,
                1,
                ClientMessage::Create {
                    version: PROTOCOL_VERSION,
                    name: None
                },
                now
            )
            .is_err()
        );
        assert_eq!(data.slots[0].as_ref().unwrap().input.held(), RIGHT);
    }

    #[test]
    fn restart_needs_two_fresh_votes_even_after_outcome_and_blocks_held_triggers() {
        let now = Instant::now();
        let room = paired(now);
        let mut data = lock(&room.data);
        data.arena.outcome = Some(Outcome::Winner(0));
        data.message(0, 1, input(1, 0, LIGHT, LIGHT), now).unwrap();
        data.message(0, 1, ClientMessage::Restart { round: 0 }, now)
            .unwrap();
        assert_eq!(data.round, 0);
        assert!(data.arena.outcome.is_some());
        data.message(
            1,
            2,
            ClientMessage::Restart { round: 0 },
            now + VOTE_WINDOW + Duration::from_millis(1),
        )
        .unwrap();
        assert_eq!(data.round, 0, "expired vote cannot pair");
        data.message(
            0,
            1,
            ClientMessage::Restart { round: 0 },
            now + VOTE_WINDOW + Duration::from_millis(2),
        )
        .unwrap();
        assert_eq!(data.round, 1);
        assert_eq!(data.arena, Arena::default());
        assert!(data.slots.iter().flatten().all(|s| s.vote.is_none()));
        let trigger = data.slots[0].as_mut().unwrap().input.take();
        assert_eq!(trigger.held & LIGHT, 0);
        assert_eq!(trigger.pressed & LIGHT, 0);
        data.message(0, 1, input(2, 0, 0, LIGHT), now + VOTE_WINDOW)
            .unwrap();
        assert_eq!(
            data.slots[0].as_ref().unwrap().input.held(),
            LIGHT,
            "old-round packet is ignored"
        );
        data.message(0, 1, input(3, 1, LIGHT, LIGHT), now + VOTE_WINDOW)
            .unwrap();
        assert_eq!(
            data.slots[0].as_mut().unwrap().input.take().pressed & LIGHT,
            0
        );
        data.message(0, 1, input(4, 1, 0, 0), now + VOTE_WINDOW)
            .unwrap();
        data.message(0, 1, input(5, 1, LIGHT, LIGHT), now + VOTE_WINDOW)
            .unwrap();
        assert_ne!(
            data.slots[0].as_mut().unwrap().input.take().pressed & LIGHT,
            0
        );
    }

    #[test]
    fn stale_input_releases_both_holds_and_unconsumed_taps_while_paused() {
        let now = Instant::now();
        let room = Room::new("ABCDEFGH".into(), digest(b"p1"), A, seat_name(0), now);
        let mut data = lock(&room.data);
        data.message(0, 1, input(1, 0, RIGHT | LIGHT, LIGHT), now)
            .unwrap();
        data.step(now + INPUT_TIMEOUT);
        assert_ne!(data.slots[0].as_ref().unwrap().input.held(), 0);
        data.step(now + INPUT_TIMEOUT + Duration::from_millis(1));
        let controls = data.slots[0].as_mut().unwrap().input.take();
        assert_eq!(controls.held, 0);
        assert_eq!(controls.pressed, 0);
        let later = now + INPUT_TIMEOUT;
        let epoch = data
            .attach(1, digest(b"p2"), B, later, seat_name(1))
            .unwrap();
        data.message(1, epoch, input(1, 0, 0, 0), later).unwrap();
        let start = data.arena.fighters[0].x;
        data.step(later + Duration::from_millis(2));
        assert!(
            data.arena.ticks > 0,
            "both seats initialized, simulation runs"
        );
        assert_eq!(data.arena.fighters[0].state, FightState::Ready);
        assert_eq!(
            data.arena.fighters[0].x, start,
            "released hold does not move P1"
        );
    }

    #[test]
    fn snapshots_accumulate_hits_until_broadcast_and_watch_is_latest_only() {
        let now = Instant::now();
        let room = paired(now);
        let mut data = lock(&room.data);
        data.arena.fighters[0].x = -70 * UNIT;
        data.arena.fighters[1].x = 70 * UNIT;
        data.message(0, 1, input(1, 0, 0, 0), now).unwrap();
        warm_intro(&mut data, now);
        data.message(0, 1, input(2, 0, LIGHT, LIGHT), now).unwrap();
        for tick in 0..10 {
            data.step(now + Duration::from_millis(tick * 16));
        }
        assert_eq!(data.hits.len(), 1);
        assert_eq!(
            data.arena.fighters[0].state,
            FightState::Attack(Move::Light)
        );
        let first = data.snapshot(now);
        assert_eq!(first.hits.len(), 1);
        let second = data.snapshot(now);
        assert!(second.hits.is_empty());
        assert!(second.seq > first.seq);
        let rx = room.updates.subscribe();
        for _ in 0..100 {
            room.updates
                .send_replace(RoomUpdate::Frame(Box::new(data.snapshot(now))));
        }
        let RoomUpdate::Frame(last) = &*rx.borrow() else {
            panic!("not a snapshot");
        };
        assert_eq!(last.seq, data.snapshot_seq);
    }

    #[tokio::test]
    async fn resume_retries_and_old_connection_epoch_cannot_clear_replacement() {
        let now = Instant::now();
        let app = AppState::new(config());
        let first = app.create(A, None, now).unwrap();
        assert!(valid_code(&first.room.code));
        assert!(valid_token(&first.resume));
        let code = first.room.code.clone();
        let old_resume = first.resume.clone();
        let p2 = app.join(&code, None, B, None, now).unwrap();
        assert_eq!(p2.player, 1);
        assert_eq!(
            app.join(&code, None, C, None, now).err(),
            Some(JoinError::Unavailable)
        );
        assert!(
            app.join(&code, Some(&"0".repeat(64)), A, None, now)
                .is_err()
        );
        // A resume capability is useless to any other logged-in session.
        for thief in [B, C] {
            assert_eq!(
                app.join(&code, Some(&old_resume), thief, None, now).err(),
                Some(JoinError::Unavailable)
            );
        }
        assert!(lock(&first.room.data).is_current(0, first.epoch));
        let replacement = app.join(&code, Some(&old_resume), A, None, now).unwrap();
        assert_eq!(replacement.player, 0);
        assert_eq!(replacement.resume, old_resume);
        assert_ne!(replacement.epoch, first.epoch);
        assert!(
            lock(&first.room.data)
                .message(0, first.epoch, input(1, 0, RIGHT, 0), now)
                .is_err()
        );
        drop(first);
        assert!(lock(&replacement.room.data).is_current(0, replacement.epoch));
        let retry = app.join(&code, Some(&old_resume), A, None, now).unwrap();
        assert_eq!(retry.player, 0);
        drop(retry);
        let token = replacement.resume.clone();
        lock(&replacement.room.data).disconnect(0, replacement.epoch, now);
        let resumed = app
            .join(&code, Some(&token), A, None, now + Duration::from_secs(29))
            .unwrap();
        drop(replacement);
        assert!(lock(&resumed.room.data).is_current(0, resumed.epoch));
        let token = resumed.resume.clone();
        lock(&resumed.room.data).disconnect(0, resumed.epoch, now);
        assert!(
            app.join(&code, Some(&token), A, None, now + RESUME_GRACE)
                .is_err()
        );
        assert!(lock(&app.rooms).is_empty());
        assert!(matches!(&*p2.updates.borrow(), RoomUpdate::Closed));
    }

    #[tokio::test]
    async fn rooms_live_while_a_seat_is_connected_and_cleanup_is_bounded() {
        let now = Instant::now();
        let app = AppState::new(config().with_max_rooms(1).unwrap());
        let first = app.create(A, None, now).unwrap();
        assert_eq!(app.create(B, None, now).err(), Some(JoinError::ServerFull));
        // Waiting for the other player: the room stays while its creator stays connected.
        app.prune_rooms(now + Duration::from_secs(3 * 3600));
        assert_eq!(lock(&app.rooms).len(), 1);
        // The creator drops: the seat is kept for the grace, then the room goes.
        let dropped = now + Duration::from_secs(3 * 3600);
        lock(&first.room.data).disconnect(0, first.epoch, dropped);
        app.prune_rooms(dropped + RESUME_GRACE - Duration::from_secs(1));
        assert_eq!(lock(&app.rooms).len(), 1);
        app.prune_rooms(dropped + RESUME_GRACE);
        assert!(lock(&app.rooms).is_empty());
        assert!(lock(&first.room.data).closed);
        let next = app.create(B, None, now).unwrap();
        let p2 = app.join(&next.room.code, None, C, None, now).unwrap();
        assert!(!lock(&next.room.data).expired(now + ROOM_LIFETIME - Duration::from_secs(1)));
        app.prune_rooms(now + ROOM_LIFETIME);
        assert!(lock(&app.rooms).is_empty());
        let last = app.create(A, None, now).unwrap();
        let weak = Arc::downgrade(&last.room);
        app.remove_room(&last.room); // Explicit Leave abandons immediately, not after a grace period.
        drop(last);
        assert!(
            weak.upgrade().is_none(),
            "room task holds only a weak reference"
        );
        drop(p2);
    }

    #[test]
    fn quotas_sessions_proxy_and_ws_capacity_have_hard_bounds() {
        let now = Instant::now();
        let ip: IpAddr = "192.0.2.1".parse().unwrap();
        let mut auth = Limiter::new(10, 120, now);
        for _ in 0..10 {
            assert!(auth.allow(ip, now));
        }
        assert!(!auth.allow(ip, now));
        assert!(!auth.allow(ip, now + Duration::from_secs(59)));
        assert!(auth.allow(ip, now + Duration::from_secs(60)));
        let mut message = Bucket::new(180, now);
        for _ in 0..180 {
            assert!(message.take(120.0, 180, now));
        }
        assert!(!message.take(120.0, 180, now));
        assert!(message.take(120.0, 180, now + Duration::from_millis(9)));
        let mut bounded = Limiter::new(1, 10000, now);
        for n in 0..MAX_LIMITER_IPS {
            assert!(bounded.allow(IpAddr::V6(std::net::Ipv6Addr::from(n as u128 + 1)), now));
        }
        assert!(!bounded.allow(ip, now));
        assert_eq!(bounded.ips.len(), MAX_LIMITER_IPS);
        assert!(bounded.allow(ip, now + Duration::from_secs(60)));
        assert_eq!(bounded.ips.len(), 1);
        let app = AppState::new(config());
        let mut permits = Vec::new();
        for n in 0..8 {
            let ip = IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, n + 1));
            for k in 0..4 {
                permits.push(app.reserve_ws(ip, [n * 4 + k + 10; 32]).unwrap());
            }
            assert!(app.reserve_ws(ip, [200; 32]).is_none(), "per-IP cap");
        }
        assert!(
            app.reserve_ws("198.51.100.1".parse().unwrap(), [201; 32])
                .is_none(),
            "global cap"
        );
        drop(permits);
        let a = app.reserve_ws("198.51.100.1".parse().unwrap(), A).unwrap();
        let b = app.reserve_ws("198.51.100.2".parse().unwrap(), A).unwrap();
        assert!(
            app.reserve_ws("198.51.100.3".parse().unwrap(), A).is_none(),
            "per-session cap holds across IPs"
        );
        drop((a, b));
        assert_eq!(lock(&app.ws_counts).total, 0);
        assert!(lock(&app.ws_counts).ips.is_empty());
        assert!(lock(&app.ws_counts).sessions.is_empty());
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("198.51.100.1"));
        headers.insert("x-real-ip", HeaderValue::from_static("198.51.100.2"));
        let peer = "127.0.0.1:8000".parse().unwrap();
        assert_eq!(app.client_ip(peer, &headers), Some(peer.ip()));
        let trusted = AppState::new(config().with_proxy_ips([peer.ip()]).unwrap());
        assert_eq!(
            trusted.client_ip(peer, &headers),
            Some("198.51.100.2".parse().unwrap())
        );
        headers.insert(
            "x-real-ip",
            HeaderValue::from_static("198.51.100.2, 198.51.100.3"),
        );
        assert!(trusted.client_ip(peer, &headers).is_none());
        let token = "a".repeat(64);
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("naifrog_session={token}")).unwrap(),
        );
        lock(&app.sessions).insert(
            digest(token.as_bytes()),
            Session {
                ip,
                expires: now + SESSION_LIFETIME,
            },
        );
        assert!(app.authenticated(&headers, ip, now));
        assert!(!app.authenticated(&headers, peer.ip(), now));
        assert!(!app.authenticated(&headers, ip, now + SESSION_LIFETIME));
        assert!(lock(&app.sessions).is_empty());
    }

    #[test]
    fn configuration_fails_unsafe_origins_and_short_keys() {
        let root = std::env::current_dir().unwrap();
        let bind = "127.0.0.1:18080".parse().unwrap();
        for origin in [
            "http://example.com",
            "https://example.com/",
            "https://EXAMPLE.com",
            "https://example.com:443",
            "https://user@example.com",
            "https://example.com/path",
            "null",
        ] {
            assert!(
                Config::new(bind, origin, KEY, &root).is_err(),
                "invalid origin accepted: {origin}"
            );
        }
        assert!(Config::new(bind, "https://example.com", "short", &root).is_err());
        assert!(
            Config::new(bind, "https://example.com", KEY, &root)
                .unwrap()
                .secure_cookie
        );
        assert!(Config::new(bind, "http://[::1]:18080", KEY, &root).is_ok());
        assert!(
            !Config::new(bind, "http://[::1]:18080", KEY, &root)
                .unwrap()
                .public_http
        );
        let public_http = Relaxed {
            public_http: true,
            ..Relaxed::default()
        };
        assert!(Config::new_relaxed(bind, "http://example.com", KEY, &root, public_http).is_ok());
        assert!(config().with_max_rooms(0).is_err());
        assert!(config().with_max_rooms(MAX_ROOM_CONFIG + 1).is_err());
    }

    #[test]
    fn a_server_without_a_domain_serves_plain_http_on_its_ip_only_when_asked() {
        let root = std::env::current_dir().unwrap();
        let public: SocketAddr = "0.0.0.0:18080".parse().unwrap();
        let relaxed = Relaxed {
            public_http: true,
            ..Relaxed::default()
        };
        let with = |bind, origin| Config::new_relaxed(bind, origin, KEY, &root, relaxed);
        for origin in ["http://203.0.113.10:18080", "http://[2001:db8::1]:18080"] {
            assert!(Config::new(public, origin, KEY, &root).is_err(), "{origin}");
            let config = with(public, origin).unwrap();
            assert!(config.public_http && !config.secure_cookie, "{origin}");
            assert_eq!(config.cookie_name(), "naifrog_session");
        }
        // Port 80 is the default for http, so the browser's Origin omits it.
        assert!(with(public, "http://203.0.113.10").is_ok());
        assert!(with(public, "http://203.0.113.10:80").is_err());
        assert!(with(public, "http://203.0.113.10:18080/").is_err());
        // A loopback origin served on every interface is public too.
        assert!(Config::new(public, "http://127.0.0.1:18080", KEY, &root).is_err());
        // HTTPS (a domain, or an IP certificate) never needs the flag.
        let ip_tls = Config::new(public, "https://203.0.113.10", KEY, &root).unwrap();
        assert!(ip_tls.secure_cookie && !ip_tls.public_http);
    }

    #[test]
    fn a_short_access_key_needs_its_own_flag() {
        let root = std::env::current_dir().unwrap();
        let bind = "127.0.0.1:18080".parse().unwrap();
        let origin = "http://127.0.0.1:18080";
        let short = Relaxed {
            short_key: true,
            ..Relaxed::default()
        };
        assert!(Config::new(bind, origin, "abcdef", &root).is_err());
        assert!(Config::new(bind, origin, "a".repeat(31), &root).is_err());
        let config = Config::new_relaxed(bind, origin, "abcdef", &root, short).unwrap();
        assert!(config.short_key && config.access_digest == digest(b"abcdef"));
        // Even with the flag a key has a floor, and a long key is never reported as short.
        assert!(Config::new_relaxed(bind, origin, "abcde", &root, short).is_err());
        assert!(
            !Config::new_relaxed(bind, origin, KEY, &root, short)
                .unwrap()
                .short_key
        );
        assert!(!Config::new(bind, origin, KEY, &root).unwrap().short_key);
    }

    #[tokio::test]
    async fn auth_session_map_is_bounded_and_https_cookie_is_secure() {
        let config = Config::new(
            "127.0.0.1:18080".parse().unwrap(),
            "https://example.com",
            KEY,
            std::env::current_dir().unwrap(),
        )
        .unwrap();
        let app = AppState::new(config);
        let ip = "192.0.2.1".parse().unwrap();
        let now = Instant::now();
        for n in 0..MAX_SESSIONS {
            lock(&app.sessions).insert(
                digest(&n.to_le_bytes()),
                Session {
                    ip,
                    expires: now + SESSION_LIFETIME,
                },
            );
        }
        let request = || {
            let mut req = Request::builder()
                .method("POST")
                .uri("/api/auth")
                .header(header::ORIGIN, "https://example.com")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"access_key": KEY}).to_string(),
                ))
                .unwrap();
            req.extensions_mut().insert(ClientIp(ip));
            req
        };
        assert_eq!(
            authenticate(State(app.clone()), request()).await.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(lock(&app.sessions).len(), MAX_SESSIONS);
        lock(&app.sessions).clear();
        let response = authenticate(State(app.clone()), request()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(cookie.starts_with("__Host-naifrog_session="));
        assert!(cookie.contains("; Secure"));
        assert!(cookie.contains("HttpOnly; SameSite=Strict; Path=/; Max-Age=43200"));
        assert!(!cookie.to_ascii_lowercase().contains("domain"));
        assert_eq!(lock(&app.sessions).len(), 1);
        // The reader uses the same prefixed name; the unprefixed one is ignored.
        let pair = cookie.split(';').next().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(pair).unwrap());
        assert!(app.authenticated(&headers, ip, now));
        let plain = pair.replacen("__Host-", "", 1);
        headers.insert(header::COOKIE, HeaderValue::from_str(&plain).unwrap());
        assert!(!app.authenticated(&headers, ip, now));
    }

    struct TempRoot(PathBuf);
    impl TempRoot {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("naifrog-server-test-{}", random_token().unwrap()));
            std::fs::create_dir_all(root.join("web/pkg")).unwrap();
            std::fs::create_dir_all(root.join("assets/processed")).unwrap();
            for (path, bytes) in [
                ("web/index.html", b"<!doctype html>ok".as_slice()),
                ("web/main.js", b"export {};"),
                ("web/network.js", b"export {};"),
                ("web/style.css", b"canvas{}"),
                ("web/pkg/game.wasm", b"wasm-test-data"),
                ("web/pkg/game.js", b"export {};"),
                ("assets/processed/fighter.webp", b"processed-webp"),
                ("source.rs", b"PRIVATE_SOURCE"),
                ("assets/original.png", b"PRIVATE_ART"),
            ] {
                std::fs::write(root.join(path), bytes).unwrap();
            }
            Self(root)
        }
    }
    impl Drop for TempRoot {
        fn drop(&mut self) {
            // Verify the absolute cleanup target is our uniquely named temporary fixture.
            if let (Ok(root), Ok(temp)) =
                (self.0.canonicalize(), std::env::temp_dir().canonicalize())
                && root.parent() == Some(temp.as_path())
                && root
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with("naifrog-server-test-"))
            {
                let _ = std::fs::remove_dir_all(root);
            }
        }
    }
    struct TestServer {
        addr: SocketAddr,
        origin: String,
        state: Arc<AppState>,
        task: tokio::task::JoinHandle<()>,
        _root: TempRoot,
    }
    impl Drop for TestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    async fn http_server() -> TestServer {
        http_server_with(|config| config).await
    }
    async fn http_server_with(adjust: impl FnOnce(Config) -> Config) -> TestServer {
        let root = TempRoot::new();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let origin = format!("http://{addr}");
        let state = AppState::new(adjust(Config::new(addr, &origin, KEY, &root.0).unwrap()));
        let app = routes(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap();
        });
        TestServer {
            addr,
            origin,
            state,
            task,
            _root: root,
        }
    }
    async fn http(
        server: &TestServer,
        method: &str,
        path: &str,
        headers: &str,
        body: &str,
    ) -> String {
        String::from_utf8(http_bytes(server, method, path, headers, body).await).unwrap()
    }
    async fn http_bytes(
        server: &TestServer,
        method: &str,
        path: &str,
        headers: &str,
        body: &str,
    ) -> Vec<u8> {
        let mut stream = TcpStream::connect(server.addr).await.unwrap();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}",
            server.addr,
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        timeout(Duration::from_secs(3), stream.read_to_end(&mut response))
            .await
            .unwrap()
            .unwrap();
        response
    }
    async fn login(server: &TestServer) -> String {
        let response = http(
            server,
            "POST",
            "/api/auth",
            &format!(
                "Origin: {}\r\nContent-Type: application/json\r\n",
                server.origin
            ),
            &serde_json::json!({"access_key": KEY}).to_string(),
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 200"));
        let cookie = response
            .lines()
            .find_map(|line| line.strip_prefix("set-cookie: "))
            .unwrap();
        assert!(
            cookie.contains("HttpOnly")
                && cookie.contains("SameSite=Strict")
                && cookie.contains("Max-Age=43200")
        );
        cookie.split(';').next().unwrap().to_string()
    }

    #[tokio::test]
    async fn raw_http_auth_origin_asset_protection_headers_and_wasm_limits() {
        let server = http_server().await;
        let public = http(&server, "GET", "/api/config", "", "").await;
        assert!(public.ends_with(r#"{"multiplayer":true,"authenticated":false,"protocol":9}"#));
        assert!(public.contains("content-security-policy:"));
        assert!(public.contains("wasm-unsafe-eval") && public.contains("frame-ancestors 'none'"));
        assert!(
            public.contains("x-content-type-options: nosniff")
                && public.contains("referrer-policy: no-referrer")
        );
        for path in [
            "/pkg/game.wasm",
            "/assets/processed/fighter.webp",
            "/api/ws",
        ] {
            assert!(
                http(&server, "GET", path, "", "")
                    .await
                    .starts_with("HTTP/1.1 401")
            );
        }
        let wrong = http(
            &server,
            "POST",
            "/api/auth",
            "Origin: https://wrong.example\r\nContent-Type: application/json\r\n",
            &serde_json::json!({"access_key": KEY}).to_string(),
        )
        .await;
        assert!(wrong.starts_with("HTTP/1.1 401") && !wrong.contains(KEY));
        let huge = http(
            &server,
            "POST",
            "/api/auth",
            &format!(
                "Origin: {}\r\nContent-Type: application/json\r\n",
                server.origin
            ),
            &"x".repeat(1025),
        )
        .await;
        assert!(huge.starts_with("HTTP/1.1 401"));
        let cookie = login(&server).await;
        let headers = format!("Cookie: {cookie}\r\n");
        let authenticated = http(&server, "GET", "/api/config", &headers, "").await;
        assert!(authenticated.contains("\"authenticated\":true"));
        for path in ["/assets/processed/fighter.webp", "/pkg/game.js"] {
            assert!(
                http(&server, "GET", path, &headers, "")
                    .await
                    .starts_with("HTTP/1.1 200")
            );
        }
        for path in [
            "/source.rs",
            "/assets/original.png",
            "/Cargo.toml",
            "/pkg/../source.rs",
            "/pkg/%2e%2e/source.rs",
            "/assets/processed/",
        ] {
            let response = http(&server, "GET", path, &headers, "").await;
            assert!(!response.starts_with("HTTP/1.1 200"));
            assert!(!response.contains("PRIVATE_SOURCE") && !response.contains("PRIVATE_ART"));
        }
        assert!(
            http(
                &server,
                "GET",
                "/api/ws",
                &format!("{headers}Origin: https://wrong.example\r\n"),
                ""
            )
            .await
            .starts_with("HTTP/1.1 401")
        );
        for _ in 0..WASM_PER_IP_PER_MINUTE {
            assert!(
                http(&server, "GET", "/pkg/game.wasm", &headers, "")
                    .await
                    .starts_with("HTTP/1.1 200")
            );
        }
        assert!(
            http(&server, "GET", "/pkg/game.wasm", &headers, "")
                .await
                .starts_with("HTTP/1.1 429")
        );
    }

    async fn ws_connect(server: &TestServer, cookie: &str) -> TcpStream {
        let mut stream = TcpStream::connect(server.addr).await.unwrap();
        let headers = format!(
            "GET /api/ws HTTP/1.1\r\nHost: {}\r\nOrigin: {}\r\nCookie: {cookie}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
            server.addr, server.origin
        );
        stream.write_all(headers.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        while !response.ends_with(b"\r\n\r\n") {
            response.push(
                timeout(Duration::from_secs(2), stream.read_u8())
                    .await
                    .unwrap()
                    .unwrap(),
            );
            assert!(response.len() < 8192);
        }
        assert!(response.starts_with(b"HTTP/1.1 101"));
        stream
    }
    fn masked_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
        let mask = [13, 57, 91, 127];
        let mut frame = vec![0x80 | opcode];
        if payload.len() < 126 {
            frame.push(0x80 | payload.len() as u8);
        } else {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        frame.extend_from_slice(&mask);
        frame.extend(payload.iter().enumerate().map(|(n, b)| b ^ mask[n % 4]));
        frame
    }
    async fn ws_send(stream: &mut TcpStream, message: &ClientMessage) {
        stream
            .write_all(&masked_frame(
                1,
                serde_json::to_string(message).unwrap().as_bytes(),
            ))
            .await
            .unwrap();
    }
    async fn ws_event(stream: &mut TcpStream) -> ServerMessage {
        timeout(Duration::from_secs(3), async {
            loop {
                let first = stream.read_u8().await.unwrap();
                let second = stream.read_u8().await.unwrap();
                assert_eq!(second & 128, 0);
                let length = match second & 127 {
                    126 => stream.read_u16().await.unwrap() as usize,
                    127 => stream.read_u64().await.unwrap() as usize,
                    n => n as usize,
                };
                assert!(length <= 64 * 1024);
                let mut payload = vec![0; length];
                stream.read_exact(&mut payload).await.unwrap();
                match first & 15 {
                    1 => return serde_json::from_slice(&payload).unwrap(),
                    9 => stream.write_all(&masked_frame(10, &payload)).await.unwrap(),
                    8 => panic!("socket closed unexpectedly"),
                    _ => {}
                }
            }
        })
        .await
        .unwrap()
    }
    async fn ws_snapshot(
        stream: &mut TcpStream,
        predicate: impl Fn(&Snapshot) -> bool,
    ) -> Snapshot {
        // A socket left unread through the versus screen and intro buffers ~70 frames
        // (20 Hz for 3.5 s); drain far past that backlog before giving up.
        for _ in 0..400 {
            if let ServerMessage::Snapshot { frame } = ws_event(stream).await
                && predicate(&frame)
            {
                return *frame;
            }
        }
        panic!("snapshot condition never arrived");
    }

    #[tokio::test]
    async fn raw_ws_two_players_inputs_paired_restart_disconnect_resume_and_leave() {
        let server = http_server().await;
        let cookie = login(&server).await;
        let cookie_b = login(&server).await;
        let thief = login(&server).await;
        assert!(cookie != cookie_b && cookie_b != thief);
        let mut p1 = ws_connect(&server, &cookie).await;
        ws_send(
            &mut p1,
            &ClientMessage::Create {
                version: PROTOCOL_VERSION,
                name: None,
            },
        )
        .await;
        let ServerMessage::Welcome {
            code,
            player,
            resume,
        } = ws_event(&mut p1).await
        else {
            panic!("missing welcome");
        };
        assert_eq!(player, 0);
        assert!(valid_code(&code) && valid_token(&resume));
        let mut p2 = ws_connect(&server, &cookie_b).await;
        ws_send(
            &mut p2,
            &ClientMessage::Join {
                version: PROTOCOL_VERSION,
                code: code.clone(),
                resume: None,
                name: None,
            },
        )
        .await;
        let ServerMessage::Welcome { player, .. } = ws_event(&mut p2).await else {
            panic!("missing welcome");
        };
        assert_eq!(player, 1);
        // The round introduction plays out in real time (90 ticks) while the input timeout
        // clears holds after 250 ms, so both players keep re-sending their holds and the
        // test reads one snapshot per round until the fight is live and P1 has moved.
        let mut seq = 1u32;
        let frame = loop {
            ws_send(&mut p1, &input(seq, 0, RIGHT, 0)).await;
            ws_send(&mut p2, &input(seq, 0, LEFT, 0)).await;
            seq += 1;
            let ServerMessage::Snapshot { frame } = ws_event(&mut p1).await else {
                panic!("expected a snapshot");
            };
            if frame.connected == [true; 2]
                && frame.held == [RIGHT, LEFT]
                && frame.arena.fighters[0].x > Arena::default().fighters[0].x
            {
                break frame;
            }
            assert!(seq < 400, "fight never went live");
        };
        assert!(frame.arena.fighters[0].x > Arena::default().fighters[0].x);
        ws_send(&mut p1, &ClientMessage::Restart { round: 0 }).await;
        ws_snapshot(&mut p1, |f| f.votes == [true, false]).await;
        ws_send(&mut p2, &ClientMessage::Restart { round: 0 }).await;
        ws_snapshot(&mut p1, |f| f.round == 1 && f.votes == [false; 2]).await;
        p1.write_all(&masked_frame(
            1,
            br#"{"type":"input","seq":2,"round":1,"held":0,"pressed":0,"hp":999}"#,
        ))
        .await
        .unwrap();
        let error = loop {
            let event = ws_event(&mut p1).await;
            if matches!(event, ServerMessage::Error { .. }) {
                break event;
            }
        };
        assert!(matches!(error, ServerMessage::Error { message } if message == "request denied"));
        ws_snapshot(&mut p2, |f| f.connected == [false, true]).await;
        // Another logged-in session holding the leaked resume string cannot take the seat.
        let mut stolen = ws_connect(&server, &thief).await;
        ws_send(
            &mut stolen,
            &ClientMessage::Join {
                version: PROTOCOL_VERSION,
                code: code.clone(),
                resume: Some(resume.clone()),
                name: None,
            },
        )
        .await;
        assert!(matches!(
            ws_event(&mut stolen).await,
            ServerMessage::Error { message } if message == "room unavailable"
        ));
        let mut restored = ws_connect(&server, &cookie).await;
        ws_send(
            &mut restored,
            &ClientMessage::Join {
                version: PROTOCOL_VERSION,
                code: code.clone(),
                resume: Some(resume.clone()),
                name: None,
            },
        )
        .await;
        let ServerMessage::Welcome {
            player,
            resume: rotated,
            ..
        } = ws_event(&mut restored).await
        else {
            panic!("missing welcome");
        };
        assert_eq!(player, 0);
        assert_eq!(rotated, resume);
        ws_snapshot(&mut restored, |f| f.connected == [true; 2] && f.round == 1).await;
        ws_send(&mut restored, &ClientMessage::Leave).await;
        loop {
            if let ServerMessage::Closed { reason } = ws_event(&mut p2).await {
                assert_eq!(reason, "room closed");
                break;
            }
        }
    }

    #[tokio::test]
    async fn raw_ws_rejects_wrong_version_oversized_frames_and_message_flood() {
        let server = http_server().await;
        let mut bad_version = ws_connect(&server, &login(&server).await).await;
        ws_send(
            &mut bad_version,
            &ClientMessage::Create {
                version: PROTOCOL_VERSION + 1,
                name: None,
            },
        )
        .await;
        assert!(matches!(
            ws_event(&mut bad_version).await,
            ServerMessage::Error { message } if message == "client outdated, reload the page"
        ));
        let mut huge = ws_connect(&server, &login(&server).await).await;
        huge.write_all(&masked_frame(1, &vec![b'x'; MAX_PACKET + 1]))
            .await
            .unwrap();
        // Tungstenite rejects before parsing; the generic error is emitted if transport remains writable.
        let mut response = [0; 128];
        assert!(
            timeout(Duration::from_secs(3), huge.read(&mut response))
                .await
                .unwrap()
                .is_ok()
        );
        let mut flood = ws_connect(&server, &login(&server).await).await;
        ws_send(
            &mut flood,
            &ClientMessage::Create {
                version: PROTOCOL_VERSION,
                name: None,
            },
        )
        .await;
        assert!(matches!(
            ws_event(&mut flood).await,
            ServerMessage::Welcome { .. }
        ));
        let mut frames = Vec::new();
        for seq in 0..400 {
            frames.extend(masked_frame(
                1,
                serde_json::to_string(&input(seq, 0, 0, 0))
                    .unwrap()
                    .as_bytes(),
            ));
        }
        let _ = flood.write_all(&frames).await;
        for _ in 0..60 {
            if matches!(ws_event(&mut flood).await, ServerMessage::Error { .. }) {
                return;
            }
        }
        panic!("message flood was not rejected");
    }

    #[test]
    fn loading_gate_pauses_until_both_seats_send_current_round_input_again_after_resume() {
        let now = Instant::now();
        let room = paired(now);
        let mut data = lock(&room.data);
        let original = data.arena.clone();
        data.step(now);
        assert_eq!(data.arena, original, "P1 is still loading");
        data.message(0, 1, input(1, 0, RIGHT, 0), now).unwrap();
        data.step(now);
        assert!(data.arena.ticks > 0);
        assert_ne!(data.arena, original);
        // P2 reconnects: a new epoch starts uninitialized and the fight pauses.
        data.disconnect(1, 2, now);
        let epoch = data.attach(1, digest(b"p2"), B, now, seat_name(1)).unwrap();
        let paused = data.arena.clone();
        data.message(0, 1, input(2, 0, RIGHT, 0), now).unwrap();
        data.step(now);
        assert_eq!(data.arena, paused, "resuming seat has not sent input yet");
        // An old-round packet does not count as loaded and does not refresh freshness.
        data.round = 1;
        data.message(1, epoch, input(1, 0, 0, 0), now).unwrap();
        assert!(!data.slots[1].as_ref().unwrap().initialized);
        assert!(data.slots[1].as_ref().unwrap().last_input.is_none());
        data.message(1, epoch, input(2, 1, 0, 0), now).unwrap();
        data.message(0, 1, input(3, 1, RIGHT, 0), now).unwrap();
        data.step(now);
        assert_ne!(data.arena, paused);
    }

    #[test]
    fn an_interrupted_round_counts_back_in_before_it_continues() {
        let now = Instant::now();
        let room = paired(now);
        let mut data = lock(&room.data);
        data.message(0, 1, input(1, 0, RIGHT, 0), now).unwrap();
        while data.arena.intro > 0 {
            data.step(now);
        }
        data.step(now);
        assert!(data.arena.ticks > 0);
        assert_eq!(
            data.arena.resume, 0,
            "a match nobody interrupted plays no countdown"
        );
        // Mid-round drop and return: once both seats are current again the round
        // counts back in instead of continuing from one tick to the next.
        data.disconnect(1, 2, now);
        let epoch = data.attach(1, digest(b"p2"), B, now, seat_name(1)).unwrap();
        data.message(0, 1, input(2, 0, 0, 0), now).unwrap();
        data.message(1, epoch, input(1, 0, 0, 0), now).unwrap();
        data.step(now);
        assert_eq!(
            data.arena.resume,
            RESUME_TICKS - 1,
            "the tick that notices both seats back is the first countdown tick"
        );
        let frozen = data.arena.remaining;
        data.step(now);
        assert_eq!(
            data.arena.remaining, frozen,
            "the countdown freezes the clock"
        );
        assert_eq!(data.arena.resume, RESUME_TICKS - 2);
    }

    #[test]
    fn a_drop_while_waiting_for_the_rival_plays_no_countdown() {
        use crate::combat::{INTRO_TICKS, VS_TICKS};
        let now = Instant::now();
        // The host builds the room, drops while waiting (say, switched apps to send the
        // match code), then returns; the rival joins and both send their first input.
        let room = Room::new("ABCDEFGH".into(), digest(b"p1"), A, seat_name(0), now);
        let mut data = lock(&room.data);
        data.disconnect(0, 1, now);
        let p1 = data.attach(0, digest(b"p1"), A, now, seat_name(0)).unwrap();
        let p2 = data.attach(1, digest(b"p2"), B, now, seat_name(1)).unwrap();
        data.message(0, p1, input(0, 0, 0, 0), now).unwrap();
        data.message(1, p2, input(0, 0, 0, 0), now).unwrap();
        data.step(now);
        assert_eq!(
            data.arena.resume, 0,
            "the versus screen continues; no 3-2-1 stacked on top of it"
        );
        assert_eq!(
            data.arena.intro,
            VS_TICKS + INTRO_TICKS - 1,
            "the introduction advances a tick as usual"
        );
    }

    #[test]
    fn snapshots_count_down_each_disconnected_seats_grace() {
        let now = Instant::now();
        let room = paired(now);
        let mut data = lock(&room.data);
        assert_eq!(
            data.snapshot(now).grace_left,
            [None, None],
            "connected seats show no grace"
        );
        data.disconnect(1, 2, now);
        assert_eq!(
            data.snapshot(now).grace_left,
            [None, Some(300)],
            "a dropped seat starts at the full five minutes"
        );
        assert_eq!(
            data.snapshot(now + Duration::from_secs(61)).grace_left[1],
            Some(239)
        );
        let past = now + RESUME_GRACE + Duration::from_secs(2);
        assert_eq!(
            data.snapshot(past).grace_left[1],
            Some(0),
            "clamped at zero, never wrapped"
        );
        assert!(data.expired(past), "the room dies with the grace");
    }

    #[test]
    fn restart_votes_need_both_seats_online_and_the_current_round() {
        let now = Instant::now();
        let room = paired(now);
        let mut data = lock(&room.data);
        data.message(0, 1, ClientMessage::Restart { round: 1 }, now)
            .unwrap();
        data.message(0, 1, ClientMessage::Restart { round: 7 }, now)
            .unwrap();
        assert!(
            data.slots.iter().flatten().all(|s| s.vote.is_none()),
            "future round ignored"
        );
        data.disconnect(1, 2, now);
        data.message(0, 1, ClientMessage::Restart { round: 0 }, now)
            .unwrap();
        assert!(
            data.slots[0].as_ref().unwrap().vote.is_none(),
            "opponent offline"
        );
        let epoch = data.attach(1, digest(b"p2"), B, now, seat_name(1)).unwrap();
        data.message(0, 1, ClientMessage::Restart { round: 0 }, now)
            .unwrap();
        data.message(1, epoch, ClientMessage::Restart { round: 0 }, now)
            .unwrap();
        assert_eq!(data.round, 1);
        // A vote for the finished round is stale now.
        data.message(0, 1, ClientMessage::Restart { round: 0 }, now)
            .unwrap();
        assert!(data.slots[0].as_ref().unwrap().vote.is_none());
    }

    #[tokio::test]
    async fn names_are_validated_carried_and_survive_resume() {
        let now = Instant::now();
        let app = AppState::new(config());
        // A hostile or unrenderable name is cleaned to the seat default, not rejected.
        let hostile = app
            .create(A, Some("<script>alert(1)</script>"), now)
            .unwrap();
        let hostile_b = app
            .join(
                &hostile.room.code,
                None,
                B,
                Some("奶蛙\u{7}name-way-too-long-for-the-plate"),
                now,
            )
            .unwrap();
        assert_eq!(hostile_b.player, 1);
        assert_eq!(
            lock(&hostile.room.data).snapshot(now).names,
            ["P1".to_string(), "P2".to_string()]
        );
        drop(hostile_b);
        drop(hostile);
        // The named room carries both seats, trimmed.
        let named = app.create(A, Some(" Pad-One "), now).unwrap();
        {
            let data = lock(&named.room.data);
            assert_eq!(data.slots[0].as_ref().unwrap().name, "Pad-One");
        }
        let joined = app
            .join(&named.room.code, None, B, Some("Frog-Two"), now)
            .unwrap();
        assert_eq!(
            lock(&named.room.data).snapshot(now).names,
            ["Pad-One".to_string(), "Frog-Two".to_string()]
        );
        // Dropping and resuming the seat: an absent or invalid name keeps the stored
        // one; a fresh valid name replaces it.
        let token = joined.resume.clone();
        drop(joined);
        let back = app
            .join(&named.room.code, Some(&token), B, Some("  \u{0} "), now)
            .unwrap();
        assert_eq!(
            lock(&named.room.data).snapshot(now).names,
            ["Pad-One".to_string(), "Frog-Two".to_string()]
        );
        let token = back.resume.clone();
        drop(back);
        let renamed = app
            .join(&named.room.code, Some(&token), B, Some("New2"), now)
            .unwrap();
        assert_eq!(renamed.player, 1, "the resume still takes the same seat");
        assert_eq!(
            lock(&named.room.data).snapshot(now).names,
            ["Pad-One".to_string(), "New2".to_string()]
        );
    }

    #[tokio::test]
    async fn one_live_room_per_session_and_disconnected_seats_are_abandoned() {
        let now = Instant::now();
        let app = AppState::new(config());
        let first = app.create(A, None, now).unwrap();
        assert_eq!(app.create(A, None, now).err(), Some(JoinError::Busy));
        let other = app.create(B, None, now).unwrap();
        assert_eq!(
            app.join(&other.room.code, None, A, None, now).err(),
            Some(JoinError::Busy),
            "a connected seat elsewhere blocks joining"
        );
        let first_room = first.room.clone();
        drop(first); // A's tab closes: seat disconnected, still within resume grace.
        let joined = app.join(&other.room.code, None, A, None, now).unwrap();
        assert_eq!(joined.player, 1);
        assert!(lock(&first_room.data).closed, "abandoned room is closed");
        assert!(!lock(&app.rooms).contains_key(&first_room.code));
        assert_eq!(lock(&app.rooms).len(), 1);
        // Resuming the room you are in is not blocked by the quota.
        let resumed = app
            .join(&other.room.code, Some(&joined.resume), A, None, now)
            .unwrap();
        assert_eq!(resumed.player, 1);
        assert!(
            app.join("ABC", None, C, None, now).is_err()
                && app.join(&other.room.code, Some("x"), C, None, now).is_err()
        );
    }

    #[test]
    fn egress_window_is_bounded_rolling_and_public_share_is_capped() {
        let now = Instant::now();
        let minute = Duration::from_secs(60);
        let mut egress = Egress::new(1000, now);
        assert!(egress.fits(1000, now));
        egress.commit(600, now);
        assert!(!egress.fits(401, now + Duration::from_secs(59)));
        egress.commit(400, now + minute * 30);
        assert!(
            !egress.fits(1, now + minute * 60),
            "61 bins include the oldest minute"
        );
        assert!(
            egress.fits(600, now + minute * 61),
            "first minute rolled out"
        );
        assert!(!egress.fits(601, now + minute * 61));
        assert!(egress.fits(1000, now + minute * 91));
        let mut budget = EgressBudget {
            total: Egress::new(1000, now),
            public: Egress::new(100, now),
        };
        assert!(!budget.reserve(101, true, now));
        assert!(budget.reserve(100, true, now));
        assert!(!budget.reserve(1, true, now), "public share exhausted");
        assert!(
            budget.reserve(900, false, now),
            "players still have the rest"
        );
        assert!(!budget.reserve(1, false, now));
    }

    #[tokio::test]
    async fn stalled_reader_releases_transfer_permit_at_the_deadline() {
        let root = TempRoot::new();
        let path = root.0.join("web/pkg/big.wasm");
        std::fs::write(&path, vec![7u8; 1024 * 1024]).unwrap();
        let semaphore = Arc::new(Semaphore::new(1));
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let file = tokio::fs::File::open(&path).await.unwrap();
        let body = stream_file(file, 1024 * 1024, Duration::from_millis(50), permit);
        assert_eq!(semaphore.available_permits(), 0);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            semaphore.available_permits(),
            1,
            "permit freed without reader"
        );
        let partial = to_bytes(body, usize::MAX).await.unwrap();
        assert!(
            partial.len() < 1024 * 1024,
            "truncated body aborts the response"
        );
    }

    /// A gzip member holding `data` in one stored deflate block.
    fn gzip_stored(data: &[u8]) -> Vec<u8> {
        let mut crc = !0u32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        let len = u16::try_from(data.len()).unwrap();
        let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff, 1];
        out.extend(len.to_le_bytes());
        out.extend((!len).to_le_bytes());
        out.extend(data);
        out.extend((!crc).to_le_bytes());
        out.extend(u32::try_from(data.len()).unwrap().to_le_bytes());
        out
    }

    #[test]
    fn gzip_only_when_the_client_accepts_it() {
        let accepts = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::ACCEPT_ENCODING,
                HeaderValue::from_str(value).unwrap(),
            );
            accepts_gzip(&headers)
        };
        assert!(accepts("gzip, deflate"));
        assert!(accepts("br;q=1.0, GZIP;q=0.5"));
        assert!(!accepts("gzip;q=0"));
        assert!(!accepts("deflate, gzip; Q=0.000"));
        assert!(!accepts("deflate, br, zstd"));
        assert!(!accepts("x-gzip-like"));
        assert!(!accepts_gzip(&HeaderMap::new()));
    }

    #[tokio::test]
    async fn wasm_goes_out_gzipped_when_a_matching_copy_lies_beside_it() {
        let server = http_server().await;
        let root = server._root.0.clone();
        let plain = b"wasm-test-data";
        let gz = gzip_stored(plain);
        std::fs::write(root.join("web/pkg/game.wasm.gz"), &gz).unwrap();
        std::fs::write(root.join("assets/processed/fighter.webp.gz"), &gz).unwrap();
        let cookie = login(&server).await;
        let gzip = format!("Cookie: {cookie}\r\nAccept-Encoding: deflate, gzip\r\n");
        let response = http_bytes(&server, "GET", "/pkg/game.wasm", &gzip, "").await;
        let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let head = String::from_utf8(response[..split].to_vec()).unwrap();
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert!(head.contains("content-type: application/wasm"));
        assert!(head.contains("content-encoding: gzip") && head.contains("vary: accept-encoding"));
        assert!(head.contains(&format!("content-length: {}\r\n", gz.len())));
        assert!(head.contains(&format!("{UNCOMPRESSED_LENGTH}: {}\r\n", plain.len())));
        assert_eq!(response[split..], gz[..]);
        let head = http(&server, "HEAD", "/pkg/game.wasm", &gzip, "").await;
        assert!(head.contains("content-encoding: gzip") && head.ends_with("\r\n\r\n"));
        // Without gzip, or with gzip refused, the plain file goes out.
        for headers in [
            format!("Cookie: {cookie}\r\n"),
            format!("Cookie: {cookie}\r\nAccept-Encoding: gzip;q=0\r\n"),
        ] {
            let response = http(&server, "GET", "/pkg/game.wasm", &headers, "").await;
            assert!(
                response.starts_with("HTTP/1.1 200") && response.contains("vary: accept-encoding")
            );
            assert!(!response.contains("content-encoding") && response.ends_with("wasm-test-data"));
        }
        // Only WASM is encoded, and the .gz file is not served by its own name.
        let art = http(&server, "GET", "/assets/processed/fighter.webp", &gzip, "").await;
        assert!(!art.contains("content-encoding") && art.ends_with("processed-webp"));
        let direct = http(&server, "GET", "/pkg/game.wasm.gz", &gzip, "").await;
        assert!(direct.starts_with("HTTP/1.1 404"));
        // A copy from another build records another length in its trailer: ignored.
        std::fs::write(
            root.join("web/pkg/game.wasm.gz"),
            gzip_stored(b"older-wasm"),
        )
        .unwrap();
        let stale = http(&server, "GET", "/pkg/game.wasm", &gzip, "").await;
        assert!(!stale.contains("content-encoding") && stale.ends_with("wasm-test-data"));
    }

    #[tokio::test]
    async fn gzip_rejects_equal_length_stale_and_corrupt_copies() {
        let server = http_server().await;
        let root = &server._root.0;
        let plain = b"wasm-test-data";
        let compressed = root.join("web/pkg/game.wasm.gz");
        let cookie = login(&server).await;
        let headers = format!("Cookie: {cookie}\r\nAccept-Encoding: gzip\r\n");
        let version = hex(&digest(plain)[..10]);
        let url = format!("/v/{version}/pkg/game.wasm");
        let mut older = plain.to_vec();
        older[0] ^= 1;
        let mut corrupt = gzip_stored(plain);
        let crc = corrupt.len() - 8;
        corrupt[crc] ^= 1;
        for gz in [gzip_stored(&older), corrupt] {
            std::fs::write(&compressed, gz).unwrap();
            let response = http_bytes(&server, "GET", &url, &headers, "").await;
            let end = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            let head = String::from_utf8(response[..end].to_vec()).unwrap();
            assert!(head.starts_with("HTTP/1.1 200"));
            assert!(!head.contains("content-encoding: gzip"));
            assert_eq!(&response[end..], plain);
        }
        // Replacing a rejected copy with the matching content must invalidate the cache.
        std::fs::write(&compressed, gzip_stored(plain)).unwrap();
        let response = http_bytes(&server, "GET", &url, &headers, "").await;
        let end = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let head = String::from_utf8(response[..end].to_vec()).unwrap();
        assert!(head.contains("content-encoding: gzip"));
        assert_eq!(response[end..], gzip_stored(plain));
        // An equal-length plain-file update also invalidates the verified pair.
        std::fs::write(root.join("web/pkg/game.wasm"), &older).unwrap();
        let response = http(&server, "GET", "/pkg/game.wasm", &headers, "").await;
        assert!(!response.contains("content-encoding: gzip"));
        assert!(response.ends_with(std::str::from_utf8(&older).unwrap()));
    }

    /// The value of a lowercase response header, if present.
    fn header_value<'a>(response: &'a str, name: &str) -> Option<&'a str> {
        let head = &response[..response.find("\r\n\r\n")?];
        head.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix(": "))
    }

    /// The manifest entries, from a logged-in GET /api/manifest.
    async fn manifest(
        server: &TestServer,
        auth: &str,
    ) -> serde_json::Map<String, serde_json::Value> {
        let response = http(server, "GET", "/api/manifest", auth, "").await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert_eq!(header_value(&response, "cache-control"), Some(NO_STORE));
        let body = &response[response.find("\r\n\r\n").unwrap() + 4..];
        let value: serde_json::Value = serde_json::from_str(body).unwrap();
        value["files"].as_object().unwrap().clone()
    }

    #[tokio::test]
    async fn manifest_lists_game_files_and_versioned_urls_are_kept() {
        let server = http_server().await;
        let root = server._root.0.clone();
        let anonymous = http(&server, "GET", "/api/manifest", "", "").await;
        assert!(anonymous.starts_with("HTTP/1.1 401"), "{anonymous}");
        assert_eq!(header_value(&anonymous, "cache-control"), Some(NO_STORE));
        let cookie = login(&server).await;
        let auth = format!("Cookie: {cookie}\r\n");
        std::fs::write(root.join("web/pkg/game.wasm.gz"), b"not listed").unwrap();
        // The name font is a game file; its license text ships beside it but is not served.
        std::fs::write(root.join("assets/processed/name_font.otf"), b"otf-test").unwrap();
        std::fs::write(root.join("assets/processed/name_font-OFL.txt"), b"OFL").unwrap();
        let files = manifest(&server, &auth).await;
        let names: Vec<&str> = files.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "assets/processed/fighter.webp",
                "assets/processed/name_font.otf",
                "pkg/game.js",
                "pkg/game.wasm"
            ]
        );
        let font_url = format!(
            "/v/{}/assets/processed/name_font.otf",
            files["assets/processed/name_font.otf"].as_str().unwrap()
        );
        let font = http(&server, "GET", &font_url, &auth, "").await;
        assert!(font.starts_with("HTTP/1.1 200") && font.ends_with("otf-test"));
        assert_eq!(header_value(&font, "content-type"), Some("font/otf"));
        assert_eq!(header_value(&font, "cache-control"), Some(IMMUTABLE_GAME));
        let license = http(
            &server,
            "GET",
            "/assets/processed/name_font-OFL.txt",
            &auth,
            "",
        )
        .await;
        assert!(license.starts_with("HTTP/1.1 404"), "{license}");
        let hash = |name: &str| files[name].as_str().unwrap().to_owned();
        let (js, art_hash) = (hash("pkg/game.js"), hash("assets/processed/fighter.webp"));
        assert!(files.values().all(|v| {
            v.as_str()
                .is_some_and(|h| h.len() == 20 && split_version(&format!("/v/{h}/")).0 == Some(h))
        }));

        // A file's own hash keeps its URL for a year; a plain or foreign hash is revalidated.
        let kept = http(&server, "GET", &format!("/v/{js}/pkg/game.js"), &auth, "").await;
        assert!(kept.starts_with("HTTP/1.1 200") && kept.ends_with("export {};"));
        assert_eq!(header_value(&kept, "cache-control"), Some(IMMUTABLE_GAME));
        let art_url = format!("/v/{art_hash}/assets/processed/fighter.webp");
        let art = http(&server, "GET", &art_url, &auth, "").await;
        assert_eq!(header_value(&art, "cache-control"), Some(IMMUTABLE_GAME));
        assert_eq!(header_value(&art, "content-type"), Some("image/webp"));
        for path in [
            "/pkg/game.js".to_owned(),
            format!("/v/{art_hash}/pkg/game.js"),
        ] {
            let plain = http(&server, "GET", &path, &auth, "").await;
            assert!(plain.starts_with("HTTP/1.1 200"), "{path}");
            assert_eq!(header_value(&plain, "cache-control"), Some(REVALIDATE_GAME));
        }
        // Versioned URLs still need a login, and only game files have them.
        let anonymous = http(&server, "GET", &format!("/v/{js}/pkg/game.js"), "", "").await;
        assert!(anonymous.starts_with("HTTP/1.1 401"));
        for path in [
            format!("/v/{js}/main.js"),
            format!("/v/{js}/index.html"),
            "/v/XYZ/pkg/game.js".to_owned(),
            "/v//pkg/game.js".to_owned(),
        ] {
            let refused = http(&server, "GET", &path, &auth, "").await;
            assert!(refused.starts_with("HTTP/1.1 404"), "{path}");
        }

        // An unchanged file revalidates with a bodiless 304 that costs no egress.
        let etag = header_value(&kept, "etag").unwrap().to_owned();
        let used = lock(&server.state.egress).total.used(0);
        let same = format!("{auth}If-None-Match: W/{etag}\r\n");
        let unchanged = http(&server, "GET", "/pkg/game.js", &same, "").await;
        assert!(unchanged.starts_with("HTTP/1.1 304"), "{unchanged}");
        assert!(unchanged.ends_with("\r\n\r\n") && unchanged.contains(&etag));
        assert_eq!(
            header_value(&unchanged, "cache-control"),
            Some(REVALIDATE_GAME)
        );
        assert_eq!(lock(&server.state.egress).total.used(0), used);
        let star = http(&server, "GET", "/main.js", "If-None-Match: *\r\n", "").await;
        assert!(star.starts_with("HTTP/1.1 304"));

        // An update changes only the changed file's entry: the browser keeps the rest.
        std::fs::write(root.join("web/pkg/game.js"), b"export const v = 2;").unwrap();
        let updated = manifest(&server, &auth).await;
        assert_ne!(updated["pkg/game.js"], files["pkg/game.js"]);
        assert_eq!(updated["pkg/game.wasm"], files["pkg/game.wasm"]);
        assert_eq!(
            updated["assets/processed/fighter.webp"],
            files["assets/processed/fighter.webp"]
        );
        let art = http(&server, "GET", &art_url, &auth, "").await;
        assert_eq!(header_value(&art, "cache-control"), Some(IMMUTABLE_GAME));
        let stale = http(&server, "GET", &format!("/v/{js}/pkg/game.js"), &auth, "").await;
        assert!(stale.ends_with("export const v = 2;"));
        assert_eq!(header_value(&stale, "cache-control"), Some(REVALIDATE_GAME));
        let changed = http(&server, "GET", "/pkg/game.js", &same, "").await;
        assert!(changed.starts_with("HTTP/1.1 200") && changed.ends_with("export const v = 2;"));
        let new = updated["pkg/game.js"].as_str().unwrap();
        let fresh = http(&server, "GET", &format!("/v/{new}/pkg/game.js"), &auth, "").await;
        assert_eq!(header_value(&fresh, "cache-control"), Some(IMMUTABLE_GAME));

        // The gzip copy of the WASM carries its own ETag.
        std::fs::write(
            root.join("web/pkg/game.wasm.gz"),
            gzip_stored(b"wasm-test-data"),
        )
        .unwrap();
        let plain = http(&server, "GET", "/pkg/game.wasm", &auth, "").await;
        let gzip = format!("{auth}Accept-Encoding: gzip\r\n");
        let packed = http_bytes(&server, "GET", "/pkg/game.wasm", &gzip, "").await;
        let packed = String::from_utf8_lossy(&packed);
        let (a, b) = (header_value(&plain, "etag"), header_value(&packed, "etag"));
        assert!(a.is_some() && b.is_some() && a != b, "{a:?} {b:?}");
        // Refusals, API answers and the page itself are never stored.
        assert_eq!(header_value(&anonymous, "cache-control"), Some(NO_STORE));
        let document = http(&server, "GET", "/", "", "").await;
        assert_eq!(header_value(&document, "cache-control"), Some(NO_STORE));
        let config = http(&server, "GET", "/api/config", &auth, "").await;
        assert_eq!(header_value(&config, "cache-control"), Some(NO_STORE));
    }

    #[test]
    fn version_prefixes_are_lowercase_hex_segments() {
        assert_eq!(
            split_version("/v/0af9/pkg/a.js"),
            (Some("0af9"), "/pkg/a.js")
        );
        assert_eq!(split_version("/pkg/a.js"), (None, "/pkg/a.js"));
        for path in [
            "/v/0AF9/pkg/a.js",
            "/v/zz/pkg/a.js",
            "/v//pkg/a.js",
            "/v/0af9",
        ] {
            assert_eq!(split_version(path), (None, path), "{path}");
        }
        let long = format!("/v/{}/pkg/a.js", "a".repeat(65));
        assert_eq!(split_version(&long).0, None);
    }

    #[tokio::test]
    async fn raw_http_network_js_head_range_methods_and_metered_files() {
        let server = http_server_with(|mut config| {
            config.egress_limit = 16 * 1024;
            config
        })
        .await;
        let page = http(&server, "GET", "/network.js", "", "").await;
        assert!(page.starts_with("HTTP/1.1 200"), "{page}");
        assert!(page.contains("content-type: text/javascript; charset=utf-8"));
        assert!(page.contains("cache-control: no-cache") && page.contains("etag: \""));
        assert!(page.ends_with("export {};"));
        let head = http(&server, "HEAD", "/main.js", "", "").await;
        assert!(head.starts_with("HTTP/1.1 200") && head.contains("content-length: 10"));
        assert!(!head.contains("export"), "HEAD has no body");
        let post = http(&server, "POST", "/index.html", "", "").await;
        assert!(post.starts_with("HTTP/1.1 405") && post.contains("allow: GET, HEAD"));
        assert!(
            http(&server, "HEAD", "/pkg/game.js", "", "")
                .await
                .starts_with("HTTP/1.1 401")
        );
        let cookie = login(&server).await;
        let headers = format!("Cookie: {cookie}\r\nRange: bytes=0-1\r\n");
        let ranged = http(&server, "GET", "/pkg/game.js", &headers, "").await;
        assert!(ranged.starts_with("HTTP/1.1 200") && ranged.contains("accept-ranges: none"));
        assert!(ranged.ends_with("export {};"), "full body, Range ignored");
        // Public share is 16 KiB / 16 = 1 KiB: anonymous page loads run out first.
        let used = 10;
        let index_len = b"<!doctype html>ok".len();
        for _ in 0..(1024 - used) / index_len {
            assert!(
                http(&server, "GET", "/", "", "")
                    .await
                    .starts_with("HTTP/1.1 200")
            );
        }
        assert!(
            http(&server, "GET", "/", "", "")
                .await
                .starts_with("HTTP/1.1 503")
        );
        assert!(
            http(&server, "GET", "/", &format!("Cookie: {cookie}\r\n"), "")
                .await
                .starts_with("HTTP/1.1 200"),
            "logged-in players draw on the remaining shared budget"
        );
        let used = lock(&server.state.egress).total.used(0);
        assert!(used > 1000 && used < 16 * 1024);
    }

    #[tokio::test]
    async fn raw_ws_upgrade_rate_limit_and_egress_exhaustion_close_with_reason() {
        let server = http_server().await;
        let cookie = login(&server).await;
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        {
            let mut quotas = lock(&server.state.quotas);
            while quotas.upgrades.allow(ip, Instant::now()) {}
        }
        let mut stream = TcpStream::connect(server.addr).await.unwrap();
        let request = format!(
            "GET /api/ws HTTP/1.1\r\nHost: {}\r\nOrigin: {}\r\nCookie: {cookie}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
            server.addr, server.origin
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = [0; 12];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"HTTP/1.1 429");

        let server = http_server().await;
        let cookie = login(&server).await;
        let mut socket = ws_connect(&server, &cookie).await;
        {
            let mut egress = lock(&server.state.egress);
            let left = egress.total.limit - egress.total.used(0);
            egress.total.commit(left, Instant::now());
        }
        ws_send(
            &mut socket,
            &ClientMessage::Create {
                version: PROTOCOL_VERSION,
                name: None,
            },
        )
        .await;
        assert!(matches!(
            ws_event(&mut socket).await,
            ServerMessage::Error { message } if message == BUDGET_EXHAUSTED
        ));
    }
}
