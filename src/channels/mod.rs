use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Participant {
    pub nickname: String,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub message_count: usize,
}

pub use manager::ChannelManager;
pub use message::Message;

mod manager;
mod message;

#[derive(Debug, Clone)]
pub struct Channel {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub geohash: String,
    pub messages: Vec<Message>,
    pub participants: HashMap<String, Participant>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub is_joined: bool,
}

impl Channel {
    pub fn new(geohash: &str) -> Self {
        Self {
            name: format!("#{}", geohash),
            geohash: geohash.to_string(),
            messages: Vec::new(),
            participants: HashMap::new(),
            last_activity: chrono::Utc::now(),
            is_joined: false,
        }
    }
    
    pub fn new_joined(geohash: &str) -> Self {
        Self {
            name: format!("#{}", geohash),
            geohash: geohash.to_string(),
            messages: Vec::new(),
            participants: HashMap::new(),
            last_activity: chrono::Utc::now(),
            is_joined: true,
        }
    }
    
    pub fn add_message(&mut self, message: Message) {
        let now = chrono::Utc::now();
        
        // Update participant info
        if let Some(participant) = self.participants.get_mut(&message.nickname) {
            participant.last_seen = now;
            participant.message_count += 1;
        } else {
            self.participants.insert(
                message.nickname.clone(),
                Participant {
                    nickname: message.nickname.clone(),
                    last_seen: now,
                    message_count: 1,
                }
            );
        }
        
        // Insert message in timestamp order (newer messages at the end)
        // For performance: assume most messages are in chronological order
        // Just append to end and only sort if timestamp is out of order
        if self.messages.last().map_or(true, |last| last.timestamp <= message.timestamp) {
            // Fast path: message is in order, just append
            self.messages.push(message);
        } else {
            // Slow path: message is out of order, use binary search
            let insert_pos = self.messages.binary_search_by(|existing| {
                existing.timestamp.cmp(&message.timestamp)
            }).unwrap_or_else(|e| e);
            self.messages.insert(insert_pos, message);
        }
        self.last_activity = now;
        
        // Keep only last 250 messages per channel (reduced for better performance)
        if self.messages.len() > 250 {
            // Remove oldest messages in batches for better performance
            let remove_count = self.messages.len() - 250;
            self.messages.drain(0..remove_count);
        }
        
        // Clean up inactive participants (not seen for 1 hour)
        let cutoff = now - chrono::Duration::hours(1);
        self.participants.retain(|_, p| p.last_seen > cutoff);
    }
    
    pub fn get_message_count(&self) -> usize {
        self.messages.len()
    }
    
    pub fn get_participant_count(&self) -> usize {
        self.participants.len()
    }
    
    /// Get active participants sorted by recent activity
    #[allow(dead_code)]
    pub fn get_active_participants(&self) -> Vec<&Participant> {
        let mut participants: Vec<&Participant> = self.participants.values().collect();
        // Sort by last activity (most recent first)
        participants.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        participants
    }
    
    /// Find nicknames that start with the given prefix (case-insensitive)
    pub fn find_matching_nicknames(&self, prefix: &str) -> Vec<String> {
        let prefix_lower = prefix.to_lowercase();
        let mut matches: Vec<String> = self.participants
            .values()
            .filter(|p| p.nickname.to_lowercase().starts_with(&prefix_lower))
            .map(|p| p.nickname.clone())
            .collect();
        
        // Sort by recent activity (most recent first)
        matches.sort_by(|a, b| {
            let a_participant = self.participants.get(a);
            let b_participant = self.participants.get(b);
            match (a_participant, b_participant) {
                (Some(a), Some(b)) => b.last_seen.cmp(&a.last_seen),
                _ => std::cmp::Ordering::Equal,
            }
        });
        
        matches
    }
}