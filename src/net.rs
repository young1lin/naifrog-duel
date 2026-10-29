//! Shared wire protocol. Clients submit controls, never positions, health or winners.
//! Create/Join must match PROTOCOL_VERSION. Input.round must match the room round;
//! ignore old-round packets without refreshing freshness after paired restart.
//! Resume capabilities are session-bound; stale sockets are epoch-fenced.
//! Stable room-lifetime capabilities allow retries when Welcome delivery is lost.
use crate::{
    combat::{Arena, Hit},
    controls::{GUARD, LEFT, RIGHT, TRIG},
    i18n::HAN,
};
use serde::{Deserialize, Serialize};
/// 9: `Arena::dead` rides snapshots; result time excludes frozen (countdown, hit-stop) ticks.
/// 8: an interrupted round counts back in (`Arena::resume`, a 3-2-1 on a frozen clock).
/// 7: snapshots carry per-seat seconds of resume grace left (`grace_left`).
/// 6: snapshots carry the hit-stop counter and each fighter's combo count.
/// 5: Create/Join may carry a display name; snapshots carry both seats' names.
/// 4: snapshots carry the best-of-three match state (`Arena::game`/`wins`/`match_outcome`).
pub const PROTOCOL_VERSION: u8 = 9;
pub const INPUT_MASK: u16 = LEFT | RIGHT | GUARD | TRIG;
pub fn valid_input(held: u16, pressed: u16) -> bool {
    held & !INPUT_MASK == 0 && pressed & !TRIG == 0
}
/// Name widths in thirds of a Latin letter. Fira Mono letters are 0.6 em wide and Han
/// characters 1 em, so 10 letters or 6 Han characters fill the same 6 em of HUD.
const NAME_WIDTH: usize = 30;
/// A player-supplied name, trimmed and restricted to what the canvas can draw: ASCII
/// letters, digits, spaces, `_` and `-` (Fira Mono) and the Han characters of `i18n::HAN`
/// (the name font), at most `NAME_WIDTH` wide. `None` means the caller falls back to the
/// seat default.
pub fn valid_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    let mut width = 0;
    for c in trimmed.chars() {
        width += if c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-') {
            3
        } else if !c.is_ascii() && HAN.contains(c) {
            5
        } else {
            return None;
        };
        if width > NAME_WIDTH {
            return None;
        }
    }
    (width > 0).then(|| trimmed.to_owned())
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    Create {
        version: u8,
        /// Display name; anything `valid_name` rejects falls back to the seat default.
        #[serde(default)]
        name: Option<String>,
    },
    Join {
        version: u8,
        code: String,
        #[serde(default)]
        resume: Option<String>,
        /// Display name; anything `valid_name` rejects falls back to the seat default.
        #[serde(default)]
        name: Option<String>,
    },
    Input {
        seq: u32,
        round: u32,
        held: u16,
        pressed: u16,
    },
    Restart {
        round: u32,
    },
    Leave,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub seq: u64,
    pub round: u32,
    pub arena: Arena,
    pub held: [u16; 2],
    pub connected: [bool; 2],
    pub votes: [bool; 2],
    /// Display names by seat, already validated. Names are not identity: they are
    /// never used for authorization and never logged by the server.
    pub names: [String; 2],
    /// Seconds of resume grace left per seat: `None` while connected (or absent),
    /// counting down from the moment that seat's connection dropped. Optional on the
    /// wire so a new client still parses a pre-7 server that never sends it.
    #[serde(default)]
    pub grace_left: [Option<u16>; 2],
    pub hits: Vec<Hit>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        code: String,
        player: usize,
        resume: String,
    },
    Snapshot {
        // Boxed: the frame dwarfs the other variants, and the room broadcaster already
        // holds it boxed. Serde treats the box as transparent, so the wire is unchanged.
        #[serde(flatten)]
        frame: Box<Snapshot>,
    },
    Error {
        message: String,
    },
    Closed {
        reason: String,
    },
}
/// Browser transport view. The WebSocket implementation remains outside the renderer.
#[derive(Default, Deserialize)]
pub struct RemoteView {
    pub active: bool,
    pub player: Option<usize>,
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub transport: u32,
    pub status: String,
    pub snapshot: Option<Snapshot>,
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::SHOT;
    #[test]
    fn names_are_trimmed_and_restricted() {
        assert_eq!(valid_name(" Alice "), Some("Alice".into()));
        assert_eq!(valid_name("A_1-b Z"), Some("A_1-b Z".into()));
        assert_eq!(valid_name("1234567890"), Some("1234567890".into()));
        assert_eq!(valid_name(""), None, "empty");
        assert_eq!(valid_name("   "), None, "only spaces trim to nothing");
        assert_eq!(valid_name("12345678901"), None, "eleven characters");
        assert_eq!(valid_name("a\tb"), None, "control characters");
        assert_eq!(valid_name("a\u{0}b"), None);
        assert_eq!(valid_name("naïve"), None, "Latin beyond ASCII");
        assert_eq!(valid_name("P-1 _X"), Some("P-1 _X".into()));
    }
    #[test]
    fn names_take_common_han_characters_within_the_same_width() {
        assert_eq!(valid_name("奶蛙"), Some("奶蛙".into()));
        assert_eq!(
            valid_name("\u{3000}奶蛙 2 号\u{3000}"),
            Some("奶蛙 2 号".into())
        );
        // Name characters that GB2312 lacks are in 通用规范汉字表.
        assert_eq!(valid_name("喆堃玥祎赟"), Some("喆堃玥祎赟".into()));
        assert_eq!(
            valid_name("天下第一奶蛙"),
            Some("天下第一奶蛙".into()),
            "six Han"
        );
        assert_eq!(
            valid_name("天下第一大奶蛙"),
            None,
            "seven Han are wider than ten letters"
        );
        assert_eq!(
            valid_name("奶蛙Pro123"),
            Some("奶蛙Pro123".into()),
            "2 Han + 6 letters"
        );
        assert_eq!(valid_name("奶蛙Pro1234"), None, "2 Han + 7 letters");
        assert_eq!(valid_name("奶\u{3000}蛙"), None, "ideographic space inside");
        assert_eq!(valid_name("奶蛙！"), None, "full-width punctuation");
        assert_eq!(valid_name("ＡＢ"), None, "full-width letters");
        assert_eq!(valid_name("龘"), None, "Han outside the common list");
        assert_eq!(valid_name("奶\u{200B}蛙"), None, "zero-width space");
        assert_eq!(valid_name("奶\u{202E}蛙"), None, "bidi override");
        assert_eq!(valid_name("😀"), None, "emoji");
        // The list's own line breaks must not slip through.
        assert_eq!(valid_name("奶\n蛙"), None);
    }
    #[test]
    fn rejects_forged_health_and_player_fields() {
        assert!(
            serde_json::from_str::<ClientMessage>(
                r#"{"type":"input","seq":1,"round":0,"held":0,"pressed":0,"hp":999}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<ClientMessage>(
                r#"{"type":"input","seq":1,"round":0,"held":0,"pressed":0,"player":0}"#
            )
            .is_err()
        );
    }
    #[test]
    fn validates_input_bits() {
        assert!(valid_input(INPUT_MASK, TRIG));
        assert!(valid_input(SHOT, SHOT));
        assert!(!valid_input(256, 0));
        assert!(!valid_input(0, GUARD));
    }
    #[test]
    fn names_travel_on_create_join_and_snapshots() {
        let create = serde_json::from_str::<ClientMessage>(
            r#"{"type":"create","version":5,"name":"Alice"}"#,
        )
        .unwrap();
        assert!(matches!(
            create,
            ClientMessage::Create {
                version: 5,
                name: Some(ref n),
            } if n == "Alice"
        ));
        // The name is optional; unknown fields stay rejected either way.
        assert!(matches!(
            serde_json::from_str::<ClientMessage>(r#"{"type":"create","version":5}"#).unwrap(),
            ClientMessage::Create { name: None, .. }
        ));
        assert!(
            serde_json::from_str::<ClientMessage>(
                r#"{"type":"create","version":5,"name":"A","hp":1}"#
            )
            .is_err()
        );
        let join = serde_json::from_str::<ClientMessage>(
            r#"{"type":"join","version":5,"code":"ABCDEFGH","name":"Bob"}"#,
        )
        .unwrap();
        assert!(matches!(join, ClientMessage::Join { name: Some(ref n), .. } if n == "Bob"));
        let frame = Snapshot {
            seq: 1,
            round: 0,
            arena: Arena::default(),
            held: [0; 2],
            connected: [true; 2],
            votes: [false; 2],
            names: ["Alice".into(), "P2".into()],
            hits: vec![],
            grace_left: [None, None],
        };
        let json = serde_json::to_string(&ServerMessage::Snapshot {
            frame: Box::new(frame),
        })
        .unwrap();
        assert!(json.contains(r#""names":["Alice","P2"]"#));
        assert!(matches!(
            serde_json::from_str::<ServerMessage>(&json).unwrap(),
            ServerMessage::Snapshot { .. }
        ));
    }
    #[test]
    fn snapshot_round_trips() {
        let frame = Snapshot {
            seq: 1,
            round: 0,
            arena: Arena::default(),
            held: [0; 2],
            connected: [true; 2],
            votes: [false; 2],
            names: ["P1".into(), "P2".into()],
            hits: vec![],
            grace_left: [None, Some(239)],
        };
        let json = serde_json::to_string(&ServerMessage::Snapshot {
            frame: Box::new(frame),
        })
        .unwrap();
        assert!(
            json.contains(r#""grace_left":[null,239]"#),
            "grace rides the snapshot wire: {json}"
        );
        let ServerMessage::Snapshot { frame } =
            serde_json::from_str::<ServerMessage>(&json).unwrap()
        else {
            panic!("not a snapshot");
        };
        assert_eq!(frame.grace_left, [None, Some(239)]);
    }
}
