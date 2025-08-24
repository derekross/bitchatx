# BitchatX Channel System

BitchatX uses **geohash-based channels** for location-based messaging. Unlike traditional IRC, there are **no default channels** - you start with a clean slate.

## Default Channel Behavior

### On Startup
- **No channels joined by default**
- You start in the main status view
- Must manually join channels using `/join <geohash>`

### Status View (No Channel)
When no channels are active:
- Shows connection status and system messages  
- Displays relay connectivity information
- Shows your identity information (nickname, type)
- Cannot send messages (no channel selected)

## Channel Types

### Geohash Channels
- **Based on location coordinates**: Each geohash represents a geographic area
- **Hierarchical precision**: Shorter geohashes = larger areas, longer = smaller areas
- **Examples**:
  - `dr` - Large area (country/state level)
  - `dr5r` - City level  
  - `dr5reg` - Neighborhood level
  - `dr5reg12` - Street level

### Popular Example Channels
```bash
# Major geographic regions
/join dr     # Covers parts of USA (Louisiana, Mississippi region)  
/join u4pr   # San Francisco Bay Area
/join gcpv   # London, UK region
/join w24q   # Sydney, Australia region

# City-level examples  
/join dr5r   # New Orleans area
/join u4pr8  # San Francisco downtown
/join gcpvn  # Central London
```

## Joining Channels

### Manual Join (Recommended)
```bash
# In the application (Input Mode):
/join dr5reg

# Or auto-join on startup:
./bitchatx --channel dr5reg
```

### Finding Geohashes
1. **Use online geohash tools**:
   - http://geohash.org/
   - https://geohash-converter.com/
   
2. **From coordinates**:
   - Enter your lat/lng coordinates
   - Choose precision level (5-8 chars recommended)
   - Use the resulting geohash as channel name

3. **From popular areas**:
   - Look up geohashes for major cities
   - Use shorter codes for broader regional chat

## Channel Management Commands

### Basic Commands
```bash
/join <geohash>     # Join a channel
/leave              # Leave current channel  
/list               # List your active channels
```

### Messaging
```bash
# Send to current channel
Hello everyone!

# Send to specific channel  
/msg dr5r Hey folks in New Orleans!
```

## Channel Interface

### Channel View
When in a channel:
- **Title shows**: `Channel: #dr5reg`
- **Messages display**: Real-time chat from that location
- **Input accepted**: Can send messages to channel participants

### Channel Switching
- Currently: One channel at a time (active channel)
- Use `/join` to switch between channels
- Use `/list` to see all joined channels

## Privacy & Geohashes

### Location Privacy
- **Approximate locations**: Geohashes represent areas, not exact coordinates
- **Precision control**: Choose how specific your location sharing is
- **No GPS required**: Manually specify any geohash

### Examples by Precision:
```
Geohash Length | Approximate Area Size
dr             | ~1200km × 600km  (state/province)
dr5            | ~150km × 150km   (large city area)
dr5r           | ~38km × 19km     (city district)  
dr5re          | ~5km × 5km       (neighborhood)
dr5reg         | ~1km × 1km       (few city blocks)
dr5reg1        | ~150m × 150m     (single block)
```

## Channel Discovery

### Current Implementation
- **Manual discovery**: Users share geohash channels
- **No built-in directory**: No automatic channel list
- **Word of mouth**: Popular channels spread through community

### Future Enhancements
- **Nearby channels**: Show active channels in your area
- **Popular channels**: List most active geohashes
- **Channel directories**: Community-maintained channel lists

## Comparison to Traditional IRC

### Traditional IRC
- **Predefined channels**: #general, #random, etc.
- **Server-specific**: Channels exist on specific servers
- **Topic-based**: Organized around subjects

### BitchatX Geohash Channels
- **Location-based**: Organized around geographic areas
- **No default channels**: Start fresh, join what interests you
- **Global network**: Same channel accessible from all relays
- **Ephemeral by nature**: Aligns with temporary, location-based messaging

## Getting Started

### First Time Users
1. **Start the app**: `./bitchatx`
2. **Find your geohash**: Use http://geohash.org/
3. **Join a channel**: `/join your_geohash`
4. **Start chatting**: Say hello to your local area!

### Example Session
```bash
# Start BitchatX
./bitchatx

# In the app:
/join dr5reg         # Join New Orleans neighborhood  
Hello from downtown! # Send message to channel
/list               # See your active channels
/leave              # Leave the channel
/quit               # Exit app
```

This location-based approach makes BitchatX unique among chat clients, enabling truly local, ephemeral conversations based on physical proximity rather than abstract topics.