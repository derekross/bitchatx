use anyhow::Result;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::channels::Message;

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