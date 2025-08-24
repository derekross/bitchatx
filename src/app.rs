use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

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
                    KeyCode::Up => {
                        if self.scroll_offset > 0 {
                            self.scroll_offset -= 1;
                        }
                    }
                    KeyCode::Down => {
                        self.scroll_offset += 1;
                    }
                    KeyCode::PageUp => {
                        self.scroll_offset = self.scroll_offset.saturating_sub(10);
                    }
                    KeyCode::PageDown => {
                        self.scroll_offset += 10;
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
                self.list_all_channels();
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
    
    async fn show_help(&mut self) {
        let help_text = vec![
            "BitchatX Commands:".to_string(),
            "/join, /j <geohash> - Join a geohash channel".to_string(),
            "/leave, /part, /l - Leave current channel".to_string(),
            "/msg, /m <channel> <message> - Send message to specific channel".to_string(),
            "/nick, /n <nickname> - Change your display name (session only)".to_string(),
            "/list, /channels - List joined channels".to_string(),
            "/all - List all channels (joined + listening)".to_string(),
            "/help, /h, /commands - Show this help".to_string(),
            "/quit, /q, /exit - Exit BitchatX".to_string(),
            "".to_string(),
            "".to_string(),
            "Keyboard Commands:".to_string(),
            "i - Enter input mode, Esc - Exit to normal mode, q - Quit (normal mode)".to_string(),
            "Input mode: Stay in input mode after sending messages, only Esc exits".to_string(),
            "Tab - Nickname completion (input mode), Up/Down - Scroll messages".to_string(),
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
        while let Ok(message) = self.message_rx.try_recv() {
            self.channel_manager.add_message(message).await;
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
            
            // Replace the current word with the completion + ": "
            let mut chars: Vec<char> = self.input.chars().collect();
            
            // Remove old word
            chars.drain(start_pos..end_pos);
            
            // Insert completion + ": "
            let replacement_with_suffix = format!("{}: ", replacement);
            let replacement_chars: Vec<char> = replacement_with_suffix.chars().collect();
            for (i, &ch) in replacement_chars.iter().enumerate() {
                chars.insert(start_pos + i, ch);
            }
            
            self.input = chars.iter().collect();
            self.cursor_position = start_pos + replacement_with_suffix.len();
        }
    }
}