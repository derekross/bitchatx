use anyhow::Result;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::Identity;
use crate::channels::Message;

// Default Nostr relays for BitchatX (synchronized with bitchat-android)
// These are the same 4 core relays used in bitchat-android's NostrRelayManager.kt
// for consistent connectivity across platforms
const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",      // Damus relay - popular and reliable
    "wss://relay.primal.net",    // Primal relay - good performance
    "wss://offchain.pub",        // Offchain relay - stable connection
    "wss://nostr21.com",         // Nostr21 relay - additional redundancy
];

pub struct NostrClient {
    client: Client,
    identity: Identity,
    subscriptions: HashMap<String, SubscriptionId>,
    message_tx: mpsc::UnboundedSender<Message>,
    status_tx: mpsc::UnboundedSender<String>,
}

impl NostrClient {
    pub async fn new(
        identity: &Identity,
        message_tx: mpsc::UnboundedSender<Message>,
        status_tx: mpsc::UnboundedSender<String>,
    ) -> Result<Self> {
        let client = Client::new(&identity.keys);
        
        // Add relays
        for &relay_url in DEFAULT_RELAYS {
            client.add_relay(relay_url).await?;
        }
        
        Ok(Self {
            client,
            identity: identity.clone(),
            subscriptions: HashMap::new(),
            message_tx,
            status_tx,
        })
    }
    
    pub async fn connect(&mut self) -> Result<()> {
        let _ = self.status_tx.send("Connecting to Nostr relays...".to_string());
        
        // Connect to relays with timeout
        match timeout(Duration::from_secs(10), self.client.connect()).await {
            Ok(_) => {
                let _ = self.status_tx.send("Connected to Nostr network".to_string());
                
                // Start listening for notifications
                self.start_notification_listener().await?;
                Ok(())
            }
            Err(_) => {
                let _ = self.status_tx.send("Connection timeout - using available relays".to_string());
                // Continue with partial connectivity
                self.start_notification_listener().await?;
                Ok(())
            }
        }
    }
    
    async fn start_notification_listener(&self) -> Result<()> {
        let mut notifications = self.client.notifications();
        let message_tx = self.message_tx.clone();
        let status_tx = self.status_tx.clone();
        let our_pubkey = self.identity.pubkey.clone();
        
        tokio::spawn(async move {
            while let Ok(notification) = notifications.recv().await {
                match notification {
                    RelayPoolNotification::Event { event, .. } => {
                        if let Err(e) = Self::handle_event(*event, &message_tx, &our_pubkey).await {
                            let _ = status_tx.send(format!("Error processing event: {}", e));
                        }
                    }
                    RelayPoolNotification::Message { message, .. } => {
                        let _ = status_tx.send(format!("Relay message: {:?}", message));
                    }
                    RelayPoolNotification::RelayStatus { relay_url, status } => {
                        let status_msg = match status {
                            RelayStatus::Connected => format!("Connected to {}", relay_url),
                            RelayStatus::Connecting => format!("Connecting to {}", relay_url),
                            RelayStatus::Disconnected => format!("Disconnected from {}", relay_url),
                            RelayStatus::Initialized => format!("Initialized {}", relay_url),
                            RelayStatus::Pending => format!("Pending connection to {}", relay_url),
                            RelayStatus::Stopped => format!("Stopped {}", relay_url),
                            RelayStatus::Terminated => format!("Terminated {}", relay_url),
                        };
                        let _ = status_tx.send(status_msg);
                    }
                    _ => {}
                }
            }
        });
        
        Ok(())
    }
    
    async fn handle_event(
        event: Event,
        message_tx: &mpsc::UnboundedSender<Message>,
        our_pubkey: &str,
    ) -> Result<()> {
        // Only process kind 20000 (ephemeral events)
        if event.kind() != Kind::Ephemeral(20000) {
            return Ok(());
        }
        
        // Extract geohash from 'g' tag
        let geohash = event
            .tags()
            .iter()
            .find_map(|tag| {
                match tag.as_vec() {
                    vec if vec.len() >= 2 && vec[0] == "g" => {
                        Some(vec[1].to_string())
                    }
                    _ => None
                }
            });
            
        // Extract nickname from 'n' tag  
        let nickname = event
            .tags()
            .iter()
            .find_map(|tag| {
                match tag.as_vec() {
                    vec if vec.len() >= 2 && vec[0] == "n" => {
                        Some(vec[1].to_string())
                    }
                    _ => None
                }
            })
            .unwrap_or_else(|| format!("anon{}", &event.pubkey.to_hex()[..8]));
        
        if let Some(channel) = geohash {
            let is_own = event.pubkey.to_hex() == our_pubkey;
            
            // Skip our own messages if we already have local echo
            if is_own {
                return Ok(());
            }
            
            let message = Message {
                channel,
                nickname,
                content: event.content().to_string(),
                timestamp: chrono::DateTime::from_timestamp(event.created_at().as_u64() as i64, 0)
                    .unwrap_or_else(chrono::Utc::now),
                pubkey: Some(event.pubkey.to_hex()),
                is_own,
            };
            
            let _ = message_tx.send(message);
        }
        
        Ok(())
    }
    
    pub async fn subscribe_to_channel(&mut self, geohash: &str) -> Result<()> {
        // Create subscription filter for ephemeral events in this geohash
        let filter = Filter::new()
            .kind(Kind::Ephemeral(20000))
            .custom_tag(SingleLetterTag::lowercase(Alphabet::G), vec![geohash.to_string()])
            .limit(100)
            .since(Timestamp::now() - Duration::from_secs(3600)); // Last hour
        
        let subscription_id = self.client.subscribe(vec![filter], None).await;
        self.subscriptions.insert(geohash.to_string(), subscription_id);
        
        let _ = self.status_tx.send(format!("Subscribed to channel #{}", geohash));
        Ok(())
    }
    
    pub async fn unsubscribe_from_channel(&mut self, geohash: &str) -> Result<()> {
        if let Some(subscription_id) = self.subscriptions.remove(geohash) {
            self.client.unsubscribe(subscription_id).await;
            let _ = self.status_tx.send(format!("Unsubscribed from channel #{}", geohash));
        }
        Ok(())
    }
    
    pub async fn send_message(&self, channel: &str, content: &str, nickname: &str) -> Result<()> {
        let tags = vec![
            Tag::parse(vec!["g", channel]).unwrap(),
            Tag::parse(vec!["n", nickname]).unwrap(),
            Tag::parse(vec!["t", "bitchatx"]).unwrap(),
            Tag::parse(vec!["client", "bitchatx"]).unwrap(),
        ];
        
        let event_builder = EventBuilder::new(
            Kind::Ephemeral(20000),
            content,
            tags,
        );
        
        let event = self.identity.sign_event(event_builder)?;
        
        // Send to all connected relays with timeout
        match timeout(Duration::from_secs(5), self.client.send_event(event)).await {
            Ok(event_id) => {
                let _ = self.status_tx.send(format!("Message sent to #{}", channel));
            }
            Err(_) => {
                let _ = self.status_tx.send(format!("Message send timeout to #{}", channel));
            }
        }
        
        Ok(())
    }
    
    pub fn get_relay_count(&self) -> usize {
        // Return the number of configured relays (matches bitchat-android default count)
        DEFAULT_RELAYS.len()
    }
}