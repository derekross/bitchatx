# BitchatX Keyboard Commands

BitchatX uses a modal interface similar to vi/vim, with **Normal Mode** and **Input Mode**.

## Interface Modes

### Normal Mode (Default)
- **Navigation and control mode**
- Used for scrolling, quitting, and entering input mode
- Status shows: `[NORMAL] Press 'i' to enter input mode`

### Input Mode  
- **Text entry mode** for typing messages and commands
- Used for writing messages and IRC commands
- Status shows: `[INPUT] ESC=normal, ENTER=send`

## Keyboard Commands

### Mode Switching
- **`i`** - Enter input mode (from normal mode)
- **`Esc`** - Exit to normal mode (from input mode)

### Application Control
- **`q`** - Quit application (from normal mode only)
- **`Ctrl+C`** - Force quit (emergency exit)

### Message Navigation (Normal Mode)
- **`↑` (Up Arrow)** - Scroll up through messages
- **`↓` (Down Arrow)** - Scroll down through messages  
- **`Page Up`** - Scroll up quickly (10 messages)
- **`Page Down`** - Scroll down quickly (10 messages)

### Text Input (Input Mode)
- **`Enter`** - Send message or execute command
- **`←` (Left Arrow)** - Move cursor left
- **`→` (Right Arrow)** - Move cursor right
- **`Home`** - Move cursor to beginning of line
- **`End`** - Move cursor to end of line
- **`Backspace`** - Delete character before cursor
- **`Delete`** - Delete character after cursor

## Usage Flow

### Typical Session
```
1. Start in Normal Mode
2. Press 'i' to enter Input Mode  
3. Type message or command
4. Press Enter to send
5. Automatically return to Normal Mode
6. Use arrows to scroll through messages
7. Press 'q' to quit
```

### Sending a Message
```
[Normal Mode] → Press 'i' → [Input Mode] → Type "Hello!" → Press Enter → [Normal Mode]
```

### Executing Commands
```
[Normal Mode] → Press 'i' → [Input Mode] → Type "/join dr5reg" → Press Enter → [Normal Mode]
```

## Visual Indicators

### Title Bar
Shows current status:
- `BitchatX v0.1.0 | nickname | #channel | connected`

### Input Area
Shows current mode:
- `[NORMAL] Press 'i' to enter input mode`
- `[INPUT] ESC=normal, ENTER=send`

### Cursor
- **Normal Mode**: No visible cursor
- **Input Mode**: Cursor visible at current position

## Command Line vs In-App Commands

### Startup Commands (Before Launch)
```bash
./bitchatx --nsec nsec1...      # Login with Nostr key
./bitchatx --channel dr5reg     # Auto-join channel
./bitchatx --no-logo            # Skip startup animation
```

### In-App Commands (After Launch, in Input Mode)
```
/join dr5reg                    # Join channel
/nick cyberpunk                 # Change nickname  
/list                          # List channels
/help                          # Show help
/quit                          # Exit app
```

## Tips & Tricks

### Quick Exit
- **From Normal Mode**: Press `q` (fastest)
- **From Input Mode**: Press `Esc` then `q`
- **From Anywhere**: Type `/quit` or `/exit`

### Efficient Navigation
- Use `Page Up`/`Page Down` for fast scrolling
- Use arrow keys for precise navigation
- Stay in Normal Mode for reading, switch to Input Mode only for typing

### Command History
- Currently no command history (potential future feature)
- Use `/help` to see all available commands

### Copy/Paste
- **Copy**: Depends on your terminal (usually Ctrl+Shift+C)
- **Paste**: Depends on your terminal (usually Ctrl+Shift+V)
- Text appears in input field when pasted

## Comparison to Other Clients

### Similar to IRC Clients
- Modal input (like some IRC clients)
- `/commands` for actions
- Channel-based messaging

### Similar to Terminal Editors  
- Normal/Input mode switching (like vi/vim)
- `i` to insert, `Esc` to exit input
- Keyboard-driven navigation

### BitchatX Specific
- Location-based channels (geohashes)
- Ephemeral identity by default
- Modern terminal UI with real-time updates

## Troubleshooting

### "Nothing happens when I type"
- You're in Normal Mode
- Press `i` to enter Input Mode

### "Can't scroll through messages"
- You're in Input Mode  
- Press `Esc` to enter Normal Mode

### "Can't quit with 'q'"
- You're in Input Mode
- Press `Esc` first, then `q`
- Or type `/quit` and press Enter

### "Cursor not visible"
- Normal behavior in Normal Mode
- Press `i` to see cursor in Input Mode

This modal design keeps the interface clean and prevents accidental commands while allowing efficient navigation and text entry.