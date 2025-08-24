#!/bin/bash

# BitchatX Cross-platform Build Script
set -e

echo "🚀 Building BitchatX..."

# Build for current platform
echo "Building for current platform..."
cargo build --release

# Check if cross is installed for cross-compilation
if command -v cross &> /dev/null; then
    echo "📦 Cross-compilation available, building for multiple targets..."
    
    # Add common targets
    echo "Building for Linux (x86_64)..."
    cross build --release --target x86_64-unknown-linux-gnu
    
    echo "Building for Windows (x86_64)..."
    cross build --release --target x86_64-pc-windows-gnu
    
    echo "Building for macOS (x86_64)..."
    cross build --release --target x86_64-apple-darwin
    
    echo "Building for macOS (ARM64)..."
    cross build --release --target aarch64-apple-darwin
    
    echo "✅ Cross-compilation complete!"
    echo "📁 Binaries available in target/<platform>/release/"
else
    echo "⚠️  Cross-compilation not available. Install with: cargo install cross"
    echo "📁 Native binary available in target/release/bitchatx"
fi

echo ""
echo "🎉 BitchatX build complete!"
echo ""
echo "🔧 Usage:"
echo "  ./target/release/bitchatx                    # Run with ephemeral identity"
echo "  ./target/release/bitchatx --nsec nsec1...   # Run with your Nostr key"  
echo "  ./target/release/bitchatx --channel dr5reg   # Auto-join channel"
echo "  ./target/release/bitchatx --help            # Show all options"
echo ""