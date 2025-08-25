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

fn main() {
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
    
    let version = env!("CARGO_PKG_VERSION");
    println!("\n\x1B[38;5;129m=== BitchatX v{} - IRC-style Nostr Client ===\x1B[0m", version);
    println!("\x1B[38;5;165mInspired by BitchX + Bitchat - Ephemeral Geohash Channels\x1B[0m");
    println!("\x1B[38;5;201mFixed logo to match logo.sh style!\x1B[0m\n");
}