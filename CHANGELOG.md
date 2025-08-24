# BitchatX Changelog

## v0.1.0 - Initial Release (2025-08-24)

### ✨ Features
- **Beautiful ANSI Startup Logo** - Purple gradient "BitchatX" logo inspired by the original Bitchat logo
- **Ephemeral Identity System** - Generate cryptographic identities per session or use nsec keys
- **Geohash Channel System** - `/join dr5reg` style commands for location-based channels
- **Full IRC Command Set**:
  - `/join <geohash>` - Join geohash channels
  - `/leave` - Leave current channel  
  - `/msg <channel> <message>` - Send message to specific channel
  - `/nick <nickname>` - Change your nickname
  - `/list` - List active channels
  - `/help`, `/commands` - Show command help
  - `/quit`, `/exit` - Exit application
- **BitchX-Style Terminal Interface**:
  - 3-panel layout (title bar, chat area, input area)
  - Real-time message display with timestamps
  - Identity & connection status panel
  - IRC-style color coding and formatting
- **Nostr Protocol Integration**:
  - Event kind 20000 (ephemeral events) for messaging
  - Multi-relay support with automatic failover
  - Geohash-based channel tagging (`g` tags)
  - Nickname tagging (`n` tags)
  - Message publishing & subscription

### 🏗️ Architecture
- **Rust** - Memory-safe systems programming language
- **Ratatui** - Modern terminal user interface framework
- **Nostr-SDK** - Robust Nostr protocol implementation
- **Crossterm** - Cross-platform terminal manipulation
- **Tokio** - Async runtime for concurrent message handling

### 🚀 Platform Support
- Linux (x86_64)
- Windows (x86_64) 
- macOS (x86_64, ARM64)

### 📦 CLI Features
- `--nsec` - Login with your Nostr private key
- `--channel` - Auto-join a channel on startup
- `--no-logo` - Skip the beautiful startup animation
- `--help` - Show usage information

### 🎯 Inspired By
- **BitchX** - Classic 1990s IRC client with advanced terminal interface
- **Bitchat** - Modern ephemeral messaging using Nostr protocol and geohash channels

### 🔧 Build System
- Native Rust compilation with `cargo build --release`
- Cross-platform build script (`build.sh`) with `cross` support
- Optimized release builds with LTO and panic=abort

### 📚 Documentation
- Comprehensive README with installation and usage instructions
- Built-in help system accessible via `/help` command
- CLI help via `--help` flag