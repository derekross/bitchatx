use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use rand::prelude::*;

use crate::channels::{ChannelManager, Message, Channel};
use crate::nostr::{NostrClient, Identity};

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Connecting,
    Connected,
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
}

#[derive(Debug, Clone)]
pub struct TabCompletionState {
    original_input: String,
    original_cursor: usize,
    prefix: String,
    pub matches: Vec<String>,
    pub current_match_index: usize,
}

impl App {
    pub async fn new(nsec: Option<&str>, auto_channel: Option<&str>) -> Result<Self> {
        let identity = if let Some(nsec_str) = nsec {
            Identity::from_nsec(nsec_str)?
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
            should_autoscroll: true,
            
            nostr_client,
            identity,
            
            channel_manager,
            current_channel: Some("system".to_string()),
            system_channel: "system".to_string(),
            
            message_rx,
            status_rx,
            tab_completion_state: None,
        };
        
        // Add welcome message to system channel
        app.add_status_message("Welcome to BitchatX v0.1.0!".to_string());
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
        app.nostr_client.connect().await?;
        app.state = AppState::Connected;
        
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
        // Ignore key events with modifiers (except Shift for some keys)
        // This prevents Ctrl+C, Ctrl+D, etc. from causing unexpected behavior
        if !key.modifiers.is_empty() {
            match key.modifiers {
                KeyModifiers::SHIFT => {
                    // Allow Shift + Tab (BackTab) and Shift + letter keys
                    if key.code != KeyCode::BackTab && !matches!(key.code, KeyCode::Char(_)) {
                        return Ok(());
                    }
                }
                _ => {
                    // Ignore all other modifier combinations (Ctrl, Alt, etc.)
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
                        // Stay in input mode after sending message
                    }
                    KeyCode::Esc => {
                        self.input.clear();
                        self.cursor_position = 0;
                        self.input_mode = InputMode::Normal;
                    }
                    KeyCode::Char(c) => {
                        // Reset tab completion on any character input
                        self.tab_completion_state = None;
                        self.input.insert(self.cursor_position, c);
                        self.cursor_position += 1;
                    }
                    KeyCode::Tab => {
                        self.handle_tab_completion().await;
                    }
                    KeyCode::Backspace => {
                        self.tab_completion_state = None;
                        if self.cursor_position > 0 {
                            self.input.remove(self.cursor_position - 1);
                            self.cursor_position -= 1;
                        }
                    }
                    KeyCode::Delete => {
                        self.tab_completion_state = None;
                        if self.cursor_position < self.input.len() {
                            self.input.remove(self.cursor_position);
                        }
                    }
                    KeyCode::Left => {
                        self.tab_completion_state = None;
                        if self.cursor_position > 0 {
                            self.cursor_position -= 1;
                        }
                    }
                    KeyCode::Right => {
                        self.tab_completion_state = None;
                        if self.cursor_position < self.input.len() {
                            self.cursor_position += 1;
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
                    }
                    KeyCode::End => {
                        self.cursor_position = self.input.len();
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
                    self.add_status_message("Usage: /msg <channel> <message>".to_string());
                    return Ok(());
                }
                let channel = parts[1];
                let message = parts[2..].join(" ");
                self.send_message(channel, &message).await?;
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
                let slap_message = format!("* {} slaps {} around a bit with a large trout", self.identity.nickname, nickname);
                self.send_action_message(&slap_message).await?;
            }
            "version" => {
                self.show_version();
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
        self.nostr_client.send_message(channel, content, &self.identity.nickname).await?;
        
        // Add local echo
        let message = Message {
            channel: channel.to_string(),
            nickname: self.identity.nickname.clone(),
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
            pubkey: Some(self.identity.pubkey.clone()),
            is_own: true,
        };
        
        self.channel_manager.add_message(message).await;
        
        // Enable auto-scrolling after sending a message
        self.should_autoscroll = true;
        
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
                let message_count = self.channel_manager.get_message_count(&channel);
                let indicator = if Some(&channel) == self.current_channel.as_ref() { "*" } else { " " };
                self.add_status_message(format!("{}#{} ({} messages)", indicator, channel, message_count));
            }
        }
    }
    
    fn list_all_channels(&mut self) {
        let channels = self.channel_manager.list_all_channels();
        if channels.is_empty() {
            self.add_status_message("No channels available".to_string());
        } else {
            self.add_status_message("All channels (joined + listening):".to_string());
            for (channel, is_joined) in channels {
                let message_count = self.channel_manager.get_message_count(&channel);
                let indicator = if Some(&channel) == self.current_channel.as_ref() { "*" } else { " " };
                let status = if is_joined { "joined" } else { "listening" };
                self.add_status_message(format!("{}#{} ({} messages, {})", indicator, channel, message_count, status));
            }
        }
    }
    
    async fn show_all_recent_messages(&mut self) {
        let ten_minutes_ago = chrono::Utc::now() - chrono::Duration::minutes(10);
        let all_channels = self.get_all_channels();
        
        // Collect all recent messages first to avoid borrow issues
        let mut recent_activity: Vec<(String, Vec<String>)> = Vec::new();
        
        for channel_name in all_channels {
            if let Some(channel) = self.channel_manager.get_channel(&channel_name) {
                let recent_messages: Vec<String> = channel.messages
                    .iter()
                    .filter(|msg| msg.timestamp >= ten_minutes_ago)
                    .map(|msg| {
                        let timestamp = msg.timestamp.format("%H:%M:%S");
                        format!("[{}] <{}> {}", timestamp, msg.nickname, msg.content)
                    })
                    .collect();
                
                if !recent_messages.is_empty() {
                    recent_activity.push((channel_name, recent_messages));
                }
            }
        }
        
        // Now add all status messages
        self.add_status_message("=== Recent Activity (Last 10 Minutes) ===".to_string());
        
        if recent_activity.is_empty() {
            self.add_status_message("No recent activity in any channel (last 10 minutes)".to_string());
        } else {
            for (channel_name, messages) in recent_activity {
                // Channel header
                if channel_name == "system" {
                    self.add_status_message("--- System Channel ---".to_string());
                } else {
                    self.add_status_message(format!("--- Channel #{} ---", channel_name));
                }
                
                // Show recent messages
                for message in messages {
                    self.add_status_message(message);
                }
                
                // Add separator between channels
                self.add_status_message("".to_string());
            }
            
            self.add_status_message("=== End of Recent Activity ===".to_string());
        }
    }
    
    async fn show_help(&mut self) {
        let help_text = vec![
            "BitchatX Commands:".to_string(),
            "/join, /j <geohash> - Join a geohash channel".to_string(),
            "/leave, /part, /l - Leave current channel".to_string(),
            "/msg, /m <channel> <message> - Send message to specific channel".to_string(),
            "/nick, /n <nickname> - Change your display name (session only)".to_string(),
            "/list, /channels - List joined channels".to_string(),
            "/all - Show recent activity from all channels (last 10 minutes)".to_string(),
            "/hug <nickname> - Send a hug to someone 🫂".to_string(),
            "/slap <nickname> - Slap someone with a large trout".to_string(),
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
        ];
        
        for line in help_text {
            self.add_status_message(line);
        }
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
        };
        
        // Add directly to channel manager without going through async receiver
        // This ensures immediate display
        let _ = self.channel_manager.add_message_sync(system_message);
    }
    
    pub async fn on_tick(&mut self) -> Result<()> {
        // Process incoming messages
        let mut new_messages_count = 0;
        while let Ok(message) = self.message_rx.try_recv() {
            self.channel_manager.add_message(message).await;
            new_messages_count += 1;
        }
        
        // Auto-scroll to bottom if we received new messages and should auto-scroll
        if new_messages_count > 0 && self.should_autoscroll {
            self.scroll_to_bottom();
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
    
    pub fn get_visible_messages(&self, height: usize) -> Vec<&Message> {
        if let Some(channel) = self.get_current_channel() {
            let messages = &channel.messages;
            let start = self.scroll_offset.min(messages.len().saturating_sub(height));
            let end = (start + height).min(messages.len());
            messages[start..end].iter().collect()
        } else {
            vec![]
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
            if let Some((word, _start_pos, _end_pos)) = word_info {
                if word.len() >= 2 { // Minimum 2 characters to start completion
                    if let Some(channel) = self.channel_manager.get_channel(&current_channel) {
                        let matches = channel.find_matching_nicknames(&word);
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
            
            // Check if we're in a slash command context
            let is_slash_command_context = self.is_slash_command_context(start_pos);
            
            // Replace the current word with the completion
            let mut chars: Vec<char> = self.input.chars().collect();
            
            // Remove old word
            chars.drain(start_pos..end_pos);
            
            // Only add ": " if this is NOT a slash command context
            let replacement_with_suffix = if is_slash_command_context {
                replacement.to_string()
            } else {
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
    
    async fn send_action_message(&mut self, action: &str) -> Result<()> {
        if let Some(channel) = &self.current_channel {
            if channel == "system" {
                self.add_status_message("Cannot send actions to system channel".to_string());
                return Ok(());
            }
            
            // Create an action message (similar to regular message but marked as action)
            let message = Message {
                channel: channel.clone(),
                nickname: self.identity.nickname.clone(),
                content: action.to_string(),
                timestamp: chrono::Utc::now(),
                pubkey: Some(self.identity.pubkey.clone()),
                is_own: true,
            };
            
            // Send to Nostr
            self.nostr_client.send_message(channel, action, &self.identity.nickname).await?;
            
            // Add local echo
            self.channel_manager.add_message_sync(message);
        } else {
            self.add_status_message("No channel selected".to_string());
        }
        Ok(())
    }
    
    fn show_version(&mut self) {
        let version = env!("CARGO_PKG_VERSION");
        let quotes = vec![
            "The purple pill helps the orange pill go down.",
            "Nostr is the protocol that binds all of your applications together.",
            "GM. PV.",
            "Nost fixes this.",
            "Decentralized social media is not a bug, it's a feature.",
            "My keys, my keys, my kingdom for my keys!",
            "Relays gonna relay.",
            "In Nostr we trust.",
            "Kind 1 is the message, kind 3 is the medium.",
            "To the moon!",
            "Not your keys, not your crypto.",
            "Web5 is just Nostr with extra steps.",
        ];
        
        let random_quote = quotes[rand::random::<usize>() % quotes.len()];
        
        let version_message = format!(
            "Running BitchatX version {} by Derek Ross. {}", 
            version, random_quote
        );
        
        self.add_status_message(version_message);
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
    
    fn scroll_to_bottom(&mut self) {
        if let Some(channel) = self.get_current_channel() {
            let message_count = channel.messages.len();
            if message_count > 0 {
                // Set scroll_offset to show bottom messages
                // This will be handled by get_visible_messages logic
                self.scroll_offset = message_count.saturating_sub(1);
            }
        }
    }
    
    fn update_autoscroll_status(&mut self) {
        if let Some(channel) = self.get_current_channel() {
            let message_count = channel.messages.len();
            let visible_height = 20; // Approximate visible message count
            
            // If we're near bottom, re-enable auto-scrolling
            if self.scroll_offset >= message_count.saturating_sub(visible_height) {
                self.should_autoscroll = true;
            }
        }
    }
}