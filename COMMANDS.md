# BitchatX IRC Commands Reference

BitchatX supports familiar IRC-style commands for managing channels and messaging:

## Channel Commands
- `/join <geohash>` - Join a geohash-based channel (e.g., `/join dr5reg`)
- `/leave` - Leave the current channel
- `/list` - List all active channels you've joined

## Messaging Commands  
- `/msg <channel> <message>` - Send a message to a specific channel
- `/nick <nickname>` - Change your display name (session only, doesn't update Nostr profile)

## Information Commands
- `/help` - Show command help and usage information
- `/commands` - Alias for `/help` (show all available commands)

## Application Commands
- `/quit` - Exit BitchatX
- `/exit` - Alternative command to quit BitchatX

## Key Bindings
- `i` - Enter input/editing mode to type messages or commands
- `Esc` - Exit input mode and return to normal navigation mode
- `q` - Quit application (when in normal mode)
- `Up/Down Arrow Keys` - Scroll through message history
- `Page Up/Page Down` - Fast scroll through messages
- `Home` - Move cursor to beginning of input
- `End` - Move cursor to end of input

## Usage Examples

```
# Join a geohash channel
/join dr5reg

# Send a message to current channel
Hello everyone in this location!

# Send a message to specific channel
/msg u4pr Hey folks in San Francisco!

# Change your nickname
/nick cyberpunk2025

# List your active channels
/list

# Get help
/help

# Exit the application
/exit
```

## Notes
- Geohash channels represent geographical locations
- Messages use Nostr ephemeral events (kind 20000) 
- Your identity can be ephemeral (new each session) or persistent (using --nsec)
- All commands are case-insensitive
- Commands can often be shortened (e.g., `/h` instead of `/help`)