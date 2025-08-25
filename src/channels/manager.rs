use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::mpsc;

use super::{Channel, Message};

pub struct ChannelManager {
    channels: HashMap<String, Channel>,
    #[allow(dead_code)]
    message_tx: mpsc::UnboundedSender<Message>,
}

impl ChannelManager {
    pub fn new(message_tx: mpsc::UnboundedSender<Message>) -> Self {
        Self {
            channels: HashMap::new(),
            message_tx,
        }
    }
    
    pub async fn join_channel(&mut self, geohash: &str) -> Result<()> {
        if let Some(channel) = self.channels.get_mut(geohash) {
            // Mark existing channel as joined
            channel.is_joined = true;
        } else {
            // Create new joined channel
            let channel = Channel::new_joined(geohash);
            self.channels.insert(geohash.to_string(), channel);
        }
        Ok(())
    }
    
    pub async fn leave_channel(&mut self, geohash: &str) -> Result<()> {
        self.channels.remove(geohash);
        Ok(())
    }
    
    pub async fn add_message(&mut self, message: Message) {
        let channel_name = message.channel.clone();
        
        // Create channel if it doesn't exist
        if !self.channels.contains_key(&channel_name) {
            let channel = Channel::new(&channel_name);
            self.channels.insert(channel_name.clone(), channel);
        }
        
        // Add message to channel
        if let Some(channel) = self.channels.get_mut(&channel_name) {
            channel.add_message(message);
        }
    }
    
    pub fn add_message_sync(&mut self, message: Message) {
        let channel_name = message.channel.clone();
        
        // Create channel if it doesn't exist
        if !self.channels.contains_key(&channel_name) {
            let channel = Channel::new(&channel_name);
            self.channels.insert(channel_name.clone(), channel);
        }
        
        // Add message to channel
        if let Some(channel) = self.channels.get_mut(&channel_name) {
            channel.add_message(message);
        }
    }
    
    pub fn get_channel(&self, geohash: &str) -> Option<&Channel> {
        self.channels.get(geohash)
    }
    
    pub fn get_channel_mut(&mut self, geohash: &str) -> Option<&mut Channel> {
        self.channels.get_mut(geohash)
    }
    
    pub fn list_channels(&self) -> Vec<String> {
        // Only return actually joined channels
        let mut channels: Vec<String> = self.channels
            .iter()
            .filter(|(_, channel)| channel.is_joined)
            .map(|(name, _)| name.clone())
            .collect();
        channels.sort_by(|a, b| {
            let a_activity = self.channels.get(a).map(|c| c.last_activity);
            let b_activity = self.channels.get(b).map(|c| c.last_activity);
            b_activity.cmp(&a_activity) // Most recent first
        });
        channels
    }
    
    pub fn list_all_channels(&self) -> Vec<(String, bool)> {
        // Return all channels with joined status
        let mut channels: Vec<(String, bool)> = self.channels
            .iter()
            .map(|(name, channel)| (name.clone(), channel.is_joined))
            .collect();
        channels.sort_by(|a, b| {
            let a_activity = self.channels.get(&a.0).map(|c| c.last_activity);
            let b_activity = self.channels.get(&b.0).map(|c| c.last_activity);
            b_activity.cmp(&a_activity) // Most recent first
        });
        channels
    }
    
    pub fn get_message_count(&self, geohash: &str) -> usize {
        self.channels.get(geohash)
            .map(|c| c.get_message_count())
            .unwrap_or(0)
    }
    
    pub fn get_active_user_count(&self, geohash: &str) -> usize {
        self.channels.get(geohash)
            .map(|c| c.get_participant_count())
            .unwrap_or(0)
    }
    
    pub fn get_active_channel_count(&self) -> usize {
        self.channels.len()
    }
    
    pub fn get_recent_messages(&self, geohash: &str, limit: usize) -> Vec<&Message> {
        if let Some(channel) = self.channels.get(geohash) {
            let start = channel.messages.len().saturating_sub(limit);
            channel.messages[start..].iter().collect()
        } else {
            vec![]
        }
    }
    
    pub fn search_messages(&self, geohash: &str, query: &str) -> Vec<&Message> {
        if let Some(channel) = self.channels.get(geohash) {
            channel.messages
                .iter()
                .filter(|msg| msg.content.to_lowercase().contains(&query.to_lowercase()))
                .collect()
        } else {
            vec![]
        }
    }
    
    /// Clear all messages from a specific channel
    pub fn clear_channel(&mut self, geohash: &str) -> bool {
        if let Some(channel) = self.channels.get_mut(geohash) {
            let message_count = channel.messages.len();
            channel.messages.clear();
            message_count > 0
        } else {
            false
        }
    }
}