//! Screen layout, identity rules and canvas text shared by drawing, hit-testing and tests.
//! Coordinates are fractions of the window: x and y run from 0 to 1, top-left origin.
//! Every text comes in both languages (`Lang`); `i18n::drawn` says what the canvas can draw.
//! P1 / P2, VS and key names (R, Esc, Start, A) stay the same in Chinese, as on the page.
use crate::{
    combat::{Arena, INTRO_TICKS, Outcome},
    controls::{DASH, GUARD, HEAVY, JUMP, LEFT, LIGHT, RIGHT, SHOT},
    i18n::Lang,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Nothing chosen yet: no simulation, no fighter input.
    #[default]
    Menu,
    /// The local player is P1; `ai::Cpu` drives P2.
    Cpu,
    /// Two people on one device: P1 and P2 have separate controls.
    Local,
    /// Server-authoritative room; every local device controls the assigned seat.
    Online,
    /// CPU versus CPU, only reachable from the native command line.
    Demo,
}

impl Mode {
    pub fn code(self) -> u8 {
        match self {
            Self::Menu => 0,
            Self::Cpu => 1,
            Self::Local => 2,
            Self::Online => 3,
            Self::Demo => 4,
        }
    }
    pub fn layout(self) -> Layout {
        match self {
            Self::Cpu | Self::Online => Layout::Single,
            Self::Local => Layout::Double,
            Self::Menu | Self::Demo => Layout::None,
        }
    }
    /// The seat a local input slot controls. Online seats come from the server and
    /// may be P2, so `you` decides instead of screen side or join order.
    pub fn slot_seat(self, slot: usize, you: Option<usize>) -> Option<usize> {
        match (self, slot) {
            (Self::Local, 0 | 1) => Some(slot),
            (Self::Cpu, 0) => Some(0),
            (Self::Online, 0) => you.filter(|s| *s < 2),
            _ => None,
        }
    }
    /// Which input slot a keyboard binding set or gamepad index feeds.
    pub fn device_slot(self, device: usize) -> Option<usize> {
        match self {
            Self::Local => (device < 2).then_some(device),
            Self::Cpu | Self::Online => Some(0),
            Self::Menu | Self::Demo => None,
        }
    }
    /// Role shown next to the seat number. Follows the seat index, never a position.
    pub fn role(self, seat: usize, you: Option<usize>) -> Option<Role> {
        match self {
            Self::Cpu if seat == 0 => Some(Role::You),
            Self::Cpu | Self::Demo => Some(Role::Cpu),
            Self::Online => you.map(|y| if y == seat { Role::You } else { Role::Rival }),
            Self::Local | Self::Menu => None,
        }
    }
    /// The head tag: the seat, then its role if it has one (`P2 YOU`, `P2 你`).
    pub fn tag(self, seat: usize, you: Option<usize>, lang: Lang) -> String {
        match self.role(seat, you) {
            None => format!("P{}", seat + 1),
            Some(role) => format!("P{} {}", seat + 1, role.label(lang)),
        }
    }
    /// The Chinese names are the page's.
    pub fn name(self, lang: Lang) -> &'static str {
        match self {
            Self::Menu => lang.pick("未选择模式", "NONE"),
            Self::Cpu => lang.pick("单机打人机", "VS CPU"),
            Self::Local => lang.pick("本地同屏双人", "LOCAL 2 PLAYERS"),
            Self::Online => lang.pick("私人联机", "PRIVATE ONLINE"),
            Self::Demo => lang.pick("演示", "DEMO"),
        }
    }
    /// The line under the clock.
    pub fn status(
        self,
        lang: Lang,
        paused: bool,
        outcome: Option<Outcome>,
        you: Option<usize>,
        connected: [bool; 2],
    ) -> String {
        let zh = lang == Lang::Zh;
        if self == Self::Menu {
            return lang.pick("请选择模式", "CHOOSE A MODE").into();
        }
        if paused {
            return format!("{}{}", self.name(lang), lang.pick("：已暂停", ": PAUSED"));
        }
        // A full-width ！ carries its own space, so Chinese needs one less.
        let again = match (self, lang) {
            (Self::Online, Lang::Zh) => " 双方按 R / Start / 手柄 A 重开",
            (Self::Online, Lang::En) => "  BOTH: R / START / PAD A",
            (_, Lang::Zh) => " R / Start / 手柄 A：再来一局",
            (_, Lang::En) => "  R / START / PAD A: REMATCH",
        };
        let seat = |y: usize| match lang {
            Lang::Zh => format!("你是 P{}（{}）", y + 1, color_name(y, lang)),
            Lang::En => format!("YOU ARE P{} {}", y + 1, color_name(y, lang)),
        };
        match outcome {
            Some(Outcome::Draw) => format!("{}{again}", lang.pick("平局！", "DRAW!")),
            Some(Outcome::Winner(w)) if self.role(w, you) == Some(Role::You) => {
                format!("{}{again}", lang.pick("你赢了！", "YOU WIN!"))
            }
            Some(Outcome::Winner(w)) => format!(
                "{}{}{again}",
                self.tag(w, you, lang),
                lang.pick("获胜！", " WINS!")
            ),
            None => match (self, you) {
                (Self::Online, None) => lang
                    .pick("联机：请创建或加入房间", "ONLINE: CREATE OR JOIN A ROOM")
                    .into(),
                (Self::Online, Some(y)) if connected.iter().all(|c| *c) => {
                    if zh {
                        format!("联机：{} · 对战中", seat(y))
                    } else {
                        format!("ONLINE: {}  FIGHT!", seat(y))
                    }
                }
                (Self::Online, Some(y)) => {
                    if zh {
                        format!("联机：{} · 等待对方 / 断线暂停", seat(y))
                    } else {
                        format!("ONLINE: {}  WAITING / PAUSED", seat(y))
                    }
                }
                (Self::Cpu, _) => lang
                    .pick("你（P1）对战电脑（P2）", "YOU (P1) VS CPU (P2)")
                    .into(),
                (Self::Local, _) => lang.pick("本地双人：P1 对战 P2", "LOCAL: P1 VS P2").into(),
                _ => lang.pick("演示：电脑对战电脑", "DEMO: CPU VS CPU").into(),
            },
        }
    }
    /// The controls reminder line at the bottom of the top HUD. A touch screen gets the stick
    /// and buttons instead of the keyboard keys.
    pub fn controls(self, lang: Lang, you: Option<usize>, touch: bool) -> String {
        let text = match self {
            Self::Menu => "",
            Self::Cpu if touch => lang.pick(
                "你 = P1 金色：左侧摇杆移动，上推跳，下拉防御  |  右侧按钮出招",
                "YOU = P1 GOLD: LEFT STICK MOVE, UP JUMP, DOWN GUARD  |  RIGHT BUTTONS",
            ),
            Self::Local if touch => lang.pick(
                "P1 金色：左半屏    P2 蓝色：右半屏    摇杆移动，上推跳，下拉防御",
                "P1 GOLD: LEFT HALF    P2 BLUE: RIGHT HALF    STICK MOVE, UP JUMP, DOWN GUARD",
            ),
            Self::Cpu => lang.pick(
                "你 = P1 金色：A D / 方向键移动  W 跳  S 防御  F G H 或 J K L 出招  T 或 I 奶泡  任意手柄  |  Esc：菜单",
                "YOU = P1 GOLD: A D / ARROWS MOVE  W JUMP  S GUARD  F G H OR J K L ATTACK  T OR I SHOT  ANY PAD  |  ESC: MENU",
            ),
            Self::Local => lang.pick(
                "P1 金色：A D W S + F G H T / 1 号手柄    P2 蓝色：方向键 + J K L I / 2 号手柄    F1：判定框  Esc：菜单",
                "P1 GOLD: A D W S + F G H T / PAD 1    P2 BLUE: ARROWS + J K L I / PAD 2    F1: BOXES  ESC: MENU",
            ),
            Self::Online => match you {
                Some(y) => {
                    return match lang {
                        Lang::Zh => format!(
                            "你 = P{} {}：这台设备上的所有按键、手柄和触屏都控制你  |  Esc：菜单",
                            y + 1,
                            color_name(y, lang)
                        ),
                        Lang::En => format!(
                            "YOU = P{} {}: EVERY KEY, PAD AND TOUCH ON THIS DEVICE CONTROLS YOU  |  ESC: MENU",
                            y + 1,
                            color_name(y, lang)
                        ),
                    };
                }
                None => lang.pick("等待分配座位  |  Esc：菜单", "WAITING FOR A SEAT  |  ESC: MENU"),
            },
            Self::Demo => lang.pick("演示：R 重开  |  Esc：菜单", "DEMO: R RESTART  |  ESC: MENU"),
        };
        text.into()
    }
}

/// Who a seat is to the person at this screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    You,
    Cpu,
    Rival,
}

impl Role {
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::You => lang.pick("你", "YOU"),
            Self::Cpu => lang.pick("电脑", "CPU"),
            Self::Rival => lang.pick("对手", "RIVAL"),
        }
    }
}

/// P1 is always gold and P2 always blue, whoever is "you".
pub fn color_name(seat: usize, lang: Lang) -> &'static str {
    if seat == 0 {
        lang.pick("金色", "GOLD")
    } else {
        lang.pick("蓝色", "BLUE")
    }
}

/// How long a dropped rival's seat still exists, for the status line: `RIVAL DISCONNECTED 4:59`.
/// The seconds are padded so the line does not jitter every tick.
pub fn grace_note(lang: Lang, grace_secs: u16) -> String {
    let (m, s) = (grace_secs / 60, grace_secs % 60);
    match lang {
        Lang::Zh => format!("对手已断线，席位保留 {m}:{s:02}"),
        Lang::En => format!("RIVAL DISCONNECTED {m}:{s:02}"),
    }
}

/// The hit counter under the health bar of the seat landing the combo.
pub fn combo_note(lang: Lang, hits: u8) -> String {
    match lang {
        Lang::Zh => format!("{hits} 连击"),
        Lang::En => format!("{hits} HITS"),
    }
}

pub fn rematch_label(lang: Lang) -> &'static str {
    lang.pick("再来一局 / R", "REMATCH / R")
}

/// The status line while the textures load.
pub fn loading_note(lang: Lang) -> &'static str {
    lang.pick("正在载入画面……", "LOADING ART...")
}

/// The big centred announcement: the round introduction counts down "ROUND N" (the
/// deciding round is "FINAL ROUND") into "FIGHT!", a resumed interruption counts
/// "3" "2" "1", and the result says how the round ended. `None` while the fight
/// runs. The phases live in the simulation, so this is drawing only; a `String`
/// because the countdown's digits change every second.
pub fn banner(lang: Lang, arena: &Arena) -> Option<String> {
    if let Some(outcome) = arena.outcome {
        return Some(
            match outcome {
                Outcome::Draw => lang.pick("平局", "DRAW"),
                Outcome::Winner(w) => {
                    if arena.fighters[w].hp == 100 {
                        lang.pick("完胜！", "PERFECT!")
                    } else if arena.fighters[1 - w].hp == 0 {
                        lang.pick("击倒！", "K.O.")
                    } else {
                        lang.pick("时间到", "TIME OVER")
                    }
                }
            }
            .into(),
        );
    }
    // An interrupted round counts back in before it continues, mirroring `step`'s order.
    if arena.resume > 0 {
        return Some(arena.resume.div_ceil(60).to_string());
    }
    // The versus screen that precedes round one draws its own overlay.
    if arena.intro > INTRO_TICKS {
        return None;
    }
    (arena.intro > 0).then(|| {
        if arena.intro > INTRO_TICKS / 3 {
            // The round that can hand someone the second win, or the fifth and last
            // round of the match, is announced as the final one.
            if arena.game >= 4 || arena.wins.contains(&1) {
                lang.pick("决胜局", "FINAL ROUND")
            } else {
                let n = arena.game as usize % 4;
                lang.pick(
                    ["第一局", "第二局", "第三局", "第四局"][n],
                    ["ROUND 1", "ROUND 2", "ROUND 3", "ROUND 4"][n],
                )
            }
        } else {
            lang.pick("开打！", "FIGHT!")
        }
        .to_owned()
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    None,
    /// One player on this device: movement on the left, attacks on the right.
    Single,
    /// P1 owns the left half, P2 the right half.
    Double,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl Region {
    pub const fn new(left: f32, top: f32, width: f32, height: f32) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.left
            && x <= self.left + self.width
            && y >= self.top
            && y <= self.top + self.height
    }
    pub fn overlaps(self, o: Region) -> bool {
        self.left < o.left + o.width
            && o.left < self.left + self.width
            && self.top < o.top + o.height
            && o.top < self.top + self.height
    }
}

/// A round on-screen button. `x` is a fraction of the window width, `y` of its height, and the
/// radius `r` a fraction of its height, so a button stays round at any aspect ratio. `name`
/// identifies it in any language (browser tests aim by it); `label` is what it shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Button {
    pub slot: usize,
    pub bit: u16,
    pub name: &'static str,
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

impl Button {
    pub fn label(self, lang: Lang) -> &'static str {
        button_label(self.bit, lang)
    }
    /// Whether a point (fractions of the window) is on the button; `aspect` is width / height.
    pub fn contains(self, aspect: f32, x: f32, y: f32) -> bool {
        let dx = (x - self.x) * aspect;
        let dy = y - self.y;
        dx * dx + dy * dy <= self.r * self.r
    }
}

/// Where a finger may start the floating stick of `slot`. The stick appears under the finger;
/// while idle a faded hint rests at `home` (fractions of the window). `radius`, a fraction of
/// the window height, is how far the knob travels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StickZone {
    pub slot: usize,
    pub region: Region,
    pub home: (f32, f32),
    pub radius: f32,
}

/// Touches above this line belong to the HUD and the play area, never to a stick.
pub const STICK_TOP: f32 = 0.3;
pub const REMATCH: Region = Region::new(0.43, 0.172, 0.14, 0.044);
/// How much to enlarge the 1280×720 stage art so it covers the whole view at `aspect`
/// (width / height) while the ground line at `floor` stays put. The camera always shows at
/// least 1280×720 world units; screen shake moves it up to 6 units sideways.
pub fn stage_cover(aspect: f32, floor: f32) -> f32 {
    let aspect = aspect.clamp(0.5, 4.0);
    let (w, h) = if aspect >= 16.0 / 9.0 {
        (720.0 * aspect, 720.0)
    } else {
        (1280.0, 1280.0 / aspect)
    };
    // Art pixel y maps to y * s + floor * (1 - s): its top (360) and bottom (-360) must
    // reach the view's edges, and its sides the shaken view's.
    let top = (h / 2.0 - floor) / (360.0 - floor);
    let bottom = (h / 2.0 + floor) / (360.0 + floor);
    let sides = (w / 2.0 + 6.0) / 640.0;
    top.max(bottom).max(sides).max(1.0)
}
/// Fixed ticks a result stays on screen before a pad's A may start the next round, so
/// mashing jump through a knockout does not skip it.
pub const REMATCH_DELAY: u32 = 60;
/// Whether a fresh pad A may restart: only once the whole match is decided and its result
/// has shown for `REMATCH_DELAY`. A mid-match round result ignores the pad's A entirely —
/// the auto-advance owns the next round — while R, Start and the on-screen button keep
/// their reset-the-match meaning at any time.
pub fn pad_rematch_ready(arena: &Arena) -> bool {
    arena.match_outcome.is_some()
        && arena
            .ticks_since_outcome()
            .is_some_and(|t| t >= REMATCH_DELAY)
}
/// Two players share one screen: each gets a smaller stick and buttons in their own half.
const DOUBLE_SCALE: f32 = 0.8;
/// A safe-area inset never eats more than this share of the window height, so a bogus
/// report cannot push the controls past reach.
const MAX_SIDE_INSET: f32 = 0.12;
const MAX_BOTTOM_INSET: f32 = 0.10;

/// Safe-area insets (notch, rounded corners, system gesture bar) in fractions of the
/// window height. `left`/`right` shift the controls anchored at that screen edge inward,
/// `bottom` lifts every bottom-anchored control. Zero on desktops and plain browsers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}
impl Insets {
    pub const fn new(left: f32, right: f32, bottom: f32) -> Self {
        Self {
            left: if left > MAX_SIDE_INSET {
                MAX_SIDE_INSET
            } else {
                left
            },
            right: if right > MAX_SIDE_INSET {
                MAX_SIDE_INSET
            } else {
                right
            },
            bottom: if bottom > MAX_BOTTOM_INSET {
                MAX_BOTTOM_INSET
            } else {
                bottom
            },
        }
    }
}

/// Four attack buttons in a diamond like a pad's face buttons (X light, Y heavy, B dash), with
/// the bubble where A would be, ending at `right` (a fraction of the width). There is no jump
/// button: pushing the stick up jumps, up and forward jumps forward.
fn cluster(slot: usize, right: f32, scale: f32, aspect: f32, insets: Insets) -> [Button; 4] {
    let r = 0.068 * scale;
    let gap = 0.125 * scale;
    // A cluster anchored at the screen's right edge dodges the right safe area; one anchored
    // at the midline (two players, left half) has nothing to dodge. Everything bottom-anchored
    // rises by the bottom inset; x offsets are height fractions, hence the / aspect.
    let edge = if right >= 1.0 { insets.right } else { 0.0 };
    let x = right - (0.03 + gap + r) / aspect - edge / aspect;
    let y = 0.975 - gap - r - insets.bottom;
    let at = |bit, name, dx: f32, dy: f32| Button {
        slot,
        bit,
        name,
        x: x + dx * gap / aspect,
        y: y + dy * gap,
        r,
    };
    [
        at(SHOT, "SHOT", 0.0, 1.0),
        at(LIGHT, "LIGHT", -1.0, 0.0),
        at(HEAVY, "HEAVY", 0.0, -1.0),
        at(DASH, "DASH", 1.0, 0.0),
    ]
}

/// The words on an attack button.
pub fn button_label(bit: u16, lang: Lang) -> &'static str {
    match bit {
        LIGHT => lang.pick("轻拳", "LIGHT"),
        HEAVY => lang.pick("重拳", "HEAVY"),
        DASH => lang.pick("冲刺", "DASH"),
        SHOT => lang.pick("奶泡", "SHOT"),
        _ => "",
    }
}

/// The on-screen buttons for a window of this width / height. Drawing, touch hit-testing and
/// ownership all use this list.
pub fn buttons(layout: Layout, aspect: f32, insets: Insets) -> Vec<Button> {
    match layout {
        Layout::None => vec![],
        Layout::Single => cluster(0, 1.0, 1.0, aspect, insets).to_vec(),
        Layout::Double => [
            cluster(0, 0.5, DOUBLE_SCALE, aspect, insets),
            cluster(1, 1.0, DOUBLE_SCALE, aspect, insets),
        ]
        .concat(),
    }
}

/// The stick zones: the left half for one player; with two, each half up to its own buttons.
pub fn sticks(layout: Layout, aspect: f32, insets: Insets) -> Vec<StickZone> {
    let zone = |slot, left: f32, right: f32, scale: f32| {
        let radius = 0.11 * scale;
        // A zone at the screen's left edge starts after the left safe area, and no zone
        // reaches into the bottom band; the resting stick sits inset by both.
        let edge = if left <= 0.0 { insets.left } else { 0.0 };
        StickZone {
            slot,
            region: Region::new(
                left + edge / aspect,
                STICK_TOP,
                right - left - edge / aspect,
                1.0 - STICK_TOP - insets.bottom,
            ),
            home: (
                left + (0.04 + radius + edge) / aspect,
                0.955 - radius - insets.bottom,
            ),
            radius,
        }
    };
    match layout {
        Layout::None => vec![],
        Layout::Single => vec![zone(0, 0.0, 0.5, 1.0)],
        Layout::Double => (0..2)
            .map(|slot| {
                let left = slot as f32 * 0.5;
                // Up to the leftmost button of this player's cluster, with a small gap.
                let edge = cluster(slot, left + 0.5, DOUBLE_SCALE, aspect, insets)
                    .iter()
                    .map(|b| b.x - (b.r + 0.02) / aspect)
                    .fold(f32::INFINITY, f32::min);
                zone(slot, left, edge, DOUBLE_SCALE)
            })
            .collect(),
    }
}

pub fn button_at(
    layout: Layout,
    aspect: f32,
    x: f32,
    y: f32,
    insets: Insets,
) -> Option<(usize, u16)> {
    buttons(layout, aspect, insets)
        .into_iter()
        .find(|b| b.contains(aspect, x, y))
        .map(|b| (b.slot, b.bit))
}

pub fn stick_at(layout: Layout, aspect: f32, x: f32, y: f32, insets: Insets) -> Option<StickZone> {
    sticks(layout, aspect, insets)
        .into_iter()
        .find(|z| z.region.contains(x, y))
}

/// Controls from a stick's knob offset in units of its radius (screen y grows downward).
/// Sideways past 30 % walks; mostly up past 55 % jumps; mostly down past 55 % guards.
pub fn stick_bits(dx: f32, dy: f32) -> u16 {
    let mut bits = 0;
    if dx < -0.3 {
        bits |= LEFT;
    }
    if dx > 0.3 {
        bits |= RIGHT;
    }
    if -dy > 0.55 && -dy > 0.8 * dx.abs() {
        bits |= JUMP;
    }
    if dy > 0.55 && dy > 0.8 * dx.abs() {
        bits |= GUARD;
    }
    bits
}

/// Drags a stick's center along when the finger passes its rim, so turning around never needs
/// a long swipe back. Any units, as long as all three agree.
pub fn stick_follow(center: (f32, f32), finger: (f32, f32), radius: f32) -> (f32, f32) {
    let (dx, dy) = (finger.0 - center.0, finger.1 - center.1);
    let d = (dx * dx + dy * dy).sqrt();
    if d <= radius {
        return center;
    }
    let k = (d - radius) / d;
    (center.0 + dx * k, center.1 + dy * k)
}

/// Native menu entries (the browser uses a DOM menu so Chinese text renders).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    Cpu,
    Local,
    /// Desktop online play against naifrog-server (the game was started with `--server`).
    Online,
    /// Typed access key; Enter logs in.
    Key,
    Create,
    /// Typed room code; Enter joins.
    Join,
    Leave,
    Resume,
    Quit,
}

impl MenuItem {
    /// Rows that take typed text: letters and digits go into the field, not to shortcuts.
    pub fn typed(self) -> bool {
        matches!(self, Self::Key | Self::Join)
    }
}

/// Where desktop online play stands, for the native menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnlineState {
    /// No server was given: the menu offers no online entry.
    #[default]
    Unavailable,
    LoggedOut,
    LoggedIn,
    InRoom,
}

pub const MENU_ROWS: usize = 5;

pub fn menu_items(mode: Mode, online: OnlineState) -> Vec<MenuItem> {
    let mut items = if mode == Mode::Online {
        // Online play never pauses: RESUME only closes the menu over a seated match.
        match online {
            OnlineState::InRoom => vec![MenuItem::Resume, MenuItem::Leave],
            OnlineState::LoggedIn => vec![MenuItem::Create, MenuItem::Join],
            OnlineState::LoggedOut | OnlineState::Unavailable => vec![MenuItem::Key],
        }
    } else {
        let mut items = vec![MenuItem::Cpu, MenuItem::Local];
        if online != OnlineState::Unavailable {
            items.push(MenuItem::Online);
        }
        if mode != Mode::Menu {
            items.push(MenuItem::Resume);
        }
        items.push(MenuItem::Quit);
        return items;
    };
    items.extend([MenuItem::Cpu, MenuItem::Local, MenuItem::Quit]);
    items
}

pub const MENU_PANEL: Region = Region::new(0.22, 0.24, 0.56, 0.62);
/// The three help lines under the rows.
pub const MENU_FOOTER: Region = Region::new(0.25, 0.765, 0.5, 0.085);

pub fn menu_region(row: usize) -> Region {
    Region::new(0.26, 0.35 + row as f32 * 0.08, 0.48, 0.07)
}

pub fn menu_at(mode: Mode, online: OnlineState, x: f32, y: f32) -> Option<MenuItem> {
    menu_items(mode, online)
        .into_iter()
        .enumerate()
        .find(|(row, _)| menu_region(*row).contains(x, y))
        .map(|(_, item)| item)
}

/// A native menu row's label and hint. The access key is drawn as `key_len` stars, `code` is
/// the typed room code, and `editing` draws the typing caret on a text row.
pub fn menu_row(
    item: MenuItem,
    mode: Mode,
    lang: Lang,
    key_len: usize,
    code: &str,
    editing: bool,
) -> (String, &'static str) {
    let caret = if editing { "_" } else { "" };
    let fixed = |zh: &str, en: &str| lang.pick(zh, en).to_owned();
    match item {
        MenuItem::Cpu => (
            fixed("1  单机打人机", "1  VS CPU"),
            lang.pick(
                "你是 P1（金色），电脑是 P2（蓝色）。",
                "YOU ARE P1 (GOLD).  THE CPU IS P2 (BLUE).",
            ),
        ),
        MenuItem::Local => (
            fixed("2  本地同屏双人", "2  LOCAL 2 PLAYERS"),
            lang.pick(
                "P1 金色：WASD + F G H / 1 号手柄    P2 蓝色：方向键 + J K L / 2 号手柄",
                "P1 GOLD: WASD + F G H / PAD 1    P2 BLUE: ARROWS + J K L / PAD 2",
            ),
        ),
        MenuItem::Online => (
            fixed("3  私人联机", "3  PRIVATE ONLINE"),
            lang.pick(
                "和浏览器用同一台服务器、同样的匹配码",
                "THE SAME SERVER AND ROOM CODES AS THE BROWSER",
            ),
        ),
        MenuItem::Key => (
            format!(
                "{}{}{caret}",
                lang.pick("服务口令：", "ACCESS KEY: "),
                "*".repeat(key_len)
            ),
            lang.pick(
                "用键盘输入口令，再按 Enter",
                "TYPE THE KEY ON THE KEYBOARD, THEN ENTER",
            ),
        ),
        MenuItem::Create => (
            fixed("创建房间", "CREATE ROOM"),
            lang.pick(
                "拿到匹配码，对方在浏览器或这里加入",
                "GET A CODE; THE OTHER PLAYER JOINS IT IN THE BROWSER OR HERE",
            ),
        ),
        MenuItem::Join => (
            format!("{}{code}{caret}", lang.pick("加入房间：", "JOIN ROOM: ")),
            lang.pick(
                "输入 8 位匹配码，再按 Enter",
                "TYPE THE 8-CHARACTER ROOM CODE, THEN ENTER",
            ),
        ),
        MenuItem::Leave => (
            fixed("离开房间", "LEAVE ROOM"),
            lang.pick("房间会为双方关闭", "CLOSES THE ROOM FOR BOTH PLAYERS"),
        ),
        MenuItem::Resume if mode == Mode::Online => (
            fixed("返回对局", "RESUME"),
            lang.pick(
                "回到对战：联机对局不会暂停",
                "BACK TO THE FIGHT: ONLINE PLAY NEVER PAUSES",
            ),
        ),
        MenuItem::Resume => (
            fixed("返回对局", "RESUME"),
            lang.pick("回到暂停的对局", "BACK TO THE PAUSED MATCH"),
        ),
        MenuItem::Quit => (
            fixed("退出", "QUIT"),
            lang.pick("关闭游戏", "CLOSE THE GAME"),
        ),
    }
}

pub fn menu_title(lang: Lang) -> &'static str {
    lang.pick("奶蛙对打", "NAIFROG DUEL")
}

/// The line under the menu title outside online play, which shows the connection instead.
pub fn menu_current(mode: Mode, lang: Lang) -> String {
    match (mode, lang) {
        (Mode::Menu, _) => lang.pick("请选择模式", "CHOOSE A MODE").into(),
        (_, Lang::Zh) => format!("当前：{}（已暂停）", mode.name(lang)),
        (_, Lang::En) => format!("CURRENT: {} (PAUSED)", mode.name(lang)),
    }
}

/// The menu's fixed help lines: how to steer it, and how to switch the language.
pub fn menu_help(lang: Lang) -> [&'static str; 2] {
    [
        lang.pick(
            "方向键 + Enter、点击、触屏、手柄十字键 + A。Esc / B：返回，开始菜单按 Esc：退出",
            "ARROWS + ENTER, CLICK, TOUCH, PAD D-PAD + A.  ESC / B: BACK, ESC AT START: QUIT",
        ),
        lang.pick("F2：English", "F2: 中文"),
    ]
}

/// The menu's last line: which server desktop online play uses, why the given address was
/// refused, or how to name one.
pub fn menu_server(lang: Lang, origin: Option<&str>, problem: Option<&str>) -> String {
    match (origin, problem, lang) {
        (Some(origin), _, _) => format!("{}{origin}", lang.pick("联机服务器：", "ONLINE SERVER: ")),
        (None, Some(problem), _) => format!("--server{}{problem}", lang.pick("：", ": ")),
        (None, None, Lang::Zh) => {
            "私人联机：启动时加 --server http://IP:端口（浏览器用的那台服务器）".into()
        }
        (None, None, Lang::En) => {
            "PRIVATE ONLINE: START WITH --server http://IP:PORT (THE BROWSER'S SERVER)".into()
        }
    }
}

/// The desktop window's title.
pub fn window_title(lang: Lang) -> &'static str {
    lang.pick("奶蛙对打", "Naifrog Duel")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combat::ROUND_END_TICKS,
        i18n::{drawn, em_width},
    };
    const ALL: [u16; 8] = [LEFT, RIGHT, JUMP, GUARD, LIGHT, HEAVY, DASH, SHOT];
    const MODES: [Mode; 5] = [Mode::Menu, Mode::Cpu, Mode::Local, Mode::Online, Mode::Demo];

    fn center(r: Region) -> (f32, f32) {
        (r.left + r.width / 2.0, r.top + r.height / 2.0)
    }
    /// A live round-zero arena (`wins` and `match_outcome` set as asked) frozen `since`
    /// ticks after its result; `match_outcome: None` keeps the match open.
    fn result_arena(since: u32, match_outcome: Option<Outcome>) -> Arena {
        let mut a = Arena {
            intro: 0,
            ..Arena::default()
        };
        a.outcome = Some(Outcome::Winner(0));
        a.match_outcome = match_outcome;
        if let Some(Outcome::Winner(w)) = match_outcome {
            let mut wins = [0; 2];
            wins[w] = 2;
            a.wins = wins;
        }
        // ticks_since_outcome reads ticks - intro - dead - (TIME_LIMIT - remaining);
        // pin everything else so the requested offset falls out exactly.
        a.ticks = 210 + since;
        a
    }
    #[test]
    fn pad_a_never_restarts_a_match_still_being_fought() {
        let mut a = Arena {
            intro: 0,
            ..Arena::default()
        };
        assert!(!pad_rematch_ready(&a), "a live round ignores the pad's A");
        a.outcome = Some(Outcome::Winner(1));
        a.ticks = 210 + 5 * ROUND_END_TICKS;
        assert!(
            !pad_rematch_ready(&a),
            "no match decision, no pad restart either"
        );
    }
    #[test]
    fn pad_a_waits_for_a_decided_match_even_past_the_delay() {
        for since in 0..ROUND_END_TICKS {
            assert!(
                !pad_rematch_ready(&result_arena(since, None)),
                "a mid-match round result ignores the pad's A at tick {since}"
            );
        }
    }
    #[test]
    fn pad_a_restarts_only_after_the_match_result_has_shown() {
        for since in 0..REMATCH_DELAY {
            assert!(
                !pad_rematch_ready(&result_arena(since, Some(Outcome::Winner(0)))),
                "the decided match must show first (tick {since})"
            );
        }
        for since in REMATCH_DELAY..=ROUND_END_TICKS {
            assert!(
                pad_rematch_ready(&result_arena(since, Some(Outcome::Winner(0)))),
                "the decided match's result has shown (tick {since})"
            );
        }
        assert!(pad_rematch_ready(&result_arena(
            REMATCH_DELAY,
            Some(Outcome::Draw)
        )));
    }

    /// Tablet (4:3, 1180x820, 16:10) through phone (19.5:9) landscape shapes.
    const ASPECTS: [f32; 8] = [1.25, 1.333, 1.44, 1.6, 1.778, 2.0, 2.2, 2.4];

    fn bounds(b: Button, aspect: f32) -> Region {
        Region::new(b.x - b.r / aspect, b.y - b.r, 2.0 * b.r / aspect, 2.0 * b.r)
    }

    /// Every control a slot can reach: its buttons plus the stick's four directions.
    fn reachable(layout: Layout, aspect: f32, slot: usize) -> u16 {
        let buttons = buttons(layout, aspect, Insets::default())
            .into_iter()
            .filter(|b| b.slot == slot);
        let stick = sticks(layout, aspect, Insets::default())
            .iter()
            .any(|z| z.slot == slot);
        let from_stick = [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]
            .into_iter()
            .filter(|_| stick)
            .fold(0, |a, (dx, dy)| a | stick_bits(dx, dy));
        buttons.fold(from_stick, |a, b| a | b.bit)
    }

    /// A phone-style safe area: a gesture bar at the bottom and a deep notch band each side.
    const NOTCH: Insets = Insets {
        left: 0.08,
        right: 0.08,
        bottom: 0.05,
    };

    #[test]
    fn controls_fit_tablet_and_phone_screens_without_overlaps() {
        for insets in [Insets::default(), NOTCH] {
            for layout in [Layout::Single, Layout::Double] {
                for aspect in ASPECTS {
                    let buttons = buttons(layout, aspect, insets);
                    let zones = sticks(layout, aspect, insets);
                    let half = |slot: usize| match layout {
                        Layout::Double => (slot as f32 * 0.5, slot as f32 * 0.5 + 0.5),
                        _ => (0.0, 1.0),
                    };
                    for (i, b) in buttons.iter().enumerate() {
                        let r = bounds(*b, aspect);
                        let (lo, hi) = half(b.slot);
                        assert!(r.left >= lo && r.left + r.width <= hi, "{aspect} {b:?}");
                        assert!(r.top >= STICK_TOP && r.top + r.height <= 0.99, "{b:?}");
                        assert!(!r.overlaps(REMATCH));
                        for z in &zones {
                            assert!(!r.overlaps(z.region), "{aspect} {b:?} in {z:?}");
                        }
                        for o in &buttons[i + 1..] {
                            let d = ((b.x - o.x) * aspect).hypot(b.y - o.y);
                            assert!(d >= b.r + o.r + 0.02, "{aspect} {b:?} touches {o:?}");
                        }
                        assert_eq!(
                            button_at(layout, aspect, b.x, b.y, insets),
                            Some((b.slot, b.bit))
                        );
                    }
                    for (i, z) in zones.iter().enumerate() {
                        let (lo, hi) = half(z.slot);
                        let g = z.region;
                        assert!(g.left >= lo && g.left + g.width <= hi, "{aspect} {z:?}");
                        assert_eq!((g.top, g.top + g.height), (STICK_TOP, 1.0 - insets.bottom));
                        // Room to start a whole stick, and its resting hint fully on screen.
                        assert!(g.width * aspect >= 2.0 * z.radius, "{aspect} {z:?}");
                        assert!(g.contains(z.home.0, z.home.1), "{z:?}");
                        assert!(z.home.0 - z.radius / aspect >= 0.0 && z.home.1 + z.radius <= 1.0);
                        for o in &zones[i + 1..] {
                            assert!(!g.overlaps(o.region));
                        }
                        let found = stick_at(layout, aspect, z.home.0, z.home.1, insets);
                        assert_eq!(found.map(|f| f.slot), Some(z.slot));
                    }
                }
            }
        }
        assert!(
            buttons(Layout::None, 1.5, Insets::default()).is_empty()
                && sticks(Layout::None, 1.5, Insets::default()).is_empty()
        );
        assert_eq!(
            button_at(Layout::None, 1.5, 0.8, 0.8, Insets::default()),
            None
        );
        assert_eq!(
            stick_at(Layout::None, 1.5, 0.1, 0.8, Insets::default()),
            None
        );
    }

    #[test]
    fn safe_area_pushes_edge_controls_inward() {
        for layout in [Layout::Single, Layout::Double] {
            for aspect in ASPECTS {
                let plain = buttons(layout, aspect, Insets::default());
                let safe = buttons(layout, aspect, NOTCH);
                for (p, s) in plain.iter().zip(&safe) {
                    assert_eq!(p.slot, s.slot);
                    // Only the cluster at the screen's right edge dodges the right inset;
                    // the two-player left cluster is anchored at the midline.
                    let edge = match (layout, s.slot) {
                        (Layout::Double, 0) => 0.0,
                        _ => NOTCH.right,
                    };
                    assert!((p.x - s.x - edge / aspect).abs() < 1e-6, "{aspect} {s:?}");
                    assert!((p.y - s.y - NOTCH.bottom).abs() < 1e-6, "{aspect} {s:?}");
                }
                let zones = sticks(layout, aspect, NOTCH);
                for (slot, z) in zones.iter().enumerate() {
                    // Only the zone at the screen's left edge starts after the left inset.
                    let left = if slot == 0 { NOTCH.left } else { 0.0 };
                    let plain_home = sticks(layout, aspect, Insets::default())[slot].home;
                    assert!(
                        (z.region.left - slot as f32 * 0.5 - left / aspect).abs() < 1e-6,
                        "{aspect} {z:?}"
                    );
                    assert!(
                        (z.home.0 - plain_home.0 - left / aspect).abs() < 1e-6,
                        "{aspect} {z:?}"
                    );
                    assert!(
                        (z.home.1 - (plain_home.1 - NOTCH.bottom)).abs() < 1e-6,
                        "{aspect} {z:?}"
                    );
                }
                // A touch inside the notch bands hits nothing: no button, no stick.
                assert_eq!(
                    button_at(layout, aspect, 0.995, 0.97, NOTCH),
                    None,
                    "the right notch band must not be tappable"
                );
                assert_eq!(stick_at(layout, aspect, 0.0, 0.5, NOTCH), None);
            }
        }
    }

    #[test]
    fn absurd_safe_areas_are_clamped_to_reachable() {
        let huge = Insets::new(0.9, 0.9, 0.9);
        assert!(huge.left <= MAX_SIDE_INSET && huge.right <= MAX_SIDE_INSET);
        assert!(huge.bottom <= MAX_BOTTOM_INSET);
        for layout in [Layout::Single, Layout::Double] {
            for aspect in ASPECTS {
                for b in buttons(layout, aspect, huge) {
                    let r = bounds(b, aspect);
                    assert!(r.left >= 0.0 && r.left + r.width <= 1.0, "{aspect} {b:?}");
                    assert!(r.top >= STICK_TOP, "{aspect} {b:?}");
                }
                for z in sticks(layout, aspect, huge) {
                    assert!(z.region.contains(z.home.0, z.home.1), "{aspect} {z:?}");
                    assert!(
                        z.home.0 - z.radius / aspect >= 0.0 && z.home.1 + z.radius <= 1.0,
                        "{aspect} {z:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn one_player_steers_on_the_left_and_presses_pad_shaped_buttons_on_the_right() {
        for aspect in ASPECTS {
            let b = buttons(Layout::Single, aspect, Insets::default());
            let mut bits: Vec<u16> = b.iter().map(|b| b.bit).collect();
            bits.sort();
            assert_eq!(
                bits,
                [LIGHT, HEAVY, DASH, SHOT],
                "each attack once, no jump button"
            );
            assert!(b.iter().all(|b| b.slot == 0 && b.x - b.r / aspect > 0.5));
            // A pad's diamond: shot below (where A is), X light left, Y heavy above, B dash right.
            let at = |bit| b.iter().find(|b| b.bit == bit).unwrap();
            assert!(at(SHOT).y > at(LIGHT).y && at(HEAVY).y < at(LIGHT).y);
            assert!(at(LIGHT).x < at(SHOT).x && at(DASH).x > at(SHOT).x);
            let z = sticks(Layout::Single, aspect, Insets::default());
            assert_eq!(z.len(), 1);
            assert_eq!(
                (z[0].slot, z[0].region.left, z[0].region.width),
                (0, 0.0, 0.5)
            );
            assert_eq!(
                reachable(Layout::Single, aspect, 0),
                ALL.iter().fold(0, |a, b| a | b)
            );
            // The play area above the controls and the gap right of the stick do nothing.
            assert_eq!(
                button_at(Layout::Single, aspect, 0.75, 0.4, Insets::default()),
                None
            );
            assert_eq!(
                stick_at(Layout::Single, aspect, 0.75, 0.4, Insets::default()),
                None
            );
            assert_eq!(
                stick_at(Layout::Single, aspect, 0.2, 0.2, Insets::default()),
                None
            );
        }
    }

    #[test]
    fn two_players_each_get_a_stick_and_buttons_in_their_own_half() {
        for aspect in ASPECTS {
            for slot in 0..2 {
                assert_eq!(
                    buttons(Layout::Double, aspect, Insets::default())
                        .iter()
                        .filter(|b| b.slot == slot)
                        .count(),
                    4
                );
                assert_eq!(
                    sticks(Layout::Double, aspect, Insets::default())
                        .iter()
                        .filter(|z| z.slot == slot)
                        .count(),
                    1
                );
                assert_eq!(
                    reachable(Layout::Double, aspect, slot),
                    ALL.iter().fold(0, |a, b| a | b)
                );
            }
        }
    }

    #[test]
    fn stick_walks_sideways_jumps_up_guards_down_and_follows_the_finger() {
        assert_eq!(stick_bits(0.0, 0.0), 0);
        assert_eq!(stick_bits(0.2, 0.1), 0, "dead zone");
        assert_eq!(stick_bits(-1.0, 0.0), LEFT);
        assert_eq!(stick_bits(1.0, 0.0), RIGHT);
        assert_eq!(stick_bits(0.0, -1.0), JUMP);
        assert_eq!(stick_bits(0.0, 1.0), GUARD);
        assert_eq!(stick_bits(0.7, -0.7), RIGHT | JUMP, "jump forward");
        assert_eq!(stick_bits(-0.7, -0.7), LEFT | JUMP, "jump back");
        assert_eq!(
            stick_bits(0.95, -0.3),
            RIGHT,
            "a slightly raised thumb still only walks"
        );
        assert_eq!(stick_bits(-0.9, 0.35), LEFT);
        assert_eq!(stick_follow((0.0, 0.0), (0.5, 0.0), 1.0), (0.0, 0.0));
        assert_eq!(stick_follow((0.0, 0.0), (3.0, 0.0), 1.0), (2.0, 0.0));
        assert_eq!(stick_follow((1.0, 1.0), (1.0, -2.0), 1.0), (1.0, -1.0));
    }

    #[test]
    fn local_devices_control_only_their_own_seat() {
        assert_eq!(Mode::Cpu.slot_seat(0, None), Some(0));
        assert_eq!(
            Mode::Cpu.slot_seat(1, None),
            None,
            "nobody drives the CPU seat"
        );
        assert_eq!(Mode::Local.slot_seat(1, None), Some(1));
        // Joining a room may make you the blue P2; all local input follows that seat.
        assert_eq!(Mode::Online.slot_seat(0, Some(1)), Some(1));
        assert_eq!(Mode::Online.slot_seat(1, Some(1)), None);
        assert_eq!(
            Mode::Online.slot_seat(0, None),
            None,
            "no seat before welcome"
        );
        assert_eq!(Mode::Online.slot_seat(0, Some(7)), None);
        for device in 0..2 {
            assert_eq!(Mode::Cpu.device_slot(device), Some(0));
            assert_eq!(Mode::Online.device_slot(device), Some(0));
            assert_eq!(Mode::Local.device_slot(device), Some(device));
            assert_eq!(Mode::Menu.device_slot(device), None);
            assert_eq!(Mode::Demo.device_slot(device), None);
        }
        for mode in MODES {
            let slots = buttons(mode.layout(), 1.5, Insets::default())
                .into_iter()
                .map(|b| b.slot);
            for slot in slots.chain(
                sticks(mode.layout(), 1.5, Insets::default())
                    .into_iter()
                    .map(|z| z.slot),
            ) {
                assert!(mode.slot_seat(slot, Some(1)).is_some(), "{mode:?}");
            }
        }
    }

    #[test]
    fn roles_follow_seats_and_never_name_two_players_you() {
        let en = Lang::En;
        assert_eq!(Mode::Cpu.tag(0, None, en), "P1 YOU");
        assert_eq!(Mode::Cpu.tag(1, None, en), "P2 CPU");
        assert_eq!(Mode::Online.tag(1, Some(1), en), "P2 YOU");
        assert_eq!(Mode::Online.tag(0, Some(1), en), "P1 RIVAL");
        assert_eq!(Mode::Online.tag(0, None, en), "P1");
        assert_eq!(Mode::Local.tag(1, None, en), "P2");
        assert_eq!(Mode::Demo.tag(0, None, en), "P1 CPU");
        let zh = Lang::Zh;
        assert_eq!(Mode::Cpu.tag(0, None, zh), "P1 你");
        assert_eq!(Mode::Cpu.tag(1, None, zh), "P2 电脑");
        assert_eq!(Mode::Online.tag(0, Some(1), zh), "P1 对手");
        assert_eq!(Mode::Local.tag(1, None, zh), "P2");
        for mode in MODES {
            for you in [None, Some(0), Some(1)] {
                let yous = (0..2)
                    .filter(|s| mode.role(*s, you) == Some(Role::You))
                    .count();
                assert!(yous <= 1, "{mode:?} {you:?}");
            }
        }
        let codes: Vec<u8> = MODES.iter().map(|m| m.code()).collect();
        assert_eq!(codes, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn grace_note_names_the_disconnected_rival_and_pads_the_seconds() {
        let en = Lang::En;
        assert_eq!(grace_note(en, 300), "RIVAL DISCONNECTED 5:00");
        assert_eq!(grace_note(en, 299), "RIVAL DISCONNECTED 4:59");
        assert_eq!(grace_note(en, 60), "RIVAL DISCONNECTED 1:00");
        assert_eq!(grace_note(en, 9), "RIVAL DISCONNECTED 0:09");
        assert_eq!(grace_note(en, 0), "RIVAL DISCONNECTED 0:00");
        assert_eq!(grace_note(Lang::Zh, 299), "对手已断线，席位保留 4:59");
        for lang in LANGS {
            assert!(
                em_width(&grace_note(lang, 300)) <= STATUS_EM,
                "fits the status line"
            );
        }
    }

    #[test]
    fn banner_counts_the_resume_down_three_two_one() {
        use crate::combat::Arena;
        let mut a = Arena {
            intro: 0,
            ..Arena::default()
        };
        a.resume = 180;
        assert_eq!(banner(Lang::En, &a).as_deref(), Some("3"));
        a.resume = 121;
        assert_eq!(
            banner(Lang::En, &a).as_deref(),
            Some("3"),
            "each second shows until it runs to zero"
        );
        a.resume = 120;
        assert_eq!(banner(Lang::En, &a).as_deref(), Some("2"));
        a.resume = 61;
        assert_eq!(banner(Lang::En, &a).as_deref(), Some("2"));
        a.resume = 60;
        assert_eq!(banner(Lang::En, &a).as_deref(), Some("1"));
        a.resume = 1;
        assert_eq!(banner(Lang::En, &a).as_deref(), Some("1"));
        a.resume = 0;
        assert_eq!(
            banner(Lang::En, &a),
            None,
            "the fight continues without a word"
        );
    }

    #[test]
    fn banner_announces_the_intro_and_how_the_round_ended() {
        use crate::combat::{Arena, INTRO_TICKS, Outcome, TIME_LIMIT};
        use crate::controls::Input;
        let empty = [Input::default(); 2];
        let mut a = Arena::default();
        while a.intro > INTRO_TICKS {
            assert_eq!(
                banner(Lang::En, &a),
                None,
                "the versus screen is its own overlay"
            );
            a.step(empty);
        }
        while a.intro > INTRO_TICKS / 3 {
            assert_eq!(banner(Lang::En, &a).as_deref(), Some("ROUND 1"));
            a.step(empty);
        }
        while a.intro > 0 {
            assert_eq!(banner(Lang::En, &a).as_deref(), Some("FIGHT!"));
            a.step(empty);
        }
        a.step(empty);
        assert_eq!(
            banner(Lang::En, &a),
            None,
            "no announcement while the fight runs"
        );
        // The result banner reads the decided state; combat tests prove how it is reached.
        a.outcome = Some(Outcome::Winner(0));
        a.fighters[0].hp = 80;
        a.fighters[1].hp = 0;
        assert_eq!(banner(Lang::En, &a).as_deref(), Some("K.O."));
        a.fighters[0].hp = 100;
        assert_eq!(
            banner(Lang::En, &a).as_deref(),
            Some("PERFECT!"),
            "an unharmed winner is perfect"
        );
        a.fighters[0].hp = 80;
        a.fighters[1].hp = 30;
        a.remaining = 0;
        assert_eq!(banner(Lang::En, &a).as_deref(), Some("TIME OVER"));
        a.outcome = Some(Outcome::Draw);
        assert_eq!(banner(Lang::En, &a).as_deref(), Some("DRAW"));
        assert_eq!(a.remaining, 0, "sanity: the timeout state above is real");
        assert_eq!(TIME_LIMIT, 99 * 60);
    }

    #[test]
    fn banner_numbers_the_round_and_names_the_final_one() {
        let mut a = Arena {
            intro: INTRO_TICKS,
            ..Arena::default()
        };
        assert_eq!(
            banner(Lang::En, &a).as_deref(),
            Some("ROUND 1"),
            "a fresh match opens round 1"
        );
        a.game = 2;
        a.wins = [0, 0];
        assert_eq!(
            banner(Lang::En, &a).as_deref(),
            Some("ROUND 3"),
            "two draws lead with round 3"
        );
        // Any round that can hand someone the second win, or the fifth and last
        // round of the match, is announced as the final one.
        a.wins = [1, 0];
        assert_eq!(banner(Lang::En, &a).as_deref(), Some("FINAL ROUND"));
        a.wins = [1, 1];
        assert_eq!(banner(Lang::En, &a).as_deref(), Some("FINAL ROUND"));
        a.wins = [0, 0];
        a.game = 4;
        assert_eq!(
            banner(Lang::En, &a).as_deref(),
            Some("FINAL ROUND"),
            "the fifth round is the last"
        );
        assert_eq!(banner(Lang::Zh, &a).as_deref(), Some("决胜局"));
        a.game = 1;
        assert_eq!(banner(Lang::Zh, &a).as_deref(), Some("第二局"));
        a.intro = 1;
        assert_eq!(banner(Lang::Zh, &a).as_deref(), Some("开打！"));
        a.outcome = Some(Outcome::Draw);
        assert_eq!(banner(Lang::Zh, &a).as_deref(), Some("平局"));
    }

    #[test]
    fn status_names_the_winner_from_the_viewers_side() {
        let en = Lang::En;
        let won = Some(Outcome::Winner(1));
        assert_eq!(
            Mode::Cpu.status(en, false, won, None, [true; 2]),
            "P2 CPU WINS!  R / START / PAD A: REMATCH"
        );
        assert_eq!(
            Mode::Cpu.status(en, false, Some(Outcome::Winner(0)), None, [true; 2]),
            "YOU WIN!  R / START / PAD A: REMATCH"
        );
        assert_eq!(
            Mode::Online.status(en, false, won, Some(1), [true; 2]),
            "YOU WIN!  BOTH: R / START / PAD A"
        );
        assert_eq!(
            Mode::Online.status(en, false, won, Some(0), [true; 2]),
            "P2 RIVAL WINS!  BOTH: R / START / PAD A"
        );
        assert_eq!(
            Mode::Local.status(en, false, won, None, [true; 2]),
            "P2 WINS!  R / START / PAD A: REMATCH"
        );
        assert!(
            Mode::Online
                .status(en, false, None, Some(1), [true, false])
                .contains("P2 BLUE  WAITING")
        );
        assert_eq!(
            Mode::Cpu.status(en, true, won, None, [true; 2]),
            "VS CPU: PAUSED"
        );
        assert_eq!(
            Mode::Menu.status(en, false, won, None, [true; 2]),
            "CHOOSE A MODE"
        );
        assert!(
            Mode::Online
                .controls(en, Some(1), false)
                .starts_with("YOU = P2 BLUE")
        );
        assert!(
            Mode::Cpu
                .controls(en, None, false)
                .starts_with("YOU = P1 GOLD")
        );
        assert!(
            Mode::Cpu
                .controls(en, None, true)
                .starts_with("YOU = P1 GOLD: LEFT STICK")
        );
        assert!(!Mode::Local.controls(en, None, true).contains("ARROWS"));
        let zh = Lang::Zh;
        assert_eq!(
            Mode::Online.status(zh, false, won, Some(0), [true; 2]),
            "P2 对手获胜！ 双方按 R / Start / 手柄 A 重开"
        );
        assert_eq!(
            Mode::Cpu.status(zh, false, Some(Outcome::Winner(0)), None, [true; 2]),
            "你赢了！ R / Start / 手柄 A：再来一局"
        );
        assert_eq!(
            Mode::Local.status(zh, false, Some(Outcome::Draw), None, [true; 2]),
            "平局！ R / Start / 手柄 A：再来一局"
        );
        assert_eq!(
            Mode::Online.status(zh, false, None, Some(1), [true, false]),
            "联机：你是 P2（蓝色） · 等待对方 / 断线暂停"
        );
        assert_eq!(
            Mode::Cpu.status(zh, true, None, None, [true; 2]),
            "单机打人机：已暂停"
        );
        assert!(
            Mode::Online
                .controls(zh, Some(1), false)
                .starts_with("你 = P2 蓝色：")
        );
        assert!(!Mode::Local.controls(zh, None, true).contains("方向键"));
    }

    const LANGS: [Lang; 2] = [Lang::Zh, Lang::En];
    /// The widest status line, in em: the 41-character English online waiting line.
    const STATUS_EM: f32 = 24.6 + 1e-3;

    /// Every canvas text a language can produce: the line under the clock, the controls
    /// line, the banners, the tags and the button labels.
    fn canvas_texts(lang: Lang) -> Vec<String> {
        let outcomes = [
            None,
            Some(Outcome::Draw),
            Some(Outcome::Winner(0)),
            Some(Outcome::Winner(1)),
        ];
        let mut texts = vec![
            grace_note(lang, 299),
            combo_note(lang, 12),
            rematch_label(lang).into(),
            loading_note(lang).into(),
        ];
        for mode in MODES {
            for you in [None, Some(0), Some(1)] {
                for touch in [false, true] {
                    texts.push(mode.controls(lang, you, touch));
                }
                for seat in 0..2 {
                    texts.push(mode.tag(seat, you, lang));
                }
                for outcome in outcomes {
                    for connected in [[true; 2], [true, false], [false, true]] {
                        for paused in [false, true] {
                            texts.push(mode.status(lang, paused, outcome, you, connected));
                        }
                    }
                }
            }
        }
        for bit in [LIGHT, HEAVY, DASH, SHOT] {
            texts.push(button_label(bit, lang).into());
        }
        let mut a = Arena {
            intro: INTRO_TICKS,
            ..Arena::default()
        };
        for game in 0..5 {
            a.game = game;
            texts.extend(banner(lang, &a));
        }
        a.intro = 1;
        texts.extend(banner(lang, &a));
        a.outcome = Some(Outcome::Winner(0));
        for (hp, rival) in [(100, 0), (80, 0), (80, 30)] {
            a.fighters[0].hp = hp;
            a.fighters[1].hp = rival;
            texts.extend(banner(lang, &a));
        }
        a.outcome = Some(Outcome::Draw);
        texts.extend(banner(lang, &a));
        texts
    }

    #[test]
    fn canvas_text_is_drawable_and_chinese_keeps_only_symbols_in_latin() {
        for lang in LANGS {
            for text in canvas_texts(lang) {
                if let Some(c) = text.chars().find(|c| !drawn(*c)) {
                    panic!("{lang:?}: the canvas cannot draw {c:?} in {text}");
                }
            }
        }
        // Seats, keys and pad buttons keep their Latin names in Chinese; nothing else may.
        let kept = [
            "P1", "P2", "R", "Start", "A", "D", "W", "S", "F", "G", "H", "J", "K", "L", "T", "I",
            "Esc", "F1",
        ];
        for text in canvas_texts(Lang::Zh) {
            let words = text.split(|c: char| !c.is_ascii_alphanumeric());
            for word in words.filter(|w| w.chars().any(|c| c.is_ascii_alphabetic())) {
                assert!(kept.contains(&word), "{word:?} in a Chinese text: {text}");
            }
        }
    }

    #[test]
    fn status_fits_left_of_the_page_buttons() {
        // The 22 px status line is centred under the clock; the page's 菜单 / 房间 / 全屏 / 操作
        // buttons share its row on the right. No line may outgrow the English online waiting
        // line (41 Fira Mono characters, 24.6 em), which clears them on a ~1080 px wide
        // window. Han characters and full-width punctuation are 1 em.
        let outcomes = [
            None,
            Some(Outcome::Draw),
            Some(Outcome::Winner(0)),
            Some(Outcome::Winner(1)),
        ];
        for lang in LANGS {
            for mode in MODES {
                for outcome in outcomes {
                    for you in [None, Some(0), Some(1)] {
                        for connected in [[true; 2], [true, false], [false, true]] {
                            for paused in [false, true] {
                                let text = mode.status(lang, paused, outcome, you, connected);
                                let em = em_width(&text);
                                assert!(em <= STATUS_EM, "{em} em: {text}");
                            }
                        }
                    }
                }
            }
            assert!(em_width(&grace_note(lang, 300)) <= STATUS_EM);
        }
        // No Chinese controls line is wider than the widest English one.
        let widest = |lang| {
            MODES
                .iter()
                .flat_map(|m| [false, true].map(|t| m.controls(lang, Some(1), t)))
                .map(|t| em_width(&t))
                .fold(0.0, f32::max)
        };
        assert!(widest(Lang::Zh) <= widest(Lang::En));
    }

    #[test]
    fn menu_text_fits_its_rows_in_both_languages() {
        // At the default 1280 x 720 window: rows are 22 px labels over 12 px hints, the footer
        // three 12 px lines.
        let row_px = menu_region(0).width * 1280.0;
        let footer_px = MENU_FOOTER.width * 1280.0;
        let items = [
            MenuItem::Cpu,
            MenuItem::Local,
            MenuItem::Online,
            MenuItem::Key,
            MenuItem::Create,
            MenuItem::Join,
            MenuItem::Leave,
            MenuItem::Resume,
            MenuItem::Quit,
        ];
        for lang in LANGS {
            for mode in MODES {
                for item in items {
                    let (label, hint) = menu_row(item, mode, lang, 24, "ABCDEFGH", true);
                    assert!(em_width(&label) * 22.0 <= row_px, "{label}");
                    assert!(em_width(hint) * 12.0 <= row_px, "{hint}");
                    assert!(
                        label.chars().chain(hint.chars()).all(drawn),
                        "{label} {hint}"
                    );
                }
                let current = menu_current(mode, lang);
                assert!(em_width(&current) * 14.0 <= MENU_PANEL.width * 1280.0);
                assert!(current.chars().all(drawn), "{current}");
            }
            let server = [
                menu_server(lang, Some("http://192.168.1.20:8080"), None),
                menu_server(lang, None, Some("use http:// or https://")),
                menu_server(lang, None, None),
            ];
            for line in menu_help(lang).map(String::from).into_iter().chain(server) {
                assert!(em_width(&line) * 12.0 <= footer_px, "{line}");
                assert!(line.chars().all(drawn), "{line}");
            }
            assert!(menu_title(lang).chars().all(drawn));
            assert!(window_title(lang).chars().all(drawn));
        }
        let (label, _) = menu_row(MenuItem::Key, Mode::Online, Lang::Zh, 3, "", true);
        assert_eq!(label, "服务口令：***_");
        let (label, _) = menu_row(MenuItem::Join, Mode::Online, Lang::En, 0, "ABCD", false);
        assert_eq!(label, "JOIN ROOM: ABCD");
        assert_eq!(
            menu_current(Mode::Cpu, Lang::Zh),
            "当前：单机打人机（已暂停）"
        );
        assert_eq!(menu_help(Lang::Zh)[1], "F2：English");
    }

    #[test]
    fn menu_rows_hit_test_and_resume_only_exists_with_a_match() {
        let off = OnlineState::Unavailable;
        assert!(!menu_items(Mode::Menu, off).contains(&MenuItem::Resume));
        assert!(menu_items(Mode::Cpu, off).contains(&MenuItem::Resume));
        let states = [
            OnlineState::Unavailable,
            OnlineState::LoggedOut,
            OnlineState::LoggedIn,
            OnlineState::InRoom,
        ];
        for mode in MODES {
            for online in states {
                let items = menu_items(mode, online);
                assert!(items.len() <= MENU_ROWS, "{mode:?} {online:?}");
                for (row, item) in items.iter().enumerate() {
                    let r = menu_region(row);
                    assert!(MENU_PANEL.contains(r.left, r.top));
                    assert!(MENU_PANEL.contains(r.left + r.width, r.top + r.height));
                    let (x, y) = center(r);
                    assert_eq!(menu_at(mode, online, x, y), Some(*item));
                    if row > 0 {
                        assert!(!r.overlaps(menu_region(row - 1)));
                    }
                    assert!(!r.overlaps(MENU_FOOTER));
                }
                assert_eq!(menu_at(mode, online, 0.1, 0.5), None);
            }
        }
    }

    #[test]
    fn stage_art_covers_every_window_shape_with_the_ground_fixed() {
        let floor = -180.0;
        for aspect in [4.0 / 3.0, 5.0 / 3.0, 16.0 / 9.0, 2.1, 21.0 / 9.0] {
            let s = stage_cover(aspect, floor);
            let (w, h) = if aspect >= 16.0 / 9.0 {
                (720.0 * aspect, 720.0)
            } else {
                (1280.0, 1280.0 / aspect)
            };
            let y = |art: f32| art * s + floor * (1.0 - s);
            assert!(y(360.0) >= h / 2.0 - 1e-3, "{aspect}: top");
            assert!(y(-360.0) <= -h / 2.0 + 1e-3, "{aspect}: bottom");
            assert!(640.0 * s >= w / 2.0 + 6.0 - 1e-3, "{aspect}: sides");
            assert!((y(floor) - floor).abs() < 1e-3, "{aspect}: ground moved");
        }
        // 16:9 needs only the shake margin.
        assert!(stage_cover(16.0 / 9.0, floor) < 1.01);
    }

    #[test]
    fn online_entries_follow_the_login_and_the_room() {
        let has = |mode, online, item| menu_items(mode, online).contains(&item);
        assert!(!has(Mode::Menu, OnlineState::Unavailable, MenuItem::Online));
        assert!(has(Mode::Menu, OnlineState::LoggedOut, MenuItem::Online));
        assert_eq!(
            menu_items(Mode::Online, OnlineState::LoggedOut)[0],
            MenuItem::Key
        );
        assert_eq!(
            menu_items(Mode::Online, OnlineState::LoggedIn)[..2],
            [MenuItem::Create, MenuItem::Join]
        );
        assert_eq!(
            menu_items(Mode::Online, OnlineState::InRoom)[..2],
            [MenuItem::Resume, MenuItem::Leave]
        );
        // From online play, the local modes and quit stay one press away.
        for online in [
            OnlineState::LoggedOut,
            OnlineState::LoggedIn,
            OnlineState::InRoom,
        ] {
            for item in [MenuItem::Cpu, MenuItem::Local, MenuItem::Quit] {
                assert!(has(Mode::Online, online, item));
            }
        }
        assert!(MenuItem::Key.typed() && MenuItem::Join.typed() && !MenuItem::Create.typed());
    }
}
