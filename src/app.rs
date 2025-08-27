use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use tokio::sync::mpsc;
use rand::Rng;
use std::collections::{HashSet, HashMap};
use std::time::{Duration, Instant};
use arboard::Clipboard;

use crate::channels::{ChannelManager, Message, Channel};
use crate::nostr::{NostrClient, Identity};
use nostr::{PublicKey, ToBech32};

#[derive(Debug)]
pub struct SpamFilter {
    // Track message frequency per user (pubkey -> (message_count, first_message_time))
    user_message_frequency: HashMap<String, (u32, Instant)>,
    
    // Recently auto-muted users (pubkey -> mute_time)
    auto_muted_users: HashMap<String, Instant>,
    
    // Spam detection thresholds
    max_messages_per_minute: u32,
    duplicate_message_threshold: u32,
    max_future_time_seconds: u64, // Maximum time into the future allowed
    max_past_time_hours: u64, // Maximum time into the past allowed (hours)
    
    // Track recent messages for duplicate detection (content_hash -> (count, pubkey))
    recent_message_hashes: HashMap<u64, (u32, String)>,
    
    // Common spam patterns (regex would be better but keeping it simple)
    spam_keywords: Vec<String>,
}

impl SpamFilter {
    pub fn new() -> Self {
        Self {
            user_message_frequency: HashMap::new(),
            auto_muted_users: HashMap::new(),
            max_messages_per_minute: 15, // Allow up to 15 messages per minute
            duplicate_message_threshold: 3, // Mute after 3 identical messages
            max_future_time_seconds: 300, // Allow up to 5 minutes into the future
            max_past_time_hours: 24, // Allow up to 24 hours into the past
            recent_message_hashes: HashMap::new(),
            spam_keywords: vec![
                "🚀🚀🚀".to_string(),
                "CLICK HERE".to_string(),
                "FREE MONEY".to_string(),
                "telegram.me".to_string(),
                "bit.ly".to_string(),
                "JOIN NOW".to_string(),
                "LIMITED TIME".to_string(),
                "EARN $$$".to_string(),
                "CRYPTO PUMP".to_string(),
                "🎰🎰🎰".to_string(),
            ],
        }
    }
    
    pub fn is_spam(&mut self, message: &Message) -> bool {
        let pubkey = match &message.pubkey {
            Some(pk) => pk,
            None => return false, // Don't filter messages without pubkey
        };
        
        let now = Instant::now();
        let current_time = chrono::Utc::now();
        
        // Check for future-dated messages (timestamp manipulation)
        if message.timestamp > current_time + chrono::Duration::seconds(self.max_future_time_seconds as i64) {
            if self.auto_mute_user(pubkey.clone(), "future timestamp") {
                // Newly muted for future timestamp spam
            }
            return true;
        }
        
        // Check for messages that are too far in the past (can be spam technique)
        if message.timestamp < current_time - chrono::Duration::hours(self.max_past_time_hours as i64) {
            if self.auto_mute_user(pubkey.clone(), "old timestamp") {
                // Newly muted for old timestamp spam
            }
            return true;
        }
        
        // Check if user is currently auto-muted
        if let Some(mute_time) = self.auto_muted_users.get(pubkey) {
            if now.duration_since(*mute_time) < Duration::from_secs(600) {
                return true; // Still muted
            } else {
                // Mute expired, remove from auto-muted list
                self.auto_muted_users.remove(pubkey);
            }
        }
        
        // Check for spam keywords
        let content_lower = message.content.to_lowercase();
        for keyword in &self.spam_keywords {
            if content_lower.contains(&keyword.to_lowercase()) {
                if self.auto_mute_user(pubkey.clone(), "spam keywords") {
                    // Return the pubkey for notification (will be handled by caller)
                }
                return true;
            }
        }
        
        // Check message frequency
        if self.check_message_frequency(pubkey) {
            if self.auto_mute_user(pubkey.clone(), "high message frequency") {
                // Return the pubkey for notification (will be handled by caller)
            }
            return true;
        }
        
        // Check for duplicate messages
        if self.check_duplicate_message(message, pubkey) {
            if self.auto_mute_user(pubkey.clone(), "duplicate messages") {
                // Return the pubkey for notification (will be handled by caller)
            }
            return true;
        }
        
        // Check for all caps spam (more than 20 characters and 80% uppercase)
        if message.content.len() > 20 {
            let uppercase_count = message.content.chars().filter(|c| c.is_uppercase()).count();
            let letter_count = message.content.chars().filter(|c| c.is_alphabetic()).count();
            if letter_count > 0 && (uppercase_count as f64 / letter_count as f64) > 0.8 {
                if self.auto_mute_user(pubkey.clone(), "excessive caps") {
                    // Return the pubkey for notification (will be handled by caller)
                }
                return true;
            }
        }
        
        false
    }
    
    fn check_message_frequency(&mut self, pubkey: &str) -> bool {
        let now = Instant::now();
        
        if let Some((count, first_time)) = self.user_message_frequency.get_mut(pubkey) {
            if now.duration_since(*first_time) < Duration::from_secs(60) {
                *count += 1;
                if *count > self.max_messages_per_minute {
                    return true; // Spam detected
                }
            } else {
                // Reset counter for new minute
                *count = 1;
                *first_time = now;
            }
        } else {
            // First message from this user
            self.user_message_frequency.insert(pubkey.to_string(), (1, now));
        }
        
        false
    }
    
    fn check_duplicate_message(&mut self, message: &Message, pubkey: &str) -> bool {
        // Simple hash of message content
        let content_hash = self.simple_hash(&message.content);
        
        if let Some((count, existing_pubkey)) = self.recent_message_hashes.get_mut(&content_hash) {
            if existing_pubkey == pubkey {
                *count += 1;
                if *count >= self.duplicate_message_threshold {
                    return true; // Duplicate spam detected
                }
            }
        } else {
            self.recent_message_hashes.insert(content_hash, (1, pubkey.to_string()));
        }
        
        false
    }
    
    fn simple_hash(&self, content: &str) -> u64 {
        // Simple hash function for duplicate detection
        let mut hash = 0u64;
        for byte in content.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash
    }
    
    fn auto_mute_user(&mut self, pubkey: String, _reason: &str) -> bool {
        if self.auto_muted_users.contains_key(&pubkey) {
            return false; // Already muted, don't send notification again
        }
        
        let now = Instant::now();
        self.auto_muted_users.insert(pubkey.clone(), now);
        
        // Clean up old frequency data
        self.user_message_frequency.remove(&pubkey);
        
        true // Newly muted
    }
    
    pub fn is_user_auto_muted(&self, pubkey: &str) -> bool {
        if let Some(mute_time) = self.auto_muted_users.get(pubkey) {
            Instant::now().duration_since(*mute_time) < Duration::from_secs(600)
        } else {
            false
        }
    }
    
    pub fn manually_unmute_user(&mut self, pubkey: &str) {
        self.auto_muted_users.remove(pubkey);
    }
    
    pub fn get_auto_muted_users(&self) -> Vec<(String, Duration)> {
        let now = Instant::now();
        self.auto_muted_users
            .iter()
            .filter_map(|(pubkey, mute_time)| {
                let elapsed = now.duration_since(*mute_time);
                if elapsed < Duration::from_secs(600) {
                    Some((pubkey.clone(), Duration::from_secs(600) - elapsed))
                } else {
                    None
                }
            })
            .collect()
    }
    
    pub fn cleanup_old_data(&mut self) {
        let now = Instant::now();
        
        // Clean up old frequency tracking (older than 2 minutes)
        self.user_message_frequency.retain(|_, (_, time)| {
            now.duration_since(*time) < Duration::from_secs(120)
        });
        
        // Clean up old message hashes (older than 5 minutes)
        self.recent_message_hashes.clear(); // Simple cleanup for now
        
        // Clean up expired auto-mutes
        self.auto_muted_users.retain(|_, mute_time| {
            now.duration_since(*mute_time) < Duration::from_secs(600)
        });
    }
    
    pub fn is_enabled(&self) -> bool {
        // Spam filter is always enabled in this implementation
        true
    }
    
    pub fn get_auto_muted_count(&self) -> usize {
        self.auto_muted_users.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Connecting,
    Connected,
    #[allow(dead_code)]
    Disconnected,
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

pub struct App {
    pub should_quit: bool,
    pub state: AppState,
    pub input_mode: InputMode,
    pub input: String,
    pub cursor_position: usize,
    pub scroll_offset: usize,
    pub input_horizontal_scroll: usize,
    pub should_autoscroll: bool,
    
    // Nostr client
    pub nostr_client: NostrClient,
    pub identity: Identity,
    
    // Channel management
    pub channel_manager: ChannelManager,
    pub current_channel: Option<String>,
    pub system_channel: String,
    
    // Message receivers
    message_rx: mpsc::UnboundedReceiver<Message>,
    status_rx: mpsc::UnboundedReceiver<String>,
    
    // Tab completion state
    pub tab_completion_state: Option<TabCompletionState>,
    
    // Blocking functionality - using pubkey hex strings like Android geohash blocking
    blocked_users: HashSet<String>,
    
    // Private messaging support
    pub private_chats: HashMap<String, String>, // pubkey -> nickname mapping
    
    // Spam filtering
    spam_filter: SpamFilter,
    
    // Clickable regions for nostr URIs
    pub clickable_regions: Vec<ClickableRegion>,
    
    // Track actual viewport height for proper scrolling
    pub viewport_height: usize,
    
    // Track actual input width for proper horizontal scrolling
    pub input_width: usize,
    
    // Flag to prevent UI from overriding autoscroll after new messages
    pub just_processed_messages: bool,
}

#[derive(Debug, Clone)]
pub struct ClickableRegion {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub nostr_uri: String,
}

#[derive(Debug, Clone)]
pub struct TabCompletionState {
    #[allow(dead_code)]
    original_input: String,
    #[allow(dead_code)]
    original_cursor: usize,
    #[allow(dead_code)]
    prefix: String,
    pub matches: Vec<String>,
    pub current_match_index: usize,
}

impl App {
    pub async fn new(nsec: Option<&str>, auto_channel: Option<&str>) -> Result<Self> {
        let identity = if let Some(nsec_str) = nsec {
            match Identity::from_nsec(nsec_str).await {
            Ok(identity) => identity,
            Err(e) => return Err(e),
        }
        } else {
            Identity::ephemeral()
        };
        
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let (status_tx, status_rx) = mpsc::unbounded_channel();
        
        let nostr_client = NostrClient::new(&identity, message_tx.clone(), status_tx.clone()).await?;
        let channel_manager = ChannelManager::new(message_tx);
        
        let mut app = Self {
            should_quit: false,
            state: AppState::Connecting,
            input_mode: InputMode::Normal,
            input: String::new(),
            cursor_position: 0,
            scroll_offset: 0,
            input_horizontal_scroll: 0,
            should_autoscroll: true,
            
            nostr_client,
            identity,
            
            channel_manager,
            current_channel: Some("system".to_string()),
            system_channel: "system".to_string(),
            
            message_rx,
            status_rx,
            tab_completion_state: None,
            blocked_users: HashSet::new(),
            private_chats: HashMap::new(),
            spam_filter: SpamFilter::new(),
            clickable_regions: Vec::new(),
            viewport_height: 25, // Default fallback, will be updated by UI
            input_width: 80, // Default fallback, will be updated by UI
            just_processed_messages: false,
        };
        
        // Add welcome message to system channel
        let version = env!("CARGO_PKG_VERSION");
        app.add_status_message(format!("Welcome to BitchatX v{}!", version));
        app.add_status_message(format!("Connected as {} ({})",
            app.identity.nickname,
            if nsec.is_some() { "authenticated" } else { "ephemeral" }
        ));
        app.add_status_message("Type /help for available commands".to_string());
        app.add_status_message("To receive messages, join a geohash channel: /join <geohash>".to_string());
        
        // Auto-join channel if specified
        if let Some(channel) = auto_channel {
            app.join_channel(channel).await?;
        }
        
        // Start Nostr client
        match app.nostr_client.connect().await {
            Ok(()) => {
                app.state = AppState::Connected;
            }
            Err(e) => {
                app.state = AppState::Error(format!("Connection failed: {}", e));
                app.add_status_message(format!("Connection error: {}", e));
            }
        }
        
        Ok(app)
    }
    
    pub async fn handle_input(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) => self.handle_key_event(key).await?,
            Event::Mouse(mouse) => self.handle_mouse_event(mouse).await?,
            _ => {}
        }
        Ok(())
    }
    
    async fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        // Handle modifier key combinations
        if !key.modifiers.is_empty() {
            match key.modifiers {
                KeyModifiers::SHIFT => {
                    // Allow Shift + Tab (BackTab) and Shift + letter keys
                    if key.code != KeyCode::BackTab && !matches!(key.code, KeyCode::Char(_)) {
                        return Ok(());
                    }
                }
                KeyModifiers::CONTROL => {
                    // Handle Ctrl key combinations for clipboard operations
                    match key.code {
                        KeyCode::Char('c') => {
                            if self.input_mode == InputMode::Editing {
                                self.copy_to_clipboard();
                            }
                            return Ok(());
                        }
                        KeyCode::Char('v') => {
                            if self.input_mode == InputMode::Editing {
                                self.paste_from_clipboard();
                            }
                            return Ok(());
                        }
                        KeyCode::Char('x') => {
                            if self.input_mode == InputMode::Editing {
                                self.cut_to_clipboard();
                            }
                            return Ok(());
                        }
                        KeyCode::Char('a') => {
                            if self.input_mode == InputMode::Editing {
                                self.select_all();
                            }
                            return Ok(());
                        }
                        _ => {
                            // Ignore other Ctrl combinations
                            return Ok(());
                        }
                    }
                }
                _ => {
                    // Ignore all other modifier combinations (Alt, etc.)
                    return Ok(());
                }
            }
        }
        match self.input_mode {
            InputMode::Normal => {
                match key.code {
                    KeyCode::Char('q') => {
                        self.should_quit = true;
                    }
                    KeyCode::Char('i') => {
                        self.input_mode = InputMode::Editing;
                    }
                    KeyCode::Tab => {
                        self.switch_to_next_channel();
                    }
                    KeyCode::Up => {
                        if self.scroll_offset > 0 {
                            self.scroll_offset -= 1;
                        }
                        // Check autoscroll status after scrolling
                        self.update_autoscroll_status();
                    }
                    KeyCode::Down => {
                        self.scroll_offset += 1;
                        // Check if user scrolled to bottom
                        self.update_autoscroll_status();
                    }
                    KeyCode::PageUp => {
                        self.scroll_offset = self.scroll_offset.saturating_sub(10);
                        // Check autoscroll status after scrolling
                        self.update_autoscroll_status();
                    }
                    KeyCode::PageDown => {
                        self.scroll_offset += 10;
                        // Check if user scrolled to bottom
                        self.update_autoscroll_status();
                    }
                    _ => {}
                }
            }
            InputMode::Editing => {
                match key.code {
                    KeyCode::Enter => {
                        self.submit_input().await?;
                        self.input.clear();
                        self.cursor_position = 0;
                        self.input_horizontal_scroll = 0;
                        // Stay in input mode after sending message
                    }
                    KeyCode::Esc => {
                        self.input.clear();
                        self.cursor_position = 0;
                        self.input_horizontal_scroll = 0;
                        self.input_mode = InputMode::Normal;
                    }
                    KeyCode::Char(c) => {
                        // Reset tab completion on any character input
                        self.tab_completion_state = None;
                        self.input.insert(self.cursor_position, c);
                        self.cursor_position += 1;
                        
                        // Update horizontal scroll to keep cursor visible
                        self.update_input_scroll();
                    }
                    KeyCode::Tab => {
                        self.handle_tab_completion().await;
                    }
                    KeyCode::Backspace => {
                        self.tab_completion_state = None;
                        if self.cursor_position > 0 {
                            self.input.remove(self.cursor_position - 1);
                            self.cursor_position -= 1;
                            
                            // Update horizontal scroll when deleting
                            self.update_input_scroll();
                        }
                    }
                    KeyCode::Delete => {
                        self.tab_completion_state = None;
                        if self.cursor_position < self.input.len() {
                            self.input.remove(self.cursor_position);
                            // Update horizontal scroll when deleting
                            self.update_input_scroll();
                        }
                    }
                    KeyCode::Left => {
                        self.tab_completion_state = None;
                        if self.cursor_position > 0 {
                            self.cursor_position -= 1;
                            self.update_input_scroll();
                        }
                    }
                    KeyCode::Right => {
                        self.tab_completion_state = None;
                        if self.cursor_position < self.input.len() {
                            self.cursor_position += 1;
                            self.update_input_scroll();
                        }
                    }
                    KeyCode::Up => {
                        // Allow scrolling up in edit mode
                        if self.scroll_offset > 0 {
                            self.scroll_offset -= 1;
                        }
                    }
                    KeyCode::Down => {
                        // Allow scrolling down in edit mode
                        self.scroll_offset += 1;
                    }
                    KeyCode::PageUp => {
                        // Allow page up in edit mode
                        self.scroll_offset = self.scroll_offset.saturating_sub(10);
                    }
                    KeyCode::PageDown => {
                        // Allow page down in edit mode
                        self.scroll_offset += 10;
                    }
                    KeyCode::Home => {
                        self.cursor_position = 0;
                        self.update_input_scroll();
                    }
                    KeyCode::End => {
                        self.cursor_position = self.input.len();
                        self.update_input_scroll();
                    }
                    // Explicitly ignore other keys to prevent exiting edit mode
                    KeyCode::F(_) => {}  // Function keys
                    KeyCode::BackTab => {}  // Shift+Tab
                    KeyCode::Insert => {}  // Insert key
                    KeyCode::Null => {}  // Null key
                    // Control keys - explicitly handle to prevent unexpected behavior
                    KeyCode::CapsLock => {}
                    KeyCode::ScrollLock => {}
                    KeyCode::NumLock => {}
                    KeyCode::PrintScreen => {}
                    KeyCode::Pause => {}
                    KeyCode::Menu => {}
                    KeyCode::KeypadBegin => {}
                    KeyCode::Media(_) => {}
                    KeyCode::Modifier(_) => {}
                }
            }
        }
        Ok(())
    }
    
    async fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<()> {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                // Scroll up (towards older messages)
                if self.scroll_offset > 0 {
                    self.scroll_offset = self.scroll_offset.saturating_sub(3); // Scroll 3 lines at a time
                }
                self.update_autoscroll_status();
            }
            MouseEventKind::ScrollDown => {
                // Scroll down (towards newer messages)
                self.scroll_offset += 3; // Scroll 3 lines at a time
                self.update_autoscroll_status();
            }
            MouseEventKind::Down(button) => {
                // Handle mouse clicks
                if matches!(button, crossterm::event::MouseButton::Left) {
                    self.handle_mouse_click(mouse.column, mouse.row).await;
                }
            }
            _ => {
                // Ignore other mouse events (moves, etc.)
            }
        }
        Ok(())
    }
    
    async fn submit_input(&mut self) -> Result<()> {
        let input = self.input.trim().to_string();
        if input.is_empty() {
            return Ok(());
        }
        
        if input.starts_with('/') {
            self.handle_command(&input).await?;
        } else if let Some(channel) = self.current_channel.clone() {
            self.send_message(&channel, &input).await?;
        } else {
            self.add_status_message("No channel selected. Use /join <geohash> to join a channel.".to_string());
        }
        
        // Enable auto-scrolling after sending a message
        self.should_autoscroll = true;
        
        Ok(())
    }
    
    async fn handle_command(&mut self, input: &str) -> Result<()> {
        let parts = self.parse_command_args(&input[1..]);
        if parts.is_empty() {
            return Ok(());
        }
        
        match parts[0].to_lowercase().as_str() {
            "join" | "j" => {
                if parts.len() != 2 {
                    self.add_status_message("Usage: /join <geohash>".to_string());
                    return Ok(());
                }
                self.join_channel(&parts[1]).await?;
            }
            "leave" | "part" | "l" => {
                if let Some(channel) = &self.current_channel.clone() {
                    if channel == "system" {
                        self.add_status_message("Cannot leave system channel".to_string());
                    } else {
                        self.leave_channel(&channel).await?;
                    }
                } else {
                    self.add_status_message("No channel to leave".to_string());
                }
            }
            "nick" | "n" => {
                if parts.len() != 2 {
                    self.add_status_message("Usage: /nick <nickname>".to_string());
                    return Ok(());
                }
                self.change_nickname(&parts[1]).await?;
            }
            "msg" | "m" => {
                if parts.len() < 3 {
                    self.add_status_message("Usage: /msg <channel/nickname> <message>".to_string());
                    return Ok(());
                }
                let target = &parts[1];
                let message_content = parts[2..].join(" ");
                self.send_msg_to_target(target, &message_content).await?;
            }
            "list" | "channels" => {
                self.list_channels();
            }
            "all" => {
                self.show_all_recent_messages().await;
            }
            "hug" => {
                if parts.len() != 2 {
                    self.add_status_message("Usage: /hug <nickname>".to_string());
                    return Ok(());
                }
                let nickname = &parts[1];
                let hug_message = format!("* {} hugs {} 🫂 *", self.identity.nickname, nickname);
                self.send_action_message(&hug_message).await?;
            }
            "slap" => {
                if parts.len() != 2 {
                    self.add_status_message("Usage: /slap <nickname>".to_string());
                    return Ok(());
                }
                let nickname = &parts[1];
                let slap_message = format!("* {} slaps {} around a bit with a large trout 🐟 *", self.identity.nickname, nickname);
                self.send_action_message(&slap_message).await?;
            }
            "block" => {
                if parts.len() > 1 {
                    let nickname = parts[1].trim_start_matches('@');
                    self.block_user(nickname).await;
                } else {
                    self.list_blocked_users();
                }
            }
            "unblock" => {
                if parts.len() > 1 {
                    let nickname = parts[1].trim_start_matches('@');
                    self.unblock_user(nickname).await;
                } else {
                    self.add_status_message("Usage: /unblock <nickname>".to_string());
                }
            }
            "whois" | "w" => {
                if parts.len() > 1 {
                    let user_input = parts[1].trim_start_matches('@');
                    self.whois_user(user_input).await;
                } else {
                    self.add_status_message("Usage: /whois <nickname> or /whois <nickname#pubkey>".to_string());
                }
            }
            "version" => {
                self.show_version().await?;
            }
            "status" => {
                self.show_status().await;
            }
            "spam" => {
                if parts.len() < 2 {
                    self.add_message_to_current_channel("Usage: /spam <list|unmute|status>".to_string());
                } else {
                    match parts[1].as_str() {
                        "list" => {
                            self.list_auto_muted_users();
                        }
                        "unmute" => {
                            if parts.len() < 3 {
                                self.add_message_to_current_channel("Usage: /spam unmute <nickname>".to_string());
                            } else {
                                let nickname = parts[2].trim_start_matches('@');
                                self.unmute_spammer(nickname).await;
                            }
                        }
                        "status" => {
                            self.show_spam_filter_status();
                        }
                        _ => {
                            self.add_message_to_current_channel("Unknown spam command. Use: list, unmute, or status".to_string());
                        }
                    }
                }
            }
            "clear" => {
                self.clear_current_channel();
            }
            "help" | "h" | "commands" => {
                self.add_status_message("Help command received!".to_string());
                self.show_help().await;
            }
            "quit" | "q" | "exit" => {
                self.should_quit = true;
            }
            _ => {
                self.add_status_message(format!("Unknown command: {}. Type /help for available commands.", parts[0]));
            }
        }
        
        Ok(())
    }
    
    async fn join_channel(&mut self, geohash: &str) -> Result<()> {
        // Validate geohash format
        if !self.is_valid_geohash(geohash) {
            self.add_status_message(format!("Invalid geohash format: {}", geohash));
            return Ok(());
        }
        
        self.current_channel = Some(geohash.to_string());
        self.channel_manager.join_channel(geohash).await?;
        self.nostr_client.subscribe_to_channel(geohash).await?;
        
        self.add_status_message(format!("Joined channel #{}", geohash));
        
        // Force scroll to bottom when joining a channel
        self.force_scroll_to_bottom();
        
        Ok(())
    }
    
    async fn leave_channel(&mut self, geohash: &str) -> Result<()> {
        // Prevent leaving system channel
        if geohash == "system" {
            self.add_status_message("Cannot leave system channel".to_string());
            return Ok(());
        }
        
        self.channel_manager.leave_channel(geohash).await?;
        self.nostr_client.unsubscribe_from_channel(geohash).await?;
        
        if self.current_channel.as_deref() == Some(geohash) {
            self.current_channel = Some(self.system_channel.clone());
        }
        
        self.add_status_message(format!("Left channel #{}", geohash));
        Ok(())
    }
    
    async fn send_message(&mut self, channel: &str, content: &str) -> Result<()> {
        // Add local echo immediately for instant feedback
        let message = Message {
            channel: channel.to_string(),
            nickname: self.identity.nickname.clone(),
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
            pubkey: Some(self.identity.pubkey.clone()),
            is_own: true,
            is_private: false,
            recipient_pubkey: None,
        };
        
        // Use sync version for immediate display
        let _ = self.channel_manager.add_message_sync(message);
        
        // Send to network after UI is updated
        // Send to network in background without blocking UI
        // Network send variables removed to fix borrow checker
        
        // Create a future for the network send
        // Network send will happen after scroll_to_bottom()
        
        // Send to network after UI operations are complete
        let _ = self.nostr_client.send_message(channel, content, &self.identity.nickname).await;
        
        // Enable auto-scrolling before network operations
        self.should_autoscroll = true;
        self.scroll_to_bottom();
        
        Ok(())
    }
    
    async fn send_msg_to_target(&mut self, target: &str, content: &str) -> Result<()> {
        // First check if target is a joined channel
        let joined_channels = self.channel_manager.list_channels();
        if joined_channels.contains(&target.to_string()) {
            // Send to channel
            self.send_message(target, content).await?;
            return Ok(());
        }
        
        // Check if target looks like a geohash pattern (valid channel but not joined)
        if self.is_valid_geohash(target) {
            // Send to channel even if not joined
            self.send_message(target, content).await?;
            return Ok(());
        }
        
        // Otherwise, treat as private message to user
        self.send_private_message(target, content).await?;
        Ok(())
    }
    
    async fn send_private_message(&mut self, nickname: &str, content: &str) -> Result<()> {
        // Find the pubkey for this nickname
        let recipient_pubkey = self.find_pubkey_for_nickname(nickname).await;
        
        if let Some(pubkey) = recipient_pubkey {
            // Create a private message channel name based on the pubkey
            let dm_channel = format!("dm:{}", &pubkey);
            
            // Add to private chats if not already there
            self.private_chats.insert(pubkey.clone(), nickname.to_string());
            
            // Create the private message
            let message = Message {
                channel: dm_channel.clone(),
                nickname: self.identity.nickname.clone(),
                content: content.to_string(),
                timestamp: chrono::Utc::now(),
                pubkey: Some(self.identity.pubkey.clone()),
                is_own: true,
                is_private: true,
                recipient_pubkey: Some(pubkey.clone()),
            };
            
            // Add to channel manager for display
            let _ = self.channel_manager.add_message_sync(message);
            
            // TODO: Send via Nostr using NIP-17 (for now just show locally)
            self.add_status_message(format!("Private message sent to {} (local only for now)", nickname));
            
            // Enable auto-scrolling
            self.should_autoscroll = true;
            self.scroll_to_bottom();
        } else {
            self.add_status_message(format!("User '{}' not found. They must have sent a message in a channel first.", nickname));
        }
        
        Ok(())
    }
    
    async fn change_nickname(&mut self, new_nick: &str) -> Result<()> {
        let old_nick = self.identity.nickname.clone();
        self.identity.nickname = new_nick.to_string();
        self.add_status_message(format!("Nickname changed from {} to {}", old_nick, new_nick));
        Ok(())
    }
    
    fn list_channels(&mut self) {
        let channels = self.channel_manager.list_channels();
        if channels.is_empty() {
            self.add_status_message("No joined channels".to_string());
        } else {
            self.add_status_message("Joined channels:".to_string());
            for channel in channels {
                let active_users = self.channel_manager.get_active_user_count(&channel);
                let indicator = if Some(&channel) == self.current_channel.as_ref() { "*" } else { " " };
                self.add_status_message(format!("{}#{} ({} users)", indicator, channel, active_users));
            }
        }
    }
    
    async fn show_all_recent_messages(&mut self) {
        let ten_minutes_ago = chrono::Utc::now() - chrono::Duration::minutes(10);
        
        // Enable autoscroll to ensure all messages are visible
        self.should_autoscroll = true;
        
        // Get all channels (both joined and listening-only) from channel manager
        let all_channels = self.channel_manager.list_all_channels();
        
        // Collect all recent messages first to avoid borrow issues
        let mut recent_activity: Vec<(String, Vec<String>, bool)> = Vec::new();
        
        for (channel_name, is_joined) in all_channels {
            if let Some(channel) = self.channel_manager.get_channel(&channel_name) {
                let recent_messages: Vec<String> = channel.messages
                    .iter()
                    .filter(|msg| msg.timestamp >= ten_minutes_ago)
                    .map(|msg| {
                        let timestamp = msg.timestamp.with_timezone(&chrono::Local).format("%H:%M:%S");
                        let display_nickname = self.format_display_nickname(&msg.nickname, &msg.pubkey);
                        format!("[{}] <{}> {}", timestamp, display_nickname, msg.content)
                    })
                    .collect();
                
                if !recent_messages.is_empty() {
                    recent_activity.push((channel_name, recent_messages, is_joined));
                }
            }
        }
        
        // Sort by channel name for consistent display
        recent_activity.sort_by(|a, b| a.0.cmp(&b.0));
        
        // Now add all status messages
        self.add_message_to_current_channel("=== Recent Activity (Last 10 Minutes) ===".to_string());
        
        if recent_activity.is_empty() {
            self.add_message_to_current_channel("No recent activity in any geohash channel (last 10 minutes)".to_string());
        } else {
            for (channel_name, messages, is_joined) in recent_activity {
                // Channel header with joined status
                if channel_name == "system" {
                    self.add_message_to_current_channel("--- System Channel ---".to_string());
                } else {
                    let status = if is_joined { "joined" } else { "listening" };
                    self.add_message_to_current_channel(format!("--- Channel #{} ({}) ---", channel_name, status));
                }
                
                // Show recent messages
                for message in messages {
                    self.add_message_to_current_channel(message);
                }
                
                // Add separator between channels
                self.add_message_to_current_channel("".to_string());
            }
            
            self.add_message_to_current_channel("=== End of Recent Activity ===".to_string());
        }
        
        // Ensure we scroll to bottom after adding all messages
        self.scroll_to_bottom();
    }
    
    async fn show_help(&mut self) {
        // Enable autoscroll to ensure help text is visible
        self.should_autoscroll = true;
        
        let help_text = vec![
            "BitchatX Commands:".to_string(),
            "/join, /j <geohash> - Join a geohash channel".to_string(),
            "/leave, /part, /l - Leave current channel".to_string(),
            "/msg, /m <channel> <message> - Send message to specific channel".to_string(),
            "/nick, /n <nickname> - Change your display name (session only)".to_string(),
            "/list, /channels - List joined channels".to_string(),
            "/all - Show recent activity from all geohash channels with active users (last 10 minutes)".to_string(),
            "/hug <nickname> - Send a hug to someone 🫂".to_string(),
            "/slap <nickname> - Slap someone with a large trout".to_string(),
            "/block [nickname] - Block user or list blocked users".to_string(),
            "/unblock <nickname> - Unblock a user".to_string(),
            "/spam <list|unmute|status> - Manage spam filter".to_string(),
            "/whois, /w <nickname[#pubkey]> - Show user information (npub, channels)".to_string(),
            "/clear - Clear all messages from current channel".to_string(),
            "/status - Show connection status and relay information".to_string(),
            "/version - Show application version and fun quote".to_string(),
            "/help, /h, /commands - Show this help".to_string(),
            "/quit, /q, /exit - Exit BitchatX".to_string(),
            "".to_string(),
            "".to_string(),
            "Keyboard Commands:".to_string(),
            "i - Enter input mode, Esc - Exit to normal mode, q - Quit (normal mode)".to_string(),
            "Input mode: Stay in input mode after sending messages, only Esc exits".to_string(),
            "Tab - Nickname completion (input mode), Switch channels (normal mode)".to_string(),
            "Channel switching: Esc then Tab to cycle through channels".to_string(),
            "Page Up/Down - Fast scroll, Home/End - Cursor start/end".to_string(),
            "Clipboard: Ctrl+C - Copy, Ctrl+V - Paste, Ctrl+X - Cut, Ctrl+A - Select All".to_string(),
            "Mouse: Click on nostr: URI links to open in browser (via njump.me)".to_string(),
        ];
        
        for line in help_text {
            self.add_message_to_current_channel(line);
        }
        
        // Ensure we scroll to bottom after adding help text
        self.scroll_to_bottom();
    }
    
    fn is_valid_geohash(&self, geohash: &str) -> bool {
        // Basic geohash validation
        geohash.len() >= 1 && geohash.len() <= 12 && 
        geohash.chars().all(|c| "0123456789bcdefghjkmnpqrstuvwxyz".contains(c))
    }
    
    pub fn add_status_message(&mut self, message: String) {
        // Add system messages to the system channel
        let system_message = Message {
            channel: self.system_channel.clone(),
            nickname: "system".to_string(),
            content: message,
            timestamp: chrono::Local::now().into(),
            is_own: false,
            pubkey: None,
            is_private: false,
            recipient_pubkey: None,
        };
        
        // Add directly to channel manager without going through async receiver
        // This ensures immediate display
        let _ = self.channel_manager.add_message_sync(system_message);
        
        // Trigger autoscroll if we're in system channel
        if self.current_channel.as_deref() == Some(&self.system_channel) && self.should_autoscroll {
            self.scroll_to_bottom();
        }
    }
    
    pub fn add_message_to_current_channel(&mut self, message: String) {
        // Add system messages to the current channel (or system if no current channel)
        let target_channel = self.current_channel.clone().unwrap_or_else(|| self.system_channel.clone());
        let system_message = Message {
            channel: target_channel,
            nickname: "system".to_string(),
            content: message,
            timestamp: chrono::Local::now().into(),
            is_own: false,
            pubkey: None,
            is_private: false,
            recipient_pubkey: None,
        };
        
        // Add directly to channel manager without going through async receiver
        let _ = self.channel_manager.add_message_sync(system_message);
        
        // Trigger autoscroll since we added a new message
        if self.should_autoscroll {
            self.scroll_to_bottom();
        }
    }
    
    pub async fn on_tick(&mut self) -> Result<()> {
        // Process incoming messages
        let mut new_messages_count = 0;
        while let Ok(message) = self.message_rx.try_recv() {
            // Filter out messages from blocked users (like Android app's MeshDelegateHandler)
            if self.is_user_blocked(&message.pubkey) {
                continue; // Skip blocked messages entirely
            }
            
            // Filter out spam messages and notify if timestamp manipulation detected
            if self.spam_filter.is_spam(&message) {
                // Check if this was timestamp-based spam for notification
                let current_time = chrono::Utc::now();
                let is_future_spam = message.timestamp > current_time + chrono::Duration::seconds(300);
                let is_old_spam = message.timestamp < current_time - chrono::Duration::hours(24);
                
                if is_future_spam {
                    let nickname = message.nickname.clone();
                    let minutes_future = (message.timestamp - current_time).num_minutes();
                    self.add_status_message(format!("⚠️ Filtered future-dated message from {} ({}min in future)", nickname, minutes_future));
                } else if is_old_spam {
                    let nickname = message.nickname.clone();
                    let hours_old = (current_time - message.timestamp).num_hours();
                    self.add_status_message(format!("⚠️ Filtered old message from {} ({}hr old)", nickname, hours_old));
                }
                
                continue; // Skip spam messages
            }
            
            // Use sync version for faster processing (no await overhead)
            let _ = self.channel_manager.add_message_sync(message);
            new_messages_count += 1;
        }
        
        // Auto-scroll to bottom if we received new messages
        if new_messages_count > 0 {
            // For new messages, completely reset scrolling state to ensure visibility
            self.force_scroll_to_bottom();
            self.just_processed_messages = true;
        } else {
            self.just_processed_messages = false;
        }
        
        // Process status updates
        while let Ok(status) = self.status_rx.try_recv() {
            self.add_status_message(status);
        }
        
        // Periodically clean up old spam filter data
        self.spam_filter.cleanup_old_data();
        
        Ok(())
    }
    
    pub fn get_current_channel(&self) -> Option<&Channel> {
        if let Some(channel_name) = &self.current_channel {
            self.channel_manager.get_channel(channel_name)
        } else {
            None
        }
    }
    
    fn get_all_channels(&self) -> Vec<String> {
        let mut channels = Vec::new();
        
        // Always include system channel first
        channels.push(self.system_channel.clone());
        
        // Add joined channels (excluding system channel to avoid duplication)
        let joined_channels = self.channel_manager.list_channels();
        for channel in joined_channels {
            if channel != self.system_channel {
                channels.push(channel);
            }
        }
        
        channels
    }
    
    fn switch_to_next_channel(&mut self) {
        let all_channels = self.get_all_channels();
        if all_channels.len() <= 1 {
            return; // No other channels to switch to
        }
        
        if let Some(current) = &self.current_channel {
            if let Some(current_index) = all_channels.iter().position(|ch| ch == current) {
                let next_index = (current_index + 1) % all_channels.len();
                self.current_channel = Some(all_channels[next_index].clone());
                
                // Force scroll to bottom when switching channels
                self.force_scroll_to_bottom();
                
                // Add status message about channel switch
                let new_channel = &all_channels[next_index];
                if new_channel == "system" {
                    self.add_status_message("Switched to system channel".to_string());
                } else {
                    self.add_status_message(format!("Switched to channel #{}", new_channel));
                }
            }
        } else {
            // If no current channel, switch to first channel (system)
            self.current_channel = Some(all_channels[0].clone());
            self.force_scroll_to_bottom();
            self.add_status_message("Switched to system channel".to_string());
        }
    }
    
    pub fn get_visible_messages(&self, height: usize) -> (Vec<(String, String, String, bool, Option<String>)>, usize) {
        if let Some(channel) = self.get_current_channel() {
            let message_count = channel.messages.len();
            
            if message_count == 0 {
                return (vec![], 0);
            }
            
            // Calculate how many messages to show based on scroll position and viewport
            let viewport_height = height;
            let total_messages = channel.messages.len();
            
            // Calculate effective scroll offset based on autoscroll setting and bounds checking  
            let effective_scroll_offset = if self.should_autoscroll {
                // Auto-scroll: show the most recent messages at the bottom
                // Ensure we don't scroll past the last message
                if total_messages > viewport_height {
                    total_messages.saturating_sub(viewport_height)
                } else {
                    0
                }
            } else {
                // Manual scroll: use current scroll_offset but ensure it's valid
                if self.scroll_offset >= total_messages {
                    // If scroll_offset is beyond available messages, fix it
                    if total_messages > viewport_height {
                        total_messages.saturating_sub(viewport_height)
                    } else {
                        0
                    }
                } else {
                    // Ensure scroll_offset doesn't go beyond bounds
                    let max_offset = if total_messages > viewport_height {
                        total_messages - viewport_height
                    } else {
                        0
                    };
                    self.scroll_offset.min(max_offset)
                }
            };
            
            // Calculate range of messages to display
            let start_index = effective_scroll_offset;
            let end_index = (start_index + viewport_height).min(total_messages);
            
            // Convert messages to owned data and return with the effective offset
            let message_data: Vec<_> = channel.messages[start_index..end_index]
                .iter()
                .map(|msg| (
                    msg.timestamp.with_timezone(&chrono::Local).format("%H:%M:%S").to_string(),
                    msg.nickname.clone(),
                    msg.content.clone(),
                    msg.is_own,
                    msg.pubkey.clone()
                ))
                .collect();
            (message_data, effective_scroll_offset)
        } else {
            (vec![], 0)
        }
    }
    
    async fn handle_tab_completion(&mut self) {
        // Only work in channels
        let current_channel = match &self.current_channel {
            Some(channel) => channel.clone(),
            None => return,
        };
        
        if let Some(mut state) = self.tab_completion_state.take() {
            // Continue existing tab completion - cycle to next match
            if !state.matches.is_empty() {
                state.current_match_index = (state.current_match_index + 1) % state.matches.len();
                self.apply_tab_completion(&state);
                self.tab_completion_state = Some(state);
            }
        } else {
            // Start new tab completion
            let word_info = self.find_current_word();
            if let Some((word, start_pos, _end_pos)) = word_info {
                if word.len() >= 2 { // Minimum 2 characters to start completion
                    let matches = if self.is_action_command_context(start_pos) && 
                                     (self.input.trim_start().starts_with("/msg ") || self.input.trim_start().starts_with("/m ")) {
                        // For /msg command, complete both channels and nicknames
                        self.get_msg_completion_matches(&word)
                    } else if let Some(channel) = self.channel_manager.get_channel(&current_channel) {
                        // Regular nickname completion for current channel
                        channel.find_matching_nicknames(&word)
                    } else {
                        vec![]
                    };
                    
                    if !matches.is_empty() {
                        let state = TabCompletionState {
                            original_input: self.input.clone(),
                            original_cursor: self.cursor_position,
                            prefix: word,
                            matches,
                            current_match_index: 0,
                        };
                        self.apply_tab_completion(&state);
                        self.tab_completion_state = Some(state);
                    }
                }
            }
        }
    }
    
    fn find_current_word(&self) -> Option<(String, usize, usize)> {
        if self.input.is_empty() || self.cursor_position == 0 {
            return None;
        }
        
        let chars: Vec<char> = self.input.chars().collect();
        let cursor = self.cursor_position.min(chars.len());
        
        // Find word boundaries
        let mut start = cursor;
        let mut end = cursor;
        
        // Look backward for start of word
        while start > 0 {
            let ch = chars[start - 1];
            if ch.is_whitespace() || ch == ':' || ch == ',' {
                break;
            }
            start -= 1;
        }
        
        // Look forward for end of word (if cursor is in middle of word)
        while end < chars.len() {
            let ch = chars[end];
            if ch.is_whitespace() || ch == ':' || ch == ',' {
                break;
            }
            end += 1;
        }
        
        if start == end {
            return None;
        }
        
        let word: String = chars[start..end].iter().collect();
        Some((word, start, end))
    }
    
    fn apply_tab_completion(&mut self, state: &TabCompletionState) {
        if let Some((_, start_pos, end_pos)) = self.find_current_word() {
            let replacement = &state.matches[state.current_match_index];
            
            // Check if we're in a slash command context or action command context
            let is_slash_command_context = self.is_slash_command_context(start_pos);
            let is_action_command = self.is_action_command_context(start_pos);
            let is_msg_command = self.is_msg_command_context();
            
            // Replace the current word with the completion
            let mut chars: Vec<char> = self.input.chars().collect();
            
            // Remove old word
            chars.drain(start_pos..end_pos);
            
            // Determine the appropriate suffix based on context
            let replacement_with_suffix = if is_slash_command_context {
                replacement.to_string()
            } else if is_msg_command {
                // For /msg and /m commands, use space instead of ": "
                format!("{} ", replacement)
            } else if is_action_command {
                // For action commands, wrap nicknames with spaces in quotes
                if replacement.contains(' ') {
                    format!("\"{}\"", replacement)
                } else {
                    replacement.to_string()
                }
            } else {
                // Regular nickname completion gets ": "
                format!("{}: ", replacement)
            };
            
            let replacement_chars: Vec<char> = replacement_with_suffix.chars().collect();
            for (i, &ch) in replacement_chars.iter().enumerate() {
                chars.insert(start_pos + i, ch);
            }
            
            self.input = chars.iter().collect();
            self.cursor_position = start_pos + replacement_with_suffix.len();
        }
    }
    
    /// Get completion matches for /msg command (both channels and nicknames)
    fn get_msg_completion_matches(&self, prefix: &str) -> Vec<String> {
        let mut matches = Vec::new();
        
        // Add joined channels
        let joined_channels = self.channel_manager.list_channels();
        for channel in &joined_channels {
            if channel != "system" && channel.to_lowercase().starts_with(&prefix.to_lowercase()) {
                matches.push(channel.clone());
            }
        }
        
        // Add nicknames from all channels
        let all_channels = self.channel_manager.list_all_channels();
        for (channel_name, _) in &all_channels {
            if let Some(channel) = self.channel_manager.get_channel(channel_name) {
                let channel_matches = channel.find_matching_nicknames(prefix);
                for nickname in channel_matches {
                    // Remove the pubkey suffix for /msg completion (user just types plain nickname)
                    let plain_nickname = if let Some(hash_pos) = nickname.find('#') {
                        &nickname[..hash_pos]
                    } else {
                        &nickname
                    };
                    
                    if !matches.contains(&plain_nickname.to_string()) {
                        matches.push(plain_nickname.to_string());
                    }
                }
            }
        }
        
        // Add private chat nicknames
        for nickname in self.private_chats.values() {
            if nickname.to_lowercase().starts_with(&prefix.to_lowercase()) && !matches.contains(nickname) {
                matches.push(nickname.clone());
            }
        }
        
        matches.sort();
        matches
    }
    
    async fn send_action_message(&mut self, action: &str) -> Result<()> {
        if let Some(channel) = &self.current_channel {
            // Create an action message (similar to regular message but marked as action)
            let message = Message {
                channel: channel.clone(),
                nickname: self.identity.nickname.clone(),
                content: action.to_string(),
                timestamp: chrono::Utc::now(),
                pubkey: Some(self.identity.pubkey.clone()),
                is_own: true,
                is_private: false,
                recipient_pubkey: None,
            };
            
            if channel == "system" {
                // For system channel, just show locally without sending to network
                self.channel_manager.add_message_sync(message);
            } else {
                // Send to Nostr for other channels
                self.nostr_client.send_message(channel, action, &self.identity.nickname).await?;
                
                // Add local echo
                self.channel_manager.add_message_sync(message);
            }
        } else {
            self.add_status_message("No channel selected".to_string());
        }
        Ok(())
    }
    
    async fn show_version(&mut self) -> Result<()> {
        let version = env!("CARGO_PKG_VERSION");
        let quotes = vec![
            "The purple pill helps the orange pill go down.",
            "Nostr is the protocol that binds all of your applications together.",
            "GM. PV.",
            "Nostr fixes this.",
            "Decentralized social media is not a bug, it's a feature.",
            "Protocols, not platforms.",
            "Relays gonna relay.",
            "In Nostr we trust.",
            "Kind 1 is the message, kind 3 is the medium.",
            "GM Fiatjaf.",
            "Not your keys, not your identity.",
            "Web3 is a VC backed scam. Nostr is the future.",
            "Zaps are the signal.",
            "Be your own algorithm.",
        ];
        
        let random_quote = if quotes.is_empty() {
            "No quotes available."
        } else {
            let index = rand::thread_rng().gen_range(0..quotes.len());
            &quotes[index]
        };
        
        let version_message = format!(
            "Running BitchatX version {} by Derek Ross. {}", 
            version, random_quote
        );
        
        // Send as regular message to the channel, not system message
        if let Some(channel) = self.current_channel.clone() {
            if channel == "system" {
                // For system channel, just show locally
                self.add_message_to_current_channel(version_message);
            } else {
                // For other channels, send as regular chat message
                self.send_message(&channel, &version_message).await?;
            }
        } else {
            self.add_status_message("No channel selected".to_string());
        }
        
        Ok(())
    }
    
    async fn show_status(&mut self) {
        // Get relay connection information
        let relay_count = self.nostr_client.get_relay_count();
        let (default_relays, georelays) = self.nostr_client.get_relay_stats();
        
        // Build status message
        let mut status_lines = Vec::new();
        status_lines.push("=== BitchatX Status ===".to_string());
        status_lines.push(format!("Connected Relays: {}", relay_count));
        status_lines.push(format!("  Default Relays: {}", default_relays));
        status_lines.push(format!("  GeoRelays: {}", georelays));
        
        // Show current channel info
        if let Some(current) = &self.current_channel {
            status_lines.push(format!("Current Channel: {}", current));
        }
        
        // Show identity info
        let npub = match PublicKey::from_hex(&self.identity.pubkey) {
            Ok(pk) => pk.to_bech32().unwrap_or_else(|_| self.identity.pubkey.clone()),
            Err(_) => self.identity.pubkey.clone(),
        };
        status_lines.push(format!("Your NPub: {}", npub));
        status_lines.push(format!("Nickname: {}", self.identity.nickname));
        
        // Show some stats
        let joined_channels = self.channel_manager.list_channels();
        status_lines.push(format!("Joined Channels: {}", joined_channels.len()));
        
        // Show spam filter status if enabled
        if self.spam_filter.is_enabled() {
            let muted_count = self.spam_filter.get_auto_muted_count();
            status_lines.push(format!("Spam Filter: Enabled ({} muted users)", muted_count));
        } else {
            status_lines.push("Spam Filter: Disabled".to_string());
        }
        
        status_lines.push("=== End Status ===".to_string());
        
        // Output each line to current channel
        for line in status_lines {
            self.add_message_to_current_channel(line);
        }
    }
    
    fn is_slash_command_context(&self, word_start_pos: usize) -> bool {
        // Check if the word being completed is part of a slash command
        let chars: Vec<char> = self.input.chars().collect();
        let word_start = word_start_pos.min(chars.len());
        
        // Look backwards from word start to find if there's a slash
        let mut pos = word_start;
        while pos > 0 {
            pos -= 1;
            let ch = chars[pos];
            
            if ch == '/' {
                return true; // Found a slash before this word
            } else if ch.is_whitespace() {
                continue; // Keep looking for slash
            } else {
                break; // Found non-whitespace, non-slash character
            }
        }
        
        false
    }
    
    fn is_action_command_context(&self, _word_start_pos: usize) -> bool {
        // Simple check: if input starts with action commands, we're in action command context
        let input = self.input.trim_start();
        input.starts_with("/hug ") || input.starts_with("/slap ") || 
        input.starts_with("/block ") || input.starts_with("/unblock ") ||
        input.starts_with("/whois ") || input.starts_with("/w ")
    }
    
    fn is_msg_command_context(&self) -> bool {
        // Check if we're completing arguments for /msg or /m commands
        let input = self.input.trim_start();
        input.starts_with("/msg ") || input.starts_with("/m ")
    }
    
    /// Parse command arguments, handling quoted strings properly
    fn parse_command_args(&self, input: &str) -> Vec<String> {
        let mut args = Vec::new();
        let mut current_arg = String::new();
        let mut in_quotes = false;
        let mut chars = input.chars().peekable();
        
        while let Some(ch) = chars.next() {
            match ch {
                '"' => {
                    in_quotes = !in_quotes;
                }
                ' ' if !in_quotes => {
                    if !current_arg.is_empty() {
                        args.push(current_arg.clone());
                        current_arg.clear();
                    }
                }
                _ => {
                    current_arg.push(ch);
                }
            }
        }
        
        // Add the last argument if not empty
        if !current_arg.is_empty() {
            args.push(current_arg);
        }
        
        args
    }
    
    fn scroll_to_bottom(&mut self) {
        self.scroll_to_bottom_with_height(self.viewport_height);
    }
    
    pub fn scroll_to_bottom_with_height(&mut self, viewport_height: usize) {
        if let Some(channel) = self.get_current_channel() {
            let message_count = channel.messages.len();
            
            // Scroll to show the most recent messages at the bottom
            if message_count > viewport_height {
                self.scroll_offset = message_count.saturating_sub(viewport_height);
            } else {
                // If all messages fit on screen, show from beginning
                self.scroll_offset = 0;
            }
        }
    }
    
    pub fn update_scroll_offset(&mut self, new_offset: usize) {
        self.scroll_offset = new_offset;
    }
    
    pub fn update_viewport_height(&mut self, height: usize) {
        self.viewport_height = height;
    }
    
    pub fn force_scroll_to_bottom(&mut self) {
        // Reset scroll state completely to ensure clean scrolling
        self.should_autoscroll = true;
        self.scroll_to_bottom_with_height(self.viewport_height);
    }
    
    fn update_autoscroll_status(&mut self) {
        self.update_autoscroll_status_with_height(self.viewport_height);
    }
    
    pub fn update_autoscroll_status_with_height(&mut self, viewport_height: usize) {
        if let Some(channel) = self.get_current_channel() {
            let message_count = channel.messages.len();
            
            if message_count <= viewport_height {
                // If all messages fit on screen, always autoscroll
                self.should_autoscroll = true;
                return;
            }
            
            // Calculate the position where we show the most recent messages
            let bottom_scroll_position = message_count.saturating_sub(viewport_height);
            
            // Very lenient threshold - if we're within 3 messages of the bottom, enable autoscroll
            let threshold = 3;
            if self.scroll_offset >= bottom_scroll_position.saturating_sub(threshold) {
                self.should_autoscroll = true;
            } else {
                // User has scrolled significantly away from bottom, disable autoscroll
                self.should_autoscroll = false;
            }
        }
    }
    
    async fn block_user(&mut self, nickname: &str) {
        // Find pubkey for this nickname in current channel
        if let Some(pubkey) = self.find_pubkey_for_nickname(nickname).await {
            if self.blocked_users.insert(pubkey.clone()) {
                self.add_status_message(format!("Blocked user {}", nickname));
                // Add system message to current channel to announce the block
                self.add_message_to_current_channel(format!("* {} has blocked {}", self.identity.nickname, nickname));
            } else {
                self.add_status_message(format!("User {} is already blocked", nickname));
            }
        } else {
            self.add_status_message(format!("User '{}' not found", nickname));
        }
    }
    
    async fn unblock_user(&mut self, nickname: &str) {
        // Find pubkey for this nickname in current channel or in blocked list
        if let Some(pubkey) = self.find_pubkey_for_nickname(nickname).await {
            if self.blocked_users.remove(&pubkey) {
                self.add_status_message(format!("Unblocked user {}", nickname));
                // Add system message to current channel to announce the unblock
                self.add_message_to_current_channel(format!("* {} has unblocked {}", self.identity.nickname, nickname));
            } else {
                self.add_status_message(format!("User {} is not blocked", nickname));
            }
        } else {
            // Try to find in blocked users by scanning all channels for this nickname
            let mut found_and_removed = false;
            for pubkey in self.blocked_users.clone() {
                if let Some(nick) = self.find_nickname_for_pubkey(&pubkey) {
                    if nick.eq_ignore_ascii_case(nickname) {
                        self.blocked_users.remove(&pubkey);
                        self.add_status_message(format!("Unblocked user {}", nickname));
                        // Add system message to current channel to announce the unblock
                        self.add_message_to_current_channel(format!("* {} has unblocked {}", self.identity.nickname, nickname));
                        found_and_removed = true;
                        break;
                    }
                }
            }
            if !found_and_removed {
                self.add_status_message(format!("User '{}' not found", nickname));
            }
        }
    }
    
    fn list_blocked_users(&mut self) {
        if self.blocked_users.is_empty() {
            self.add_status_message("No blocked users".to_string());
        } else {
            self.add_status_message("Blocked users:".to_string());
            
            // Clone the blocked users set to avoid borrowing issues
            let blocked_users_clone = self.blocked_users.clone();
            for pubkey in blocked_users_clone {
                // Try to find nickname for this pubkey
                if let Some(nickname) = self.find_nickname_for_pubkey(&pubkey) {
                    let short_pubkey = if pubkey.len() > 8 { &pubkey[..8] } else { &pubkey };
                    self.add_status_message(format!("  {} ({}...)", nickname, short_pubkey));
                } else {
                    let short_pubkey = if pubkey.len() > 16 { &pubkey[..16] } else { &pubkey };
                    self.add_status_message(format!("  {}...", short_pubkey));
                }
            }
        }
    }
    
    async fn find_pubkey_for_nickname(&self, nickname: &str) -> Option<String> {
        // Search through all channels to find a message from this nickname with a pubkey
        let all_channels = self.get_all_channels();
        for channel_name in all_channels {
            if let Some(channel) = self.channel_manager.get_channel(&channel_name) {
                for message in &channel.messages {
                    if message.nickname.eq_ignore_ascii_case(nickname) {
                        if let Some(ref pubkey) = message.pubkey {
                            return Some(pubkey.clone());
                        }
                    }
                }
            }
        }
        None
    }
    
    fn find_nickname_for_pubkey(&self, pubkey: &str) -> Option<String> {
        // Search through all channels to find the most recent nickname for this pubkey
        let all_channels = self.get_all_channels();
        for channel_name in all_channels {
            if let Some(channel) = self.channel_manager.get_channel(&channel_name) {
                // Search in reverse order to get most recent nickname
                for message in channel.messages.iter().rev() {
                    if let Some(ref msg_pubkey) = message.pubkey {
                        if msg_pubkey == pubkey {
                            return Some(message.nickname.clone());
                        }
                    }
                }
            }
        }
        None
    }
    
    pub fn is_user_blocked(&self, pubkey: &Option<String>) -> bool {
        if let Some(pk) = pubkey {
            self.blocked_users.contains(pk)
        } else {
            false
        }
    }
    
    #[allow(dead_code)]
    pub fn handle_connection_lost(&mut self) {
        self.state = AppState::Disconnected;
        self.add_status_message("Connection lost. Attempting to reconnect...".to_string());
    }
    
    #[allow(dead_code)]
    pub fn handle_connection_error(&mut self, error: String) {
        self.state = AppState::Error(error.clone());
        self.add_status_message(format!("Connection error: {}", error));
    }
    
    fn copy_to_clipboard(&self) {
        if let Ok(mut clipboard) = Clipboard::new() {
            if let Err(_) = clipboard.set_text(self.input.clone()) {
                // Silently fail if clipboard access fails
            }
        }
    }
    
    fn paste_from_clipboard(&mut self) {
        if let Ok(mut clipboard) = Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                // Insert clipboard text at cursor position
                let before = &self.input[..self.cursor_position];
                let after = &self.input[self.cursor_position..];
                self.input = format!("{}{}{}", before, text, after);
                self.cursor_position += text.len();
            }
        }
    }
    
    fn cut_to_clipboard(&mut self) {
        if let Ok(mut clipboard) = Clipboard::new() {
            if let Err(_) = clipboard.set_text(self.input.clone()) {
                // Silently fail if clipboard access fails
            }
            self.input.clear();
            self.cursor_position = 0;
        }
    }
    
    fn select_all(&mut self) {
        // Move cursor to end (simulates selecting all)
        self.cursor_position = self.input.len();
    }
    
    async fn whois_user(&mut self, input: &str) {        
        // Parse input to extract nickname and optional pubkey suffix
        let (target_nickname, target_pubkey_prefix) = if let Some(hash_pos) = input.rfind('#') {
            let nickname = &input[..hash_pos];
            let pubkey_prefix = &input[hash_pos + 1..];
            (nickname, Some(pubkey_prefix))
        } else {
            (input, None)
        };
        
        // Search through all channels to find user information
        let mut user_info = None;
        let mut channels_found = Vec::new();
        let mut total_messages = 0;
        let mut searched_channels = 0;
        
        // Look through all channels for this user
        let all_channels = self.get_all_channels();
        for channel_name in all_channels {
            if let Some(channel) = self.channel_manager.get_channel(&channel_name) {
                searched_channels += 1;
                total_messages += channel.messages.len();
                
                // Find most recent message from this user
                for message in channel.messages.iter().rev() {
                    let mut is_match = false;
                    
                    // Check if this message matches our search criteria
                    if let Some(prefix) = target_pubkey_prefix {
                        // Search by pubkey prefix if provided
                        if let Some(ref pubkey) = message.pubkey {
                            if pubkey.starts_with(prefix) && 
                               message.nickname.eq_ignore_ascii_case(target_nickname) {
                                is_match = true;
                            }
                        }
                    } else {
                        // Search by nickname only
                        if message.nickname.eq_ignore_ascii_case(target_nickname) {
                            is_match = true;
                        }
                    }
                    
                    if is_match {
                        if let Some(ref pubkey) = message.pubkey {
                            // Convert pubkey to npub format
                            let npub = match PublicKey::from_hex(pubkey) {
                                Ok(pk) => pk.to_bech32().unwrap_or_else(|_| "invalid".to_string()),
                                Err(_) => "invalid".to_string(),
                            };
                            
                            // Store the most recent user info (only set once)
                            if user_info.is_none() {
                                user_info = Some((message.nickname.clone(), pubkey.clone(), npub));
                            }
                            
                            // Track all channels where user was found
                            if !channels_found.contains(&channel_name) {
                                channels_found.push(channel_name.clone());
                            }
                        }
                        break; // Found user in this channel, move to next channel
                    }
                }
            }
        }
        
        // Also search private chats
        for (pubkey, nickname_stored) in &self.private_chats {
            let mut is_match = false;
            
            if let Some(prefix) = target_pubkey_prefix {
                // Search by pubkey prefix if provided
                if pubkey.starts_with(prefix) && 
                   nickname_stored.eq_ignore_ascii_case(target_nickname) {
                    is_match = true;
                }
            } else {
                // Search by nickname only
                if nickname_stored.eq_ignore_ascii_case(target_nickname) {
                    is_match = true;
                }
            }
            
            if is_match {
                let npub = match PublicKey::from_hex(pubkey) {
                    Ok(pk) => pk.to_bech32().unwrap_or_else(|_| "invalid".to_string()),
                    Err(_) => "invalid".to_string(),
                };
                
                if user_info.is_none() {
                    user_info = Some((nickname_stored.clone(), pubkey.clone(), npub));
                }
                
                // Check if we have a private chat channel for this user
                let private_channel = format!("@{}", nickname_stored);
                if self.channel_manager.get_channel(&private_channel).is_some() {
                    channels_found.push("private chat".to_string());
                }
                break;
            }
        }
        
        match user_info {
            Some((found_nickname, pubkey, npub)) => {
                self.add_message_to_current_channel("=== WHOIS Information ===".to_string());
                
                // Show display name with pubkey suffix
                let display_name = self.format_display_nickname(&found_nickname, &Some(pubkey.clone()));
                self.add_message_to_current_channel(format!("Display Name: {}", display_name));
                self.add_message_to_current_channel(format!("Nickname: {}", found_nickname));
                self.add_message_to_current_channel(format!("NPub: {}", npub));
                
                let short_pubkey = if pubkey.len() > 16 { 
                    format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-8..])
                } else { 
                    pubkey.clone()
                };
                self.add_message_to_current_channel(format!("PubKey: {}", short_pubkey));
                self.add_message_to_current_channel(format!("Full PubKey: {}", pubkey));
                
                if channels_found.is_empty() {
                    self.add_message_to_current_channel("Channels: No recent activity".to_string());
                } else {
                    let channels_str = channels_found.join(", ");
                    self.add_message_to_current_channel(format!("Seen in: {}", channels_str));
                }
                self.add_message_to_current_channel("=== End WHOIS ===".to_string());
            }
            None => {
                let search_target = if target_pubkey_prefix.is_some() {
                    input.to_string()
                } else {
                    target_nickname.to_string()
                };
                self.add_message_to_current_channel(format!("No information found for user '{}'", search_target));
                self.add_message_to_current_channel(format!("Searched {} channels with {} total messages", searched_channels, total_messages));
            }
        }
    }
    
    /// Format a nickname with pubkey suffix if available (e.g., "alice#02c1")
    pub fn format_display_nickname(&self, nickname: &str, pubkey: &Option<String>) -> String {
        match pubkey {
            Some(pk) if pk.len() >= 4 => {
                // Take first 4 characters of pubkey as suffix
                let suffix = &pk[..4];
                format!("{}#{}", nickname, suffix)
            }
            _ => nickname.to_string(),
        }
    }
    
    
    /// Update input horizontal scroll to keep cursor visible with a specific width
    pub fn update_input_scroll_with_width(&mut self, available_width: usize) {
        if available_width <= 2 {
            self.input_horizontal_scroll = 0;
            return;
        }
        
        // Leave space for cursor (1 char) but be less conservative than before
        let usable_width = available_width.saturating_sub(1);
        
        // If cursor is beyond the usable area, scroll right to keep it visible
        if self.cursor_position >= self.input_horizontal_scroll + usable_width {
            self.input_horizontal_scroll = self.cursor_position.saturating_sub(usable_width) + 1;
        }
        // If cursor is before the left edge, scroll left
        else if self.cursor_position < self.input_horizontal_scroll {
            self.input_horizontal_scroll = self.cursor_position;
        }
    }
    
    /// Update input horizontal scroll to keep cursor visible (fallback with estimate)
    fn update_input_scroll(&mut self) {
        // Use the tracked input width for accurate scrolling
        self.update_input_scroll_with_width(self.input_width);
    }
    
    fn list_auto_muted_users(&mut self) {
        let auto_muted = self.spam_filter.get_auto_muted_users();
        
        if auto_muted.is_empty() {
            self.add_message_to_current_channel("No users are currently auto-muted for spam".to_string());
        } else {
            self.add_message_to_current_channel("Auto-muted spammers:".to_string());
            for (pubkey, remaining_time) in auto_muted {
                let nickname = self.find_nickname_for_pubkey(&pubkey)
                    .unwrap_or_else(|| format!("{}...", &pubkey[..8.min(pubkey.len())]));
                let minutes = remaining_time.as_secs() / 60;
                let seconds = remaining_time.as_secs() % 60;
                self.add_message_to_current_channel(format!("  {} ({}:{:02} remaining)", nickname, minutes, seconds));
            }
        }
    }
    
    async fn unmute_spammer(&mut self, nickname: &str) {
        if let Some(pubkey) = self.find_pubkey_for_nickname(nickname).await {
            if self.spam_filter.is_user_auto_muted(&pubkey) {
                self.spam_filter.manually_unmute_user(&pubkey);
                self.add_message_to_current_channel(format!("Manually unmuted {} from spam filter", nickname));
            } else {
                self.add_message_to_current_channel(format!("{} is not currently auto-muted", nickname));
            }
        } else {
            self.add_message_to_current_channel(format!("User '{}' not found", nickname));
        }
    }
    
    fn show_spam_filter_status(&mut self) {
        let auto_muted = self.spam_filter.get_auto_muted_users();
        let muted_count = auto_muted.len();
        
        self.add_message_to_current_channel("=== Spam Filter Status ===".to_string());
        self.add_message_to_current_channel(format!("Currently auto-muted users: {}", muted_count));
        self.add_message_to_current_channel("Filters enabled:".to_string());
        self.add_message_to_current_channel("  • Message frequency limit (15/minute)".to_string());
        self.add_message_to_current_channel("  • Duplicate message detection".to_string());
        self.add_message_to_current_channel("  • Spam keyword filtering".to_string());
        self.add_message_to_current_channel("  • Excessive caps detection".to_string());
        self.add_message_to_current_channel("  • Future timestamp rejection (>5min)".to_string());
        self.add_message_to_current_channel("  • Old timestamp rejection (>24hr)".to_string());
        self.add_message_to_current_channel("Auto-mute duration: 10 minutes".to_string());
        self.add_message_to_current_channel("Use '/spam list' to see muted users".to_string());
        self.add_message_to_current_channel("Use '/spam unmute <nickname>' to manually unmute".to_string());
    }
    
    fn clear_current_channel(&mut self) {
        if let Some(channel_name) = &self.current_channel {
            let was_cleared = self.channel_manager.clear_channel(channel_name);
            
            if was_cleared {
                // Reset scroll position after clearing
                self.scroll_offset = 0;
                self.should_autoscroll = true;
                
                // Add a confirmation message
                if channel_name == "system" {
                    self.add_status_message("🧹 System channel cleared".to_string());
                } else if channel_name.starts_with("dm:") {
                    // For private messages, show the nickname instead of the channel ID
                    let pubkey = &channel_name[3..];
                    let display_name = self.private_chats.get(pubkey)
                        .map(|nick| format!("@{}", nick))
                        .unwrap_or_else(|| format!("dm:{}", &pubkey[..8]));
                    self.add_status_message(format!("🧹 Private chat with {} cleared", display_name));
                } else {
                    self.add_status_message(format!("🧹 Channel #{} cleared", channel_name));
                }
            } else {
                // Channel was already empty or doesn't exist
                if channel_name == "system" {
                    self.add_status_message("System channel is already empty".to_string());
                } else {
                    self.add_status_message(format!("Channel #{} is already empty", channel_name));
                }
            }
        } else {
            self.add_status_message("No channel selected to clear".to_string());
        }
    }
    
    /// Handle mouse clicks and check for nostr URIs at precise coordinates
    async fn handle_mouse_click(&mut self, column: u16, row: u16) {
        // Check if click is on any of the tracked clickable regions
        for region in &self.clickable_regions {
            if row == region.y && column >= region.x && column < region.x + region.width {
                // Click is within this nostr URI region
                let nostr_uri = region.nostr_uri.clone();
                self.open_nostr_uri(&nostr_uri).await;
                return;
            }
        }
    }
    
    
    /// Open a nostr URI in the browser via njump.me
    async fn open_nostr_uri(&mut self, nostr_uri: &str) {
        // Convert nostr: URI to njump.me URL
        let njump_url = format!("https://njump.me/{}", &nostr_uri[6..]); // Remove "nostr:" prefix
        
        match open::that(&njump_url) {
            Ok(_) => {
                self.add_status_message(format!("🔗 Opened {} in browser", nostr_uri));
            }
            Err(e) => {
                self.add_status_message(format!("❌ Failed to open browser: {}", e));
            }
        }
    }
}