use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
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
    pub status_messages: Vec<String>,
    
    // Message receivers
    message_rx: mpsc::UnboundedReceiver<Message>,
    status_rx: mpsc::UnboundedReceiver<String>,
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
            current_channel: None,
            status_messages: Vec::new(),
            
            message_rx,
            status_rx,
        };
        
        // Add startup status message
        app.add_status_message(format!(
            "BitchatX v0.1.0 - Connected as {} ({})",
            app.identity.nickname,
            if nsec.is_some() { "authenticated" } else { "ephemeral" }
        ));
        
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
                        self.input_mode = InputMode::Normal;
                    }
                    KeyCode::Char(c) => {
                        self.input.insert(self.cursor_position, c);
                        self.cursor_position += 1;
                    }
                    KeyCode::Backspace => {
                        if self.cursor_position > 0 {
                            self.input.remove(self.cursor_position - 1);
                            self.cursor_position -= 1;
                        }
                    }
                    KeyCode::Delete => {
                        if self.cursor_position < self.input.len() {
                            self.input.remove(self.cursor_position);
                        }
                    }
                    KeyCode::Left => {
                        if self.cursor_position > 0 {
                            self.cursor_position -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if self.cursor_position < self.input.len() {
                            self.cursor_position += 1;
                        }
                    }
                    KeyCode::Home => {
                        self.cursor_position = 0;
                    }
                    KeyCode::End => {
                        self.cursor_position = self.input.len();
                    }
                    KeyCode::Esc => {
                        self.input.clear();
                        self.cursor_position = 0;
                        self.input_mode = InputMode::Normal;
                    }
                    _ => {}
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
                    self.leave_channel(channel).await?;
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
                self.list_channels().await;
            }
            "all" => {
                self.list_all_channels().await;
            }
            "help" | "h" | "commands" => {
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
        self.channel_manager.leave_channel(geohash).await?;
        self.nostr_client.unsubscribe_from_channel(geohash).await?;
        
        if self.current_channel.as_deref() == Some(geohash) {
            self.current_channel = None;
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
    
    async fn list_channels(&mut self) {
        let channels = self.channel_manager.list_channels().await;
        if channels.is_empty() {
            self.add_status_message("No joined channels".to_string());
        } else {
            self.add_status_message("Joined channels:".to_string());
            for channel in channels {
                let message_count = self.channel_manager.get_message_count(&channel).await;
                let indicator = if Some(&channel) == self.current_channel.as_ref() { "*" } else { " " };
                self.add_status_message(format!("{}#{} ({} messages)", indicator, channel, message_count));
            }
        }
    }
    
    async fn list_all_channels(&mut self) {
        let channels = self.channel_manager.list_all_channels().await;
        if channels.is_empty() {
            self.add_status_message("No channels available".to_string());
        } else {
            self.add_status_message("All channels (joined + listening):".to_string());
            for (channel, is_joined) in channels {
                let message_count = self.channel_manager.get_message_count(&channel).await;
                let indicator = if Some(&channel) == self.current_channel.as_ref() { "*" } else { " " };
                let status = if is_joined { "joined" } else { "listening" };
                self.add_status_message(format!("{}#{} ({} messages, {})", indicator, channel, message_count, status));
            }
        }
    }
    
    async fn show_help(&mut self) {
        let help_text = vec![
            "BitchatX Commands:".to_string(),
            "/join <geohash> - Join a geohash channel".to_string(),
            "/leave - Leave current channel".to_string(),
            "/msg <channel> <message> - Send message to specific channel".to_string(),
            "/nick <nickname> - Change your display name (session only)".to_string(),
            "/list - List joined channels".to_string(),
            "/all - List all channels (joined + listening)".to_string(),
            "/help, /commands - Show this help".to_string(),
            "/quit, /exit - Exit BitchatX".to_string(),
            "".to_string(),
            "".to_string(),
            "Keyboard Commands:".to_string(),
            "i=enter input mode, Esc=exit to normal mode, q=quit (normal mode)".to_string(),
            "Up/Down=scroll messages, Page Up/Down=fast scroll".to_string(),
            "Home/End=cursor start/end, Enter=send (input mode)".to_string(),
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
        self.status_messages.push(format!("[{}] {}", 
            chrono::Local::now().format("%H:%M:%S"), message));
        
        // Keep only last 1000 status messages
        if self.status_messages.len() > 1000 {
            self.status_messages.remove(0);
        }
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
    
    pub fn get_visible_status_messages(&self, height: usize) -> Vec<&String> {
        let start = self.scroll_offset.min(self.status_messages.len().saturating_sub(height));
        let end = (start + height).min(self.status_messages.len());
        self.status_messages[start..end].iter().collect()
    }
}