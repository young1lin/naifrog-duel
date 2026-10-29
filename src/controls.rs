//! Aggregate device sources without losing taps between fixed updates.
use std::collections::BTreeMap;
pub const LEFT: u16 = 1;
pub const RIGHT: u16 = 2;
pub const JUMP: u16 = 4;
pub const LIGHT: u16 = 8;
pub const HEAVY: u16 = 16;
pub const GUARD: u16 = 32;
pub const DASH: u16 = 64;
pub const SHOT: u16 = 128;
pub const TRIG: u16 = JUMP | LIGHT | HEAVY | DASH | SHOT;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input {
    pub held: u16,
    pub pressed: u16,
}
impl Input {
    pub fn holds(self, b: u16) -> bool {
        self.held & b != 0
    }
    pub fn presses(self, b: u16) -> bool {
        self.pressed & b != 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// Keyboard binding set (P1 keys = 0, P2 keys = 1).
    Keyboard(u8),
    /// Gamepad assignment index (first connected = 0).
    Gamepad(u8),
    Mouse,
    Touch(u64),
    /// A network client's already merged controls (server side).
    Remote,
}
#[derive(Clone, Debug, Default)]
pub struct InputMux {
    sources: BTreeMap<Source, u16>,
    pending: BTreeMap<Source, u16>,
    blocked: u16,
}
impl InputMux {
    pub fn held(&self) -> u16 {
        self.sources.values().fold(0, |a, b| a | b)
    }
    pub fn sample(&mut self, source: Source, held: u16) {
        let old = self.held();
        let own = self.sources.get(&source).copied().unwrap_or(0);
        let unconsumed = self.pending.values().fold(0, |a, b| a | b);
        if held == 0 {
            self.sources.remove(&source);
        } else {
            self.sources.insert(source, held);
        }
        let new = self.held();
        self.blocked &= new;
        let edge = ((new & !old) | (held & !own & unconsumed)) & !self.blocked;
        if edge != 0 {
            *self.pending.entry(source).or_default() |= edge;
        }
    }
    pub fn sample_with_press(&mut self, source: Source, held: u16, pressed: u16) {
        let other_held = self
            .sources
            .iter()
            .filter(|(s, _)| **s != source)
            .fold(0, |a, (_, b)| a | b);
        self.sample(source, held);
        let edge = pressed & !other_held & !self.blocked;
        if edge != 0 {
            *self.pending.entry(source).or_default() |= edge;
        }
    }
    pub fn cancel(&mut self, source: Source) {
        self.sources.remove(&source);
        self.pending.remove(&source);
        self.blocked &= self.held();
    }
    pub fn baseline(&mut self, source: Source, held: u16) {
        self.pending.remove(&source);
        if held == 0 {
            self.sources.remove(&source);
        } else {
            self.sources.insert(source, held);
        }
        self.blocked |= held & TRIG;
    }
    pub fn cancel_all(&mut self) {
        *self = Self::default();
    }
    pub fn restart(&mut self) {
        self.pending.clear();
        self.blocked = self.held() & TRIG;
    }
    pub fn take(&mut self) -> Input {
        let pressed = self.pending.values().fold(0, |a, b| a | b) & !self.blocked;
        self.pending.clear();
        Input {
            held: self.held() & !self.blocked,
            pressed,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canceling_one_overlapping_finger_keeps_the_other_edge() {
        let mut m = InputMux::default();
        m.sample(Source::Touch(1), LIGHT);
        m.sample(Source::Touch(2), LIGHT);
        m.cancel(Source::Touch(1));
        assert_eq!(m.take().pressed, LIGHT);
        assert_eq!(m.take().pressed, 0);
    }
    #[test]
    fn explicit_repress_within_frame_is_preserved() {
        let mut m = InputMux::default();
        m.sample(Source::Keyboard(0), LIGHT);
        m.take();
        m.sample_with_press(Source::Keyboard(0), LIGHT, LIGHT);
        assert_eq!(m.take().pressed, LIGHT);
        m.sample(Source::Gamepad(0), LIGHT);
        m.take();
        m.sample_with_press(Source::Keyboard(0), LIGHT, LIGHT);
        assert_eq!(m.take().pressed, 0);
    }
    #[test]
    fn focus_baseline_requires_a_fresh_release() {
        let mut m = InputMux::default();
        m.baseline(Source::Gamepad(0), LIGHT);
        m.sample_with_press(Source::Gamepad(0), LIGHT, 0);
        assert_eq!(m.take().pressed, 0);
        m.sample(Source::Gamepad(0), 0);
        m.sample_with_press(Source::Gamepad(0), LIGHT, LIGHT);
        assert_eq!(m.take().pressed, LIGHT);
    }
    #[test]
    fn quick_tap_survives_release() {
        let mut m = InputMux::default();
        m.sample(Source::Touch(7), LIGHT);
        m.sample(Source::Touch(7), 0);
        assert_eq!(
            m.take(),
            Input {
                held: 0,
                pressed: LIGHT
            }
        );
        assert_eq!(m.take(), Input::default());
    }
    #[test]
    fn fixed_catchup_consumes_edge_once() {
        let mut m = InputMux::default();
        m.sample(Source::Keyboard(0), HEAVY);
        assert_eq!(m.take().pressed, HEAVY);
        assert_eq!(
            m.take(),
            Input {
                held: HEAVY,
                pressed: 0
            }
        );
    }
    #[test]
    fn second_binding_does_not_retrigger() {
        let mut m = InputMux::default();
        m.sample(Source::Keyboard(0), LIGHT);
        m.take();
        m.sample(Source::Gamepad(0), LIGHT);
        assert_eq!(m.take().pressed, 0);
        m.sample(Source::Keyboard(0), 0);
        assert_eq!(m.take().held, LIGHT);
    }
    #[test]
    fn cancellation_only_removes_its_own_edge() {
        let mut m = InputMux::default();
        m.sample(Source::Keyboard(0), LIGHT);
        m.sample(Source::Keyboard(0), 0);
        m.sample(Source::Touch(1), HEAVY);
        m.cancel(Source::Touch(1));
        assert_eq!(m.take().pressed, LIGHT);
    }
    #[test]
    fn held_attack_suppressed_on_rematch() {
        let mut m = InputMux::default();
        m.sample(Source::Keyboard(0), LIGHT | GUARD);
        m.restart();
        assert_eq!(
            m.take(),
            Input {
                held: GUARD,
                pressed: 0
            }
        );
        m.sample(Source::Keyboard(0), LIGHT | GUARD);
        assert_eq!(m.take().pressed, 0);
        m.sample(Source::Keyboard(0), GUARD);
        m.sample(Source::Keyboard(0), LIGHT | GUARD);
        assert_eq!(m.take().pressed, LIGHT);
    }
    #[test]
    fn synthetic_poll_tap_is_latched() {
        let mut m = InputMux::default();
        m.sample_with_press(Source::Keyboard(0), 0, JUMP);
        assert_eq!(m.take().pressed, JUMP);
    }
    #[test]
    fn one_player_on_two_pads_does_not_retrigger_when_one_leaves() {
        let mut m = InputMux::default();
        m.sample(Source::Gamepad(0), LIGHT);
        m.sample(Source::Keyboard(1), RIGHT);
        m.sample(Source::Gamepad(1), LIGHT);
        assert_eq!(m.take().pressed, LIGHT | RIGHT);
        m.cancel(Source::Gamepad(1));
        m.sample(Source::Gamepad(0), LIGHT);
        assert_eq!(
            m.take(),
            Input {
                held: LIGHT | RIGHT,
                pressed: 0
            }
        );
    }
    #[test]
    fn multiple_fingers_hold_independent_controls() {
        let mut m = InputMux::default();
        m.sample(Source::Touch(1), RIGHT);
        m.sample(Source::Touch(2), LIGHT);
        assert_eq!(m.take().held, RIGHT | LIGHT);
        m.sample(Source::Touch(2), 0);
        assert_eq!(m.take().held, RIGHT);
    }
}
