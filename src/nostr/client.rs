use anyhow::Result;
use nostr_sdk::prelude::*;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::{Identity, GeoRelayDirectory};
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
    geo_relay_directory: GeoRelayDirectory,
    connected_relays: HashSet<String>,
}

impl NostrClient {
    pub async fn new(
        identity: &Identity,
        message_tx: mpsc::UnboundedSender<Message>,
        status_tx: mpsc::UnboundedSender<String>,
    ) -> Result<Self> {
        let client = Client::new(&identity.keys);
        
        // Initialize georelay directory
        let geo_relay_directory = GeoRelayDirectory::new()?;
        geo_relay_directory.initialize().await?;
        
        // Add default relays for initial connection
        // These will be supplemented with geohash-specific relays when joining channels
        let mut connected_relays = HashSet::new();
        for &relay_url in DEFAULT_RELAYS {
            client.add_relay(relay_url).await?;
            connected_relays.insert(relay_url.to_string());
        }
        
        Ok(Self {
            client,
            identity: identity.clone(),
            subscriptions: HashMap::new(),
            message_tx,
            status_tx,
            geo_relay_directory,
            connected_relays,
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
                // Process notifications immediately without any buffering
                match notification {
                    RelayPoolNotification::Event { event, .. } => {
                        if let Err(e) = Self::handle_event(*event, &message_tx, &status_tx, &our_pubkey).await {
                            let _ = status_tx.send(format!("Error processing event: {}", e));
                        }
                    }
                    RelayPoolNotification::Message { .. } => {
                        // Don't show raw relay messages to users
                    }
                    RelayPoolNotification::RelayStatus { .. } => {
                        // Don't show relay connection status messages to users
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
        _status_tx: &mpsc::UnboundedSender<String>,
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
                is_private: false,
                recipient_pubkey: None,
            };
            
            let _ = message_tx.send(message);
        }
        
        Ok(())
    }
    
    pub async fn subscribe_to_channel(&mut self, geohash: &str) -> Result<()> {
        // Create subscription filter first (for immediate subscription to default relays)
        let filter = Filter::new()
            .kind(Kind::Ephemeral(20000))
            .custom_tag(SingleLetterTag::lowercase(Alphabet::G), vec![geohash.to_string()])
            .limit(1000); // Remove time filter to get messages immediately
        
        // Connect to geohash-specific relays first to get best coverage
        self.ensure_georelays_connected(geohash).await?;
        
        // Then subscribe to all connected relays (including new georelays)
        let subscription_id = self.client.subscribe(vec![filter], None).await;
        self.subscriptions.insert(geohash.to_string(), subscription_id);
        
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
        
        // Send to all connected relays in background (fire-and-forget)
        let client = self.client.clone();
        let status_tx = self.status_tx.clone();
        let channel = channel.to_string();
        tokio::spawn(async move {
            match timeout(Duration::from_secs(5), client.send_event(event)).await {
                Ok(_event_id) => {
                    // Don't spam with "Message sent" notifications
                }
                Err(_) => {
                    let _ = status_tx.send(format!("Message send timeout to #{}", channel));
                }
            }
        });
        
        Ok(())
    }
    
    /// Ensure that georelays are connected for a specific geohash
    async fn ensure_georelays_connected(&mut self, geohash: &str) -> Result<()> {
        // Get closest relays for this geohash
        let georelay_urls = self.geo_relay_directory.closest_relays_for_geohash(geohash, Some(5)).await;
        
        // Add geohash-specific relays to client
        for relay_url in &georelay_urls {
            // Only add if not already connected
            if !self.connected_relays.contains(relay_url) {
                match self.client.add_relay(relay_url.clone()).await {
                    Ok(_) => {
                        self.connected_relays.insert(relay_url.clone());
                        let total_relays = self.connected_relays.len();
                        let _ = self.status_tx.send(format!("Connected to georelay: {} (total: {})", relay_url, total_relays));
                    }
                    Err(e) => {
                        let _ = self.status_tx.send(format!("Failed to add georelay {}: {}", relay_url, e));
                    }
                }
            }
        }
        
        // Connect to any new relays
        if !georelay_urls.is_empty() {
            let _ = self.client.connect().await;
        }
        
        Ok(())
    }
    
    pub fn get_relay_count(&self) -> usize {
        // Return the actual number of connected relays (defaults + georelays)
        self.connected_relays.len()
    }
    
    /// Get the current relay count including georelays
    
    /// Get relay connection statistics
    pub fn get_relay_stats(&self) -> (usize, usize) {
        let total_connected = self.connected_relays.len();
        let default_count = DEFAULT_RELAYS.len();
        let georelay_count = total_connected.saturating_sub(default_count);
        (default_count, georelay_count)
    }
}