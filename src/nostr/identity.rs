use anyhow::{anyhow, Result};
use nostr_sdk::prelude::*;
use rand::{thread_rng, Rng};

#[derive(Debug, Clone)]
pub struct Identity {
    pub keys: Keys,
    pub pubkey: String,
    pub nickname: String,
    pub is_ephemeral: bool,
}

impl Identity {
    /// Create an ephemeral identity (generates new keypair each session)
    pub fn ephemeral() -> Self {
        let keys = Keys::generate();
        let pubkey = keys.public_key().to_hex();
        let nickname = generate_random_nickname();
        
        Self {
            keys,
            pubkey,
            nickname,
            is_ephemeral: true,
        }
    }
    
    /// Create identity from nsec private key and fetch profile
    pub async fn from_nsec(nsec: &str) -> Result<Self> {
        let secret_key = SecretKey::from_bech32(nsec)
            .map_err(|_| anyhow!("Invalid nsec format"))?;
        let keys = Keys::new(secret_key);
        let pubkey_hex = keys.public_key().to_hex();
        let pubkey = keys.public_key();
        
        // Try to fetch profile metadata from Nostr relays
        let nickname = match Self::fetch_profile_name(&pubkey).await {
            Ok(name) if !name.trim().is_empty() => name,
            _ => {
                // Fallback to a user-friendly format if profile fetch fails
                format!("user{}", &pubkey_hex[..8])
            }
        };
        
        Ok(Self {
            keys,
            pubkey: pubkey_hex,
            nickname,
            is_ephemeral: false,
        })
    }
    
    /// Fetch profile name from Nostr relays
    async fn fetch_profile_name(pubkey: &PublicKey) -> Result<String> {
        // Create a temporary client to fetch profile metadata
        let client = Client::default();
        
        // Add some popular relays for profile fetching
        client.add_relay("wss://relay.damus.io").await?;
        client.add_relay("wss://nos.lol").await?;
        client.add_relay("wss://relay.nostr.band").await?;
        
        client.connect().await;
        
        // Create a filter to get the user's profile metadata (kind 0)
        let metadata_filter = Filter::new()
            .author(*pubkey)
            .kind(Kind::Metadata)
            .limit(1);
        
        // Try to get the profile with a timeout
        let timeout = std::time::Duration::from_secs(3);
        match tokio::time::timeout(timeout, client.get_events_of(vec![metadata_filter], None)).await {
            Ok(Ok(events)) => {
                if let Some(event) = events.first() {
                    // Parse the metadata JSON
                    if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&event.content) {
                        // Try to get 'name' field, fallback to 'display_name'
                        if let Some(name) = metadata.get("name").and_then(|n| n.as_str()) {
                            if !name.trim().is_empty() {
                                return Ok(name.trim().to_string());
                            }
                        }
                        if let Some(display_name) = metadata.get("display_name").and_then(|n| n.as_str()) {
                            if !display_name.trim().is_empty() {
                                return Ok(display_name.trim().to_string());
                            }
                        }
                    }
                }
            }
            _ => {
                // Timeout or error, will use fallback
            }
        }
        
        Err(anyhow!("No profile name found"))
    }
    
    /// Get the public key as a PublicKey
    pub fn public_key(&self) -> PublicKey {
        self.keys.public_key()
    }
    
    /// Sign an event
    pub fn sign_event(&self, event_builder: EventBuilder) -> Result<Event> {
        Ok(event_builder.to_event(&self.keys)?)
    }
}

/// Generate a random nickname in the style of bitmap project
/// Format: {adjective}{noun}{number}
fn generate_random_nickname() -> String {
    let adjectives = [
        "shadow", "cyber", "quantum", "neon", "digital", "ghost", "phantom", "void",
        "dark", "bright", "swift", "silent", "electric", "cosmic", "neural", "viral",
        "stealth", "rapid", "mystic", "plasma", "atomic", "crystal", "sonic", "lunar",
        "solar", "techno", "binary", "matrix", "nexus", "vertex", "zenith", "omega"
    ];
    
    let nouns = [
        "agent", "runner", "hacker", "coder", "node", "byte", "bit", "cipher",
        "protocol", "stream", "signal", "pulse", "wave", "core", "link", "port",
        "terminal", "console", "daemon", "thread", "process", "kernel", "shell", "root",
        "user", "admin", "ghost", "spirit", "entity", "being", "form", "shadow"
    ];
    
    let mut rng = thread_rng();
    let adjective = adjectives[rng.gen_range(0..adjectives.len())];
    let noun = nouns[rng.gen_range(0..nouns.len())];
    let number: u16 = rng.gen_range(100..9999);
    
    format!("{}{}{}", adjective, noun, number)
}