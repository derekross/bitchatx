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
    
    /// Create identity from nsec private key
    pub fn from_nsec(nsec: &str) -> Result<Self> {
        let secret_key = SecretKey::from_bech32(nsec)
            .map_err(|e| anyhow!("Invalid nsec format: {}", e))?;
        let keys = Keys::new(secret_key);
        let pubkey = keys.public_key().to_hex();
        
        // For authenticated users, try to fetch their profile name
        // For now, use a default format
        let nickname = format!("user{}", &pubkey[..8]);
        
        Ok(Self {
            keys,
            pubkey,
            nickname,
            is_ephemeral: false,
        })
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