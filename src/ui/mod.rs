use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, AppState, InputMode};

pub fn draw(f: &mut Frame<'_>, app: &mut App) {
    let size = f.size();
    
    // Clear clickable regions for this frame
    app.clickable_regions.clear();
    
    // Create main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Input area
        ])
        .split(size);
    
    // Draw title bar
    draw_title_bar(f, app, chunks[0]);
    
    // Draw main content
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(75), // Chat area
            Constraint::Percentage(25), // Status/info panel
        ])
        .split(chunks[1]);
        
    draw_chat_area(f, app, main_chunks[0]);
    draw_info_panel(f, app, main_chunks[1]);
    
    // Draw input area
    draw_input_area(f, app, chunks[2]);
}

fn draw_title_bar(f: &mut Frame, app: &App, area: Rect) {
    let title_style = match app.state {
        AppState::Connected => Style::default().fg(Color::Green),
        AppState::Connecting => Style::default().fg(Color::Yellow),
        AppState::Disconnected => Style::default().fg(Color::Red),
        AppState::Error(_) => Style::default().fg(Color::Red),
    };
    
    let current_channel = app.current_channel.as_deref().unwrap_or("no channel");
    let version = env!("CARGO_PKG_VERSION");
    let title = format!(
        " BitchatX v{} | {} | #{} | {} ",
        version,
        app.identity.nickname,
        current_channel,
        match &app.state {
            AppState::Connected => "connected",
            AppState::Connecting => "connecting...",
            AppState::Disconnected => "disconnected",
            AppState::Error(e) => e,
        }
    );
    
    let title_block = Block::default()
        .borders(Borders::ALL)
        .style(title_style)
        .title(" BitchatX ");
        
    let title_paragraph = Paragraph::new(title)
        .block(title_block)
        .alignment(Alignment::Center);
        
    f.render_widget(title_paragraph, area);
}

fn draw_chat_area(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(if let Some(channel) = &app.current_channel {
            if channel == "system" {
                " System Messages ".to_string()
            } else {
                format!(" Channel: #{} ", channel)
            }
        } else {
            " BitchatX - Status ".to_string()
        })
        .style(Style::default().fg(Color::Cyan));
    
    let inner = block.inner(area);
    f.render_widget(block, area);
    
    let viewport_height = inner.height as usize;
    
    let mut lines = Vec::new();
    
    if app.current_channel.is_some() {
        // Show channel messages with automatic scroll handling
        let (visible_messages, effective_scroll_offset) = app.get_visible_messages(viewport_height);
        
        // Always update the app's scroll offset to match what's being displayed
        app.update_scroll_offset(effective_scroll_offset);
        
        // Update autoscroll status with actual viewport height for better accuracy
        app.update_autoscroll_status_with_height(viewport_height);
        
        for (timestamp, nickname, content, is_own, pubkey) in visible_messages {
            let nick_color = if is_own { 
                Color::Green 
            } else { 
                Color::Magenta 
            };
            
            let display_nickname = app.format_display_nickname(&nickname, &pubkey);
            
            let mut message_spans = vec![
                Span::styled(format!("[{}] ", timestamp), Style::default().fg(Color::Gray)),
                Span::styled(format!("<{}> ", display_nickname), Style::default().fg(nick_color)),
            ];
            
            // Parse markdown formatting and track nostr URIs
            let (content_spans, nostr_uris) = parse_markdown_with_tracking(&content);
            message_spans.extend(content_spans);
            
            // Track clickable regions for nostr URIs in this message
            let base_y = inner.y + lines.len() as u16;
            let prefix_text = format!("[{}] <{}> ", timestamp, display_nickname);
            let available_width = inner.width as usize;
            
            // Calculate the actual rendered position of each nostr URI accounting for wrapping
            for nostr_uri in nostr_uris {
                let regions = calculate_wrapped_regions(
                    &prefix_text,
                    &content,
                    &nostr_uri,
                    available_width,
                    inner.x,
                    base_y,
                );
                
                for region in regions {
                    app.clickable_regions.push(region);
                }
            }
            
            let line = Line::from(message_spans);
            lines.push(line);
        }
    }
    
    // Status messages are now handled in the system channel
    // No need to show them separately here
    
    // Show hint if no messages at all
    if lines.is_empty() {
        let hint_text = if app.current_channel.is_some() {
            "No messages in this channel yet. Type a message and press Enter to send."
        } else {
            "Not in a channel. Use /join <geohash> to join a channel, or /help for commands."
        };
        lines.push(Line::from(Span::styled(
            hint_text,
            Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC),
        )));
    }
    
    let messages_widget = Paragraph::new(lines)
        .wrap(Wrap { trim: false });
        
    f.render_widget(messages_widget, inner);
}

fn draw_info_panel(f: &mut Frame<'_>, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Identity info
            Constraint::Length(6),  // Connection info  
            Constraint::Min(0),     // Channel list
        ])
        .split(area);
    
    // Identity info
    let identity_block = Block::default()
        .borders(Borders::ALL)
        .title(" Identity ")
        .style(Style::default().fg(Color::Blue));
        
    let identity_text = vec![
        Line::from(vec![
            Span::raw("Nick: "),
            Span::styled(&app.identity.nickname, Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::raw("Type: "),
            Span::styled(
                if app.identity.is_ephemeral { "ephemeral" } else { "authenticated" },
                Style::default().fg(if app.identity.is_ephemeral { Color::Yellow } else { Color::Green })
            ),
        ]),
        Line::from(vec![
            Span::raw("Pubkey: "),
            Span::styled(&app.identity.pubkey[..16], Style::default().fg(Color::Gray)),
            Span::styled("...", Style::default().fg(Color::Gray)),
        ]),
    ];
    
    let identity_paragraph = Paragraph::new(identity_text).block(identity_block);
    f.render_widget(identity_paragraph, chunks[0]);
    
    // Connection info
    let connection_block = Block::default()
        .borders(Borders::ALL)
        .title(" Connection ")
        .style(Style::default().fg(Color::Blue));
        
    let relay_count = app.nostr_client.get_relay_count();
    let connection_text = vec![
        Line::from(vec![
            Span::raw("Status: "),
            Span::styled(
                match app.state {
                    AppState::Connected => "Connected",
                    AppState::Connecting => "Connecting",
                    AppState::Disconnected => "Disconnected", 
                    AppState::Error(_) => "Error",
                },
                match app.state {
                    AppState::Connected => Style::default().fg(Color::Green),
                    AppState::Connecting => Style::default().fg(Color::Yellow),
                    _ => Style::default().fg(Color::Red),
                }
            ),
        ]),
        Line::from(vec![
            Span::raw("Relays: "),
            Span::styled(format!("{}", relay_count), Style::default().fg(Color::Cyan)),
        ]),
    ];
    
    let connection_paragraph = Paragraph::new(connection_text).block(connection_block);
    f.render_widget(connection_paragraph, chunks[1]);
    
    // Channel list - show system channel, joined channels, and channels with messages
    let channels_block = Block::default()
        .borders(Borders::ALL)
        .title(" Channels ")
        .style(Style::default().fg(Color::Blue));
        
    let mut all_channels = Vec::new();
    
    // Always show system channel first
    let system_style = if app.current_channel.as_deref() == Some("system") {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };
    all_channels.push(ListItem::new("system").style(system_style));
    
    // Add all channels with messages (both joined and listening-only)
    let all_channel_info = app.channel_manager.list_all_channels();
    for (channel, is_joined) in all_channel_info {
        if channel != "system" {  // Don't duplicate system channel
            if channel.starts_with("dm:") {
                // This is a private message channel
                let pubkey = &channel[3..]; // Remove "dm:" prefix
                if let Some(nickname) = app.private_chats.get(pubkey) {
                    let style = if app.current_channel.as_deref() == Some(&channel) {
                        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Magenta)
                    };
                    
                    let dm_label = format!("@{}", nickname);
                    all_channels.push(ListItem::new(dm_label).style(style));
                }
            } else {
                // Regular geohash channel
                let style = if app.current_channel.as_deref() == Some(&channel) {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else if is_joined {
                    Style::default().fg(Color::White)
                } else {
                    Style::default().fg(Color::Gray)  // Different color for listening-only channels
                };
                
                let active_users = app.channel_manager.get_active_user_count(&channel);
                let channel_label = if is_joined {
                    format!("#{} ({})", channel, active_users)
                } else {
                    format!("#{} ({})", channel, active_users)  // Show active user count for all channels
                };
                all_channels.push(ListItem::new(channel_label).style(style));
            }
        }
    }
    
    let channels_list = if all_channels.is_empty() {
        List::new(vec![ListItem::new("No channels").style(Style::default().fg(Color::Gray))])
    } else {
        List::new(all_channels)
    }
        .block(channels_block)
        .highlight_style(Style::default().bg(Color::DarkGray));
        
    f.render_widget(channels_list, chunks[2]);
}

fn draw_input_area(f: &mut Frame, app: &mut App, area: Rect) {
    let input_style = match app.input_mode {
        InputMode::Normal => Style::default().fg(Color::White),
        InputMode::Editing => Style::default().fg(Color::Green),
    };
    
    let mode_indicator = match app.input_mode {
        InputMode::Normal => "[NORMAL] Press 'i' to enter input mode".to_string(),
        InputMode::Editing => {
            if let Some(ref state) = app.tab_completion_state {
                format!("[INPUT] TAB completion: {} ({}/{})", 
                    state.matches[state.current_match_index],
                    state.current_match_index + 1, 
                    state.matches.len())
            } else {
                "[INPUT] ESC=normal, ENTER=send, TAB=complete".to_string()
            }
        }
    };
    
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(mode_indicator)
        .style(input_style);
    
    // Calculate inner area before consuming input_block
    let inner_area = input_block.inner(area);
    
    // Update scroll offset based on actual available width
    if app.input_mode == InputMode::Editing {
        app.update_input_scroll_with_width(inner_area.width as usize);
    }
    
    let input_text = if app.input_mode == InputMode::Editing {
        let text = app.input.as_str();
        let scroll_start = app.input_horizontal_scroll;
        
        // Truncate text to show only the visible portion
        if scroll_start < text.len() {
            let visible_width = inner_area.width as usize;
            let remaining_text = &text[scroll_start..];
            if remaining_text.len() > visible_width {
                &remaining_text[..visible_width]
            } else {
                remaining_text
            }
        } else {
            ""
        }
    } else {
        ""
    };
    
    let input_paragraph = Paragraph::new(input_text)
        .block(input_block);
        
    f.render_widget(input_paragraph, area);
    
    // Set cursor position when in editing mode with horizontal scrolling
    if app.input_mode == InputMode::Editing {
        // Calculate visible cursor position accounting for horizontal scroll
        let cursor_x = (app.cursor_position as i16 - app.input_horizontal_scroll as i16).max(0) as u16;
        let cursor_y = 0; // First line of inner area (0-indexed)
        
        // Ensure cursor stays within inner area bounds
        let max_x = inner_area.width.saturating_sub(1);
        let cursor_x = cursor_x.min(max_x);
        
        f.set_cursor(
            inner_area.x + cursor_x,
            inner_area.y + cursor_y,
        );
    }
}


/// Parse markdown formatting and track nostr URIs, returning both spans and found URIs
fn parse_markdown_with_tracking(text: &str) -> (Vec<Span<'static>>, Vec<String>) {
    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut nostr_uris = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
    
    while i < chars.len() {
        if chars[i] == '*' {
            // Handle markdown formatting
            if i + 1 < chars.len() && chars[i + 1] == '*' {
                // Handle **bold**
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }
                
                if let Some(end_pos) = find_closing_bold(&chars[i + 2..]) {
                    let bold_text: String = chars[i + 2..i + 2 + end_pos].iter().collect();
                    spans.push(Span::styled(
                        bold_text,
                        Style::default().add_modifier(Modifier::BOLD)
                    ));
                    i += 4 + end_pos; // Skip past **text**
                } else {
                    current_text.push_str("**");
                    i += 2;
                }
            } else {
                // Handle *italic*
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }
                
                if let Some(end_pos) = find_closing_italic(&chars[i + 1..]) {
                    let italic_text: String = chars[i + 1..i + 1 + end_pos].iter().collect();
                    spans.push(Span::styled(
                        italic_text,
                        Style::default().add_modifier(Modifier::ITALIC)
                    ));
                    i += 2 + end_pos; // Skip past *text*
                } else {
                    current_text.push('*');
                    i += 1;
                }
            }
        } else if i + 6 <= chars.len() && chars[i..i + 6].iter().collect::<String>() == "nostr:" {
            // Handle nostr: URIs
            if !current_text.is_empty() {
                spans.push(Span::raw(current_text.clone()));
                current_text.clear();
            }
            
            // Find the end of the nostr URI (space or end of string)
            let mut uri_end = i + 6;
            while uri_end < chars.len() && !chars[uri_end].is_whitespace() {
                uri_end += 1;
            }
            
            let nostr_uri: String = chars[i..uri_end].iter().collect();
            
            // Store this nostr URI for tracking
            nostr_uris.push(nostr_uri.clone());
            
            // Create a clickable link span with cyan color and underline
            spans.push(Span::styled(
                nostr_uri,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::UNDERLINED)
            ));
            
            i = uri_end;
        } else {
            current_text.push(chars[i]);
            i += 1;
        }
    }
    
    // Add any remaining text
    if !current_text.is_empty() {
        spans.push(Span::raw(current_text));
    }
    
    // If no spans were created, return the original text as a single span
    if spans.is_empty() {
        spans.push(Span::raw(text.to_string()));
    }
    
    (spans, nostr_uris)
}

/// Find the position of closing ** for bold text
fn find_closing_bold(chars: &[char]) -> Option<usize> {
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '*' && chars[i + 1] == '*' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find the position of closing * for italic text  
fn find_closing_italic(chars: &[char]) -> Option<usize> {
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '*' {
            return Some(i);
        }
    }
    None
}

/// Calculate clickable regions for a nostr URI that may wrap across multiple lines
/// This simulates ratatui's text wrapping behavior more accurately
fn calculate_wrapped_regions(
    prefix: &str,
    content: &str,
    nostr_uri: &str,
    available_width: usize,
    base_x: u16,
    base_y: u16,
) -> Vec<crate::app::ClickableRegion> {
    let mut regions = Vec::new();
    
    // Find where the nostr URI starts in the content
    let uri_start = match content.find(nostr_uri) {
        Some(start) => start,
        None => return regions,
    };
    
    // Create the full text that would be rendered (prefix + content)
    let full_text = format!("{}{}", prefix, content);
    let uri_start_in_full = prefix.len() + uri_start;
    let uri_end_in_full = uri_start_in_full + nostr_uri.len();
    
    // Simulate ratatui's text wrapping behavior
    let mut current_line = 0u16;
    let mut current_pos = 0usize;
    let chars: Vec<char> = full_text.chars().collect();
    
    while current_pos < chars.len() && current_line < 100 {
        // Determine how many characters fit on this line
        let chars_that_fit = if current_pos + available_width > chars.len() {
            chars.len() - current_pos
        } else {
            available_width
        };
        
        let line_end = current_pos + chars_that_fit;
        
        // Check if any part of the URI is on this line
        if current_pos < uri_end_in_full && uri_start_in_full < line_end {
            // Calculate the intersection of this line with the URI
            let uri_start_on_line = uri_start_in_full.max(current_pos);
            let uri_end_on_line = uri_end_in_full.min(line_end);
            
            if uri_start_on_line < uri_end_on_line {
                let x_offset = uri_start_on_line - current_pos;
                let width = uri_end_on_line - uri_start_on_line;
                
                // Only create a region if the width is reasonable (not extending beyond line)
                let max_width_on_line = available_width.saturating_sub(x_offset);
                let actual_width = width.min(max_width_on_line);
                
                if actual_width > 0 {
                    regions.push(crate::app::ClickableRegion {
                        x: base_x + x_offset as u16,
                        y: base_y + current_line,
                        width: actual_width as u16,
                        nostr_uri: nostr_uri.to_string(),
                    });
                }
            }
        }
        
        // Move to next line - ratatui will break at character boundaries for long words
        current_pos = line_end;
        current_line += 1;
        
        // Break if we've processed all characters
        if current_pos >= chars.len() {
            break;
        }
    }
    
    regions
}