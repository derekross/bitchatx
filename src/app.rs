use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use rand::Rng;
use std::collections::{HashSet, HashMap};
use arboard::Clipboard;

use crate::channels::{ChannelManager, Message, Channel};
use crate::nostr::{NostrClient, Identity};
use nostr::{PublicKey, ToBech32};

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
                        // User scrolled up, disable auto-scrolling
                        self.should_autoscroll = false;
                    }
                    KeyCode::Down => {
                        self.scroll_offset += 1;
                        // Check if user scrolled to bottom
                        self.update_autoscroll_status();
                    }
                    KeyCode::PageUp => {
                        self.scroll_offset = self.scroll_offset.saturating_sub(10);
                        // User scrolled up, disable auto-scrolling
                        self.should_autoscroll = false;
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
        let parts: Vec<&str> = input[1..].split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }
        
        match parts[0].to_lowercase().as_str() {
            "join" | "j" => {
                if parts.len() != 2 {
                    self.add_status_message("Usage: /join <geohash>".to_string());
                    return Ok(());
                }
                self.join_channel(parts[1]).await?;
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
                self.change_nickname(parts[1]).await?;
            }
            "msg" | "m" => {
                if parts.len() < 3 {
                    self.add_status_message("Usage: /msg <channel/nickname> <message>".to_string());
                    return Ok(());
                }
                let target = parts[1];
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
                let nickname = parts[1];
                let hug_message = format!("* {} hugs {} 🫂", self.identity.nickname, nickname);
                self.send_action_message(&hug_message).await?;
            }
            "slap" => {
                if parts.len() != 2 {
                    self.add_status_message("Usage: /slap <nickname>".to_string());
                    return Ok(());
                }
                let nickname = parts[1];
                let slap_message = format!("* {} slaps {} around a bit with a large trout 🐟", self.identity.nickname, nickname);
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
                    let nickname = parts[1].trim_start_matches('@');
                    self.whois_user(nickname).await;
                } else {
                    self.add_status_message("Usage: /whois <nickname>".to_string());
                }
            }
            "version" => {
                self.show_version().await?;
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
        
        // Enable auto-scrolling when joining a channel
        self.should_autoscroll = true;
        
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
            "/whois, /w <nickname> - Show user information (npub, channels)".to_string(),
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
            
            // Use sync version for faster processing (no await overhead)
            let _ = self.channel_manager.add_message_sync(message);
            new_messages_count += 1;
        }
        
        // Auto-scroll to bottom if we received new messages and should auto-scroll
        if new_messages_count > 0 && self.should_autoscroll {
            // We don't know the viewport height here, so we'll let the UI handle the scroll position
            // by keeping should_autoscroll = true, and the UI will position it correctly
        }
        
        // Process status updates
        while let Ok(status) = self.status_rx.try_recv() {
            self.add_status_message(status);
        }
        
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
                replacement.to_string()
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
    
    fn scroll_to_bottom(&mut self) {
        // This will be called with the actual viewport height from the UI
        self.scroll_to_bottom_with_height(25); // Default fallback
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
    
    fn update_autoscroll_status(&mut self) {
        self.update_autoscroll_status_with_height(25); // Default fallback
    }
    
    pub fn update_autoscroll_status_with_height(&mut self, viewport_height: usize) {
        if let Some(channel) = self.get_current_channel() {
            let message_count = channel.messages.len();
            
            // If we're at or near bottom, re-enable auto-scrolling
            let bottom_threshold = message_count.saturating_sub(viewport_height);
            if self.scroll_offset >= bottom_threshold.saturating_sub(5) {
                self.should_autoscroll = true;
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
    
    async fn whois_user(&mut self, nickname: &str) {
        // Search through all channels to find user information
        let mut user_info = None;
        let mut relay_info = Vec::new();
        
        // Look through all channels for this user
        let all_channels = self.get_all_channels();
        for channel_name in all_channels {
            if let Some(channel) = self.channel_manager.get_channel(&channel_name) {
                // Find most recent message from this user
                for message in channel.messages.iter().rev() {
                    if message.nickname.eq_ignore_ascii_case(nickname) {
                        if let Some(ref pubkey) = message.pubkey {
                            // Convert pubkey to npub format
                            let npub = match PublicKey::from_hex(pubkey) {
                                Ok(pk) => pk.to_bech32().unwrap_or_else(|_| "invalid".to_string()),
                                Err(_) => "invalid".to_string(),
                            };
                            
                            user_info = Some((message.nickname.clone(), pubkey.clone(), npub));
                            
                            // For now, we don't have detailed relay info, so we'll show basic info
                            // In a full implementation, this would come from the Nostr client
                            if !relay_info.contains(&channel_name) {
                                relay_info.push(channel_name.clone());
                            }
                        }
                        break; // Found user info, stop searching this channel
                    }
                }
            }
        }
        
        match user_info {
            Some((found_nickname, pubkey, npub)) => {
                self.add_message_to_current_channel("=== WHOIS Information ===".to_string());
                self.add_message_to_current_channel(format!("Nickname: {}", found_nickname));
                self.add_message_to_current_channel(format!("NPub: {}", npub));
                
                let short_pubkey = if pubkey.len() > 16 { 
                    format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-8..])
                } else { 
                    pubkey 
                };
                self.add_message_to_current_channel(format!("PubKey: {}", short_pubkey));
                
                if relay_info.is_empty() {
                    self.add_message_to_current_channel("Relays: No recent activity".to_string());
                } else {
                    let channels_str = relay_info.join(", #");
                    self.add_message_to_current_channel(format!("Channels: #{}", channels_str));
                }
                self.add_message_to_current_channel("=== End WHOIS ===".to_string());
            }
            None => {
                self.add_message_to_current_channel(format!("No information found for user '{}'", nickname));
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
        if available_width <= 1 {
            self.input_horizontal_scroll = 0;
            return;
        }
        
        // Keep cursor within the visible area with some buffer
        let visible_width = available_width.saturating_sub(1); // Account for cursor
        
        // If cursor is beyond the right edge of visible area, scroll right
        if self.cursor_position >= self.input_horizontal_scroll + visible_width {
            self.input_horizontal_scroll = self.cursor_position.saturating_sub(visible_width) + 1;
        }
        // If cursor is before the left edge of visible area, scroll left
        else if self.cursor_position < self.input_horizontal_scroll {
            self.input_horizontal_scroll = self.cursor_position;
        }
    }
    
    /// Update input horizontal scroll to keep cursor visible (fallback with estimate)
    fn update_input_scroll(&mut self) {
        self.update_input_scroll_with_width(80); // Conservative fallback estimate
    }
}