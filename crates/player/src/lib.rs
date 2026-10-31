//! Tocar YouTube Music: resolver (yt-dlp), baixar (reqwest), decodificar
//! (Symphonia) e tocar (fastframe-audio), com a fila e o rádio do site.

pub mod engine;
mod job;
pub mod m4a;
pub mod offline;
mod render;
pub mod source;
pub mod ytdlp;

pub use engine::{Command, Config, Player, Repeat, Snapshot, Status, curve};
pub use offline::Offline;
