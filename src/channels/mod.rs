use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;

pub use manager::ChannelManager;
pub use message::Message;

mod manager;
mod message;

#[derive(Debug, Clone)]
pub struct Channel {
    pub name: String,
    pub geohash: String,
    pub messages: Vec<Message>,
    pub participants: Vec<String>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub is_joined: bool,
}

impl Channel {
    pub fn new(geohash: &str) -> Self {
        Self {
            name: format!("#{}", geohash),
            geohash: geohash.to_string(),
            messages: Vec::new(),
            participants: Vec::new(),
            last_activity: chrono::Utc::now(),
            is_joined: false,
        }
    }
    
    pub fn new_joined(geohash: &str) -> Self {
        Self {
            name: format!("#{}", geohash),
            geohash: geohash.to_string(),
            messages: Vec::new(),
            participants: Vec::new(),
            last_activity: chrono::Utc::now(),
            is_joined: true,
        }
    }
    
    pub fn add_message(&mut self, message: Message) {
        // Add participant if not already in list
        if !self.participants.contains(&message.nickname) {
            self.participants.push(message.nickname.clone());
        }
        
        self.messages.push(message);
        self.last_activity = chrono::Utc::now();
        
        // Keep only last 1000 messages per channel
        if self.messages.len() > 1000 {
            self.messages.remove(0);
        }
    }
    
    pub fn get_message_count(&self) -> usize {
        self.messages.len()
    }
    
    pub fn get_participant_count(&self) -> usize {
        self.participants.len()
    }
}