#[cfg(not(target_arch = "wasm32"))]
use bevy::input::{
    ButtonState,
    keyboard::{Key, KeyboardInput},
};
use bevy::{
    app::AppExit,
    asset::{AssetMetaCheck, UntypedAssetLoadFailedEvent},
    camera::ScalingMode,
    ecs::system::SystemParam,
    input::{
        InputSystems,
        touch::{TouchInput, TouchPhase},
    },
    prelude::*,
    sprite::Anchor,
    text::FontCx,
    window::{PrimaryWindow, WindowResolution},
};
use naifrog_duel::{
    ai::Cpu,
    combat::{Arena, Hit, INTRO_TICKS, Pose, SHOT_BOTTOM, SHOT_TOP, Shot, UNIT},
    controls::{DASH, GUARD, HEAVY, InputMux, JUMP, LEFT, LIGHT, RIGHT, SHOT, Source},
    i18n::{Lang, em_width},
    ui::{self, Layout, Mode},
};
#[cfg(not(target_arch = "wasm32"))]
use naifrog_duel::{
    client::{Client, Server, ServerProblem},
    net::valid_name,
    ui::{MenuItem, OnlineState},
};
use parley::fontique::{FallbackKey, Script};
use std::{collections::BTreeMap, path::Path};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;
const FLOOR: f32 = -180.0;
/// Height of the identity tag above the feet, in world units.
const TAG_Y: f32 = 282.0;
const TAG_FONT: f32 = 20.0;
/// Noto Sans SC Medium cut down to the Han characters a name may use (`net::valid_name`) and
/// the punctuation of the Chinese texts (`i18n::FONT_EXTRA`); written by
/// scripts/prepare-name-font.mjs.
const NAME_FONT: &str = "processed/name_font.otf";
const NAMES: [&str; 12] = [
    "idle",
    "walk",
    "jump",
    "guard",
    "hurt",
    "knockdown",
    "getup",
    "light",
    "heavy",
    "dash",
    "shot",
    "air",
];
const GOLD: Color = Color::srgb(1.0, 0.76, 0.18);
const BLUE: Color = Color::srgb(0.30, 0.78, 1.0);
/// Fixed by seat, never by who "you" are: P1 gold, P2 blue.
fn seat_color(seat: usize) -> Color {
    if seat == 0 { GOLD } else { BLUE }
}
fn pressed_color(seat: usize) -> Color {
    if seat == 0 {
        Color::srgba(0.52, 0.37, 0.04, 0.98)
    } else {
        Color::srgba(0.06, 0.36, 0.48, 0.98)
    }
}
#[cfg(not(target_arch = "wasm32"))]
const KEY_IDLE: Color = Color::srgba(0.13, 0.18, 0.24, 0.92);
/// On-screen touch controls are see-through so the fight stays visible under them.
const BUTTON_IDLE: Color = Color::srgba(0.04, 0.07, 0.11, 0.30);
fn faded(c: Color, alpha: f32) -> Color {
    c.with_alpha(alpha)
}
/// The left mouse button steers the on-screen controls like one more finger.
const MOUSE_POINTER: u64 = u64::MAX;
const BINDINGS: [[KeyCode; 8]; 2] = [
    [
        KeyCode::KeyA,
        KeyCode::KeyD,
        KeyCode::KeyW,
        KeyCode::KeyS,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyT,
    ],
    [
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::KeyL,
        KeyCode::KeyI,
    ],
];
#[derive(Resource)]
struct Art {
    poses: BTreeMap<&'static str, Handle<Image>>,
    layout: Handle<TextureAtlasLayout>,
    stage: Handle<Image>,
    hit: Handle<Image>,
    shadow: Handle<Image>,
    /// Milk bubble: frames 0-3 fly, 4-7 burst.
    bubble: Handle<Image>,
    /// HUD portrait beside each name plate.
    portrait: Handle<Image>,
}
impl Art {
    fn loaded(&self, a: &AssetServer) -> bool {
        self.poses
            .values()
            .chain([
                &self.stage,
                &self.hit,
                &self.shadow,
                &self.bubble,
                &self.portrait,
            ])
            .all(|h| a.is_loaded_with_dependencies(h))
    }
}
/// The font Han text and Chinese punctuation fall back to; see `name_font_fallback`.
#[derive(Resource)]
struct NameFont {
    handle: Handle<Font>,
    /// Registered as the fallback, or failed to load: either way the game may start.
    settled: bool,
}
/// The language of every canvas text: the page's in the browser; on the desktop `--lang`,
/// `NAIFROG_LANG` or the system's, and F2 switches. Texts are rebuilt from it every frame.
#[derive(Resource, Clone, Copy, Default)]
struct Locale(Lang);
/// Local input slots. Which seat a slot drives depends on the mode (`Mode::slot_seat`).
#[derive(Resource, Default)]
struct Devices {
    focused: bool,
    slots: [InputMux; 2],
    pads: [Option<Entity>; 2],
    /// Which slot and button each finger (or the mouse) started on.
    owners: BTreeMap<u64, (usize, u16)>,
    /// The floating stick a finger is steering, per slot.
    sticks: [Option<Stick>; 2],
    /// Show and hit-test the on-screen controls: from the start in a touch-capable browser,
    /// otherwise (desktop mouse, native build) only after the first touch.
    touch_ui: bool,
}
/// A floating stick in window pixels: it appears where the finger lands and its center
/// follows the finger past the rim.
#[derive(Clone, Copy)]
struct Stick {
    id: u64,
    center: Vec2,
    knob: Vec2,
}
impl Devices {
    /// Drop every held control and touch owner. The next focused frame re-baselines
    /// all devices, so keys held across a switch cannot fire in the new mode.
    fn release(&mut self) {
        for s in &mut self.slots {
            s.cancel_all();
        }
        self.owners.clear();
        self.sticks = [None; 2];
        self.focused = false;
    }
}
fn touch_capable() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        page_touch_capable()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}
#[derive(Resource, Default)]
struct Remote {
    active: bool,
    player: Option<usize>,
    connected: [bool; 2],
    /// Display names by seat; empty until a snapshot arrives.
    names: [String; 2],
    received: f64,
    hits: Vec<Hit>,
    seq: Option<u64>,
    room: Option<String>,
    transport: u32,
    round: Option<u32>,
    /// Seconds of resume grace left per seat; `None` while that seat is connected.
    grace: [Option<u16>; 2],
    /// The connection's one-line state; the desktop shows it, the page shows its own.
    status: String,
}
/// Desktop online play: the server from `--server` or `NAIFROG_SERVER`, and the menu's
/// typed fields. The key stays in memory and is drawn masked.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
struct Desktop {
    client: Option<Client>,
    /// Why the given server address was refused.
    problem: Option<ServerProblem>,
    key: String,
    code: String,
}
#[cfg(not(target_arch = "wasm32"))]
impl Desktop {
    fn new(address: Option<String>, name: Option<String>, lang: Lang) -> Self {
        match address.map(|a| Server::parse(&a)) {
            Some(Ok(server)) => Self {
                client: Some(Client::start(server, name.as_deref(), lang)),
                ..default()
            },
            Some(Err(problem)) => {
                eprintln!("--server: {}", problem.text(Lang::En));
                Self {
                    problem: Some(problem),
                    ..default()
                }
            }
            None => Self::default(),
        }
    }
    fn state(&self, net: &Remote) -> OnlineState {
        match &self.client {
            None => OnlineState::Unavailable,
            Some(_) if net.player.is_some() => OnlineState::InRoom,
            Some(c) if c.logged_in() => OnlineState::LoggedIn,
            Some(_) => OnlineState::LoggedOut,
        }
    }
}
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = __naifrogNetPoll)]
    fn net_poll() -> String;
    #[wasm_bindgen(js_namespace = window, js_name = __naifrogNetSend)]
    fn net_send(held: u16, pressed: u16);
    #[wasm_bindgen(js_namespace = window, js_name = __naifrogNetRestart)]
    fn net_restart();
    #[wasm_bindgen(js_namespace = window, js_name = __naifrogGameReady)]
    fn game_ready();
    #[wasm_bindgen(js_namespace = window, js_name = __naifrogLocalPoll)]
    fn local_poll() -> u8;
    #[wasm_bindgen(js_namespace = window, js_name = __naifrogLocalChanged)]
    fn local_changed(mode: u8, paused: bool);
    #[wasm_bindgen(js_namespace = window, js_name = __naifrogTouchCapable)]
    fn page_touch_capable() -> bool;
    #[wasm_bindgen(js_namespace = window, js_name = __naifrogTouchLayout)]
    fn touch_layout(json: &str);
    #[wasm_bindgen(js_namespace = window, js_name = __naifrogSafeArea)]
    fn safe_area() -> String;
    /// "zh" or "en": the language the page shows (web/main.js).
    #[wasm_bindgen(js_namespace = window, js_name = __naifrogLang)]
    fn page_lang() -> String;
}
/// The page checks the name field with the rule the server applies.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn valid_player_name(name: &str) -> Option<String> {
    naifrog_duel::net::valid_name(name)
}
#[derive(Resource)]
struct Fight {
    arena: Arena,
    previous: Before,
    generation: u64,
    shake: u8,
    shake_strength: f32,
}
/// Fighter and bubble positions before the latest tick, for render interpolation only.
#[derive(Clone, Copy)]
struct Before {
    fighters: [[i32; 2]; 2],
    shots: [Option<i32>; 2],
}
fn positions(arena: &Arena) -> Before {
    Before {
        fighters: arena.fighters.each_ref().map(|f| [f.x, f.y]),
        shots: arena.shots.map(|s| s.map(|s| s.x)),
    }
}
impl Default for Fight {
    fn default() -> Self {
        let arena = Arena::default();
        Self {
            previous: positions(&arena),
            arena,
            generation: 0,
            shake: 0,
            shake_strength: 0.0,
        }
    }
}
#[derive(Resource)]
struct Game {
    mode: Mode,
    /// Menu shown over the match. Local matches pause; online play never does.
    menu_open: bool,
    cursor: usize,
    boxes: bool,
    /// Debug start with P1 on the right, to check markers follow seats, not sides.
    swap_sides: bool,
    cpu: [Cpu; 2],
    /// A pad's A pressed after the result has shown long enough: its release starts the next round.
    rematch_armed: bool,
}
impl Game {
    fn layout(&self) -> Layout {
        if self.menu_open {
            Layout::None
        } else {
            self.mode.layout()
        }
    }
}
fn report(_game: &Game) {
    #[cfg(target_arch = "wasm32")]
    local_changed(_game.mode.code(), _game.menu_open);
}
/// Everything a mode switch, pause or rematch must keep consistent.
#[derive(SystemParam)]
struct Match<'w> {
    game: ResMut<'w, Game>,
    fight: ResMut<'w, Fight>,
    io: ResMut<'w, Devices>,
    time: Res<'w, Time>,
}
impl Match<'_> {
    fn start(&mut self, mode: Mode) {
        self.game.mode = mode;
        self.game.menu_open = mode == Mode::Menu;
        self.game.cursor = 0;
        self.io.release();
        self.rematch();
        report(&self.game);
    }
    fn rematch(&mut self) {
        let fight = &mut *self.fight;
        fight.arena.restart();
        if self.game.swap_sides {
            for f in &mut fight.arena.fighters {
                f.x = -f.x;
                f.facing = -f.facing;
            }
        }
        fight.previous = positions(&fight.arena);
        fight.generation += 1;
        fight.shake = 0;
        for s in &mut self.io.slots {
            s.restart();
        }
        let seed = (self.time.elapsed().as_nanos() as u32)
            ^ (fight.generation as u32).wrapping_mul(0x9e37_79b9);
        self.game.cpu = [Cpu::new(seed), Cpu::new(seed.rotate_left(13) ^ 0x5bd1_e995)];
    }
    /// Open or close the menu over a local match. Online play is not paused: the desktop
    /// opens its menu over it, and in the browser the page shows its own.
    fn pause(&mut self, open: bool) {
        let fixed = self.game.mode == Mode::Menu
            || (self.game.mode == Mode::Online && cfg!(target_arch = "wasm32"));
        if fixed || self.game.menu_open == open {
            return;
        }
        self.game.menu_open = open;
        self.game.cursor = 0;
        self.io.release();
        report(&self.game);
    }
}
#[cfg(target_arch = "wasm32")]
fn poll_remote(mut net: ResMut<Remote>, mut m: Match) {
    if let Ok(view) = serde_json::from_str(&net_poll()) {
        apply_view(view, &mut net, &mut m);
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn poll_remote(mut net: ResMut<Remote>, desk: Res<Desktop>, mut m: Match) {
    let Some(client) = &desk.client else {
        return;
    };
    let seated = net.player.is_some();
    apply_view(client.poll(), &mut net, &mut m);
    if !net.active {
        return;
    }
    // A seat closes the menu; an error or a closed room opens it so its line can be read.
    if client.take_attention() || (seated && net.player.is_none()) {
        m.pause(true);
    } else if !seated && net.player.is_some() {
        m.pause(false);
    }
}
/// Take the server's view of online play; the server owns the fight.
fn apply_view(view: naifrog_duel::net::RemoteView, net: &mut Remote, m: &mut Match) {
    net.status = view.status;
    if net.active != view.active
        || net.player != view.player
        || net.room != view.room
        || net.transport != view.transport
    {
        net.seq = None;
        net.round = None;
        net.hits.clear();
        net.grace = [None, None];
        net.names = [String::new(), String::new()];
        m.io.release();
    }
    net.active = view.active;
    net.player = view.player;
    net.room = view.room;
    net.transport = view.transport;
    if net.active != (m.game.mode == Mode::Online) {
        // Entering shows a fresh arena until the first snapshot; leaving returns to the menu.
        m.start(if net.active { Mode::Online } else { Mode::Menu });
    }
    if !net.active {
        return;
    }
    let Some(frame) = view.snapshot else {
        return;
    };
    if net.seq.is_some_and(|seq| frame.seq <= seq) {
        return;
    }
    let fight = &mut *m.fight;
    // A paired restart bumps the room counter; the match's own round advance bumps
    // `game`. Both reset every seat and the interpolation baseline.
    let new_round = net.round != Some(frame.round) || fight.arena.game != frame.arena.game;
    if new_round {
        fight.generation += 1;
        for s in &mut m.io.slots {
            s.restart();
        }
        net.hits.clear();
    }
    fight.previous = positions(if new_round {
        &frame.arena
    } else {
        &fight.arena
    });
    fight.arena = frame.arena;
    net.seq = Some(frame.seq);
    net.round = Some(frame.round);
    net.connected = frame.connected;
    net.grace = frame.grace_left;
    net.names = frame.names;
    net.received = m.time.elapsed_secs_f64();
    net.hits.extend(frame.hits.into_iter().take(32));
    if net.hits.len() > 32 {
        let n = net.hits.len() - 32;
        net.hits.drain(..n);
    }
}
/// Requests from the DOM menu: 0 open menu, 1 new CPU match, 2 new local match, 3 resume.
#[cfg(target_arch = "wasm32")]
fn poll_local(net: Res<Remote>, mut m: Match) {
    let request = local_poll();
    if net.active {
        // The page leaves the room before asking for a local mode.
        return;
    }
    match request {
        0 => m.pause(true),
        1 => m.start(Mode::Cpu),
        2 => m.start(Mode::Local),
        3 => m.pause(false),
        _ => {}
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn poll_local() {}
#[derive(Resource, Default)]
struct Loading {
    ready: bool,
}
/// Safe-area insets (notch, rounded corners, gesture bar) in fractions of the window
/// height, polled from the page. Zero on the desktop and wherever the page reports none;
/// `ui::Insets::new` clamps whatever arrives.
#[derive(Resource, Clone, Copy, Default)]
struct SafeArea {
    left: f32,
    right: f32,
    bottom: f32,
}
impl SafeArea {
    fn insets(self) -> naifrog_duel::ui::Insets {
        naifrog_duel::ui::Insets::new(self.left, self.right, self.bottom)
    }
}
/// The page reports "left,right,bottom,viewport-height" in CSS pixels every frame; fractions
/// of the height are what the layout wants, so the device pixel ratio cancels out.
#[cfg(target_arch = "wasm32")]
fn poll_safe_area(mut safe: ResMut<SafeArea>) {
    let values: Vec<f32> = safe_area()
        .split(',')
        .filter_map(|t| t.parse().ok())
        .collect();
    if let [left, right, bottom, height] = values[..]
        && height > 0.0
    {
        (safe.left, safe.right, safe.bottom) = (left / height, right / height, bottom / height);
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn poll_safe_area() {}
#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
struct Smoke {
    path: Option<String>,
    frames: u32,
    ready: u32,
    requested: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Body,
    Shadow,
    Ring,
    Tag,
}
/// A world entity drawn at a fighter's position. Bound to the seat index, so it
/// stays on the same fighter through jumps, knockdowns, rounds and reconnects.
#[derive(Component)]
struct Follow {
    seat: usize,
    part: Part,
}
#[derive(Component)]
struct TagText(usize);
/// Background plates of the head tag; `inset` shrinks the inner plate.
#[derive(Component)]
struct TagPlate {
    inset: f32,
}
#[derive(Component)]
struct Health(usize);
/// The red damage trail behind a health bar; `hp` lags the real value.
#[derive(Component)]
struct Trail {
    seat: usize,
    hp: f32,
}
/// Fills the window with the stage art at any aspect ratio, ground line fixed.
#[derive(Component)]
struct StageArt;
#[derive(Component)]
enum Label {
    Clock,
    Status,
    /// The seat's display name: the server-supplied name online, `P1`/`P2` otherwise.
    Name(usize),
    /// The small `YOU` / `CPU` / `RIVAL` (你 / 电脑 / 对手) annotation beside the name.
    Role(usize),
    Hp(usize),
    /// The combo counter on seat `i`'s side: hits `i` has landed into the rival's stun chain.
    Combo(usize),
    Controls,
    /// The centred round introduction / result announcement from `ui::banner`.
    Banner,
    /// The words on the on-screen rematch button.
    Rematch,
}
#[derive(Component, PartialEq, Eq)]
enum Shown {
    Rematch,
    /// The controls help: bottom of the screen, or under the status on touch screens.
    Hint,
    /// The big centred announcement; hidden while the fight runs.
    Banner,
    /// The versus screen that opens a match: both names, the fighters behind it.
    Versus,
}
/// One round-win marker under a seat's health bar; lit once that round is won.
#[derive(Component)]
struct WinDot {
    seat: usize,
    index: u8,
}
const HUD_SHADOW: TextShadow = TextShadow {
    offset: Vec2::new(1.5, 2.0),
    color: Color::srgba(0.0, 0.0, 0.0, 0.75),
};
/// One round on-screen button: `ui::buttons(layout, aspect)[index]`.
#[derive(Component)]
struct VirtualKey {
    layout: Layout,
    index: usize,
}
/// The ring (or, with `knob`, the thumb) of a slot's floating stick.
#[derive(Component)]
struct StickPart {
    layout: Layout,
    slot: usize,
    knob: bool,
}
#[derive(Component)]
struct KeyText {
    layout: Layout,
    slot: usize,
    bit: u16,
}
#[derive(Component)]
struct Spark {
    born: u32,
    generation: u64,
    blocked: bool,
    /// A bubble burst (`art.bubble` frames 4-7) instead of the body-hit flash.
    shot: bool,
}
/// The sprite of one seat's milk bubble; hidden while that seat has none.
#[derive(Component)]
struct Bubble {
    seat: usize,
}
#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
enum MenuNode {
    Panel,
    Row(usize),
}
#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
enum MenuText {
    Title,
    Current,
    Label(usize),
    Hint(usize),
    Help(usize),
    Server,
}
#[cfg(target_arch = "wasm32")]
fn asset_path() -> String {
    "assets".into()
}
#[cfg(not(target_arch = "wasm32"))]
fn asset_path() -> String {
    let current = std::env::current_dir().unwrap_or_default().join("assets");
    let mut candidates = vec![current];
    // Locate development assets at runtime too: embedding CARGO_MANIFEST_DIR
    // would disclose the build machine's absolute project path in the binary.
    if let Ok(executable) = std::env::current_exe() {
        candidates.extend(
            executable
                .ancestors()
                .skip(1)
                .take(4)
                .map(|directory| directory.join("assets")),
        );
    }
    candidates
        .into_iter()
        .find(|p| p.join("processed/game-assets.json").is_file())
        .unwrap_or_else(|| std::path::PathBuf::from("assets"))
        .to_string_lossy()
        .into_owned()
}
/// `--mode menu|cpu|local|demo` (aliases `--ai`, `--local`, `--demo`). Online play is
/// chosen in the menu: the desktop's needs `--server`, the browser's a login on the page.
fn start_mode(args: &[String], smoke: bool) -> Result<Mode, String> {
    let named = args
        .iter()
        .position(|a| a == "--mode")
        .map(|i| args.get(i + 1).map(String::as_str).unwrap_or(""));
    let flag = |f: &str| args.iter().any(|a| a == f);
    Ok(match named {
        Some("menu") => Mode::Menu,
        Some("cpu") => Mode::Cpu,
        Some("local") => Mode::Local,
        Some("demo") => Mode::Demo,
        Some(other) => {
            return Err(format!(
                "unknown --mode {other:?}; use menu, cpu, local or demo"
            ));
        }
        None if flag("--ai") => Mode::Cpu,
        None if flag("--local") => Mode::Local,
        None if flag("--demo") || smoke => Mode::Demo,
        None => Mode::Menu,
    })
}
/// `--server http://IP:PORT`, else `NAIFROG_SERVER`: the naifrog-server the browser uses.
#[cfg(not(target_arch = "wasm32"))]
fn server_address(args: &[String]) -> Option<String> {
    match args.iter().position(|a| a == "--server") {
        Some(i) => Some(args.get(i + 1).cloned().unwrap_or_default()),
        None => std::env::var("NAIFROG_SERVER")
            .ok()
            .filter(|s| !s.trim().is_empty()),
    }
}
/// `--lang zh|en`, else `NAIFROG_LANG`, else the system locale: Chinese for any `zh` tag
/// (`zh-CN`, `zh_TW.UTF-8`), English for everything else.
#[cfg(not(target_arch = "wasm32"))]
fn start_lang(args: &[String]) -> Result<Lang, String> {
    if let Some(i) = args.iter().position(|a| a == "--lang") {
        let tag = args.get(i + 1).map(String::as_str).unwrap_or("");
        return Lang::from_tag(tag).ok_or_else(|| format!("unknown --lang {tag:?}; use zh or en"));
    }
    Ok(std::env::var("NAIFROG_LANG")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(sys_locale::get_locale)
        .and_then(|tag| Lang::from_tag(&tag))
        .unwrap_or(Lang::En))
}
#[cfg(target_arch = "wasm32")]
fn start_lang(_args: &[String]) -> Result<Lang, String> {
    Ok(Lang::from_tag(&page_lang()).unwrap_or(Lang::En))
}
/// `--name ALICE` or `--name 奶蛙`: the display name sent with Create/Join. Anything the
/// canvas cannot show (see `net::valid_name`) is dropped with a note, and the seat default
/// `P1`/`P2` is used instead.
#[cfg(not(target_arch = "wasm32"))]
fn player_name(args: &[String]) -> Option<String> {
    let typed = args
        .iter()
        .position(|a| a == "--name")
        .and_then(|i| args.get(i + 1).cloned())?;
    match valid_name(&typed) {
        Some(name) => Some(name),
        None => {
            eprintln!(
                "--name: {typed:?} cannot be shown (A-Z a-z 0-9 space _ - and common Han characters, at most 10 letters or 6 Han wide); the seat default is used"
            );
            None
        }
    }
}
fn main() -> AppExit {
    #[cfg(not(target_arch = "wasm32"))]
    let args: Vec<String> = std::env::args().collect();
    #[cfg(target_arch = "wasm32")]
    let args: Vec<String> = vec![];
    let smoke_path = args.iter().position(|a| a == "--smoke").map(|i| {
        args.get(i + 1)
            .filter(|p| !p.starts_with("--"))
            .cloned()
            .unwrap_or_else(|| "artifacts/smoke.png".into())
    });
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(p) = &smoke_path
        && let Some(parent) = std::path::Path::new(p).parent()
    {
        std::fs::create_dir_all(parent).expect("create smoke directory");
    }
    let (mode, lang) = match start_mode(&args, smoke_path.is_some())
        .and_then(|mode| Ok((mode, start_lang(&args)?)))
    {
        Ok(started) => started,
        Err(message) => {
            eprintln!("{message}");
            return AppExit::error();
        }
    };
    // `--size 1920x912` opens another window shape, e.g. to check the HUD in a smoke shot.
    let size = args
        .iter()
        .position(|a| a == "--size")
        .and_then(|i| args.get(i + 1)?.split_once('x'))
        .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
        .unwrap_or((1280, 720));
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: asset_path(),
                // No .meta files exist; skipping the probe avoids a 404 per texture.
                meta_check: AssetMetaCheck::Never,
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: ui::window_title(lang).into(),
                    resolution: WindowResolution::new(size.0, size.1),
                    canvas: Some("#game".into()),
                    fit_canvas_to_parent: true,
                    prevent_default_event_handling: true,
                    ..default()
                }),
                ..default()
            }),
    )
    .insert_resource(ClearColor(Color::srgb(0.035, 0.055, 0.08)))
    .insert_resource(Time::<Fixed>::from_hz(60.0))
    .insert_resource(Game {
        mode,
        menu_open: mode == Mode::Menu,
        cursor: 0,
        boxes: false,
        swap_sides: args.iter().any(|a| a == "--swap-sides"),
        cpu: [Cpu::new(1), Cpu::new(2)],
        rematch_armed: false,
    })
    .init_resource::<Fight>()
    .init_resource::<Remote>()
    .insert_resource(Devices {
        touch_ui: touch_capable(),
        ..default()
    })
    .init_resource::<Loading>()
    .insert_resource(Locale(lang))
    .init_resource::<SafeArea>()
    .add_systems(Startup, setup)
    .add_systems(
        PreUpdate,
        (
            language,
            load_ready,
            poll_remote,
            poll_safe_area,
            poll_local,
            hotkeys,
            capture_input,
        )
            .chain()
            .after(InputSystems),
    )
    .add_systems(FixedUpdate, simulate)
    .add_systems(
        Update,
        (
            render_fighters,
            render_bubbles,
            render_tags,
            render_sparks,
            render_hud,
            render_trails,
            fit_stage,
            debug_boxes,
            name_font_fallback,
            asset_errors,
            smoke_capture,
        ),
    );
    #[cfg(not(target_arch = "wasm32"))]
    app.insert_resource(Smoke {
        path: smoke_path,
        ..default()
    })
    .insert_resource(Desktop::new(
        server_address(&args),
        player_name(&args),
        lang,
    ))
    .add_systems(Startup, setup_menu.after(setup))
    .add_systems(Update, render_menu);
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, report_touch);
    app.run()
}
fn text(s: impl Into<String>, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (
        Text::new(s),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}
fn absolute(r: ui::Region) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: percent(r.left * 100.0),
        top: percent(r.top * 100.0),
        width: percent(r.width * 100.0),
        height: percent(r.height * 100.0),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        border_radius: BorderRadius::all(px(8)),
        ..default()
    }
}
fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: 1280.0,
                min_height: 720.0,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::splat(512),
        4,
        2,
        None,
        None,
    ));
    let poses: BTreeMap<_, _> = NAMES
        .into_iter()
        .map(|n| (n, server.load(format!("processed/fighter_{n}.webp"))))
        .collect();
    let stage = server.load("processed/stage_garden.webp");
    let hit = server.load("processed/fx_hit.webp");
    let shadow = server.load("processed/shadow.webp");
    let bubble: Handle<Image> = server.load("processed/fx_shot.webp");
    let portrait = server.load("processed/portrait.webp");
    commands.insert_resource(NameFont {
        handle: server.load(NAME_FONT),
        settled: false,
    });
    for seat in 0..2 {
        commands.spawn((
            Sprite::from_atlas_image(
                bubble.clone(),
                TextureAtlas {
                    layout: layout.clone(),
                    index: 0,
                },
            ),
            Transform::from_xyz(0.0, FLOOR, 2.5).with_scale(Vec3::splat(BUBBLE_SCALE)),
            Visibility::Hidden,
            Bubble { seat },
        ));
    }
    commands.spawn((
        Sprite {
            image: stage.clone(),
            custom_size: Some(Vec2::new(1280.0, 720.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, -5.0),
        StageArt,
    ));
    let ring = meshes.add(Annulus::new(52.0, 64.0));
    let pointer = meshes.add(Triangle2d::new(
        Vec2::new(-10.0, 0.0),
        Vec2::new(10.0, 0.0),
        Vec2::new(0.0, -13.0),
    ));
    for i in 0..2 {
        let x = if i == 0 { -235.0 } else { 235.0 };
        let paint = materials.add(seat_color(i));
        commands.spawn((
            Sprite {
                image: shadow.clone(),
                custom_size: Some(Vec2::new(170.0, 42.0)),
                ..default()
            },
            Transform::from_xyz(x, FLOOR - 3.0, 0.1),
            Follow {
                seat: i,
                part: Part::Shadow,
            },
        ));
        // Seat-colored ring on the floor, flattened into an ellipse around the feet.
        commands.spawn((
            Mesh2d(ring.clone()),
            MeshMaterial2d(paint.clone()),
            Transform::from_xyz(x, FLOOR - 4.0, 0.2).with_scale(Vec3::new(1.0, 0.3, 1.0)),
            Follow {
                seat: i,
                part: Part::Ring,
            },
        ));
        let mut sprite = Sprite::from_atlas_image(
            poses["idle"].clone(),
            TextureAtlas {
                layout: layout.clone(),
                index: 0,
            },
        );
        sprite.flip_x = i == 1;
        commands.spawn((
            sprite,
            Anchor(Vec2::new(0.0, -0.3984375)),
            Transform::from_xyz(x, FLOOR, 1.0).with_scale(Vec3::splat(0.74)),
            Follow {
                seat: i,
                part: Part::Body,
            },
        ));
        // Head tag: seat-colored border, dark plate, label and a pointer down to the head.
        commands
            .spawn((
                Sprite::from_color(seat_color(i), Vec2::new(60.0, 34.0)),
                Transform::from_xyz(x, FLOOR + TAG_Y, 3.0),
                Follow {
                    seat: i,
                    part: Part::Tag,
                },
                TagPlate { inset: 0.0 },
            ))
            .with_children(|tag| {
                tag.spawn((
                    Sprite::from_color(
                        Color::srgba(0.02, 0.035, 0.06, 0.92),
                        Vec2::new(56.0, 30.0),
                    ),
                    Transform::from_xyz(0.0, 0.0, 0.01),
                    TagPlate { inset: 4.0 },
                ));
                tag.spawn((
                    Text2d::new(format!("P{}", i + 1)),
                    TextFont {
                        font_size: FontSize::Px(TAG_FONT),
                        ..default()
                    },
                    TextColor(seat_color(i)),
                    Transform::from_xyz(0.0, 0.0, 0.02),
                    TagText(i),
                ));
                tag.spawn((
                    Mesh2d(pointer.clone()),
                    MeshMaterial2d(paint.clone()),
                    Transform::from_xyz(0.0, -17.0, 0.0),
                ));
            });
    }
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|root| {
            // A soft shade keeps the HUD readable over the sky without a hard band.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(24),
                    ..default()
                },
                BackgroundGradient::from(LinearGradient::to_bottom(vec![
                    ColorStop::percent(Color::srgba(0.0, 0.02, 0.05, 0.6), 0),
                    ColorStop::percent(Color::srgba(0.0, 0.02, 0.05, 0.25), 55),
                    ColorStop::percent(Color::srgba(0.0, 0.02, 0.05, 0.0), 100),
                ])),
            ));
            let frame = Color::srgba(1.0, 1.0, 1.0, 0.82);
            let well = Color::srgba(0.02, 0.035, 0.06, 0.88);
            for i in 0..2 {
                let (x, outer) = if i == 0 {
                    (0.03, FlexDirection::Row)
                } else {
                    (0.56, FlexDirection::RowReverse)
                };
                // Name at the outer edge, hit points toward the clock.
                root.spawn(Node {
                    flex_direction: outer,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::FlexEnd,
                    ..absolute(ui::Region::new(x, 0.008, 0.41, 0.044))
                })
                .with_children(|plate| {
                    // Portrait at the outer edge: the same figure both seats play, framed
                    // in the seat's color (the fighters themselves carry no seat tint).
                    plate
                        .spawn((
                            Node {
                                width: px(34),
                                height: px(34),
                                margin: UiRect::axes(px(8), px(0)),
                                border: UiRect::all(px(2)),
                                border_radius: BorderRadius::all(px(6)),
                                ..default()
                            },
                            BorderColor::all(seat_color(i)),
                            BackgroundColor(well),
                        ))
                        .with_child(ImageNode::new(portrait.clone()));
                    // Name next to the portrait with the role as a small annotation.
                    plate
                        .spawn(Node {
                            flex_direction: outer,
                            align_items: AlignItems::FlexEnd,
                            column_gap: px(6),
                            ..default()
                        })
                        .with_children(|seat| {
                            seat.spawn((text("", 21.0, seat_color(i)), HUD_SHADOW, Label::Name(i)));
                            seat.spawn((
                                text("", 13.0, Color::srgba(1.0, 1.0, 1.0, 0.78)),
                                HUD_SHADOW,
                                Label::Role(i),
                            ));
                        });
                    plate.spawn((text("100", 21.0, Color::WHITE), HUD_SHADOW, Label::Hp(i)));
                });
                // The bar drains toward the outer edge, so damage shows next to the clock;
                // a red trail behind it marks what the last hits took.
                let anchor = if i == 0 {
                    JustifyContent::FlexStart
                } else {
                    JustifyContent::FlexEnd
                };
                root.spawn((
                    Node {
                        justify_content: anchor,
                        align_items: AlignItems::Stretch,
                        border: UiRect::all(px(2)),
                        border_radius: BorderRadius::all(px(7)),
                        overflow: Overflow::clip(),
                        ..absolute(ui::Region::new(x, 0.056, 0.41, 0.042))
                    },
                    BorderColor::all(frame),
                    BackgroundColor(well),
                ))
                .with_children(|bar| {
                    // Pinned to the same outer edge as the fill.
                    let (left, right) = if i == 0 {
                        (px(0), Val::Auto)
                    } else {
                        (Val::Auto, px(0))
                    };
                    bar.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left,
                            right,
                            width: percent(100),
                            height: percent(100),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.93, 0.27, 0.22)),
                        Trail { seat: i, hp: 100.0 },
                    ));
                    bar.spawn((
                        Node {
                            width: percent(100),
                            height: percent(100),
                            ..default()
                        },
                        BackgroundColor(seat_color(i)),
                        Health(i),
                    ));
                });
                // Round-win markers: one dot per round won, under the bar's outer edge.
                let (left, right) = if i == 0 {
                    (px(0), Val::Auto)
                } else {
                    (Val::Auto, px(0))
                };
                root.spawn(Node {
                    position_type: PositionType::Absolute,
                    flex_direction: outer,
                    column_gap: px(5),
                    left,
                    right,
                    top: percent(10.8),
                    ..default()
                })
                .with_children(|dots| {
                    for index in 0..2u8 {
                        dots.spawn((
                            Node {
                                width: px(9),
                                height: px(9),
                                border_radius: BorderRadius::MAX,
                                ..default()
                            },
                            BackgroundColor(well),
                            WinDot { seat: i, index },
                        ));
                    }
                });
                // Combo counter under the dots, on the attacker's side: it counts the
                // hits this seat has landed into the rival's current stun chain.
                root.spawn(Node {
                    position_type: PositionType::Absolute,
                    left,
                    right,
                    top: percent(15.4),
                    ..default()
                })
                .with_child((
                    text("", 19.0, seat_color(i)),
                    HUD_SHADOW,
                    Label::Combo(i),
                ));
            }
            root.spawn((
                Node {
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(10)),
                    ..absolute(ui::Region::new(0.462, 0.008, 0.076, 0.098))
                },
                BorderColor::all(Color::srgba(1.0, 0.82, 0.35, 0.9)),
                BackgroundColor(well),
            ))
            .with_child((
                text("99", 38.0, Color::WHITE),
                TextLayout::justify(Justify::Center),
                Label::Clock,
            ));
            // One centred pill under the clock for the status line.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                top: percent(11.8),
                width: percent(100),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_child((
                Node {
                    padding: UiRect::axes(px(16), px(4)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::MAX,
                    max_width: percent(80),
                    ..default()
                },
                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.22)),
                BackgroundColor(Color::srgba(0.02, 0.035, 0.06, 0.78)),
                children![(
                    text("", 18.0, Color::WHITE),
                    TextLayout::justify(Justify::Center),
                    Label::Status,
                )],
            ));
            root.spawn((
                Node {
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::MAX,
                    ..absolute(ui::REMATCH)
                },
                BorderColor::all(Color::srgba(1.0, 0.82, 0.35, 0.85)),
                BackgroundColor(Color::srgba(0.06, 0.1, 0.16, 0.88)),
                Shown::Rematch,
            ))
            .with_child((text("", 16.0, Color::WHITE), Label::Rematch));
            // ROUND 1 / FIGHT! / K.O. — centred, clear of the rematch button above it.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: percent(27.0),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Shown::Banner,
            ))
            .with_child((
                text("", 46.0, Color::WHITE),
                HUD_SHADOW,
                TextLayout::justify(Justify::Center),
                Label::Banner,
            ));
            // The versus screen that opens a match: both names over the stage, the
            // fighters standing at their spawn positions behind them.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: percent(33.0),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: px(22),
                    ..default()
                },
                Shown::Versus,
            ))
            .with_children(|versus| {
                versus.spawn((text("", 34.0, seat_color(0)), HUD_SHADOW, Label::Name(0)));
                versus.spawn((text("VS", 22.0, Color::WHITE), HUD_SHADOW));
                versus.spawn((text("", 34.0, seat_color(1)), HUD_SHADOW, Label::Name(1)));
            });
            // Keyboard and pad help sits at the bottom; on touch screens the bottom belongs
            // to the sticks and buttons, so render_hud moves it under the status line.
            root.spawn((
                absolute(ui::Region::new(0.012, 0.958, 0.976, 0.03)),
                text("", 13.0, Color::srgba(1.0, 1.0, 1.0, 0.78)),
                HUD_SHADOW,
                TextLayout::justify(Justify::Center),
                Label::Controls,
                Shown::Hint,
            ));
            // Round see-through buttons and floating sticks; render_hud places them in pixels.
            let round = || Node {
                position_type: PositionType::Absolute,
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::MAX,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            };
            for layout in [Layout::Single, Layout::Double] {
                for (index, b) in ui::buttons(layout, 16.0 / 9.0, Default::default())
                    .into_iter()
                    .enumerate()
                {
                    root.spawn((
                        round(),
                        BackgroundColor(BUTTON_IDLE),
                        BorderColor::all(Color::NONE),
                        VirtualKey { layout, index },
                    ))
                    .with_child((
                        text("", 15.0, Color::WHITE),
                        // Keeps the label readable over bright parts of the stage.
                        TextShadow {
                            offset: Vec2::splat(1.5),
                            color: Color::srgba(0.0, 0.0, 0.0, 0.8),
                        },
                        KeyText {
                            layout,
                            slot: b.slot,
                            bit: b.bit,
                        },
                    ));
                }
                for zone in ui::sticks(layout, 16.0 / 9.0, Default::default()) {
                    for knob in [false, true] {
                        root.spawn((
                            round(),
                            BackgroundColor(Color::NONE),
                            BorderColor::all(Color::NONE),
                            StickPart {
                                layout,
                                slot: zone.slot,
                                knob,
                            },
                        ));
                    }
                }
            }
        });
    commands.insert_resource(Art {
        poses,
        layout,
        stage,
        hit,
        shadow,
        bubble,
        portrait,
    });
}
/// Native-only menu. The browser shows a DOM menu instead so Chinese text renders.
#[cfg(not(target_arch = "wasm32"))]
fn setup_menu(mut commands: Commands) {
    let dim = Color::srgba(1.0, 1.0, 1.0, 0.78);
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::FlexStart,
                    padding: UiRect::top(percent(1.2)),
                    border: UiRect::all(px(2)),
                    ..absolute(ui::MENU_PANEL)
                },
                BorderColor::all(Color::srgb(0.33, 0.42, 0.51)),
                BackgroundColor(Color::srgba(0.035, 0.075, 0.12, 0.96)),
                MenuNode::Panel,
            ))
            .with_children(|panel| {
                panel.spawn((text("", 34.0, GOLD), MenuText::Title));
                panel.spawn((text("", 14.0, dim), MenuText::Current));
            });
            for row in 0..ui::MENU_ROWS {
                root.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        ..absolute(ui::menu_region(row))
                    },
                    BackgroundColor(KEY_IDLE),
                    MenuNode::Row(row),
                ))
                .with_children(|item| {
                    item.spawn((text("", 22.0, Color::WHITE), MenuText::Label(row)));
                    item.spawn((text("", 12.0, dim), MenuText::Hint(row)));
                });
            }
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    ..absolute(ui::MENU_FOOTER)
                },
                BackgroundColor(Color::NONE),
                MenuNode::Row(usize::MAX),
            ))
            .with_children(|footer| {
                footer.spawn((text("", 12.0, dim), MenuText::Help(0)));
                footer.spawn((text("", 12.0, dim), MenuText::Help(1)));
                footer.spawn((text("", 12.0, dim), MenuText::Server));
            });
        });
}
#[cfg(not(target_arch = "wasm32"))]
fn render_menu(
    game: Res<Game>,
    net: Res<Remote>,
    desk: Res<Desktop>,
    locale: Res<Locale>,
    mut nodes: Query<(&MenuNode, &mut Node, &mut BackgroundColor)>,
    mut texts: Query<(&MenuText, &mut Text)>,
) {
    let lang = locale.0;
    let items = ui::menu_items(game.mode, desk.state(&net));
    let row_text = |row: usize| {
        items.get(row).map(|item| {
            let editing = row == game.cursor && item.typed();
            let key_len = desk.key.chars().count();
            ui::menu_row(*item, game.mode, lang, key_len, &desk.code, editing)
        })
    };
    let show = |on: bool| if on { Display::Flex } else { Display::None };
    for (part, mut node, mut bg) in &mut nodes {
        match part {
            MenuNode::Panel => node.display = show(game.menu_open),
            MenuNode::Row(row) => {
                node.display = show(game.menu_open && (*row == usize::MAX || *row < items.len()));
                if *row != usize::MAX {
                    bg.0 = if *row == game.cursor {
                        Color::srgba(0.18, 0.32, 0.45, 0.98)
                    } else {
                        KEY_IDLE
                    };
                }
            }
        }
    }
    for (part, mut text) in &mut texts {
        let value = match part {
            MenuText::Title => ui::menu_title(lang).into(),
            MenuText::Current if game.mode == Mode::Online => net.status.clone(),
            MenuText::Current => ui::menu_current(game.mode, lang),
            MenuText::Label(row) => row_text(*row).map(|t| t.0).unwrap_or_default(),
            MenuText::Hint(row) => row_text(*row).map(|t| t.1.into()).unwrap_or_default(),
            MenuText::Help(line) => ui::menu_help(lang)[*line].into(),
            MenuText::Server => ui::menu_server(
                lang,
                desk.client.as_ref().map(|c| c.server().origin()),
                desk.problem.as_ref().map(|p| p.text(lang)).as_deref(),
            ),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}
fn load_ready(
    art: Res<Art>,
    server: Res<AssetServer>,
    name_font: Res<NameFont>,
    mut loading: ResMut<Loading>,
    mut m: Match,
) {
    if !loading.ready && name_font.settled && art.loaded(&server) {
        loading.ready = true;
        // Starting the chosen mode here seeds the CPU and applies debug starts.
        let mode = m.game.mode;
        m.start(mode);
        info!("All 17 game textures ready; fixed simulation runs at 60 Hz.");
        #[cfg(target_arch = "wasm32")]
        game_ready();
    }
}
fn keyboard(keys: &ButtonInput<KeyCode>, bindings: [KeyCode; 8]) -> (u16, u16) {
    let mut held = 0;
    let mut press = 0;
    for (k, b) in bindings
        .into_iter()
        .zip([LEFT, RIGHT, JUMP, GUARD, LIGHT, HEAVY, DASH, SHOT])
    {
        if keys.pressed(k) {
            held |= b;
        }
        if keys.just_pressed(k) {
            press |= b;
        }
    }
    (held, press)
}
fn gamepad(p: &Gamepad) -> (u16, u16) {
    let mut held = 0;
    let mut pressed = 0;
    let stick = p.left_stick();
    if stick.x < -0.25 {
        held |= LEFT;
    }
    if stick.x > 0.25 {
        held |= RIGHT;
    }
    for (k, b) in [
        (GamepadButton::DPadLeft, LEFT),
        (GamepadButton::DPadRight, RIGHT),
        (GamepadButton::South, JUMP),
        (GamepadButton::West, LIGHT),
        (GamepadButton::North, HEAVY),
        (GamepadButton::East, DASH),
        (GamepadButton::LeftTrigger, GUARD),
        // RB throws the milk bubble; LB, LT and RT guard.
        (GamepadButton::RightTrigger, SHOT),
        (GamepadButton::LeftTrigger2, GUARD),
        (GamepadButton::RightTrigger2, GUARD),
    ] {
        if p.pressed(k) {
            held |= b;
        }
        if p.just_pressed(k) {
            pressed |= b;
        }
    }
    (held, pressed)
}
fn feed(slot: &mut InputMux, source: Source, (held, pressed): (u16, u16), baseline: bool) {
    if baseline {
        slot.baseline(source, held);
    } else {
        slot.sample_with_press(source, held, pressed);
    }
}
/// Raw device state. Each system gets its own touch reader.
#[derive(SystemParam)]
struct Raw<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    windows: Query<'w, 's, (Entity, &'static Window), With<PrimaryWindow>>,
    pads: Query<'w, 's, (Entity, &'static Gamepad)>,
    touches: MessageReader<'w, 's, TouchInput>,
}
impl Raw<'_, '_> {
    fn key(&self, k: KeyCode) -> bool {
        self.keys.just_pressed(k)
    }
    fn pad(&self, b: GamepadButton) -> bool {
        self.pads.iter().any(|(_, p)| p.just_pressed(b))
    }
    fn pad_released(&self, b: GamepadButton) -> bool {
        self.pads.iter().any(|(_, p)| p.just_released(b))
    }
}
/// Menu navigation, pause, rematch and debug toggles.
fn hotkeys(
    mut raw: Raw,
    net: Res<Remote>,
    loading: Res<Loading>,
    mut m: Match,
    mut exit: MessageWriter<AppExit>,
    #[cfg(not(target_arch = "wasm32"))] mut desk: ResMut<Desktop>,
    #[cfg(not(target_arch = "wasm32"))] mut typing: MessageReader<KeyboardInput>,
) {
    // Read every frame so a menu text field never sees stale keys.
    #[cfg(not(target_arch = "wasm32"))]
    let typed: Vec<Typed> = typing
        .read()
        .filter(|k| k.state == ButtonState::Pressed)
        .filter_map(|k| match (&k.logical_key, &k.text) {
            (Key::Backspace, _) => Some(Typed::Back),
            (_, Some(t)) => t
                .chars()
                .find(|c| c.is_ascii_graphic() || *c == ' ')
                .map(Typed::Char),
            _ => None,
        })
        .collect();
    let Ok((window_entity, window)) = raw.windows.single() else {
        return;
    };
    if !window.focused {
        raw.touches.clear();
        return;
    }
    let size = Vec2::new(window.width().max(1.0), window.height().max(1.0));
    let cursor = window.cursor_position().map(|p| p / size);
    let mut taps: Vec<Vec2> = raw
        .touches
        .read()
        .filter(|t| t.window == window_entity && t.phase == TouchPhase::Started)
        .map(|t| t.position / size)
        .collect();
    if raw.mouse.just_pressed(MouseButton::Left) {
        taps.extend(cursor);
    }
    if raw.key(KeyCode::F1) {
        m.game.boxes = !m.game.boxes;
    }
    if !loading.ready {
        return;
    }
    if m.game.menu_open {
        #[cfg(not(target_arch = "wasm32"))]
        navigate(&raw, &taps, &typed, &net, &mut desk, &mut m, &mut exit);
        // The page menu takes every device while it has focus (web/main.js polls pads); this
        // runs only if the canvas was clicked meanwhile: a pad's Select still closes the menu.
        #[cfg(target_arch = "wasm32")]
        if raw.pad(GamepadButton::Select) {
            m.pause(false);
        }
        return;
    }
    // In the browser the page handles Esc; Select pauses here and the page shows its menu.
    let escape = cfg!(not(target_arch = "wasm32")) && raw.key(KeyCode::Escape);
    if escape || raw.pad(GamepadButton::Select) {
        m.pause(true);
        return;
    }
    let _ = &mut exit;
    // Once the whole match is decided and its result has shown for a moment, a fresh A
    // restarts it. A mid-match round result ignores the pad's A — the auto-advance owns
    // the next round — while R, Start and the on-screen button keep their meaning. It fires
    // on release: A is jump, and a press still held at the restart would reach the new
    // round as a jump (input runs after this system in the same frame).
    let settled = ui::pad_rematch_ready(&m.fight.arena);
    if !settled {
        m.game.rematch_armed = false;
    } else if raw.pad(GamepadButton::South) {
        m.game.rematch_armed = true;
    }
    let rematch = raw.key(KeyCode::KeyR)
        || raw.pad(GamepadButton::Start)
        || (m.game.rematch_armed && raw.pad_released(GamepadButton::South))
        || taps.iter().any(|p| ui::REMATCH.contains(p.x, p.y));
    if !rematch {
        return;
    }
    m.game.rematch_armed = false;
    match m.game.mode {
        // The server restarts only when both seats vote for the same round.
        Mode::Online if net.player.is_some() => {
            #[cfg(target_arch = "wasm32")]
            net_restart();
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(client) = &desk.client {
                client.restart();
            }
        }
        Mode::Online => {}
        Mode::Menu => {}
        _ => m.rematch(),
    }
    let _ = &net;
}
/// A key typed into a menu text field.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
enum Typed {
    Char(char),
    Back,
}
#[cfg(not(target_arch = "wasm32"))]
fn navigate(
    raw: &Raw,
    taps: &[Vec2],
    typed: &[Typed],
    net: &Remote,
    desk: &mut Desktop,
    m: &mut Match,
    exit: &mut MessageWriter<AppExit>,
) {
    let online = desk.state(net);
    let items = ui::menu_items(m.game.mode, online);
    let n = items.len();
    m.game.cursor = m.game.cursor.min(n - 1);
    // On a text row letters, digits and space type; arrows, the pad and Enter still work.
    let field = items[m.game.cursor];
    if field.typed() {
        let (text, limit) = if field == MenuItem::Key {
            (&mut desk.key, 128)
        } else {
            (&mut desk.code, 12)
        };
        for t in typed {
            match *t {
                Typed::Back => {
                    text.pop();
                }
                Typed::Char(c) if text.len() < limit => text.push(c),
                Typed::Char(_) => {}
            }
        }
    }
    let letters = !field.typed();
    let up = raw.key(KeyCode::ArrowUp) || (letters && raw.key(KeyCode::KeyW));
    if up || raw.pad(GamepadButton::DPadUp) {
        m.game.cursor = (m.game.cursor + n - 1) % n;
    }
    let down = raw.key(KeyCode::ArrowDown) || (letters && raw.key(KeyCode::KeyS));
    if down || raw.pad(GamepadButton::DPadDown) {
        m.game.cursor = (m.game.cursor + 1) % n;
    }
    let mut chosen = None;
    if raw.key(KeyCode::Enter)
        || raw.key(KeyCode::NumpadEnter)
        || (letters && raw.key(KeyCode::Space))
        || raw.pad(GamepadButton::South)
        || raw.pad(GamepadButton::Start)
    {
        chosen = Some(field);
    }
    let digit = |a, b| letters && (raw.key(a) || raw.key(b));
    if digit(KeyCode::Digit1, KeyCode::Numpad1) {
        chosen = Some(MenuItem::Cpu);
    }
    if digit(KeyCode::Digit2, KeyCode::Numpad2) {
        chosen = Some(MenuItem::Local);
    }
    if digit(KeyCode::Digit3, KeyCode::Numpad3) && items.contains(&MenuItem::Online) {
        chosen = Some(MenuItem::Online);
    }
    for p in taps {
        if let Some(item) = ui::menu_at(m.game.mode, online, p.x, p.y) {
            chosen = Some(item);
        }
    }
    let seated = online == OnlineState::InRoom;
    if raw.key(KeyCode::Escape) || raw.pad(GamepadButton::East) {
        chosen = match m.game.mode {
            Mode::Menu if raw.key(KeyCode::Escape) => Some(MenuItem::Quit),
            Mode::Menu => None,
            // Back from the login or room rows to the start menu.
            Mode::Online if !seated => {
                if let Some(client) = &desk.client {
                    client.exit();
                }
                m.start(Mode::Menu);
                None
            }
            _ => Some(MenuItem::Resume),
        };
    }
    let Some(chosen) = chosen else {
        return;
    };
    let client = desk.client.as_ref();
    match chosen {
        MenuItem::Cpu | MenuItem::Local | MenuItem::Online => {
            if let Some(client) = client {
                if chosen == MenuItem::Online {
                    client.enter();
                } else {
                    client.exit();
                }
            }
            m.start(match chosen {
                MenuItem::Cpu => Mode::Cpu,
                MenuItem::Local => Mode::Local,
                _ => Mode::Online,
            });
            // The login and room rows live in the menu.
            m.game.menu_open |= chosen == MenuItem::Online;
        }
        MenuItem::Key => {
            if let Some(client) = client {
                client.login(&desk.key);
            }
        }
        MenuItem::Create => {
            if let Some(client) = client {
                client.create();
            }
        }
        MenuItem::Join => {
            if let Some(client) = client {
                client.join(&desk.code);
            }
        }
        MenuItem::Leave => {
            if let Some(client) = client {
                client.leave();
            }
        }
        MenuItem::Resume => m.pause(false),
        MenuItem::Quit => {
            exit.write(AppExit::Success);
        }
    }
}
/// Map devices into local input slots for the current mode.
fn capture_input(mut raw: Raw, safe: Res<SafeArea>, mut m: Match) {
    let Ok((window_entity, window)) = raw.windows.single() else {
        return;
    };
    if !window.focused {
        #[cfg(not(target_arch = "wasm32"))]
        if matches!(m.game.mode, Mode::Cpu | Mode::Local) {
            m.pause(true);
        }
        m.io.release();
        raw.touches.clear();
        return;
    }
    let mode = m.game.mode;
    let layout = m.game.layout();
    let insets = safe.insets();
    let io = &mut *m.io;
    // Menus and demo feed nobody.
    if layout == Layout::None {
        io.release();
        raw.touches.clear();
        return;
    }
    let regained = !io.focused;
    io.focused = true;
    for (set, binding) in BINDINGS.into_iter().enumerate() {
        let slot = mode.device_slot(set).unwrap_or(set);
        feed(
            &mut io.slots[slot],
            Source::Keyboard(set as u8),
            keyboard(&raw.keys, binding),
            regained,
        );
    }
    for i in 0..2 {
        if io.pads[i].is_some_and(|e| raw.pads.get(e).is_err()) {
            io.pads[i] = None;
            for s in &mut io.slots {
                s.cancel(Source::Gamepad(i as u8));
            }
        }
    }
    let mut fresh = [regained; 2];
    let mut entities: Vec<_> = raw.pads.iter().map(|(e, _)| e).collect();
    entities.sort_by_key(|e| e.to_bits());
    for e in entities {
        if !io.pads.contains(&Some(e))
            && let Some(i) = io.pads.iter().position(Option::is_none)
        {
            io.pads[i] = Some(e);
            fresh[i] = true;
        }
    }
    for (i, new) in fresh.into_iter().enumerate() {
        let state = io.pads[i]
            .and_then(|e| raw.pads.get(e).ok())
            .map_or((0, 0), |(_, pad)| gamepad(pad));
        let slot = mode.device_slot(i).unwrap_or(i);
        feed(&mut io.slots[slot], Source::Gamepad(i as u8), state, new);
    }
    // Fingers and the left mouse button drive the on-screen controls the same way. A pointer
    // belongs to whatever it started on: a button for as long as it is down, or a floating stick.
    let mut pointers: Vec<(u64, TouchPhase, Vec2)> = raw
        .touches
        .read()
        .filter(|t| t.window == window_entity)
        .map(|t| (t.id, t.phase, t.position))
        .collect();
    io.touch_ui |= !pointers.is_empty();
    if !io.touch_ui {
        return;
    }
    let cursor = window.cursor_position();
    let mouse = &raw.mouse;
    if mouse.just_pressed(MouseButton::Left) {
        pointers.extend(cursor.map(|p| (MOUSE_POINTER, TouchPhase::Started, p)));
    } else if mouse.pressed(MouseButton::Left) {
        pointers.extend(cursor.map(|p| (MOUSE_POINTER, TouchPhase::Moved, p)));
    }
    if mouse.just_released(MouseButton::Left) {
        pointers.push((MOUSE_POINTER, TouchPhase::Ended, cursor.unwrap_or_default()));
    }
    let size = Vec2::new(window.width().max(1.0), window.height().max(1.0));
    let aspect = size.x / size.y;
    for (id, phase, at) in pointers {
        let source = if id == MOUSE_POINTER {
            Source::Mouse
        } else {
            Source::Touch(id)
        };
        let stick = io.sticks.iter().position(|s| s.is_some_and(|s| s.id == id));
        match phase {
            TouchPhase::Started => {
                let p = at / size;
                if let Some((slot, bit)) = ui::button_at(layout, aspect, p.x, p.y, insets) {
                    io.owners.insert(id, (slot, bit));
                    io.slots[slot].sample(source, bit);
                } else if let Some(zone) = ui::stick_at(layout, aspect, p.x, p.y, insets)
                    && io.sticks[zone.slot].is_none()
                {
                    io.sticks[zone.slot] = Some(Stick {
                        id,
                        center: at,
                        knob: at,
                    });
                }
            }
            TouchPhase::Moved => {
                let Some(slot) = stick else { continue };
                let Some(zone) = ui::sticks(layout, aspect, insets)
                    .into_iter()
                    .find(|z| z.slot == slot)
                else {
                    continue;
                };
                let radius = zone.radius * size.y;
                let s = io.sticks[slot].as_mut().unwrap();
                s.center = ui::stick_follow(s.center.into(), at.into(), radius).into();
                s.knob = at;
                let d = (s.knob - s.center) / radius;
                io.slots[slot].sample(source, ui::stick_bits(d.x, d.y));
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                let slot = io
                    .owners
                    .remove(&id)
                    .map(|(slot, _)| slot)
                    .or_else(|| stick.inspect(|&slot| io.sticks[slot] = None));
                match (slot, phase) {
                    (Some(slot), TouchPhase::Canceled) => io.slots[slot].cancel(source),
                    (Some(slot), _) => io.slots[slot].sample(source, 0),
                    (None, _) => {}
                }
            }
        }
    }
}
// A Bevy system: each parameter is a resource it reads or writes.
#[allow(clippy::too_many_arguments)]
fn simulate(
    mut commands: Commands,
    art: Res<Art>,
    loading: Res<Loading>,
    mut game: ResMut<Game>,
    mut io: ResMut<Devices>,
    mut fight: ResMut<Fight>,
    mut net: ResMut<Remote>,
    #[cfg(not(target_arch = "wasm32"))] desk: Res<Desktop>,
) {
    if !loading.ready {
        return;
    }
    let local = [io.slots[0].take(), io.slots[1].take()];
    fight.shake = fight.shake.saturating_sub(1);
    let hits = if game.mode == Mode::Online {
        // The server owns the fight: send this tick's controls, never step locally.
        let (held, pressed) = (
            local[0].held,
            local[0].pressed & naifrog_duel::controls::TRIG,
        );
        #[cfg(target_arch = "wasm32")]
        net_send(held, pressed);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(client) = &desk.client {
            client.input(held, pressed);
        }
        std::mem::take(&mut net.hits)
    } else {
        fight.previous = positions(&fight.arena);
        if game.menu_open {
            return;
        }
        let input = match game.mode {
            Mode::Cpu => [local[0], game.cpu[1].input(&fight.arena, 1)],
            Mode::Demo => [
                game.cpu[0].input(&fight.arena, 0),
                game.cpu[1].input(&fight.arena, 1),
            ],
            _ => local,
        };
        fight.arena.step(input)
    };
    for hit in hits {
        fight.shake = 7;
        fight.shake_strength = if hit.blocked { 2.0 } else { 5.0 };
        commands.spawn((
            Sprite::from_atlas_image(
                if hit.shot {
                    art.bubble.clone()
                } else {
                    art.hit.clone()
                },
                TextureAtlas {
                    layout: art.layout.clone(),
                    index: if hit.shot { 4 } else { 0 },
                },
            ),
            Transform::from_xyz(
                hit.x as f32 / UNIT as f32,
                FLOOR + hit.y as f32 / UNIT as f32,
                4.0,
            )
            .with_scale(Vec3::splat(if hit.shot { BUBBLE_SCALE } else { 0.22 })),
            Spark {
                born: fight.arena.ticks,
                generation: fight.generation,
                blocked: hit.blocked,
                shot: hit.shot,
            },
        ));
    }
}
fn frame(f: &naifrog_duel::combat::Fighter, tick: u32, i: usize) -> usize {
    let age = f.visual_age;
    match f.pose() {
        Pose::Idle => ((tick + i as u32 * 13) / 9 % 8) as usize,
        Pose::Walk => (tick / 6 % 8) as usize,
        Pose::Jump => {
            if f.vy > 10000 {
                1
            } else if f.vy > 1000 {
                2
            } else if f.vy > -5000 {
                3
            } else if f.y > 0 {
                4
            } else {
                6
            }
        }
        Pose::Guard => (age / 3).min(3) as usize,
        Pose::Hurt => (age * 8 / 18).min(7) as usize,
        Pose::Knockdown => (age / 5).min(7) as usize,
        Pose::Getup => (age / 6).min(7) as usize,
        Pose::Light => timed_frame(age, [3, 3, 2, 2, 2, 3, 3, 4]),
        Pose::Heavy => timed_frame(age, [5, 5, 4, 3, 3, 6, 6, 6]),
        Pose::Dash => timed_frame(age, [4, 4, 4, 5, 5, 7, 7, 6]),
        // The bubble leaves on frame 3 (tick 14); the press lands from frame 2 (tick 5).
        Pose::Shot => timed_frame(age, [5, 5, 4, 4, 5, 5, 5, 6]),
        Pose::Air => timed_frame(age, [2, 3, 2, 2, 3, 3, 2, 2]),
    }
}
fn timed_frame(mut age: u32, ticks: [u32; 8]) -> usize {
    for (i, d) in ticks.into_iter().enumerate() {
        if age < d {
            return i;
        }
        age = age.saturating_sub(d);
    }
    7
}
/// Render interpolation only; hit decisions never use it.
#[derive(SystemParam)]
struct Blend<'w> {
    fixed: Res<'w, Time<Fixed>>,
    time: Res<'w, Time>,
    net: Res<'w, Remote>,
    fight: Res<'w, Fight>,
}
impl Blend<'_> {
    fn alpha(&self) -> f32 {
        if self.net.active {
            ((self.time.elapsed_secs_f64() - self.net.received) / 0.05).clamp(0.0, 1.0) as f32
        } else {
            self.fixed.overstep_fraction()
        }
    }
    fn position(&self, seat: usize) -> Vec2 {
        let f = &self.fight.arena.fighters[seat];
        let [x, y] = self.fight.previous.fighters[seat];
        Vec2::new(x as f32, y as f32).lerp(Vec2::new(f.x as f32, f.y as f32), self.alpha())
            / UNIT as f32
            + Vec2::new(0.0, FLOOR)
    }
    /// The seat's bubble centre on screen; a new bubble starts where it spawned.
    fn shot(&self, seat: usize) -> Option<(Vec2, Shot)> {
        let s = self.fight.arena.shots[seat]?;
        let from = self.fight.previous.shots[seat].unwrap_or(s.x) as f32;
        let x = from + (s.x as f32 - from) * self.alpha();
        let y = (SHOT_BOTTOM + SHOT_TOP) as f32 / 2.0;
        Some((Vec2::new(x, y) / UNIT as f32 + Vec2::new(0.0, FLOOR), s))
    }
}
/// The bubble art is drawn about 270 px across in its 512 px cell; this makes it match
/// the 70-unit hit box.
const BUBBLE_SCALE: f32 = 0.25;
fn render_bubbles(
    blend: Blend,
    mut bubbles: Query<(&Bubble, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    for (b, mut t, mut sprite, mut visibility) in &mut bubbles {
        let Some((p, s)) = blend.shot(b.seat) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Visible;
        t.translation = p.extend(2.5);
        sprite.flip_x = s.dir < 0;
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = (s.age / 5 % 4) as usize;
        }
    }
}
fn render_fighters(
    blend: Blend,
    art: Res<Art>,
    mut placed: Query<(&Follow, &mut Transform), Without<Camera2d>>,
    mut sprites: Query<(&Follow, &mut Sprite)>,
    mut cameras: Query<&mut Transform, With<Camera2d>>,
) {
    let fight = &*blend.fight;
    let at = [blend.position(0), blend.position(1)];
    for (follow, mut t) in &mut placed {
        let p = at[follow.seat];
        match follow.part {
            Part::Body => t.translation = p.extend(1.0),
            Part::Shadow => t.translation.x = p.x,
            Part::Ring => t.translation.x = p.x,
            Part::Tag => t.translation = Vec3::new(p.x, p.y + TAG_Y, 3.0),
        }
        if follow.part == Part::Shadow {
            let f = &fight.arena.fighters[follow.seat];
            let strength = (1.0 - f.y as f32 / (UNIT as f32 * 450.0)).clamp(0.35, 1.0);
            t.scale = Vec3::new(strength, 1.0, 1.0);
        }
    }
    for (follow, mut sprite) in &mut sprites {
        let f = &fight.arena.fighters[follow.seat];
        match follow.part {
            Part::Body => {
                sprite.image = art.poses[f.pose().name()].clone();
                sprite.flip_x = f.facing < 0;
                if let Some(atlas) = sprite.texture_atlas.as_mut() {
                    atlas.index = frame(f, fight.arena.ticks, follow.seat);
                }
            }
            Part::Shadow => {
                let strength = (1.0 - f.y as f32 / (UNIT as f32 * 450.0)).clamp(0.35, 1.0);
                sprite.color = Color::srgba(1.0, 1.0, 1.0, strength);
            }
            Part::Ring | Part::Tag => {}
        }
    }
    for mut camera in &mut cameras {
        camera.translation.x = if fight.shake > 0 {
            (fight.arena.ticks as f32 * 2.3).sin() * fight.shake_strength * fight.shake as f32 / 7.0
        } else {
            0.0
        };
    }
}
/// Head tags name the seat and the viewer's relation to it ("P2 YOU", "P1 对手").
fn render_tags(
    game: Res<Game>,
    net: Res<Remote>,
    locale: Res<Locale>,
    mut labels: Query<(&TagText, &mut Text2d)>,
    mut plates: Query<(&TagPlate, &mut Sprite)>,
) {
    let lang = locale.0;
    let width = |seat: usize| em_width(&game.mode.tag(seat, net.player, lang)) * TAG_FONT + 22.0;
    for (tag, mut text) in &mut labels {
        let value = game.mode.tag(tag.0, net.player, lang);
        if text.0 != value {
            text.0 = value;
        }
    }
    // Plates have no seat of their own; size them from the longest label so both stay level.
    let w = width(0).max(width(1));
    for (plate, mut sprite) in &mut plates {
        let size = Vec2::new(w - plate.inset, 34.0 - plate.inset);
        if sprite.custom_size != Some(size) {
            sprite.custom_size = Some(size);
        }
    }
}
fn render_sparks(
    mut commands: Commands,
    fight: Res<Fight>,
    mut sparks: Query<(Entity, &Spark, &mut Sprite)>,
) {
    for (e, fx, mut sprite) in &mut sparks {
        let age = fight.arena.ticks.saturating_sub(fx.born);
        if age >= 16 || fx.generation != fight.generation {
            commands.entity(e).despawn();
            continue;
        }
        sprite.texture_atlas.as_mut().unwrap().index = if fx.shot {
            4 + (age / 4).min(3) as usize
        } else {
            (age / 2).min(7) as usize
        };
        sprite.color = if fx.blocked {
            Color::srgba(0.45, 0.8, 1.0, 1.0 - age as f32 / 16.0)
        } else {
            Color::srgba(1.0, 1.0, 1.0, 1.0 - age as f32 / 16.0)
        };
    }
}
#[derive(SystemParam)]
struct Hud<'w, 's> {
    game: Res<'w, Game>,
    net: Res<'w, Remote>,
    fight: Res<'w, Fight>,
    loading: Res<'w, Loading>,
    locale: Res<'w, Locale>,
    name_font: Res<'w, NameFont>,
    io: Res<'w, Devices>,
    safe: Res<'w, SafeArea>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
}
/// Damage trails: after a hit the red part stays a moment, then slides down to the bar.
fn render_trails(fight: Res<Fight>, time: Res<Time>, mut trails: Query<(&mut Trail, &mut Node)>) {
    for (mut trail, mut node) in &mut trails {
        let hp = fight.arena.fighters[trail.seat].hp as f32;
        trail.hp = if hp >= trail.hp {
            hp
        } else {
            (trail.hp - 45.0 * time.delta_secs()).max(hp)
        };
        node.width = percent(trail.hp);
    }
}
/// Scale the stage art to cover the window (no dark side bars on wide screens, none above
/// or below on 4:3 tablets) while its ground line stays under the fighters' feet.
fn fit_stage(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut stage: Query<(&mut Sprite, &mut Transform), With<StageArt>>,
) {
    let (Ok(window), Ok((mut sprite, mut transform))) = (windows.single(), stage.single_mut())
    else {
        return;
    };
    let scale = ui::stage_cover(window.width() / window.height().max(1.0), FLOOR);
    let size = Vec2::new(1280.0, 720.0) * scale;
    if sprite.custom_size != Some(size) {
        sprite.custom_size = Some(size);
        transform.translation.y = FLOOR * (1.0 - scale);
    }
}
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn render_hud(
    hud: Hud,
    time: Res<Time>,
    mut labels: Query<(&Label, &mut Text, &mut TextColor)>,
    mut bars: Query<
        (&Health, &mut Node, &mut BorderColor),
        (Without<VirtualKey>, Without<StickPart>, Without<Shown>),
    >,
    mut keys: Query<
        (
            &VirtualKey,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut Node,
        ),
        (Without<Health>, Without<StickPart>, Without<Shown>),
    >,
    mut sticks: Query<
        (
            &StickPart,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut Node,
        ),
        (Without<Health>, Without<VirtualKey>, Without<Shown>),
    >,
    mut key_text: Query<(&KeyText, &mut Text, &mut TextColor), Without<Label>>,
    mut dots: Query<
        (&WinDot, &mut BackgroundColor),
        (
            Without<Health>,
            Without<VirtualKey>,
            Without<StickPart>,
            Without<Shown>,
        ),
    >,
    mut shown: Query<
        (&Shown, &mut Node),
        (Without<Health>, Without<VirtualKey>, Without<StickPart>),
    >,
) {
    let (game, fight, you) = (&*hud.game, &*hud.fight, hud.net.player);
    let (mode, lang) = (game.mode, hud.locale.0);
    let layout = game.layout();
    // How much of the five-minute hold is left on the rival's dropped seat, as a
    // status line: empty while that seat is connected.
    let rival_grace = if mode == Mode::Online {
        you.and_then(|seat| hud.net.grace[1 - seat])
            .map_or_else(String::new, |secs| ui::grace_note(lang, secs))
    } else {
        String::new()
    };
    let display = |on: bool| if on { Display::Flex } else { Display::None };
    for (label, mut text, mut color) in &mut labels {
        let value = match label {
            Label::Controls => mode.controls(lang, you, hud.io.touch_ui),
            Label::Clock => fight.arena.remaining.div_ceil(60).to_string(),
            Label::Name(i) => {
                let name = hud.net.names[*i].as_str();
                if name.is_empty() {
                    format!("P{}", i + 1)
                } else {
                    name.to_owned()
                }
            }
            Label::Role(i) => mode.role(*i, you).map_or("", |r| r.label(lang)).into(),
            Label::Hp(i) => fight.arena.fighters[*i].hp.to_string(),
            // Two or more hits in one stun chain, on the attacker's side; one hit is
            // not a combo worth shouting about.
            Label::Combo(i) => match fight.arena.fighters[1 - *i].combo_taken {
                0 | 1 => String::new(),
                n => ui::combo_note(lang, n),
            },
            // Until the name font is in, Chinese would draw as empty boxes.
            Label::Status if !hud.name_font.settled => "...".into(),
            Label::Status if !hud.loading.ready => ui::loading_note(lang).into(),
            // The server holds a dropped rival's seat for five minutes; say how much is left.
            Label::Status if !rival_grace.is_empty() => rival_grace.clone(),
            // The desktop names the room, so the code can be read out to the other player.
            Label::Status
                if cfg!(not(target_arch = "wasm32"))
                    && mode == Mode::Online
                    && fight.arena.outcome.is_none()
                    && !hud.net.status.is_empty() =>
            {
                hud.net.status.clone()
            }
            Label::Status => mode.status(
                lang,
                game.menu_open && mode != Mode::Online,
                fight.arena.outcome,
                you,
                hud.net.connected,
            ),
            Label::Banner => ui::banner(lang, &fight.arena).unwrap_or_default(),
            Label::Rematch => ui::rematch_label(lang).into(),
        };
        if text.0 != value {
            text.0 = value;
        }
        // The clock turns red for the final ten seconds.
        if let Label::Clock = label {
            color.0 = if fight.arena.remaining < 600 {
                Color::srgb(1.0, 0.32, 0.2)
            } else {
                Color::WHITE
            };
        }
    }
    for (h, mut node, mut frame) in &mut bars {
        let hp = fight.arena.fighters[h.0].hp;
        node.width = percent(hp as f32);
        // Under a quarter of health the bar's frame pulses red. The fill keeps the
        // seat's solid color: its pixels are what the health read is measured from,
        // so dimming the fill would blur "low" into "empty".
        *frame = BorderColor::all(if hp > 0 && hp < 25 {
            let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 5.0).sin();
            Color::srgb(1.0, 0.25, 0.15).with_alpha(0.35 + 0.65 * pulse)
        } else {
            Color::srgba(1.0, 1.0, 1.0, 0.82)
        });
    }
    for (dot, mut color) in &mut dots {
        color.0 = if fight.arena.wins[dot.seat] > dot.index {
            seat_color(dot.seat)
        } else {
            Color::srgba(1.0, 1.0, 1.0, 0.16)
        };
    }
    let touch = hud.io.touch_ui && layout != Layout::None;
    for (which, mut node) in &mut shown {
        node.display = display(match which {
            Shown::Rematch => {
                hud.loading.ready
                    && layout != Layout::None
                    && (mode != Mode::Online || you.is_some())
            }
            Shown::Hint => {
                node.top = percent(if touch { 22.5 } else { 95.8 });
                true
            }
            Shown::Banner => mode != Mode::Menu && ui::banner(lang, &fight.arena).is_some(),
            Shown::Versus => mode != Mode::Menu && fight.arena.intro > INTRO_TICKS,
        });
    }
    // Touch controls: only on touch screens, laid out for the window's current shape.
    let Ok(window) = hud.windows.single() else {
        return;
    };
    let size = Vec2::new(window.width().max(1.0), window.height().max(1.0));
    let aspect = size.x / size.y;
    let insets = hud.safe.insets();
    let place = |node: &mut Node, center: Vec2, radius: f32| {
        node.left = px(center.x - radius);
        node.top = px(center.y - radius);
        node.width = px(2.0 * radius);
        node.height = px(2.0 * radius);
    };
    // Controls take the color of the seat their slot drives (blue when you joined as P2).
    let seat = |slot| mode.slot_seat(slot, you);
    let tint = |slot| seat(slot).map_or(Color::WHITE, seat_color);
    // SHOT greys out while that seat's bubble is still flying (pressing it does nothing).
    let spent = |slot, bit| {
        bit == SHOT && seat(slot).is_some_and(|s: usize| fight.arena.shots[s].is_some())
    };
    for (k, mut text, mut color) in &mut key_text {
        let label = ui::button_label(k.bit, lang);
        if text.0 != label {
            text.0 = label.into();
        }
        if k.layout == layout {
            color.0 = if spent(k.slot, k.bit) {
                faded(Color::WHITE, 0.3)
            } else {
                faded(tint(k.slot), 0.9)
            };
        }
    }
    let buttons = ui::buttons(layout, aspect, insets);
    for (k, mut bg, mut border, mut node) in &mut keys {
        let b = buttons.get(k.index).filter(|_| touch && k.layout == layout);
        node.display = display(b.is_some());
        let Some(b) = b else { continue };
        place(&mut node, Vec2::new(b.x, b.y) * size, b.r * size.y);
        let held = hud.io.slots[b.slot].held() & b.bit != 0;
        bg.0 = match seat(b.slot) {
            Some(s) if held => faded(pressed_color(s), 0.75),
            _ => BUTTON_IDLE,
        };
        *border = BorderColor::all(if spent(b.slot, b.bit) {
            faded(Color::WHITE, 0.15)
        } else {
            faded(tint(b.slot), if held { 0.9 } else { 0.5 })
        });
    }
    // A stick rests faintly at its home spot and jumps under the finger that grabs it.
    let zones = ui::sticks(layout, aspect, insets);
    for (part, mut bg, mut border, mut node) in &mut sticks {
        let zone = zones
            .iter()
            .find(|z| z.slot == part.slot)
            .filter(|_| touch && part.layout == layout);
        node.display = display(zone.is_some());
        let Some(zone) = zone else { continue };
        let radius = zone.radius * size.y;
        let home = Vec2::new(zone.home.0, zone.home.1) * size;
        let active = hud.io.sticks[zone.slot];
        let (center, knob) = active.map_or((home, home), |s| {
            (
                s.center,
                s.center + (s.knob - s.center).clamp_length_max(radius),
            )
        });
        let color = tint(zone.slot);
        let alpha = if active.is_some() { 1.0 } else { 0.5 };
        if part.knob {
            place(&mut node, knob, 0.45 * radius);
            bg.0 = faded(color, 0.45 * alpha);
            *border = BorderColor::all(faded(color, 0.8 * alpha));
        } else {
            place(&mut node, center, radius);
            bg.0 = faded(BUTTON_IDLE, 0.3 * alpha);
            *border = BorderColor::all(faded(color, 0.5 * alpha));
        }
    }
}
/// Tells the page where the touch controls sit, in canvas pixels, whenever that changes,
/// so browser tests can aim real touches at them (`window.__naifrogTouch`).
#[cfg(target_arch = "wasm32")]
fn report_touch(
    game: Res<Game>,
    io: Res<Devices>,
    safe: Res<SafeArea>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut last: Local<String>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let (w, h) = (window.width().max(1.0), window.height().max(1.0));
    let layout = game.layout();
    let visible = io.touch_ui && layout != Layout::None;
    let layout = if visible { layout } else { Layout::None };
    let insets = safe.insets();
    let buttons: Vec<_> = ui::buttons(layout, w / h, insets)
        .into_iter()
        .map(|b| {
            serde_json::json!({
                "slot": b.slot, "name": b.name, "x": b.x * w, "y": b.y * h, "r": b.r * h,
            })
        })
        .collect();
    let sticks: Vec<_> = ui::sticks(layout, w / h, insets)
        .into_iter()
        .map(|z| {
            serde_json::json!({
                "slot": z.slot, "x": z.home.0 * w, "y": z.home.1 * h, "r": z.radius * h,
                "left": z.region.left * w, "top": z.region.top * h,
                "right": (z.region.left + z.region.width) * w,
                "bottom": (z.region.top + z.region.height) * h,
            })
        })
        .collect();
    let json =
        serde_json::json!({ "visible": visible, "buttons": buttons, "sticks": sticks }).to_string();
    if *last != json {
        touch_layout(&json);
        *last = json;
    }
}
fn debug_boxes(fight: Res<Fight>, game: Res<Game>, mut gizmos: Gizmos) {
    if !game.boxes {
        return;
    }
    for f in &fight.arena.fighters {
        let r = f.hurt_box();
        let center = Vec2::new(
            (r.left + r.right) as f32 / 2.0,
            (r.bottom + r.top) as f32 / 2.0,
        ) / UNIT as f32
            + Vec2::new(0.0, FLOOR);
        let size = Vec2::new((r.right - r.left) as f32, (r.top - r.bottom) as f32) / UNIT as f32;
        gizmos.rect_2d(center, size, Color::srgb(0.2, 1.0, 0.3));
        if let Some(a) = f.attack_box() {
            gizmos.rect_2d(
                Vec2::new(
                    (a.left + a.right) as f32 / 2.0,
                    (a.bottom + a.top) as f32 / 2.0,
                ) / UNIT as f32
                    + Vec2::new(0.0, FLOOR),
                Vec2::new((a.right - a.left) as f32, (a.top - a.bottom) as f32) / UNIT as f32,
                Color::srgb(1.0, 0.1, 0.1),
            );
        }
        gizmos.rect_2d(
            Vec2::new(f.x as f32 / UNIT as f32, FLOOR + 85.0),
            Vec2::new(
                (f.hurt_box().right - f.hurt_box().left) as f32 / UNIT as f32,
                170.0,
            ),
            Color::srgb(0.1, 0.7, 1.0),
        );
    }
}
fn asset_errors(
    mut errors: MessageReader<UntypedAssetLoadFailedEvent>,
    mut name_font: ResMut<NameFont>,
    mut exit: MessageWriter<AppExit>,
) {
    for e in errors.read() {
        // Without the name font Chinese text loses its glyphs, but the game still plays.
        if e.path.path() == Path::new(NAME_FONT) {
            warn!("Name font failed, Chinese text will not draw: {}", e.error);
            name_font.settled = true;
            continue;
        }
        error!("Asset failed: {}: {}", e.path, e.error);
        exit.write(AppExit::error());
    }
}
/// Chinese text falls back to the name font: parley draws each character with the text's
/// own font (Fira Mono) when it can, otherwise with the fallback families of the character's
/// script. Punctuation takes the script of the text before it, so a full-width colon after
/// "P1" looks among the Latin fallbacks: both scripts get the name font. Runs until the font
/// is registered, once.
fn name_font_fallback(
    mut name_font: ResMut<NameFont>,
    fonts: Res<Assets<Font>>,
    mut font_cx: ResMut<FontCx>,
    mut texts: Query<&mut TextFont>,
) {
    if name_font.settled {
        return;
    }
    // Bevy sets a font's alias when it registers the font; it is empty until then.
    let Some(font) = fonts.get(&name_font.handle).filter(|f| !f.alias.is_empty()) else {
        return;
    };
    name_font.settled = true;
    let Some(family) = font_cx.collection.family_id(&font.alias) else {
        warn!("Name font did not register, Chinese text will not draw");
        return;
    };
    for script in [*b"Hani", *b"Latn"] {
        let key = FallbackKey::new(Script::from_bytes(script), None);
        font_cx
            .collection
            .set_fallbacks(key, std::iter::once(family));
    }
    // Text laid out before this keeps its missing glyphs until it is laid out again.
    for mut text in &mut texts {
        text.set_changed();
    }
    info!("Name font ready; Chinese text falls back to it.");
}
/// Follows the page's language in the browser, and F2 on the desktop; renames the window and
/// rewords the desktop client's status lines.
fn language(
    mut locale: ResMut<Locale>,
    #[cfg(not(target_arch = "wasm32"))] keys: Res<ButtonInput<KeyCode>>,
    #[cfg(not(target_arch = "wasm32"))] desk: Res<Desktop>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    #[cfg(target_arch = "wasm32")]
    let lang = Lang::from_tag(&page_lang()).unwrap_or(locale.0);
    #[cfg(not(target_arch = "wasm32"))]
    let lang = if keys.just_pressed(KeyCode::F2) {
        locale.0.other()
    } else {
        locale.0
    };
    if lang == locale.0 {
        return;
    }
    locale.0 = lang;
    for mut window in &mut windows {
        window.title = ui::window_title(lang).into();
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(client) = &desk.client {
        client.set_lang(lang);
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn smoke_capture(
    mut commands: Commands,
    mut smoke: ResMut<Smoke>,
    loading: Res<Loading>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
    if smoke.path.is_none() {
        return;
    }
    smoke.frames += 1;
    if loading.ready {
        smoke.ready += 1;
    }
    if smoke.frames >= 900 {
        error!("Smoke test timed out");
        exit.write(AppExit::error());
        return;
    }
    if smoke.frames < 180 || smoke.ready < 60 || smoke.requested {
        return;
    }
    smoke.requested = true;
    let mut save = save_to_disk(smoke.path.clone().unwrap());
    commands.spawn(Screenshot::primary_window()).observe(
        move |shot: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            save(shot);
            exit.write(AppExit::Success);
        },
    );
}
#[cfg(target_arch = "wasm32")]
fn smoke_capture() {}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    fn input_app(mode: Mode) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(100),
            ))
            .insert_resource(Game {
                mode,
                menu_open: false,
                cursor: 0,
                boxes: false,
                swap_sides: false,
                cpu: [Cpu::new(1), Cpu::new(2)],
                rematch_armed: false,
            })
            .init_resource::<Fight>()
            .init_resource::<Devices>()
            .init_resource::<Remote>()
            .init_resource::<Desktop>()
            .init_resource::<SafeArea>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_message::<TouchInput>()
            .add_message::<KeyboardInput>()
            .add_message::<AppExit>()
            .insert_resource(Loading { ready: true })
            .insert_resource(Art {
                poses: default(),
                layout: default(),
                stage: default(),
                hit: default(),
                shadow: default(),
                bubble: default(),
                portrait: default(),
            })
            .add_systems(PreUpdate, (hotkeys, capture_input).chain())
            .add_systems(FixedUpdate, simulate);
        app.world_mut().resource_mut::<Fight>().arena.intro = 0;
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.update();
        (app, window)
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn losing_focus_pauses_local_play_until_the_player_resumes() {
        for mode in [Mode::Cpu, Mode::Local] {
            let (mut app, window) = input_app(mode);
            app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::KeyW);
            let before = app.world().resource::<Fight>().arena.clone();
            for _ in 0..20 {
                app.update();
            }
            assert!(app.world().resource::<Game>().menu_open);
            assert_eq!(app.world().resource::<Fight>().arena, before);
            app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
            app.update();
            assert_eq!(
                app.world().resource::<Fight>().arena,
                before,
                "focus alone must not resume"
            );
            app.world_mut().resource_mut::<Game>().menu_open = false;
            app.update();
            assert_eq!(
                app.world().resource::<Fight>().arena.fighters[0].y,
                0,
                "a held jump must not leak through resume"
            );
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .release(KeyCode::KeyW);
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::KeyW);
            app.update();
            assert!(app.world().resource::<Fight>().arena.fighters[0].y > 0);
        }
        let (mut online, window) = input_app(Mode::Online);
        online
            .world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .focused = false;
        online.update();
        assert!(
            !online.world().resource::<Game>().menu_open,
            "online play must keep its server-controlled pause policy"
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn desktop_key_events_preserve_spaces_in_the_access_key() {
        let (mut app, window) = input_app(Mode::Online);
        app.world_mut().resource_mut::<Desktop>().client = Some(Client::start(
            Server::parse("http://127.0.0.1:18080").unwrap(),
            None,
            Lang::En,
        ));
        let items = ui::menu_items(Mode::Online, OnlineState::LoggedOut);
        let cursor = items
            .iter()
            .position(|item| *item == MenuItem::Key)
            .unwrap();
        {
            let mut game = app.world_mut().resource_mut::<Game>();
            game.menu_open = true;
            game.cursor = cursor;
        }
        for c in "review key".chars() {
            app.world_mut()
                .resource_mut::<Messages<KeyboardInput>>()
                .write(KeyboardInput {
                    key_code: if c == ' ' {
                        KeyCode::Space
                    } else {
                        KeyCode::KeyR
                    },
                    logical_key: if c == ' ' {
                        Key::Space
                    } else {
                        Key::Character(c.to_string().into())
                    },
                    state: ButtonState::Pressed,
                    text: Some(c.to_string().into()),
                    repeat: false,
                    window,
                });
        }
        app.update();
        assert_eq!(app.world().resource::<Desktop>().key, "review key");
    }
    fn args(s: &str) -> Vec<String> {
        std::iter::once("naifrog-duel")
            .chain(s.split_whitespace())
            .map(String::from)
            .collect()
    }
    #[test]
    fn command_line_picks_an_explicit_mode() {
        assert_eq!(start_mode(&args(""), false), Ok(Mode::Menu));
        assert_eq!(start_mode(&args("--ai"), false), Ok(Mode::Cpu));
        assert_eq!(start_mode(&args("--mode local"), false), Ok(Mode::Local));
        assert_eq!(start_mode(&args("--smoke x.png"), true), Ok(Mode::Demo));
        assert_eq!(
            start_mode(&args("--smoke x.png --mode menu"), true),
            Ok(Mode::Menu)
        );
        assert!(
            start_mode(&args("--mode online"), false).is_err(),
            "online play starts from the menu"
        );
        assert!(start_mode(&args("--mode"), false).is_err());
    }
    #[test]
    fn server_comes_from_the_command_line() {
        assert_eq!(
            server_address(&args("--server http://1.2.3.4:8080 --mode menu")),
            Some("http://1.2.3.4:8080".into())
        );
        // A bare --server is refused in the menu instead of silently ignored.
        assert_eq!(server_address(&args("--server")), Some(String::new()));
    }
    #[test]
    fn names_persist_between_snapshots() {
        use naifrog_duel::net::{RemoteView, Snapshot};
        let frame = Snapshot {
            seq: 1,
            round: 0,
            arena: Default::default(),
            held: [0; 2],
            connected: [true; 2],
            votes: [false; 2],
            names: ["Alice".into(), "Bob".into()],
            grace_left: [None, None],
            hits: vec![],
        };
        let view = |snapshot| RemoteView {
            active: true,
            player: Some(0),
            room: Some("ABCDEFGH".into()),
            transport: 1,
            status: String::new(),
            snapshot,
        };
        let mut world = World::new();
        world.insert_resource(Game {
            mode: Mode::Menu,
            menu_open: true,
            cursor: 0,
            boxes: false,
            swap_sides: false,
            cpu: [Cpu::new(1), Cpu::new(2)],
            rematch_armed: false,
        });
        world.init_resource::<Fight>();
        world.init_resource::<Devices>();
        world.insert_resource(Time::<()>::default());
        let mut net = Remote::default();
        let mut state = bevy::ecs::system::SystemState::<Match>::new(&mut world);
        {
            let mut m = match state.get_mut(&mut world) {
                Ok(m) => m,
                Err(error) => panic!("Match param invalid: {error:?}"),
            };
            apply_view(view(Some(frame)), &mut net, &mut m);
        }
        assert_eq!(net.names, ["Alice", "Bob"]);
        // Both transports hand each snapshot over exactly once, so most polls carry
        // none: the names from the last one must survive them.
        for _ in 0..3 {
            let mut m = match state.get_mut(&mut world) {
                Ok(m) => m,
                Err(error) => panic!("Match param invalid: {error:?}"),
            };
            apply_view(view(None), &mut net, &mut m);
        }
        assert_eq!(net.names, ["Alice", "Bob"], "names strobe without this");
    }
}
