use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, AppState, InputMode};

pub fn draw(f: &mut Frame, app: &App) {
    let size = f.size();
    
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
    let title = format!(
        " BitchatX v0.1.0 | {} | #{} | {} ",
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

fn draw_chat_area(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(if let Some(channel) = &app.current_channel {
            format!(" Channel: #{} ", channel)
        } else {
            " BitchatX - Status ".to_string()
        })
        .style(Style::default().fg(Color::Cyan));
    
    let inner = block.inner(area);
    f.render_widget(block, area);
    
    if app.current_channel.is_some() {
        // Show channel messages
        let visible_messages = app.get_visible_messages(inner.height as usize);
        let mut lines = Vec::new();
        
        for message in visible_messages {
            let timestamp = message.timestamp.format("%H:%M:%S");
            let nick_color = if message.is_own { 
                Color::Green 
            } else { 
                Color::Magenta 
            };
            
            let line = Line::from(vec![
                Span::styled(format!("[{}] ", timestamp), Style::default().fg(Color::Gray)),
                Span::styled(format!("<{}> ", message.nickname), Style::default().fg(nick_color)),
                Span::raw(&message.content),
            ]);
            lines.push(line);
        }
        
        let messages_widget = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.scroll_offset as u16, 0));
            
        f.render_widget(messages_widget, inner);
    } else {
        // Show status messages
        let visible_status = app.get_visible_status_messages(inner.height as usize);
        let mut lines = Vec::new();
        
        for status in visible_status {
            lines.push(Line::from(Span::styled(status, Style::default().fg(Color::Yellow))));
        }
        
        let status_widget = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.scroll_offset as u16, 0));
            
        f.render_widget(status_widget, inner);
    }
}

fn draw_info_panel(f: &mut Frame, app: &App, area: Rect) {
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
    
    // Channel list
    let channels_block = Block::default()
        .borders(Borders::ALL)
        .title(" Channels ")
        .style(Style::default().fg(Color::Blue));
        
    let channels = vec!["dr5r", "u4pr", "9q5"]; // Mock channels for now
    let channel_items: Vec<ListItem> = channels
        .iter()
        .map(|channel| {
            let style = if app.current_channel.as_deref() == Some(*channel) {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("#{}", channel)).style(style)
        })
        .collect();
    
    let channels_list = List::new(channel_items)
        .block(channels_block)
        .highlight_style(Style::default().bg(Color::DarkGray));
        
    f.render_widget(channels_list, chunks[2]);
}

fn draw_input_area(f: &mut Frame, app: &App, area: Rect) {
    let input_style = match app.input_mode {
        InputMode::Normal => Style::default().fg(Color::White),
        InputMode::Editing => Style::default().fg(Color::Green),
    };
    
    let mode_indicator = match app.input_mode {
        InputMode::Normal => "[NORMAL] Press 'i' to enter input mode",
        InputMode::Editing => "[INPUT] ESC=normal, ENTER=send",
    };
    
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(mode_indicator)
        .style(input_style);
    
    let input_text = if app.input_mode == InputMode::Editing {
        app.input.as_str()
    } else {
        ""
    };
    
    let input_paragraph = Paragraph::new(input_text)
        .block(input_block)
        .wrap(Wrap { trim: false });
        
    f.render_widget(input_paragraph, area);
    
    // Set cursor position when in editing mode
    if app.input_mode == InputMode::Editing {
        f.set_cursor(
            area.x + app.cursor_position as u16 + 1,
            area.y + 1,
        );
    }
}