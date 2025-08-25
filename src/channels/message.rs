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
            is_private: false,
            recipient_pubkey: None,
        }
    }
    
    pub fn new_private(
        channel: &str,
        nickname: &str,
        content: &str,
        pubkey: Option<String>,
        is_own: bool,
        recipient_pubkey: Option<String>,
    ) -> Self {
        Self {
            channel: channel.to_string(),
            nickname: nickname.to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
            pubkey,
            is_own,
            is_private: true,
            recipient_pubkey,
        }
    }
    
    pub fn format_for_display(&self) -> String {
        let display_nickname = match &self.pubkey {
            Some(pk) if pk.len() >= 4 => {
                format!("{}#{}", self.nickname, &pk[..4])
            }
            _ => self.nickname.clone(),
        };
        
        format!(
            "[{}] <{}> {}",
            self.timestamp.with_timezone(&chrono::Local).format("%H:%M:%S"),
            display_nickname,
            self.content
        )
    }
    
    pub fn is_from_user(&self, user_pubkey: &str) -> bool {
        self.pubkey.as_deref() == Some(user_pubkey)
    }
}