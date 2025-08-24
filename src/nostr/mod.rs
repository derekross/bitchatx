use serde::{Deserialize, Serialize};


pub use identity::Identity;
pub use client::NostrClient;

mod identity;
mod client;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralMessage {
    pub channel: String,
    pub content: String,
    pub nickname: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}