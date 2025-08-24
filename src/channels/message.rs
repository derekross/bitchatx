use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub channel: String,
    pub nickname: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub pubkey: Option<String>,
    pub is_own: bool,
}

impl Message {
    pub fn new(
        channel: &str,
        nickname: &str,
        content: &str,
        pubkey: Option<String>,
        is_own: bool,
    ) -> Self {
        Self {
            channel: channel.to_string(),
            nickname: nickname.to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
            pubkey,
            is_own,
        }
    }
    
    pub fn format_for_display(&self) -> String {
        format!(
            "[{}] <{}> {}",
            self.timestamp.format("%H:%M:%S"),
            self.nickname,
            self.content
        )
    }
    
    pub fn is_from_user(&self, user_pubkey: &str) -> bool {
        self.pubkey.as_deref() == Some(user_pubkey)
    }
}