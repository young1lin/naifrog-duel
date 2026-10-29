//! CPU opponent for single-player mode. It only produces controller input for its
//! own seat, which goes through the same `Arena::step` rules as a human player: it
//! never edits positions, health or the outcome. It sees the opponent with a
//! human-like delay (MIN_REACTION..=MAX_REACTION ticks) and commits to short plans,
//! so it cannot react to a move faster than a person could.
use crate::combat::{Arena, Fighter, Move, SHOT_SPEED, Shot, State, UNIT, WALL};
use crate::controls::{DASH, GUARD, HEAVY, Input, JUMP, LEFT, LIGHT, RIGHT, SHOT};

/// Fewest ticks (60 Hz) between something happening and the CPU acting on it.
pub const MIN_REACTION: u32 = 12;
/// Most ticks of perception delay for one decision.
pub const MAX_REACTION: u32 = 20;
const MEMORY: usize = 32;
// Center-to-center distances (units) at which each move connects.
const LIGHT_RANGE: i32 = 205;
const HEAVY_RANGE: i32 = 240;
const DASH_RANGE: i32 = 320;
/// Belly press when the opponent is this close (centre to centre) on the way down.
const AIR_RANGE: i32 = 200;
// A forward jump clears a bubble whose centre is about this far away when the jump starts.
const SHOT_JUMP_NEAR: i32 = 180;
const SHOT_JUMP_FAR: i32 = 320;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Plan {
    Wait,
    Approach,
    Retreat,
    Guard,
    /// Press the button once, then stay neutral through the move and a short pause.
    Attack(u16),
    /// Press jump once; hold this direction (-1, 0, 1 relative to the opponent).
    Jump(i32),
}

#[derive(Clone, Copy, Debug)]
struct Seen {
    x: i32,
    state: State,
    age: u32,
    /// The opponent's milk bubble, if one was flying.
    shot: Option<Shot>,
}
impl Seen {
    fn of(f: &Fighter, shot: Option<Shot>) -> Self {
        Self {
            x: f.x,
            state: f.state,
            age: f.age,
            shot,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Cpu {
    rng: u32,
    memory: [Option<Seen>; MEMORY],
    seen_ticks: usize,
    reaction: u32,
    plan: Plan,
    left: u32,
    fresh: bool,
    /// Belly press on the way down during the current jump.
    dive: bool,
}

impl Cpu {
    pub fn new(seed: u32) -> Self {
        Self {
            rng: seed.max(1),
            memory: [None; MEMORY],
            seen_ticks: 0,
            reaction: MIN_REACTION,
            plan: Plan::Wait,
            left: 0,
            fresh: false,
            dive: false,
        }
    }

    /// Controller input for seat `me` this tick. Call exactly once per fixed tick,
    /// before `Arena::step`, and feed the result only to seat `me`.
    pub fn input(&mut self, arena: &Arena, me: usize) -> Input {
        let own = &arena.fighters[me];
        self.memory[self.seen_ticks % MEMORY] =
            Some(Seen::of(&arena.fighters[1 - me], arena.shots[1 - me]));
        self.seen_ticks += 1;
        if arena.outcome.is_some() {
            self.left = 0;
            return Input::default();
        }
        match own.state {
            State::Ready | State::Guard => {}
            // Own move: keep the countdown so the pause after it still happens.
            State::Attack(_) => {
                self.left = self.left.saturating_sub(1);
                return Input::default();
            }
            // Stunned: the old plan is void; decide afresh once able to act.
            _ => {
                self.left = 0;
                return Input::default();
            }
        }
        let seen = self.perceived();
        if self.dive
            && own.y > 0
            && own.vy < 0
            && own.state == State::Ready
            && !own.air_used
            && (seen.x - own.x).abs() / UNIT <= AIR_RANGE
        {
            self.dive = false;
            self.left = self.left.saturating_sub(1);
            return Input {
                held: LIGHT,
                pressed: LIGHT,
            };
        }
        let interrupt = (self.threatened(own, seen) || self.incoming(own, seen).is_some())
            && matches!(self.plan, Plan::Wait | Plan::Approach | Plan::Retreat);
        if self.left == 0 || interrupt {
            self.choose(own, seen, arena.shots[me].is_some());
        }
        self.left = self.left.saturating_sub(1);
        let toward = if seen.x >= own.x { RIGHT } else { LEFT };
        let away = if toward == RIGHT { LEFT } else { RIGHT };
        let first = std::mem::take(&mut self.fresh);
        match self.plan {
            Plan::Wait => Input::default(),
            Plan::Approach => Input {
                held: toward,
                pressed: 0,
            },
            Plan::Retreat => Input {
                held: away,
                pressed: 0,
            },
            Plan::Guard => Input {
                held: GUARD,
                pressed: 0,
            },
            Plan::Attack(bit) => Input {
                held: if first { bit } else { 0 },
                pressed: if first { bit } else { 0 },
            },
            Plan::Jump(direction) => {
                let held = match direction {
                    1 => toward,
                    -1 => away,
                    _ => 0,
                };
                Input {
                    held: held | if first { JUMP } else { 0 },
                    pressed: if first { JUMP } else { 0 },
                }
            }
        }
    }

    fn roll(&mut self, n: u32) -> u32 {
        // xorshift32: deterministic per seed, no global state.
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng % n
    }
    fn between(&mut self, low: u32, high: u32) -> u32 {
        low + self.roll(high - low + 1)
    }

    /// The opponent as it was `reaction` ticks ago (or the oldest memory).
    fn perceived(&self) -> Seen {
        let back = (self.reaction as usize).min(self.seen_ticks - 1);
        self.memory[(self.seen_ticks - 1 - back) % MEMORY].expect("observed this tick")
    }

    fn threatened(&self, own: &Fighter, seen: Seen) -> bool {
        let State::Attack(m) = seen.state else {
            return false;
        };
        let d = m.data();
        seen.age < d.startup + d.active
            && (seen.x - own.x).abs() / UNIT
                < d.reach / UNIT + if m == Move::Dash { 160 } else { 90 }
    }

    /// Where the opponent's bubble is now, extrapolated from the delayed sighting, as a
    /// centre distance in units, if it is flying at this fighter and close enough to matter.
    fn incoming(&self, own: &Fighter, seen: Seen) -> Option<i32> {
        let s = seen.shot?;
        let now = s.x + s.dir * SHOT_SPEED * self.reaction as i32;
        let distance = (own.x - now) * s.dir / UNIT;
        (0..=SHOT_JUMP_FAR).contains(&distance).then_some(distance)
    }

    fn set(&mut self, plan: Plan, ticks: u32) {
        self.plan = plan;
        self.left = ticks.max(1);
        self.fresh = true;
        if let Plan::Jump(_) = plan {
            self.dive = self.roll(100) < 60;
        }
    }

    fn attack(&mut self, m: Move) {
        let bit = match m {
            Move::Light | Move::Air => LIGHT,
            Move::Heavy => HEAVY,
            Move::Dash => DASH,
            Move::Shot => SHOT,
        };
        let pause = self.between(4, 18);
        self.set(Plan::Attack(bit), m.total() + pause);
    }

    fn choose(&mut self, own: &Fighter, seen: Seen, shooting: bool) {
        self.reaction = self.between(MIN_REACTION, MAX_REACTION);
        let distance = (seen.x - own.x).abs() / UNIT;
        let toward_wall = if seen.x >= own.x { -1 } else { 1 };
        let cornered = (own.x * toward_wall) > WALL - 60 * UNIT;
        let grounded = own.y == 0;
        let r = self.roll(100);
        if !grounded {
            let ticks = self.between(4, 8);
            self.set(Plan::Wait, ticks);
            return;
        }
        if let Some(distance) = self.incoming(own, seen) {
            if distance >= SHOT_JUMP_NEAR && r < 70 {
                self.set(Plan::Jump(1), 40);
            } else {
                let ticks = self.between(18, 28);
                self.set(Plan::Guard, ticks);
            }
            return;
        }
        if self.threatened(own, seen) {
            match r {
                0..45 => {
                    let ticks = self.between(14, 26);
                    self.set(Plan::Guard, ticks)
                }
                45..60 if !cornered => {
                    let ticks = self.between(10, 18);
                    self.set(Plan::Retreat, ticks)
                }
                45..70 => self.set(Plan::Jump(-1), 40),
                _ if distance <= LIGHT_RANGE => self.attack(Move::Light),
                _ => {
                    let ticks = self.between(6, 12);
                    self.set(Plan::Wait, ticks)
                }
            }
            return;
        }
        if matches!(seen.state, State::Knockdown | State::Getup | State::Ko) {
            // Invulnerable on the ground: close in or get ready instead of whiffing.
            if distance > HEAVY_RANGE {
                let ticks = self.between(8, 16);
                self.set(Plan::Approach, ticks);
            } else if r < 30 {
                let ticks = self.between(12, 24);
                self.set(Plan::Guard, ticks);
            } else {
                let ticks = self.between(6, 14);
                self.set(Plan::Wait, ticks);
            }
            return;
        }
        if distance > DASH_RANGE {
            if r < 8 {
                self.set(Plan::Jump(1), 40);
            } else if r < 30 && !shooting {
                self.attack(Move::Shot);
            } else {
                let ticks = self.between(12, 28);
                self.set(Plan::Approach, ticks);
            }
        } else if distance > HEAVY_RANGE {
            match r {
                0..18 => self.attack(Move::Dash),
                18..75 => {
                    let ticks = self.between(8, 16);
                    self.set(Plan::Approach, ticks)
                }
                75..85 => self.set(Plan::Jump(1), 40),
                _ => {
                    let ticks = self.between(6, 14);
                    self.set(Plan::Wait, ticks)
                }
            }
        } else if distance > LIGHT_RANGE {
            match r {
                0..40 => self.attack(Move::Heavy),
                40..70 => {
                    let ticks = self.between(4, 10);
                    self.set(Plan::Approach, ticks)
                }
                70..80 => self.attack(Move::Dash),
                80..90 if !cornered => {
                    let ticks = self.between(6, 12);
                    self.set(Plan::Retreat, ticks)
                }
                _ => {
                    let ticks = self.between(6, 12);
                    self.set(Plan::Wait, ticks)
                }
            }
        } else {
            match r {
                0..45 => self.attack(Move::Light),
                45..65 => self.attack(Move::Heavy),
                65..75 => {
                    let ticks = self.between(10, 20);
                    self.set(Plan::Guard, ticks)
                }
                75..88 if !cornered => {
                    let ticks = self.between(8, 16);
                    self.set(Plan::Retreat, ticks)
                }
                75..88 => self.attack(Move::Light),
                _ => {
                    let direction = if cornered { 1 } else { -1 };
                    self.set(Plan::Jump(direction), 40)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::HIT_STOP_TICKS;
    use crate::controls::TRIG;

    const ALL: u16 = LEFT | RIGHT | JUMP | LIGHT | HEAVY | GUARD | DASH | SHOT;

    /// A simple aggressive scripted human: walk in, alternate light and heavy.
    fn bully(a: &Arena, me: usize) -> Input {
        let f = &a.fighters[me];
        let o = &a.fighters[1 - me];
        let distance = (o.x - f.x).abs() / UNIT;
        let toward = if o.x >= f.x { RIGHT } else { LEFT };
        if distance > 190 {
            Input {
                held: toward,
                pressed: 0,
            }
        } else if a.ticks.is_multiple_of(40) {
            Input {
                held: LIGHT,
                pressed: LIGHT,
            }
        } else if a.ticks % 97 == 3 {
            Input {
                held: HEAVY,
                pressed: HEAVY,
            }
        } else {
            Input::default()
        }
    }

    #[test]
    fn cpu_only_emits_valid_controls_and_never_edits_the_arena() {
        let mut arena = Arena::default();
        let mut cpu = Cpu::new(7);
        let mut landed = false;
        // The versus screen adds 120 dead ticks to the match's opening.
        for _ in 0..1320 {
            let before = arena.clone();
            let input = cpu.input(&arena, 1);
            assert_eq!(arena, before, "deciding must not touch the simulation");
            assert_eq!(input.held & !ALL, 0);
            assert_eq!(input.pressed & !TRIG, 0);
            assert_eq!(input.pressed & !input.held, 0, "a press is also held");
            arena.step([Input::default(), input]);
            // Sampled during the run, not at its end: a KO rolls the match into a
            // fresh round whose health is back to full, so the endpoint alone says
            // nothing about whether the CPU fought.
            landed |= arena.fighters[0].hp < 100;
        }
        assert!(landed, "the CPU walks over and lands hits");
        assert_eq!(arena.fighters[1].hp, 100);
    }

    fn able(f: &Fighter) -> bool {
        matches!(f.state, State::Ready | State::Guard)
    }

    #[test]
    fn approaches_from_spawn_and_attacks_only_at_range() {
        for seed in 1..=40 {
            let mut arena = Arena::default();
            let mut cpu = Cpu::new(seed);
            let start = (arena.fighters[1].x - arena.fighters[0].x).abs();
            let mut first_attack = None;
            for tick in 0..900 {
                let input = cpu.input(&arena, 1);
                let distance = (arena.fighters[1].x - arena.fighters[0].x).abs() / UNIT;
                for (bit, range) in [
                    (LIGHT, LIGHT_RANGE),
                    (HEAVY, HEAVY_RANGE),
                    (DASH, DASH_RANGE),
                ] {
                    // Releasing guard and attacking happen on the same tick.
                    if input.pressed & bit != 0 && able(&arena.fighters[1]) {
                        // Exact before the first contact; afterwards the CPU may act on a
                        // position up to MAX_REACTION ticks old (knockback in between).
                        let slack = if first_attack.is_none() { 0 } else { 100 };
                        assert!(
                            distance <= range + slack,
                            "seed {seed}: bit {bit} pressed at {distance}"
                        );
                        first_attack.get_or_insert(tick);
                    }
                }
                // A bubble thrown from afar engages too; it has no range limit to check.
                if input.pressed & SHOT != 0 && able(&arena.fighters[1]) {
                    first_attack.get_or_insert(tick);
                }
                arena.step([Input::default(), input]);
            }
            assert!(
                first_attack.is_some_and(|t| t < 300),
                "seed {seed} never engaged"
            );
            assert!((arena.fighters[1].x - arena.fighters[0].x).abs() < start);
        }
    }

    #[test]
    fn uses_every_tool_with_rhythm_but_does_not_turtle_or_spam() {
        let mut used = 0;
        let (mut guard_ticks, mut presses, mut jumps, mut total) = (0u32, 0u32, 0u32, 0u32);
        let mut air_presses = 0;
        for seed in 1..=8 {
            let mut arena = Arena::default();
            let mut cpu = Cpu::new(seed * 7919);
            for _ in 0..3600 {
                let input = cpu.input(&arena, 1);
                // Hit-stop ticks are dead: the edges they carry are dropped by the sim,
                // so rhythm is measured on live ticks only.
                let live = arena.hitstop == 0;
                if live {
                    total += 1;
                }
                if input.held & GUARD != 0 && live {
                    guard_ticks += 1;
                }
                if input.pressed & (LIGHT | HEAVY | DASH | SHOT) != 0 && live {
                    presses += 1;
                    let own = &arena.fighters[1];
                    // In the air only the one belly press per jump is possible.
                    let air = own.y > 0
                        && own.state == State::Ready
                        && !own.air_used
                        && input.pressed == LIGHT;
                    assert!(
                        matches!(own.state, State::Ready | State::Guard) && own.y == 0 || air,
                        "mashing while unable to act: {:?}",
                        own.state
                    );
                    air_presses += u32::from(air);
                }
                if input.pressed & JUMP != 0 && live {
                    jumps += 1;
                }
                used |= input.pressed | (input.held & GUARD);
                arena.step([bully(&arena, 0), input]);
                if arena.outcome.is_some() {
                    arena.restart();
                }
            }
        }
        assert_eq!(
            used,
            LIGHT | HEAVY | DASH | SHOT | JUMP | GUARD,
            "used {used:#b}"
        );
        assert!(air_presses > 0, "never pressed from the air");
        assert!(
            guard_ticks * 2 < total,
            "guards {guard_ticks} of {total} ticks"
        );
        // Hit-stop eats live ticks out of the fixed loop whenever the bully connects,
        // which trims attacks more than the bubble-triggered jumps; attacks must still
        // clearly dominate.
        assert!(jumps * 3 < presses, "jumps {jumps} vs attacks {presses}");
        assert!(presses * 30 < total, "{presses} attacks in {total} ticks");
    }

    #[test]
    fn cpu_jumps_or_guards_incoming_shots() {
        let (mut avoided, mut jumped) = (0, 0);
        for seed in 1..=40 {
            let mut arena = Arena::default();
            // Start far apart so there is time to see the bubble coming.
            arena.fighters[0].x = -500 * UNIT;
            arena.fighters[1].x = 300 * UNIT;
            let mut cpu = Cpu::new(seed);
            let mut hurt = false;
            for t in 0..120 {
                let p1 = if t == 0 {
                    Input {
                        held: SHOT,
                        pressed: SHOT,
                    }
                } else {
                    Input::default()
                };
                let input = cpu.input(&arena, 1);
                jumped += u32::from(input.pressed & JUMP != 0);
                for hit in arena.step([p1, input]) {
                    hurt |= hit.victim == 1 && hit.shot && !hit.blocked;
                }
            }
            avoided += u32::from(!hurt);
        }
        assert!(avoided >= 30, "avoided {avoided}/40");
        assert!(jumped >= 10, "jumped {jumped} times");
    }

    #[test]
    fn cannot_react_to_anything_newer_than_its_reaction_time() {
        let mut reacted = 0;
        for seed in 1..=80 {
            let mut calm = Arena::default();
            calm.fighters[0].x = -100 * UNIT;
            calm.fighters[1].x = 100 * UNIT;
            let mut cpu = Cpu::new(seed);
            // The versus screen and round introduction eat input; wait them out first.
            while calm.intro > 0 {
                let input = cpu.input(&calm, 1);
                calm.step([Input::default(), input]);
            }
            for _ in 0..30 + seed {
                let input = cpu.input(&calm, 1);
                calm.step([Input::default(), input]);
            }
            // Identical histories, except the opponent starts a heavy in one of them.
            let (mut threat, mut twin) = (calm.clone(), cpu.clone());
            let heavy = Input {
                held: HEAVY,
                pressed: HEAVY,
            };
            let mut diverged = None;
            // The hit-stop after the heavy lands freezes the threat arena for
            // HIT_STOP_TICKS ticks; post-hit divergence needs those back to show.
            for t in 0..MAX_REACTION + 30 + u32::from(HIT_STOP_TICKS) {
                let a = cpu.input(&calm, 1);
                let b = twin.input(&threat, 1);
                if a != b && diverged.is_none() {
                    diverged = Some(t);
                }
                calm.step([Input::default(), a]);
                threat.step([if t == 0 { heavy } else { Input::default() }, b]);
            }
            // The heavy starts on threat tick 0 and is first observable at t = 1.
            if let Some(t) = diverged {
                assert!(t > MIN_REACTION, "seed {seed}: reacted after {t} ticks");
                reacted += 1;
            }
        }
        assert!(reacted > 10, "the CPU does react eventually ({reacted}/80)");
    }

    #[test]
    fn same_seed_same_choices_different_seed_different_choices() {
        let run = |seed| {
            let mut arena = Arena::default();
            let mut cpu = Cpu::new(seed);
            (0..1200)
                .map(|_| {
                    let input = cpu.input(&arena, 1);
                    arena.step([bully(&arena, 0), input]);
                    input
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(5), run(5));
        assert_ne!(run(5), run(6));
    }

    #[test]
    fn two_cpus_finish_a_match_with_damage_on_both_sides() {
        use crate::combat::{Outcome, VS_TICKS};
        for seed in 1..=10 {
            let mut arena = Arena::default();
            let (mut p1, mut p2) = (Cpu::new(seed), Cpu::new(seed + 1000));
            let (mut hurt, mut hits) = ([false; 2], 0);
            while arena.match_outcome.is_none() {
                assert!(
                    arena.ticks < 99 * 60 + VS_TICKS + 10,
                    "seed {seed}: a round ran long"
                );
                let input = [p1.input(&arena, 0), p2.input(&arena, 1)];
                for hit in arena.step(input) {
                    hits += 1;
                    hurt[hit.victim] |= !hit.blocked;
                }
            }
            assert!(
                hits > 4 && hurt == [true; 2],
                "seed {seed}: {hits} hits, hurt {hurt:?}"
            );
            // The outcome must agree with the round-win majority, the way a five-round
            // match is decided: a winner holds more round wins, a draw holds equal ones.
            assert!(
                match arena.match_outcome {
                    Some(Outcome::Winner(w)) => arena.wins[w] > arena.wins[1 - w],
                    Some(Outcome::Draw) => arena.wins[0] == arena.wins[1],
                    None => false,
                },
                "seed {seed}: outcome {:?} disagrees with wins {:?}",
                arena.match_outcome,
                arena.wins
            );
        }
    }
}
