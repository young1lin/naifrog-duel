//! Shared simulation, input, UI layout and localization for Naifrog Duel.
//! `net` adds the wire protocol; `server` adds the authoritative renderer-free server.
//! `client` adds desktop networking. The default `game` feature builds the Bevy frontend.

/// Application release version, independent of the multiplayer protocol version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod ai;
#[cfg(all(feature = "client", not(target_arch = "wasm32")))]
pub mod client;
pub mod combat;
pub mod controls;
pub mod i18n;
#[cfg(feature = "net")]
pub mod net;
#[cfg(feature = "server")]
pub mod server;
pub mod ui;
