//! Renderer-independent, deterministic 60 Hz fighting simulation.
use crate::controls::{DASH, GUARD, HEAVY, Input, JUMP, LEFT, LIGHT, RIGHT, SHOT};
pub const UNIT: i32 = 1000;
pub // Matches the broad body of the supplied sprites; keep both faces readable at contact.
const HALF: i32 = 70 * UNIT;
pub const WALL: i32 = 570 * UNIT;
pub const GRAVITY: i32 = 850;
pub const TIME_LIMIT: u32 = 99 * 60;
/// Every round opens with a countdown ("ROUND N", then "FIGHT!"): inputs are dead and
/// the clock is frozen until the tick it reaches zero.
pub const INTRO_TICKS: u32 = 90;
/// The versus screen ("P1 VS P2") that precedes round one of every match. It rides
/// the same intro countdown, so it freezes input and the clock the same way.
pub const VS_TICKS: u32 = 120;
/// How long a round's result stays on screen before the next round introduces itself.
pub const ROUND_END_TICKS: u32 = 120;
/// After an interrupted round resumes (a dropped seat came back), a 3-2-1 countdown
/// plays first: three seconds of dead input and a frozen clock, like the intro, so
/// nobody is hit while their screen was frozen mid-fight.
pub const RESUME_TICKS: u32 = 180;
/// Both fighters, the milk bubbles and the clock freeze for this many ticks after an
/// unblocked hit connects. Blocked hits do not stop the game.
pub const HIT_STOP_TICKS: u8 = 4;
// Milk bubble: flight per tick, half width, spawn distance in front, height band.
pub const SHOT_SPEED: i32 = 8 * UNIT;
pub const SHOT_HALF: i32 = 35 * UNIT;
pub const SHOT_SPAWN: i32 = 90 * UNIT;
pub const SHOT_BOTTOM: i32 = 50 * UNIT;
pub const SHOT_TOP: i32 = 110 * UNIT;
/// A bubble past this distance from the centre has left the stage.
pub const SHOT_LIMIT: i32 = 700 * UNIT;
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Light,
    Heavy,
    Dash,
    /// Throws a milk bubble on its first active frame; the bubble carries the damage.
    Shot,
    /// Belly press from a jump: hits below and in front, ends on landing.
    Air,
}
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveData {
    pub startup: u32,
    pub active: u32,
    pub recovery: u32,
    pub damage: i32,
    pub reach: i32,
    pub knockdown: bool,
}
impl Move {
    pub fn data(self) -> MoveData {
        match self {
            Self::Light => MoveData {
                startup: 8,
                active: 4,
                recovery: 10,
                damage: 8,
                reach: 145 * UNIT,
                knockdown: false,
            },
            Self::Heavy => MoveData {
                startup: 14,
                active: 6,
                recovery: 18,
                damage: 16,
                reach: 180 * UNIT,
                knockdown: true,
            },
            Self::Dash => MoveData {
                startup: 12,
                active: 10,
                recovery: 20,
                damage: 14,
                reach: 132 * UNIT,
                knockdown: true,
            },
            Self::Shot => MoveData {
                startup: 14,
                active: 1,
                recovery: 24,
                damage: 10,
                reach: 0,
                knockdown: false,
            },
            Self::Air => MoveData {
                startup: 5,
                active: 14,
                recovery: 0,
                damage: 10,
                reach: 150 * UNIT,
                knockdown: false,
            },
        }
    }
    pub fn total(self) -> u32 {
        let d = self.data();
        d.startup + d.active + d.recovery
    }
}
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Ready,
    Guard,
    Attack(Move),
    Hurt,
    Knockdown,
    Getup,
    Ko,
}
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Idle,
    Walk,
    Jump,
    Guard,
    Hurt,
    Knockdown,
    Getup,
    Light,
    Heavy,
    Dash,
    Shot,
    Air,
}
impl Pose {
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Walk => "walk",
            Self::Jump => "jump",
            Self::Guard => "guard",
            Self::Hurt => "hurt",
            Self::Knockdown => "knockdown",
            Self::Getup => "getup",
            Self::Light => "light",
            Self::Heavy => "heavy",
            Self::Dash => "dash",
            Self::Shot => "shot",
            Self::Air => "air",
        }
    }
}
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fighter {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub recoil: i32,
    pub vy: i32,
    pub facing: i32,
    pub hp: i32,
    pub state: State,
    pub age: u32,
    pub visual_age: u32,
    pub hit: bool,
    /// The belly press was used during the current jump.
    pub air_used: bool,
    /// Unblocked hits taken in the current stun chain; blocked hits never count and
    /// recovering to `Ready` clears it. The HUD shows it on the attacker's side.
    pub combo_taken: u8,
}
impl Fighter {
    fn new(x: i32, facing: i32) -> Self {
        Self {
            x: x * UNIT,
            y: 0,
            vx: 0,
            recoil: 0,
            vy: 0,
            facing,
            hp: 100,
            state: State::Ready,
            age: 0,
            visual_age: 0,
            hit: false,
            air_used: false,
            combo_taken: 0,
        }
    }
    fn enter(&mut self, state: State) {
        self.state = state;
        self.age = 0;
        self.visual_age = 0;
        if state == State::Ready {
            // Back on their feet: the stun chain, and with it the combo, is over.
            self.combo_taken = 0;
        }
    }
    pub fn active(&self) -> bool {
        if let State::Attack(m) = self.state {
            let d = m.data();
            self.age >= d.startup && self.age < d.startup + d.active && !self.hit
        } else {
            false
        }
    }
    pub fn vulnerable(&self) -> bool {
        !matches!(self.state, State::Knockdown | State::Getup | State::Ko)
    }
    pub fn pose(&self) -> Pose {
        match self.state {
            State::Ready if self.y > 0 => Pose::Jump,
            State::Ready if self.vx != 0 => Pose::Walk,
            State::Ready => Pose::Idle,
            State::Guard => Pose::Guard,
            State::Attack(Move::Light) => Pose::Light,
            State::Attack(Move::Heavy) => Pose::Heavy,
            State::Attack(Move::Dash) => Pose::Dash,
            State::Attack(Move::Shot) => Pose::Shot,
            State::Attack(Move::Air) => Pose::Air,
            State::Hurt => Pose::Hurt,
            State::Knockdown | State::Ko => Pose::Knockdown,
            State::Getup => Pose::Getup,
        }
    }
    pub fn attack_box(&self) -> Option<Rect> {
        if !self.active() {
            return None;
        }
        let State::Attack(m) = self.state else {
            return None;
        };
        let far = m.data().reach;
        let (near, bottom, top) = match m {
            Move::Shot => return None,
            Move::Air => (10 * UNIT, -20 * UNIT, 120 * UNIT),
            _ => (25 * UNIT, 60 * UNIT, 205 * UNIT),
        };
        Some(Rect {
            left: self.x + if self.facing > 0 { near } else { -far },
            right: self.x + if self.facing > 0 { far } else { -near },
            bottom: self.y + bottom,
            top: self.y + top,
        })
    }
    /// Airborne fighters tuck their legs, so a well-timed jump clears a milk bubble.
    pub fn hurt_box(&self) -> Rect {
        let (half, feet) = if self.y > 0 {
            (50 * UNIT, 40 * UNIT)
        } else {
            (HALF, 10 * UNIT)
        };
        Rect {
            left: self.x - half,
            right: self.x + half,
            bottom: self.y + feet,
            top: self.y + 230 * UNIT,
        }
    }
}
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub right: i32,
    pub bottom: i32,
    pub top: i32,
}
impl Rect {
    pub fn overlaps(self, b: Self) -> bool {
        self.left < b.right && self.right > b.left && self.bottom < b.top && self.top > b.bottom
    }
}
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Winner(usize),
    Draw,
}
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub victim: usize,
    pub x: i32,
    pub y: i32,
    pub blocked: bool,
    /// A milk bubble landed (rather than a body hit).
    pub shot: bool,
}
/// A milk bubble in flight. Its height band is fixed, so only `x` moves.
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shot {
    pub x: i32,
    /// Flight direction: 1 or -1.
    pub dir: i32,
    /// Ticks since it appeared; only the renderer uses it.
    pub age: u32,
}
impl Shot {
    pub fn rect(&self) -> Rect {
        Rect {
            left: self.x - SHOT_HALF,
            right: self.x + SHOT_HALF,
            bottom: SHOT_BOTTOM,
            top: SHOT_TOP,
        }
    }
}
/// One landed hit, applied after both attacks were read from the same snapshot.
struct Contact {
    victim: usize,
    damage: i32,
    knockdown: bool,
    direction: i32,
    blocked: bool,
}
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arena {
    pub fighters: [Fighter; 2],
    pub ticks: u32,
    pub remaining: u32,
    pub outcome: Option<Outcome>,
    /// Ticks of round introduction left: input is dead and the clock frozen while it counts down.
    pub intro: u32,
    /// Ticks of resume countdown left after an interrupted round continues: same rules
    /// as the intro. Zero in every match that was never interrupted.
    #[cfg_attr(feature = "net", serde(default))]
    pub resume: u32,
    /// Ticks this round spent frozen on the resume countdown or hit-stop: `ticks`
    /// advances through them but they are not played time, so `ticks_since_outcome`
    /// subtracts them along with the introduction.
    #[cfg_attr(feature = "net", serde(default))]
    pub dead: u32,
    /// Rounds completed in this match: `game` is the 0-based index of the round being fought.
    pub game: u8,
    /// Round wins per seat; first to 2 (or a majority after five rounds) takes the match.
    pub wins: [u8; 2],
    /// Set once the match itself is decided; `restart` is the only way past it.
    pub match_outcome: Option<Outcome>,
    /// Each seat's milk bubble, at most one each.
    pub shots: [Option<Shot>; 2],
    /// Hit-stop ticks left: while any remain nobody moves, nothing flies, the clock
    /// waits and input edges are dropped, exactly like the round introduction.
    pub hitstop: u8,
}
impl Default for Arena {
    fn default() -> Self {
        Self {
            fighters: [Fighter::new(-235, 1), Fighter::new(235, -1)],
            ticks: 0,
            remaining: TIME_LIMIT,
            outcome: None,
            intro: VS_TICKS + INTRO_TICKS,
            resume: 0,
            dead: 0,
            game: 0,
            wins: [0; 2],
            match_outcome: None,
            shots: [None; 2],
            hitstop: 0,
        }
    }
}
impl Arena {
    pub fn restart(&mut self) {
        *self = Self::default();
    }
    /// Arm the post-interruption countdown: the next `RESUME_TICKS` steps play dead input
    /// on a frozen clock, whatever phase the round was frozen in continues afterwards.
    /// Only a round that was actually being fought counts back in: versus, the round
    /// introduction and a shown result already freeze input and clock their own way, so
    /// they just continue where they froze.
    pub fn resume_now(&mut self) {
        if self.outcome.is_none() && self.intro == 0 {
            self.resume = RESUME_TICKS;
        }
    }
    /// Start the next round of the same match: everything resets, the round counters carry.
    /// Only round one plays the versus screen, so the intro drops back to its short form.
    fn next_round(&mut self) {
        let (game, wins, match_outcome) = (self.game, self.wins, self.match_outcome);
        *self = Self::default();
        self.game = game + 1;
        self.wins = wins;
        self.match_outcome = match_outcome;
        self.intro = INTRO_TICKS;
    }
    /// Fixed ticks since the round was decided; `None` while it is still fought. After the
    /// outcome `ticks` keeps counting while `remaining` stays frozen, so this needs no extra
    /// state and works the same on server snapshots. The introduction's ticks are not on
    /// the clock, so they are subtracted along with the unfought remainder; round one's
    /// introduction includes the versus screen, later rounds' do not.
    pub fn ticks_since_outcome(&self) -> Option<u32> {
        let intro = INTRO_TICKS + if self.game == 0 { VS_TICKS } else { 0 };
        self.outcome.map(|_| {
            self.ticks
                .saturating_sub(intro + self.dead + TIME_LIMIT - self.remaining)
        })
    }
    pub fn step(&mut self, input: [Input; 2]) -> Vec<Hit> {
        self.ticks = self.ticks.saturating_add(1);
        if self.outcome.is_some() {
            // Once the result has shown long enough, the next round introduces itself;
            // a decided match stays on its final result until a restart.
            if self.match_outcome.is_none()
                && self
                    .ticks_since_outcome()
                    .is_some_and(|t| t >= ROUND_END_TICKS)
            {
                self.next_round();
                return vec![];
            }
            self.shots = [None; 2];
            for f in &mut self.fighters {
                f.vx = 0;
                f.recoil = 0;
                Self::physics(f);
                if f.state == State::Ko {
                    f.age = f.age.saturating_add(1);
                    f.visual_age = f.visual_age.saturating_add(1);
                } else {
                    f.enter(State::Ready);
                }
            }
            return vec![];
        }
        if self.resume > 0 {
            self.resume -= 1;
            self.dead += 1;
            return vec![];
        }
        if self.intro > 0 {
            self.intro -= 1;
            return vec![];
        }
        if self.hitstop > 0 {
            self.hitstop -= 1;
            self.dead += 1;
            return vec![];
        }
        self.remaining = self.remaining.saturating_sub(1);
        // Advance existing states before resolving this tick. New states stay at age zero.
        for f in &mut self.fighters {
            f.visual_age = f.visual_age.saturating_add(1);
            match f.state {
                State::Attack(m) => {
                    f.age += 1;
                    if f.age >= m.total() {
                        f.enter(State::Ready);
                    }
                }
                State::Hurt => {
                    f.age += 1;
                    if f.age >= 18 {
                        f.enter(State::Ready);
                    }
                }
                State::Knockdown => {
                    if f.y == 0 && f.vy == 0 {
                        f.age += 1;
                        if f.age >= 36 {
                            f.enter(State::Getup);
                            f.vx = 0;
                        }
                    }
                }
                State::Getup => {
                    f.age += 1;
                    if f.age >= 48 {
                        f.enter(State::Ready);
                    }
                }
                State::Ko => f.age = f.age.saturating_add(1),
                State::Guard => f.age = f.age.saturating_add(1),
                State::Ready => {}
            }
        }
        let old_x = [self.fighters[0].x, self.fighters[1].x];
        for (i, controls) in input.into_iter().enumerate() {
            // Both seats turn from the same pre-movement positions, including a crossing tick.
            let other = old_x[1 - i];
            let can_shoot = self.shots[i].is_none();
            let f = &mut self.fighters[i];
            if matches!(f.state, State::Ready) {
                f.facing = if other >= f.x { 1 } else { -1 };
            }
            if f.state == State::Guard && !controls.holds(GUARD) {
                f.enter(State::Ready);
            }
            if f.state == State::Ready {
                f.vx = (i32::from(controls.holds(RIGHT)) - i32::from(controls.holds(LEFT))) * 4500;
                if f.y == 0 {
                    if controls.holds(GUARD) {
                        f.enter(State::Guard);
                        f.vx = 0;
                    } else if let Some(m) = if controls.presses(DASH) {
                        Some(Move::Dash)
                    } else if controls.presses(HEAVY) {
                        Some(Move::Heavy)
                    } else if controls.presses(LIGHT) {
                        Some(Move::Light)
                    } else if controls.presses(SHOT) && can_shoot {
                        Some(Move::Shot)
                    } else {
                        None
                    } {
                        f.enter(State::Attack(m));
                        f.hit = false;
                        f.vx = 0;
                    } else if controls.presses(JUMP) {
                        f.vy = 19000;
                        f.air_used = false;
                    }
                } else if !f.air_used && (controls.presses(LIGHT) || controls.presses(HEAVY)) {
                    f.enter(State::Attack(Move::Air));
                    f.hit = false;
                    f.air_used = true;
                }
            }
            match f.state {
                State::Guard | State::Getup => f.vx = 0,
                // The belly press keeps the jump's drift.
                State::Attack(Move::Air) => {}
                State::Attack(m) => {
                    let d = m.data();
                    f.vx = if m == Move::Dash && f.age >= d.startup && f.age < d.startup + d.active
                    {
                        13000 * f.facing
                    } else {
                        0
                    };
                }
                State::Hurt | State::Knockdown => f.vx = f.vx * 17 / 20,
                _ => {}
            }
            Self::physics(f);
            if f.state == State::Attack(Move::Air) && f.y == 0 {
                f.enter(State::Ready);
            }
        }
        self.pushboxes(old_x);
        // Read both attacks from one snapshot before applying either hit: trades are symmetric.
        let snapshot = self.fighters.clone();
        let mut contacts = vec![];
        let mut events = vec![];
        for attacker in 0..2 {
            let victim = 1 - attacker;
            let a = &snapshot[attacker];
            let b = &snapshot[victim];
            if let Some(rect) = a.attack_box()
                && b.vulnerable()
                && rect.overlaps(b.hurt_box())
            {
                let State::Attack(m) = a.state else {
                    unreachable!()
                };
                let blocked = b.state == State::Guard && b.y == 0 && (a.x - b.x) * b.facing > 0;
                contacts.push(Contact {
                    victim,
                    damage: m.data().damage,
                    knockdown: m.data().knockdown,
                    direction: a.facing,
                    blocked,
                });
                self.fighters[attacker].hit = true;
                events.push(Hit {
                    victim,
                    x: b.x - b.facing * HALF / 2,
                    y: b.y + 140 * UNIT,
                    blocked,
                    shot: false,
                });
            }
        }
        self.fly_shots(&snapshot, &mut contacts, &mut events);
        for c in contacts {
            if !c.blocked {
                // The freeze lands with the hit; it is never summed on a double contact.
                self.hitstop = HIT_STOP_TICKS;
            }
            Self::land(&mut self.fighters[c.victim], &c);
        }
        self.outcome = match (self.fighters[0].hp == 0, self.fighters[1].hp == 0) {
            (true, true) => Some(Outcome::Draw),
            (true, false) => Some(Outcome::Winner(1)),
            (false, true) => Some(Outcome::Winner(0)),
            _ if self.remaining == 0 => Some(match self.fighters[0].hp.cmp(&self.fighters[1].hp) {
                std::cmp::Ordering::Greater => Outcome::Winner(0),
                std::cmp::Ordering::Less => Outcome::Winner(1),
                std::cmp::Ordering::Equal => Outcome::Draw,
            }),
            _ => None,
        };
        if self.outcome.is_some() {
            self.shots = [None; 2];
            // First to two round wins, or the round-win majority once five rounds are
            // played (equal wins draw the match); draws award nothing. Runs once, on
            // the deciding tick.
            if self.match_outcome.is_none() {
                if let Some(Outcome::Winner(w)) = self.outcome {
                    self.wins[w] += 1;
                }
                self.match_outcome = if self.wins[0] >= 2 {
                    Some(Outcome::Winner(0))
                } else if self.wins[1] >= 2 {
                    Some(Outcome::Winner(1))
                } else if self.game >= 4 {
                    match self.wins[0].cmp(&self.wins[1]) {
                        std::cmp::Ordering::Greater => Some(Outcome::Winner(0)),
                        std::cmp::Ordering::Less => Some(Outcome::Winner(1)),
                        std::cmp::Ordering::Equal => Some(Outcome::Draw),
                    }
                } else {
                    None
                };
            }
        }
        events
    }
    /// Moves, spawns and resolves milk bubbles against the same snapshot as body hits.
    fn fly_shots(
        &mut self,
        snapshot: &[Fighter; 2],
        contacts: &mut Vec<Contact>,
        events: &mut Vec<Hit>,
    ) {
        for s in self.shots.iter_mut().flatten() {
            s.x += s.dir * SHOT_SPEED;
            s.age += 1;
        }
        for (i, f) in snapshot.iter().enumerate() {
            if f.state == State::Attack(Move::Shot)
                && f.age == Move::Shot.data().startup
                && self.shots[i].is_none()
            {
                self.shots[i] = Some(Shot {
                    x: f.x + f.facing * SHOT_SPAWN,
                    dir: f.facing,
                    age: 0,
                });
            }
        }
        if let [Some(a), Some(b)] = self.shots
            && a.rect().overlaps(b.rect())
        {
            self.shots = [None; 2];
        }
        for owner in 0..2 {
            let Some(s) = self.shots[owner] else {
                continue;
            };
            let victim = 1 - owner;
            let b = &snapshot[victim];
            if b.vulnerable() && s.rect().overlaps(b.hurt_box()) {
                // Guard holds when the defender faces into the bubble's flight.
                let blocked = b.state == State::Guard && b.y == 0 && b.facing == -s.dir;
                contacts.push(Contact {
                    victim,
                    damage: Move::Shot.data().damage,
                    knockdown: false,
                    direction: s.dir,
                    blocked,
                });
                events.push(Hit {
                    victim,
                    x: s.x,
                    y: (SHOT_BOTTOM + SHOT_TOP) / 2,
                    blocked,
                    shot: true,
                });
                self.shots[owner] = None;
            } else if s.x.abs() > SHOT_LIMIT {
                self.shots[owner] = None;
            }
        }
    }
    /// A blocked hit chips a quarter (rounded up) and can still knock out.
    fn land(f: &mut Fighter, c: &Contact) {
        if c.blocked {
            f.hp = (f.hp - (c.damage + 3) / 4).max(0);
            if f.hp > 0 {
                f.recoil = c.direction * 2500;
                return;
            }
        } else {
            f.hp = (f.hp - c.damage).max(0);
            f.combo_taken = f.combo_taken.saturating_add(1);
        }
        f.vx = c.direction * 9000;
        if c.knockdown {
            f.vy = 8500;
        }
        // A second hit on the same tick must not stand a knocked-down fighter back up.
        let down = c.knockdown || f.state == State::Knockdown;
        f.enter(if f.hp == 0 {
            State::Ko
        } else if down {
            State::Knockdown
        } else {
            State::Hurt
        });
    }
    fn physics(f: &mut Fighter) {
        f.x = (f.x + f.vx + f.recoil).clamp(-WALL, WALL);
        f.recoil = f.recoil * 17 / 20;
        if f.y > 0 || f.vy > 0 {
            f.vy -= GRAVITY;
            f.y += f.vy;
            if f.y <= 0 {
                f.y = 0;
                f.vy = 0;
            }
        }
    }
    fn pushboxes(&mut self, old: [i32; 2]) {
        if (self.fighters[0].y - self.fighters[1].y).abs() > 170 * UNIT {
            return;
        }
        let left = if old[0] <= old[1] { 0 } else { 1 };
        let right = 1 - left;
        if self.fighters[right].x - self.fighters[left].x < 2 * HALF {
            let mut lx = (self.fighters[0].x + self.fighters[1].x - 2 * HALF) / 2;
            lx = lx.clamp(-WALL, WALL - 2 * HALF);
            self.fighters[left].x = lx;
            self.fighters[right].x = lx + 2 * HALF;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn near() -> Arena {
        let mut a = live();
        a.fighters[0].x = -50 * UNIT;
        a.fighters[1].x = 50 * UNIT;
        a
    }
    fn press(button: u16) -> Input {
        Input {
            held: button,
            pressed: button,
        }
    }
    fn empty() -> [Input; 2] {
        [Input::default(); 2]
    }
    fn hold(button: u16) -> Input {
        Input {
            held: button,
            pressed: 0,
        }
    }
    /// P1 throws a bubble on tick 0 from the spawn positions; P2 plays `victim(t)`.
    fn shoot_at(a: &mut Arena, victim: impl Fn(u32) -> Input, ticks: u32) -> Vec<Hit> {
        let mut hits = a.step([press(SHOT), victim(0)]);
        for t in 1..ticks {
            hits.extend(a.step([Input::default(), victim(t)]));
        }
        hits
    }
    /// An arena past the intro countdown, ready to fight.
    fn live() -> Arena {
        let mut a = Arena::default();
        warm(&mut a);
        a
    }
    /// Play out the round introduction with neutral input.
    fn warm(a: &mut Arena) {
        while a.intro > 0 {
            a.step(empty());
        }
    }
    #[test]
    fn jumping_past_a_guard_has_the_same_result_when_seats_are_mirrored() {
        let flip_bits =
            |bits: u16| (bits & !(LEFT | RIGHT)) | ((bits & LEFT) << 1) | ((bits & RIGHT) >> 1);
        let flip_input = |input: Input| Input {
            held: flip_bits(input.held),
            pressed: flip_bits(input.pressed),
        };
        let flip_fighter = |mut f: Fighter| {
            f.x = -f.x;
            f.vx = -f.vx;
            f.recoil = -f.recoil;
            f.facing = -f.facing;
            f
        };
        let mut a = live();
        let mut b = a.clone();
        b.fighters = [
            flip_fighter(a.fighters[1].clone()),
            flip_fighter(a.fighters[0].clone()),
        ];
        // Walk to contact, jump across, then guard on the crossing tick and belly press.
        // One neutral walking tick ensures the crossing falls between the two updates.
        for tick in 0..95 {
            let action = match tick {
                37 => JUMP,
                70 => LIGHT,
                _ => 0,
            };
            let input = [
                Input {
                    held: RIGHT | action,
                    pressed: action,
                },
                hold(if tick >= 66 {
                    GUARD
                } else if tick == 50 {
                    0
                } else {
                    LEFT
                }),
            ];
            a.step(input);
            b.step([flip_input(input[1]), flip_input(input[0])]);
            assert_eq!(
                b.fighters,
                [
                    flip_fighter(a.fighters[1].clone()),
                    flip_fighter(a.fighters[0].clone()),
                ],
                "seat order changed the result at tick {tick}"
            );
        }
        assert!(a.fighters[1].hp < 100, "the regression must include a hit");
    }
    #[test]
    fn intro_ignores_input_and_freezes_the_clock() {
        let mut a = Arena::default();
        assert_eq!(
            a.intro,
            VS_TICKS + INTRO_TICKS,
            "a match opens on the versus screen"
        );
        for _ in 0..(VS_TICKS + INTRO_TICKS) {
            a.step([press(LIGHT | RIGHT | JUMP), press(HEAVY)]);
        }
        assert_eq!(a.remaining, TIME_LIMIT, "the clock waits for the intro");
        assert_eq!(a.fighters[0].x, -235 * UNIT, "intro input moves nobody");
        assert_eq!(a.fighters[0].state, State::Ready);
        assert_eq!(
            a.fighters[0].y, 0,
            "a jump pressed during the intro never fires"
        );
        assert_eq!(a.shots, [None; 2]);
        // The tick the intro ends is the first tick on the clock.
        assert_eq!(a.intro, 0);
        a.step(empty());
        assert_eq!(a.remaining, TIME_LIMIT - 1);
    }
    #[test]
    fn resume_countdown_freezes_the_fight_like_the_intro() {
        let mut a = live();
        a.resume_now();
        assert_eq!(a.resume, RESUME_TICKS, "three seconds at sixty ticks");
        let x0 = a.fighters[0].x;
        for _ in 0..RESUME_TICKS {
            assert!(
                a.step([press(LIGHT | RIGHT | JUMP), press(HEAVY)])
                    .is_empty()
            );
        }
        assert_eq!(a.remaining, TIME_LIMIT, "the clock waits for the countdown");
        assert_eq!(a.fighters[0].x, x0, "countdown input moves nobody");
        assert_eq!(
            a.fighters[0].y, 0,
            "a jump pressed during the countdown never fires"
        );
        assert_eq!(a.shots, [None; 2]);
        assert_eq!(a.resume, 0);
        // The tick after the countdown is the first back on the clock.
        a.step(empty());
        assert_eq!(a.remaining, TIME_LIMIT - 1);
    }
    #[test]
    fn restart_clears_a_pending_resume_countdown() {
        let mut a = live();
        a.resume_now();
        a.restart();
        assert_eq!(a.resume, 0, "a fresh round never waits on an old countdown");
        // The fresh match opens on its versus screen, not on a countdown.
        assert_eq!(a.intro, VS_TICKS + INTRO_TICKS);
    }
    #[test]
    fn the_resume_countdown_skips_versus_and_introductions() {
        // Round one's versus screen is input-dead and clock-frozen already: a drop
        // during it just continues the versus on return, no 3-2-1 stacked on top.
        let mut a = Arena::default();
        a.resume_now();
        assert_eq!(a.resume, 0, "versus never counts back in");
        // A later round's introduction is the same: only a fought round resumes with
        // the countdown.
        let mut b = Arena {
            intro: INTRO_TICKS,
            game: 1,
            ..Arena::default()
        };
        b.resume_now();
        assert_eq!(b.resume, 0, "a round introduction never counts back in");
        let mut c = live();
        c.resume_now();
        assert_eq!(c.resume, RESUME_TICKS, "a fought round counts back in");
    }
    #[test]
    fn a_resumed_round_still_shows_its_result_before_the_next() {
        let mut a = live();
        a.resume_now();
        while a.resume > 0 {
            a.step(empty());
        }
        a.outcome = Some(Outcome::Winner(0));
        // The countdown's dead ticks are not result time: the result shows for the
        // usual spell, and only then does the next round introduce itself.
        for _ in 0..ROUND_END_TICKS - 1 {
            a.step(empty());
            assert!(a.outcome.is_some(), "the result is still on screen");
            assert_eq!(a.game, 0, "no early next round");
        }
        a.step(empty());
        assert_eq!(a.game, 1, "the next round starts after the result spell");
    }
    #[test]
    fn versus_intro_precedes_the_first_round_only() {
        let mut a = Arena::default();
        for _ in 0..VS_TICKS {
            a.step([press(LIGHT | RIGHT | JUMP), press(HEAVY)]);
        }
        assert_eq!(
            a.remaining, TIME_LIMIT,
            "the versus screen freezes the clock"
        );
        assert_eq!(a.fighters[0].x, -235 * UNIT, "versus input moves nobody");
        assert_eq!(a.fighters[0].y, 0, "a jump during versus never fires");
        assert_eq!(a.intro, INTRO_TICKS, "versus hands over to the round intro");
        // An outcome needs a round that was actually fought; warm the rest of the intro.
        for _ in 0..INTRO_TICKS {
            a.step(empty());
        }
        // Rounds after the first open straight on their intro: versus is per match.
        a.outcome = Some(Outcome::Draw);
        for _ in 0..ROUND_END_TICKS {
            a.step(empty());
        }
        assert_eq!(a.game, 1, "the draw advanced the match");
        assert_eq!(a.intro, INTRO_TICKS, "no second versus screen mid-match");
        // A restart is a whole new match, so versus returns.
        a.restart();
        assert_eq!(a.intro, VS_TICKS + INTRO_TICKS);
    }
    #[test]
    fn ticks_since_outcome_counts_from_the_result_on_every_round() {
        let mut a = Arena::default();
        for _ in 0..(VS_TICKS + INTRO_TICKS) {
            a.step(empty());
        }
        a.outcome = Some(Outcome::Winner(0));
        assert_eq!(
            a.ticks_since_outcome(),
            Some(0),
            "round one's baseline includes the versus screen"
        );
        for k in 1..=3 {
            a.step(empty());
            assert_eq!(a.ticks_since_outcome(), Some(k));
        }
        for _ in 0..ROUND_END_TICKS {
            a.step(empty());
        }
        assert_eq!(a.game, 1);
        for _ in 0..INTRO_TICKS {
            a.step(empty());
        }
        a.outcome = Some(Outcome::Winner(1));
        assert_eq!(a.ticks_since_outcome(), Some(0));
        a.step(empty());
        assert_eq!(a.ticks_since_outcome(), Some(1));
    }
    #[test]
    fn exact_attack_windows() {
        let mut f = Fighter::new(0, 1);
        for m in [Move::Light, Move::Heavy, Move::Dash, Move::Shot, Move::Air] {
            f.state = State::Attack(m);
            f.hit = false;
            for t in 0..m.total() {
                f.age = t;
                assert_eq!(
                    f.active(),
                    t >= m.data().startup && t < m.data().startup + m.data().active
                );
            }
        }
    }
    #[test]
    fn light_hits_once() {
        let mut a = near();
        a.step([press(LIGHT), Input::default()]);
        for _ in 0..40 {
            a.step([
                Input {
                    held: LIGHT,
                    pressed: 0,
                },
                Input::default(),
            ]);
        }
        assert_eq!(a.fighters[1].hp, 92);
        assert_eq!(a.fighters[0].state, State::Ready);
    }
    #[test]
    fn front_guard_chips_a_quarter() {
        let mut a = near();
        a.step([
            press(LIGHT),
            Input {
                held: GUARD,
                pressed: 0,
            },
        ]);
        let mut n = 0;
        for _ in 0..30 {
            n += a
                .step([
                    Input::default(),
                    Input {
                        held: GUARD,
                        pressed: 0,
                    },
                ])
                .len();
        }
        assert_eq!(n, 1);
        assert_eq!(a.fighters[1].hp, 98);
        assert_eq!(a.fighters[1].state, State::Guard);
    }
    #[test]
    fn chip_can_knock_out() {
        let mut a = near();
        a.fighters[1].hp = 2;
        a.shots[1] = Some(Shot {
            x: 400 * UNIT,
            dir: 1,
            age: 0,
        });
        a.step([press(LIGHT), hold(GUARD)]);
        for _ in 0..10 {
            a.step([Input::default(), hold(GUARD)]);
        }
        assert_eq!(a.fighters[1].hp, 0);
        assert_eq!(a.fighters[1].state, State::Ko);
        assert_eq!(a.outcome, Some(Outcome::Winner(0)));
        assert_eq!(a.shots, [None, None]);
    }
    #[test]
    fn shot_spawns_on_frame_14_and_flies_8_per_tick() {
        let mut a = live();
        a.step([press(SHOT), Input::default()]);
        assert_eq!(a.fighters[0].state, State::Attack(Move::Shot));
        for _ in 0..13 {
            a.step(empty());
            assert_eq!(a.shots[0], None);
        }
        a.step(empty());
        let x = -235 * UNIT + SHOT_SPAWN;
        assert_eq!(a.shots[0], Some(Shot { x, dir: 1, age: 0 }));
        a.step(empty());
        assert_eq!(a.shots[0].map(|s| s.x), Some(x + SHOT_SPEED));
    }
    #[test]
    fn standing_still_takes_the_shot() {
        let mut a = live();
        let hits = shoot_at(&mut a, |_| Input::default(), 80);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].shot && !hits[0].blocked && hits[0].victim == 1);
        assert_eq!(a.fighters[1].hp, 90);
        assert_eq!(a.shots[0], None);
    }
    /// Jump start ticks (0..80) after which P2 is still unhurt once the bubble is gone.
    fn clearing_jumps(forward: bool) -> Vec<u32> {
        (0..80)
            .filter(|&at| {
                let mut a = live();
                let drift = if forward { LEFT } else { 0 };
                shoot_at(
                    &mut a,
                    |t| match t {
                        _ if t == at => press(JUMP | drift),
                        _ if t > at && t < at + 42 => hold(drift),
                        _ => Input::default(),
                    },
                    140,
                );
                assert_eq!(a.shots[0], None);
                a.fighters[1].hp == 100
            })
            .collect()
    }
    #[test]
    fn a_timed_jump_clears_the_shot() {
        let clear = clearing_jumps(false);
        assert!(clear.len() >= 12, "{clear:?}");
        assert!(!clear.contains(&0), "jumping far too early still gets hit");
        // One contiguous window, so the timing is learnable.
        assert_eq!(clear.last().unwrap() - clear[0] + 1, clear.len() as u32);
    }
    #[test]
    fn a_forward_jump_clears_the_shot_too() {
        let clear = clearing_jumps(true);
        assert!(clear.len() >= 24, "{clear:?}");
        assert!(clear.len() > clearing_jumps(false).len(), "{clear:?}");
    }
    #[test]
    fn a_jump_stays_airborne_at_most_48_ticks() {
        // A bigger dodge window must not turn the jump into a float.
        let mut a = live();
        a.step([press(JUMP), Input::default()]);
        let mut air = 0;
        while a.fighters[0].y > 0 {
            a.step(empty());
            air += 1;
        }
        assert!(air <= 48, "{air} ticks airborne");
    }
    #[test]
    fn guarding_the_shot_chips_three() {
        let mut a = live();
        let hits = shoot_at(&mut a, |_| hold(GUARD), 80);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].shot && hits[0].blocked);
        assert_eq!(a.fighters[1].hp, 97);
        assert_eq!(a.fighters[1].state, State::Guard);
    }
    #[test]
    fn shots_cancel_each_other() {
        let mut a = live();
        let mut hits = a.step([press(SHOT); 2]);
        let mut seen = false;
        for _ in 0..80 {
            hits.extend(a.step(empty()));
            seen |= a.shots.iter().all(Option::is_some);
        }
        assert!(seen);
        assert!(hits.is_empty());
        assert_eq!(a.shots, [None, None]);
        assert_eq!([a.fighters[0].hp, a.fighters[1].hp], [100, 100]);
    }
    #[test]
    fn shot_passes_a_knocked_down_fighter_and_leaves_at_the_wall() {
        let mut a = live();
        a.fighters[1].enter(State::Knockdown);
        a.step([press(SHOT), Input::default()]);
        let mut furthest = 0;
        for _ in 0..140 {
            assert!(a.step(empty()).is_empty());
            if let Some(s) = a.shots[0] {
                furthest = s.x;
            }
        }
        assert_eq!(a.fighters[1].hp, 100);
        assert_eq!(a.shots[0], None);
        assert!(furthest > SHOT_LIMIT - SHOT_SPEED && furthest <= SHOT_LIMIT);
    }
    #[test]
    fn second_shot_press_is_ignored_and_jump_still_works() {
        let mut a = live();
        a.step([press(SHOT), Input::default()]);
        for _ in 0..39 {
            a.step(empty());
        }
        assert_eq!(a.fighters[0].state, State::Ready);
        assert!(a.shots[0].is_some());
        a.step([press(SHOT | JUMP), Input::default()]);
        assert_eq!(a.fighters[0].state, State::Ready);
        assert!(a.fighters[0].y > 0);
    }
    #[test]
    fn melee_and_shot_on_one_tick_both_count() {
        let mut a = near();
        a.fighters[0].state = State::Attack(Move::Heavy);
        a.fighters[0].age = 13;
        a.shots[0] = Some(Shot {
            x: 0,
            dir: 1,
            age: 0,
        });
        let hits = a.step(empty());
        assert_eq!(hits.len(), 2);
        assert_eq!(a.fighters[1].hp, 100 - 16 - 10);
        assert_eq!(a.fighters[1].state, State::Knockdown);
        assert_eq!(a.shots[0], None);
    }
    #[test]
    fn hits_in_stun_stack_into_a_combo_and_reset_on_recovery() {
        let mut a = near();
        a.step([press(LIGHT), Input::default()]);
        for _ in 0..7 {
            a.step(empty());
        }
        let hits = a.step(empty()); // light startup 8: the swing connects here
        assert_eq!(hits.len(), 1);
        assert_eq!(a.fighters[1].hp, 92);
        assert_eq!(
            a.fighters[1].combo_taken, 1,
            "the first clean hit opens the count"
        );
        // Hit-stop: the next HIT_STOP_TICKS ticks freeze both fighters and the clock,
        // and any press inside the freeze is dropped like intro input.
        let (frozen_x, frozen_clock) = (a.fighters[1].x, a.remaining);
        for _ in 0..HIT_STOP_TICKS {
            a.step([press(LEFT | JUMP), Input::default()]);
            assert_eq!(a.fighters[1].x, frozen_x, "the victim does not slide yet");
            assert_eq!(a.remaining, frozen_clock, "the clock waits out the stop");
        }
        a.step(empty());
        assert_eq!(
            a.remaining,
            frozen_clock - 1,
            "exactly HIT_STOP_TICKS ticks were dead"
        );
        assert_eq!(
            a.fighters[0].y, 0,
            "the jump pressed inside the freeze never fired"
        );
        // The victim recovers from the stun: the count goes back to zero.
        while a.fighters[1].state == State::Hurt {
            a.step(empty());
        }
        assert_eq!(a.fighters[1].hp, 92, "nothing else landed meanwhile");
        assert_eq!(a.fighters[1].combo_taken, 0, "recovery clears the combo");
    }
    #[test]
    fn a_blocked_hit_neither_counts_nor_stops_the_clock() {
        let mut a = near();
        a.step([press(LIGHT), hold(GUARD)]);
        for _ in 0..7 {
            a.step([Input::default(), hold(GUARD)]);
        }
        let hits = a.step([Input::default(), hold(GUARD)]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].blocked);
        assert_eq!(a.fighters[1].hp, 98);
        assert_eq!(a.fighters[1].combo_taken, 0, "blocked hits never count");
        let clock = a.remaining;
        a.step([Input::default(), hold(GUARD)]);
        assert_eq!(
            a.remaining,
            clock - 1,
            "a blocked hit does not stop the clock"
        );
        assert_ne!(
            a.fighters[1].x,
            50 * UNIT,
            "the block recoil moves at once, no freeze"
        );
    }
    #[test]
    fn a_same_tick_double_hit_is_a_two_hit_combo_with_one_stop() {
        let mut a = near();
        a.fighters[0].state = State::Attack(Move::Heavy);
        a.fighters[0].age = 13;
        a.shots[0] = Some(Shot {
            x: 0,
            dir: 1,
            age: 0,
        });
        a.step(empty());
        assert_eq!(
            a.fighters[1].combo_taken, 2,
            "two sources on one tick are a two-hit combo"
        );
        assert_eq!(
            a.hitstop, HIT_STOP_TICKS,
            "the stop is taken once, not summed"
        );
        let clock = a.remaining;
        for _ in 0..HIT_STOP_TICKS {
            a.step(empty());
            assert_eq!(a.remaining, clock);
        }
        a.step(empty());
        assert_eq!(
            a.remaining,
            clock - 1,
            "one stop, HIT_STOP_TICKS dead ticks"
        );
    }
    #[test]
    fn knockdown_visual_time_advances_while_ground_timer_waits() {
        let mut a = near();
        a.fighters[1].enter(State::Knockdown);
        a.fighters[1].y = 100 * UNIT;
        for _ in 0..6 {
            a.step([Input::default(); 2]);
        }
        assert!(a.fighters[1].y > 0);
        assert_eq!(a.fighters[1].age, 0);
        assert_eq!(a.fighters[1].visual_age, 6);
    }
    #[test]
    fn knockout_finishes_falling_and_animating_after_outcome() {
        let mut a = near();
        a.fighters[1].hp = 0;
        a.fighters[1].enter(State::Ko);
        a.fighters[1].vy = 8500;
        a.outcome = Some(Outcome::Winner(0));
        for _ in 0..80 {
            a.step([press(JUMP); 2]);
        }
        assert_eq!(a.fighters[1].y, 0);
        assert_eq!(a.fighters[1].visual_age, 80);
        assert_eq!(a.fighters[1].state, State::Ko);
        assert_eq!(a.fighters[0].y, 0);
    }
    #[test]
    fn timeout_resolves_on_exactly_99_seconds_and_freezes_remaining() {
        let mut a = live();
        a.fighters[1].hp = 92;
        for _ in 0..TIME_LIMIT - 1 {
            a.step([Input::default(); 2]);
        }
        assert_eq!(a.remaining, 1);
        assert_eq!(a.outcome, None);
        a.step([Input::default(); 2]);
        assert_eq!(a.remaining, 0);
        assert_eq!(a.outcome, Some(Outcome::Winner(0)));
        a.step([Input::default(); 2]);
        assert_eq!(a.remaining, 0);
    }
    #[test]
    fn ticks_since_outcome_counts_from_the_deciding_tick() {
        let mut a = live();
        for _ in 0..TIME_LIMIT {
            assert_eq!(a.ticks_since_outcome(), None);
            a.step([Input::default(); 2]);
        }
        assert_eq!(a.ticks_since_outcome(), Some(0));
        for k in 1..=90 {
            a.step([Input::default(); 2]);
            assert_eq!(a.ticks_since_outcome(), Some(k));
        }
        // A knockout freezes the clock early; the count still starts at zero.
        let mut a = near();
        a.fighters[1].hp = 1;
        a.step([press(LIGHT), Input::default()]);
        for _ in 0..60 {
            if a.outcome.is_some() {
                break;
            }
            a.step([Input::default(); 2]);
        }
        assert_eq!(a.outcome, Some(Outcome::Winner(0)));
        assert_eq!(a.ticks_since_outcome(), Some(0));
        a.step([Input::default(); 2]);
        assert_eq!(a.ticks_since_outcome(), Some(1));
        a.restart();
        assert_eq!(a.ticks_since_outcome(), None);
    }
    #[test]
    fn round_wins_accumulate_and_the_match_needs_two() {
        let mut a = near();
        a.fighters[1].hp = 1;
        a.step([press(LIGHT), Input::default()]);
        for _ in 0..60 {
            if a.outcome.is_some() {
                break;
            }
            a.step(empty());
        }
        assert_eq!(a.outcome, Some(Outcome::Winner(0)));
        assert_eq!((a.game, a.wins, a.match_outcome), (0, [1, 0], None));
        // The result shows for ROUND_END_TICKS, then the next round introduces itself.
        for _ in 0..ROUND_END_TICKS {
            assert_eq!(a.match_outcome, None);
            a.step(empty());
        }
        assert_eq!(a.game, 1, "the second round begins");
        assert_eq!(a.intro, INTRO_TICKS);
        assert_eq!(a.wins, [1, 0], "round wins survive the round reset");
        assert_eq!((a.remaining, a.outcome), (TIME_LIMIT, None));
        assert_eq!(
            a.fighters,
            [Fighter::new(-235, 1), Fighter::new(235, -1)],
            "the new round starts from the spawn positions"
        );
        // A second win ends the match; no further round ever starts.
        warm(&mut a);
        a.fighters[0].x = -50 * UNIT;
        a.fighters[1].x = 50 * UNIT;
        a.fighters[1].hp = 1;
        a.step([press(LIGHT), Input::default()]);
        for _ in 0..60 {
            if a.outcome.is_some() {
                break;
            }
            a.step(empty());
        }
        assert_eq!(a.match_outcome, Some(Outcome::Winner(0)));
        assert_eq!(a.wins, [2, 0]);
        for _ in 0..ROUND_END_TICKS + 60 {
            a.step(empty());
        }
        assert_eq!(a.game, 1, "the decided match never starts another round");
        assert_eq!(a.match_outcome, Some(Outcome::Winner(0)));
        a.restart();
        assert_eq!(a, Arena::default(), "restart resets the whole match");
    }
    /// Play out a simultaneous double KO in the current (live) round, from touching range.
    fn double_ko(a: &mut Arena) {
        a.fighters[0].x = -50 * UNIT;
        a.fighters[1].x = 50 * UNIT;
        for f in &mut a.fighters {
            f.hp = 8;
            f.enter(State::Attack(Move::Light));
            f.age = 8;
        }
        a.step(empty());
        assert_eq!(a.outcome, Some(Outcome::Draw));
    }
    /// A scripted round win for seat `w` from touching range (the loser sits at 1 hp).
    fn win_round(a: &mut Arena, w: usize) {
        a.fighters[w].x = if w == 0 { -50 * UNIT } else { 50 * UNIT };
        a.fighters[1 - w].x = if w == 0 { 50 * UNIT } else { -50 * UNIT };
        a.fighters[1 - w].hp = 1;
        let input = if w == 0 {
            [press(LIGHT), Input::default()]
        } else {
            [Input::default(), press(LIGHT)]
        };
        a.step(input);
        for _ in 0..60 {
            if a.outcome.is_some() {
                break;
            }
            a.step(empty());
        }
        assert_eq!(a.outcome, Some(Outcome::Winner(w)));
    }
    /// Play out the result spell and the next round's introduction.
    fn advance(a: &mut Arena) {
        for _ in 0..ROUND_END_TICKS {
            a.step(empty());
        }
        warm(a);
    }
    #[test]
    fn a_majority_of_round_wins_takes_a_five_round_match() {
        // [1,0] with three draws between, then a drawn fifth round: P1 keeps the majority.
        let mut a = near();
        win_round(&mut a, 0);
        assert_eq!(a.match_outcome, None);
        for _ in 0..3 {
            advance(&mut a);
            double_ko(&mut a);
        }
        advance(&mut a);
        assert_eq!(a.game, 4, "the fifth round is live");
        double_ko(&mut a);
        assert_eq!(a.wins, [1, 0]);
        assert_eq!(
            a.match_outcome,
            Some(Outcome::Winner(0)),
            "the round-win majority decides a full five-round match"
        );
    }
    #[test]
    fn an_even_five_round_match_is_a_draw_even_though_someone_won_the_last_round() {
        // [1,0] plus three draws, then P2 takes round five: 1-1, nobody has the majority.
        let mut a = near();
        win_round(&mut a, 0);
        for _ in 0..3 {
            advance(&mut a);
            double_ko(&mut a);
        }
        advance(&mut a);
        win_round(&mut a, 1);
        assert_eq!(a.wins, [1, 1]);
        assert_eq!(
            a.match_outcome,
            Some(Outcome::Draw),
            "the last round's winner does not take an even match"
        );
    }
    #[test]
    fn five_round_majorities_mirror() {
        for w in 0..2 {
            let mut a = near();
            win_round(&mut a, w);
            for _ in 0..3 {
                advance(&mut a);
                double_ko(&mut a);
            }
            advance(&mut a);
            double_ko(&mut a);
            assert_eq!(
                a.match_outcome,
                Some(Outcome::Winner(w)),
                "seat {w} keeps the mirrored majority"
            );
            let mut b = near();
            win_round(&mut b, w);
            for _ in 0..3 {
                advance(&mut b);
                double_ko(&mut b);
            }
            advance(&mut b);
            win_round(&mut b, 1 - w);
            assert_eq!(
                b.match_outcome,
                Some(Outcome::Draw),
                "the mirrored even match draws"
            );
        }
    }
    #[test]
    fn draws_add_no_wins_and_five_rounds_end_the_match() {
        let mut a = near();
        for game in 0..4 {
            double_ko(&mut a);
            assert_eq!(a.wins, [0, 0], "a draw lights no marker");
            assert_eq!(a.match_outcome, None, "game {game} leaves the match open");
            for _ in 0..ROUND_END_TICKS {
                a.step(empty());
            }
            assert_eq!(a.game, game as u8 + 1);
            warm(&mut a);
        }
        // The fifth round is the last: another draw decides the whole match.
        double_ko(&mut a);
        assert_eq!(a.match_outcome, Some(Outcome::Draw));
        for _ in 0..ROUND_END_TICKS + 60 {
            a.step(empty());
        }
        assert_eq!(a.game, 4, "there is no sixth round");
    }
    #[test]
    fn four_draws_then_a_fifth_round_win_takes_the_match() {
        let mut a = near();
        for _ in 0..4 {
            double_ko(&mut a);
            for _ in 0..ROUND_END_TICKS {
                a.step(empty());
            }
            warm(&mut a);
        }
        assert_eq!(a.game, 4, "the fifth round is live");
        a.fighters[0].x = -50 * UNIT;
        a.fighters[1].x = 50 * UNIT;
        a.fighters[1].hp = 1;
        a.step([press(LIGHT), Input::default()]);
        for _ in 0..60 {
            if a.outcome.is_some() {
                break;
            }
            a.step(empty());
        }
        assert_eq!(a.match_outcome, Some(Outcome::Winner(0)));
        assert_eq!(
            a.wins,
            [1, 0],
            "one round win out of five games is a majority"
        );
    }
    #[test]
    fn block_recoil_moves_a_still_guarding_defender() {
        let mut a = near();
        let x = a.fighters[1].x;
        let guard = Input {
            held: GUARD,
            pressed: 0,
        };
        a.step([press(LIGHT), guard]);
        for _ in 0..9 {
            a.step([Input::default(), guard]);
        }
        assert_eq!(a.fighters[1].hp, 98);
        assert_eq!(a.fighters[1].state, State::Guard);
        assert!(a.fighters[1].x > x);
    }
    #[test]
    fn displayed_phase_is_the_resolved_first_and_last_hit_phase() {
        let mut a = near();
        a.step([press(LIGHT), Input::default()]);
        assert_eq!(a.fighters[0].visual_age, 0);
        for age in 1..8 {
            a.step([Input::default(); 2]);
            assert_eq!(a.fighters[0].age, age);
            assert_eq!(a.fighters[1].hp, 100);
        }
        a.step([Input::default(); 2]);
        assert_eq!(a.fighters[0].age, 8);
        assert_eq!(a.fighters[0].visual_age, 8);
        assert_eq!(a.fighters[1].hp, 92);
        let mut b = near();
        b.fighters[1].x = 400 * UNIT;
        b.step([press(LIGHT), Input::default()]);
        for _ in 1..11 {
            b.step([Input::default(); 2]);
        }
        assert_eq!(b.fighters[1].hp, 100);
        b.fighters[1].x = 70 * UNIT;
        b.step([Input::default(); 2]);
        assert_eq!(b.fighters[0].age, 11);
        assert_eq!(b.fighters[0].visual_age, 11);
        assert_eq!(b.fighters[1].hp, 92);
        // The hit froze both fighters: age and visual age wait out the stop together.
        for _ in 0..HIT_STOP_TICKS {
            b.step([Input::default(); 2]);
            assert_eq!(b.fighters[0].age, 11);
            assert_eq!(b.fighters[0].visual_age, 11);
        }
        b.step([Input::default(); 2]);
        assert_eq!(b.fighters[0].age, 12);
        assert_eq!(b.fighters[0].visual_age, 12);
    }
    #[test]
    fn blocking_consumes_swing() {
        let mut a = near();
        a.fighters[0].state = State::Attack(Move::Light);
        a.fighters[0].age = 8;
        a.step([
            Input::default(),
            Input {
                held: GUARD,
                pressed: 0,
            },
        ]);
        a.step(empty());
        assert_eq!(a.fighters[1].hp, 98);
    }
    #[test]
    fn rear_guard_does_not_block() {
        let mut a = near();
        a.fighters[1].state = State::Guard;
        a.fighters[1].facing = 1;
        a.fighters[0].state = State::Attack(Move::Light);
        a.fighters[0].age = 8;
        a.step([
            Input::default(),
            Input {
                held: GUARD,
                pressed: 0,
            },
        ]);
        assert_eq!(a.fighters[1].hp, 92);
    }
    #[test]
    fn simultaneous_double_ko() {
        let mut a = near();
        for f in &mut a.fighters {
            f.state = State::Attack(Move::Light);
            f.age = 8;
            f.hp = 8;
        }
        let hits = a.step(empty());
        assert_eq!(hits.len(), 2);
        assert_eq!(a.outcome, Some(Outcome::Draw));
    }
    #[test]
    fn held_jump_does_not_repeat() {
        let mut a = live();
        a.step([press(JUMP), Input::default()]);
        let mut airborne = false;
        for _ in 0..130 {
            airborne |= a.fighters[0].y > 0;
            a.step([
                Input {
                    held: JUMP,
                    pressed: 0,
                },
                Input::default(),
            ]);
        }
        assert!(airborne);
        assert_eq!(a.fighters[0].y, 0);
        assert_eq!(a.fighters[0].vy, 0);
    }
    #[test]
    fn air_attack_once_per_jump_and_lands_ready() {
        let mut a = live();
        a.step([press(JUMP), Input::default()]);
        a.step([press(HEAVY), Input::default()]);
        assert_eq!(a.fighters[0].state, State::Attack(Move::Air));
        for _ in 0..25 {
            a.step(empty());
        }
        assert_eq!(a.fighters[0].state, State::Ready);
        assert!(a.fighters[0].y > 0);
        a.step([press(LIGHT), Input::default()]);
        assert_eq!(a.fighters[0].state, State::Ready, "one press per jump");
        while a.fighters[0].y > 0 {
            a.step(empty());
        }
        // Pressed late, the press is still going on landing and ends there.
        a.step([press(JUMP), Input::default()]);
        for _ in 0..36 {
            a.step(empty());
        }
        a.step([press(LIGHT), Input::default()]);
        assert_eq!(a.fighters[0].state, State::Attack(Move::Air));
        while a.fighters[0].y > 0 {
            assert_eq!(a.fighters[0].state, State::Attack(Move::Air));
            a.step(empty());
        }
        assert_eq!(a.fighters[0].state, State::Ready);
        a.step([press(LIGHT), Input::default()]);
        assert_eq!(a.fighters[0].state, State::Attack(Move::Light));
    }
    fn belly_press(guard: bool) -> Arena {
        let mut a = live();
        a.fighters[0].x = -100 * UNIT;
        a.fighters[1].x = 100 * UNIT;
        let p2 = if guard { hold(GUARD) } else { Input::default() };
        a.step([press(JUMP), p2]);
        a.step([press(HEAVY), p2]);
        for _ in 0..20 {
            a.step([Input::default(), p2]);
        }
        a
    }
    #[test]
    fn air_attack_hits_a_standing_fighter_and_is_blockable() {
        assert_eq!(belly_press(false).fighters[1].hp, 90);
        let blocked = belly_press(true);
        assert_eq!(blocked.fighters[1].hp, 97);
        assert_eq!(blocked.fighters[1].state, State::Guard);
    }
    #[test]
    fn knockdown_getup_returns_ready() {
        let mut a = near();
        a.fighters[0].state = State::Attack(Move::Heavy);
        a.fighters[0].age = 14;
        a.step(empty());
        assert_eq!(a.fighters[1].state, State::Knockdown);
        let mut got_up = false;
        for _ in 0..160 {
            a.step(empty());
            got_up |= a.fighters[1].state == State::Getup;
        }
        assert!(got_up);
        assert_eq!(a.fighters[1].state, State::Ready);
        assert_eq!(a.fighters[1].y, 0);
    }
    #[test]
    fn getup_is_invulnerable() {
        let mut a = near();
        a.fighters[1].state = State::Getup;
        a.fighters[0].state = State::Attack(Move::Light);
        a.fighters[0].age = 8;
        assert!(a.step(empty()).is_empty());
        assert_eq!(a.fighters[1].hp, 100);
    }
    #[test]
    fn wall_pushboxes_remain_in_bounds() {
        let mut a = near();
        a.fighters[0].x = WALL - 2 * HALF;
        a.fighters[1].x = WALL;
        for _ in 0..80 {
            a.step([
                Input {
                    held: RIGHT,
                    pressed: 0,
                },
                Input::default(),
            ]);
        }
        assert_eq!(a.fighters[1].x, WALL);
        assert!(a.fighters[1].x - a.fighters[0].x >= 2 * HALF);
    }
    #[test]
    fn swept_pushboxes_prevent_crossing() {
        let mut a = near();
        a.fighters[0].state = State::Hurt;
        a.fighters[0].vx = 900 * UNIT;
        a.step(empty());
        assert!(a.fighters[0].x < a.fighters[1].x);
    }
    #[test]
    fn release_guard_returns_ready() {
        let mut a = near();
        a.step([
            Input {
                held: GUARD,
                pressed: 0,
            },
            Input::default(),
        ]);
        assert_eq!(a.fighters[0].state, State::Guard);
        a.step(empty());
        assert_eq!(a.fighters[0].state, State::Ready);
    }
    #[test]
    fn restart_restores_entire_match() {
        let mut a = near();
        a.remaining = 0;
        a.step(empty());
        assert!(a.outcome.is_some());
        a.restart();
        assert_eq!(a, Arena::default());
    }
    #[test]
    fn replay_is_deterministic() {
        let mut a = Arena::default();
        let mut b = a.clone();
        for t in 0..900 {
            let input = [
                Input {
                    held: if t % 80 < 40 { RIGHT } else { LEFT },
                    pressed: if t % 41 == 0 { LIGHT } else { 0 },
                },
                Input {
                    held: if t % 70 < 35 { LEFT } else { RIGHT },
                    pressed: if t % 77 == 0 { HEAVY } else { 0 },
                },
            ];
            a.step(input);
            b.step(input);
            assert_eq!(a, b);
        }
    }
}
