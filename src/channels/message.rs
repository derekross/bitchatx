use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub channel: String,
    pub nickname: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub pubkey: Option<String>,
    pub is_own: bool,
    pub is_private: bool,
    pub recipient_pubkey: Option<String>,
}

