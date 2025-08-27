use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayInfo {
    pub url: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone)]
pub struct GeoRelayDirectory {
    relays: Arc<RwLock<Vec<RelayInfo>>>,
    last_fetch: Arc<RwLock<Option<Instant>>>,
    cache_path: PathBuf,
}

impl GeoRelayDirectory {
    const REMOTE_URL: &'static str = "https://raw.githubusercontent.com/permissionlesstech/georelays/refs/heads/main/nostr_relays.csv";
    const FETCH_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours
    const DEFAULT_RELAY_COUNT: usize = 5;
    
    pub fn new() -> Result<Self> {
        let cache_dir = dirs::cache_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
            .unwrap_or_else(|| PathBuf::from(".cache"));
        
        let cache_path = cache_dir.join("bitchatx").join("nostr_relays.csv");
        
        Ok(Self {
            relays: Arc::new(RwLock::new(Vec::new())),
            last_fetch: Arc::new(RwLock::new(None)),
            cache_path,
        })
    }
    
    /// Initialize the directory with cached or fallback relays
    pub async fn initialize(&self) -> Result<()> {
        // Try to load from cache first
        if let Ok(cached_relays) = self.load_from_cache().await {
            *self.relays.write().await = cached_relays;
        } else {
            // Use fallback relays if no cache
            *self.relays.write().await = self.fallback_relays();
        }
        
        // Start background fetch
        self.fetch_and_update().await?;
        
        Ok(())
    }
    
    /// Get the closest relays for a given geohash
    pub async fn closest_relays_for_geohash(&self, geohash: &str, count: Option<usize>) -> Vec<String> {
        let count = count.unwrap_or(Self::DEFAULT_RELAY_COUNT);
        
        // Decode geohash to get center coordinates
        let (lat, lon) = match geohash::decode(geohash) {
            Ok((coords, _, _)) => (coords.y, coords.x),
            Err(_) => {
                // If geohash decode fails, return fallback relays
                return self.fallback_relays().into_iter().map(|r| r.url).take(count).collect();
            }
        };
        
        self.closest_relays_to_coords(lat, lon, count).await
    }
    
    /// Get the closest relays to specific coordinates
    pub async fn closest_relays_to_coords(&self, lat: f64, lon: f64, count: usize) -> Vec<String> {
        let relays = self.relays.read().await;
        
        if relays.is_empty() {
            return self.fallback_relays().into_iter().map(|r| r.url).take(count).collect();
        }
        
        let mut relay_distances: Vec<(f64, &RelayInfo)> = relays
            .iter()
            .map(|relay| {
                let distance = haversine_distance(lat, lon, relay.latitude, relay.longitude);
                (distance, relay)
            })
            .collect();
        
        // Sort by distance and take the closest ones
        relay_distances.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        
        relay_distances
            .into_iter()
            .take(count)
            .map(|(_, relay)| format!("wss://{}", relay.url))
            .collect()
    }
    
    /// Check if we need to fetch new relay data
    pub async fn should_fetch(&self) -> bool {
        let last_fetch = self.last_fetch.read().await;
        match *last_fetch {
            Some(instant) => instant.elapsed() >= Self::FETCH_INTERVAL,
            None => true,
        }
    }
    
    /// Fetch and update relay data from remote source
    pub async fn fetch_and_update(&self) -> Result<()> {
        if !self.should_fetch().await {
            return Ok(());
        }
        
        match self.fetch_from_remote().await {
            Ok(new_relays) => {
                *self.relays.write().await = new_relays.clone();
                *self.last_fetch.write().await = Some(Instant::now());
                
                // Save to cache
                if let Err(e) = self.save_to_cache(&new_relays).await {
                    eprintln!("Warning: Failed to save relay cache: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to fetch georelays: {}", e);
                // Continue using existing relays or fallback
            }
        }
        
        Ok(())
    }
    
    /// Fetch relay data from remote CSV
    async fn fetch_from_remote(&self) -> Result<Vec<RelayInfo>> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;
            
        let response = client.get(Self::REMOTE_URL).send().await?;
        let csv_content = response.text().await?;
        
        self.parse_csv(&csv_content)
    }
    
    /// Parse CSV content into RelayInfo structs
    fn parse_csv(&self, content: &str) -> Result<Vec<RelayInfo>> {
        let mut relays = Vec::new();
        let mut reader = csv::Reader::from_reader(content.as_bytes());
        
        for result in reader.records() {
            let record = result?;
            if record.len() >= 3 {
                if let (Ok(lat), Ok(lon)) = (record[1].parse::<f64>(), record[2].parse::<f64>()) {
                    relays.push(RelayInfo {
                        url: record[0].to_string(),
                        latitude: lat,
                        longitude: lon,
                    });
                }
            }
        }
        
        Ok(relays)
    }
    
    /// Load relay data from local cache
    async fn load_from_cache(&self) -> Result<Vec<RelayInfo>> {
        let content = fs::read_to_string(&self.cache_path).await?;
        self.parse_csv(&content)
    }
    
    /// Save relay data to local cache
    async fn save_to_cache(&self, relays: &[RelayInfo]) -> Result<()> {
        // Ensure cache directory exists
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        let mut csv_content = String::from("Relay URL,Latitude,Longitude\n");
        for relay in relays {
            csv_content.push_str(&format!("{},{},{}\n", relay.url, relay.latitude, relay.longitude));
        }
        
        fs::write(&self.cache_path, csv_content).await?;
        Ok(())
    }
    
    /// Get fallback relays when georelays are unavailable
    fn fallback_relays(&self) -> Vec<RelayInfo> {
        vec![
            RelayInfo {
                url: "relay.damus.io".to_string(),
                latitude: 37.7621,
                longitude: -122.3971,
            },
            RelayInfo {
                url: "nos.lol".to_string(),
                latitude: 40.7128,
                longitude: -74.0060,
            },
            RelayInfo {
                url: "relay.nostr.band".to_string(),
                latitude: 51.5074,
                longitude: -0.1278,
            },
            RelayInfo {
                url: "nostr-pub.wellorder.net".to_string(),
                latitude: 45.5229,
                longitude: -122.9898,
            },
        ]
    }
    
}

/// Calculate haversine distance between two points in kilometers
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;
    
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    
    let a = (dlat / 2.0).sin().powi(2) +
        lat1.to_radians().cos() * lat2.to_radians().cos() *
        (dlon / 2.0).sin().powi(2);
    
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    
    EARTH_RADIUS_KM * c
}