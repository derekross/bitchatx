# BitchatX Login Methods

BitchatX supports two ways to use the application: **Ephemeral Mode** (default) and **Nostr Identity Mode** using your nsec private key.

## Ephemeral Mode (Default)

**No login required** - just run the application:

```bash
# Start with a fresh ephemeral identity
./bitchatx
```

**What happens:**
- New cryptographic keypair generated automatically
- Random cyberpunk-style nickname assigned (e.g., `cyberdaemon1337`)
- Complete anonymity - no persistent identity
- Identity discarded when you exit

## Nostr Identity Mode (nsec)

**Login with your Nostr private key** via command line:

```bash
# Login with your nsec key
./bitchatx --nsec nsec1your_private_key_here_64_characters_long

# You can also combine with other options
./bitchatx --nsec nsec1abc123... --channel dr5reg --no-logo
```

**What happens:**
- Uses your existing Nostr identity
- Your public key becomes your persistent identity
- Default nickname format: `user{first8chars_of_pubkey}`
- Same identity every time you use that nsec
- Messages cryptographically signed by your key

## Command Line Examples

### Basic Usage
```bash
# Ephemeral mode
./bitchatx

# Nostr mode
./bitchatx --nsec nsec1qpwxyz789abcdef123456789abcdef123456789abcdef123456789abcdef12
```

### Advanced Usage
```bash
# Login with nsec and auto-join channel
./bitchatx --nsec nsec1abc123... --channel dr5reg

# Skip startup logo
./bitchatx --nsec nsec1abc123... --no-logo

# All options combined
./bitchatx --nsec nsec1abc123... --channel u4pr --no-logo
```

## Security Considerations

### For Ephemeral Mode
- ✅ **Maximum privacy** - no persistent identity
- ✅ **No key management** - nothing to store or backup
- ✅ **Fresh start** - completely anonymous each session
- ❌ **No persistence** - can't maintain identity across sessions

### For Nostr Mode
- ✅ **Persistent identity** - same identity across sessions
- ✅ **Cryptographic authenticity** - messages provably from you
- ✅ **Cross-app compatibility** - works with other Nostr apps
- ❌ **Privacy trade-off** - messages linkable to your pubkey
- ⚠️ **Key responsibility** - you must secure your nsec

## Finding Your nsec

If you don't have an nsec yet, you can:

1. **Generate a new one** using Nostr tools:
   - Use a Nostr client like Damus, Amethyst, or Nostrudel
   - Use command-line tools like `nostril` or `nak`
   - Online generators (⚠️ only for testing - not secure for real use)

2. **Extract from existing Nostr client**:
   - Most Nostr clients show your nsec in settings/profile
   - Look for "Private Key", "Secret Key", or "nsec"

## nsec Format

Valid nsec keys:
- Start with `nsec1`
- Are exactly 63 characters long
- Contain only lowercase letters and numbers (bech32 format)
- Example: `nsec1qpwxyz789abcdef123456789abcdef123456789abcdef123456789abcdef12`

## Interactive vs Command Line

**Current Implementation**: Command line only
```bash
./bitchatx --nsec nsec1...
```

**Potential Future Enhancement**: Interactive login prompt
```
BitchatX Login Options:
1. Ephemeral mode (default)
2. Login with nsec
Enter choice [1]:
```

## Shell Scripting & Aliases

You can create convenient aliases:

```bash
# Add to your ~/.bashrc or ~/.zshrc
alias bitchatx-me='bitchatx --nsec nsec1your_key_here'
alias bitchatx-anon='bitchatx'

# Then use:
bitchatx-me --channel dr5reg
bitchatx-anon
```

## Environment Variables (Future Enhancement)

Could potentially support:
```bash
export BITCHATX_NSEC=nsec1your_key_here
./bitchatx  # Would automatically use the env var
```

## Comparison with Other Clients

### Similar to:
- **SSH keys**: `ssh -i ~/.ssh/id_rsa user@server`
- **Git**: `git config user.signingkey ABC123`
- **GPG**: `gpg --local-user your@email.com`

### BitchatX approach:
- **Ephemeral by default**: Privacy-first design
- **Optional persistence**: Opt-in to persistent identity
- **Command line driven**: Unix-style tool philosophy

This design ensures maximum privacy by default while allowing users to opt into persistent identity when desired.