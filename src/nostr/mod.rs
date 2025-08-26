use serde::{Deserialize, Serialize};


pub use identity::Identity;
pub use client::NostrClient;
pub use georelay_directory::GeoRelayDirectory;

mod identity;
mod client;
mod georelay_directory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralMessage {
    pub channel: String,
    pub content: String,
    pub nickname: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}