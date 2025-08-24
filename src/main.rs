use anyhow::Result;
use clap::{Arg, Command};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame, Terminal,
};
use std::{
    io,
    time::{Duration, Instant},
};
use tokio::time::timeout;

mod app;
mod channels;
mod nostr;
mod ui;

use app::App;

const BITCHATX_LOGO: &str = r#"
 █████      ███   █████             █████                 █████    █████ █████
▒▒███      ▒▒▒   ▒▒███             ▒▒███                 ▒▒███    ▒▒███ ▒▒███ 
 ▒███████  ████  ███████    ██████  ▒███████    ██████   ███████   ▒▒███ ███  
 ▒███▒▒███▒▒███ ▒▒▒███▒    ███▒▒███ ▒███▒▒███  ▒▒▒▒▒███ ▒▒▒███▒     ▒▒█████   
 ▒███ ▒███ ▒███   ▒███    ▒███ ▒▒▒  ▒███ ▒███   ███████   ▒███       ███▒███  
 ▒███ ▒███ ▒███   ▒███ ███▒███  ███ ▒███ ▒███  ███▒▒███   ▒███ ███  ███ ▒▒███ 
 ████████  █████  ▒▒█████ ▒▒██████  ████ █████▒▒████████  ▒▒█████  █████ █████
▒▒▒▒▒▒▒▒  ▒▒▒▒▒    ▒▒▒▒▒   ▒▒▒▒▒▒  ▒▒▒▒ ▒▒▒▒▒  ▒▒▒▒▒▒▒▒    ▒▒▒▒▒  ▒▒▒▒▒ ▒▒▒▒▒ 
"#;

fn show_startup_logo() {
    // Clear screen
    print!("\x1B[2J\x1B[1;1H");
    
    // Display ASCII logo with purple gradient
    let lines: Vec<&str> = BITCHATX_LOGO.lines().collect();
    let colors = [
        "\x1B[38;5;55m",  // Purple1
        "\x1B[38;5;93m",  // Purple2 
        "\x1B[38;5;129m", // Purple3
        "\x1B[38;5;165m", // Purple4
        "\x1B[38;5;201m", // Purple5
        "\x1B[38;5;207m", // Purple6
        "\x1B[38;5;213m", // Purple7
        "\x1B[38;5;219m", // Purple8
    ];
    
    for (i, line) in lines.iter().enumerate() {
        if i < colors.len() && !line.trim().is_empty() {
            println!("{}{}\x1B[0m", colors[i], line);
        } else {
            println!("{}", line);
        }
    }
    
    println!("\n\x1B[38;5;129m=== BitchatX v0.1.0 - IRC-style Nostr Client ===\x1B[0m");
    println!("\x1B[38;5;165mInspired by BitchX + Bitchat - Ephemeral Geohash Channels\x1B[0m");
    println!("\x1B[38;5;201mPress any key to continue...\x1B[0m\n");
    
    // Wait for keypress
    let _ = std::io::Read::read(&mut std::io::stdin(), &mut [0u8; 1]);
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Command::new("bitchatx")
        .version("0.1.0")
        .author("BitchatX Team")
        .about("IRC-style Nostr client for ephemeral geohash channels")
        .arg(
            Arg::new("nsec")
                .long("nsec")
                .value_name("NSEC_KEY")
                .help("Login with your Nostr private key (nsec format)")
        )
        .arg(
            Arg::new("channel")
                .short('c')
                .long("channel") 
                .value_name("GEOHASH")
                .help("Auto-join a geohash channel on startup")
        )
        .arg(
            Arg::new("no-logo")
                .long("no-logo")
                .action(clap::ArgAction::SetTrue)
                .help("Skip startup logo animation")
        )
        .get_matches();

    // Show startup logo unless disabled
    if !matches.get_flag("no-logo") {
        show_startup_logo();
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create and run app
    let nsec = matches.get_one::<String>("nsec").map(|s| s.as_str());
    let auto_channel = matches.get_one::<String>("channel").map(|s| s.as_str());
    
    let mut app = App::new(nsec, auto_channel).await?;
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, app: &mut App) -> Result<()> {
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(250);

    loop {
        terminal.draw(|f| {
            // We need to block on the async draw function
            let rt = tokio::runtime::Handle::current();
            rt.block_on(ui::draw(f, app))
        })?;

        let timeout_duration = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout_duration)? {
            let event = event::read()?;
            app.handle_input(event).await?;
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick().await?;
            last_tick = Instant::now();
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
