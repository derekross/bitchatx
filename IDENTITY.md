# BitchatX Identity System

BitchatX supports two types of identity management: **Ephemeral Identities** and **Persistent Identities** (nsec-based).

## Ephemeral Identities (Default)

### How They Work
- **New keypair per session**: A fresh cryptographic keypair is generated each time you start BitchatX
- **Random nickname**: Automatically assigned a unique nickname like `cyberdaemon1337`, `shadowbyte2048`, etc.
- **Session-only**: The identity is completely discarded when the session ends
- **No profile persistence**: The identity doesn't exist on the Nostr network permanently

### Benefits
- **Maximum privacy**: No persistent identity across sessions
- **No account management**: No keys to store or backup
- **Fresh start**: Each session is completely anonymous
- **Perfect for temporary chats**: Ideal for location-based ephemeral messaging

### Lifecycle
```
Start BitchatX → Generate new keypair → Create random nickname → Use for session → Exit → Identity discarded
```

## Persistent Identities (nsec-based)

### How They Work
- **Your Nostr keys**: Uses your existing Nostr private key (nsec format)
- **Persistent across sessions**: Same identity every time you use that nsec
- **Your Nostr profile**: Could theoretically use your actual Nostr profile data
- **Long-term identity**: Messages are cryptographically linked to your key

### Usage
```bash
# Use your Nostr identity
./bitchatx --nsec nsec1your_private_key_here
```

### Current Implementation
- Default nickname format: `user{first8chars_of_pubkey}`
- Could be enhanced to fetch actual profile name/display name from Nostr

## The `/nick` Command Behavior

### What `/nick` Does
The `/nick` command **only changes your display name locally** within the current session:

```
/nick cyberpunk2025
```

### What `/nick` Does NOT Do
- **Does NOT update your Nostr profile** (kind 0 metadata event)
- **Does NOT persist** across sessions (even with nsec identities)
- **Does NOT change your cryptographic identity** (pubkey stays the same)

### Technical Details

#### For Ephemeral Identities:
1. Changes the local nickname field in the Identity struct
2. All future messages use the new nickname in the `n` tag
3. Only affects current session - discarded on exit
4. No Nostr profile events are published

#### For nsec Identities:
1. Changes the local nickname field (same as ephemeral)
2. Messages still come from your persistent pubkey
3. Nickname change is session-only, doesn't update your actual Nostr profile
4. Your real Nostr profile remains unchanged

## Message Tagging System

When you send messages, BitchatX includes:

```
Event Kind: 20000 (ephemeral)
Tags:
  - g: <geohash>        # Geographic channel
  - n: <your_nickname>  # Display name (changeable via /nick)
  - t: bitchatx         # Topic/app identifier  
  - client: bitchatx    # Client identifier
```

## Privacy Implications

### Ephemeral Mode (Default)
- **Maximum privacy**: New identity each session
- **No linkability**: Messages from different sessions can't be linked
- **Truly ephemeral**: Aligns with Nostr event kind 20000 philosophy
- **No metadata leakage**: No persistent profile to analyze

### nsec Mode
- **Persistent identity**: All sessions linkable to your pubkey
- **Profile correlation**: Could be linked to your main Nostr activity
- **Key responsibility**: You must secure and backup your nsec
- **Cross-app visibility**: Other Nostr apps can see it's you

## Future Enhancements

### Potential Features
1. **Profile Integration**: Fetch actual Nostr profile name for nsec users
2. **Nickname Persistence**: Remember preferred nicknames per pubkey
3. **Profile Publishing**: Option to update Nostr profile via `/nick` (with confirmation)
4. **Temporary nsec**: Generate session nsecs that could be optionally saved

### Bitchat-Android Compatibility
BitchatX's ephemeral system is designed to be compatible with bitchat-android's approach:
- Same event kind (20000)
- Same tag structure (`g`, `n` tags)
- Same relay infrastructure
- Compatible message format

## Recommendations

### For Maximum Privacy
Use default ephemeral mode - no additional options needed.

### For Persistent Identity
```bash
./bitchatx --nsec nsec1...
```

### For Temporary Named Identity
1. Start in ephemeral mode
2. Use `/nick desired_name` to set preferred display name
3. Identity is discarded on exit but you controlled the display name

This system gives users complete control over their privacy/persistence trade-off while maintaining compatibility with the broader Bitchat ecosystem.