#!/bin/bash

# BitchatX Cross-platform Build Script
set -e

echo "🚀 Building BitchatX..."

# Build for current platform
echo "Building for current platform..."
cargo build --release

# Try cross-compilation for supported targets
echo "📦 Attempting cross-compilation..."

# Install Windows target
rustup target add x86_64-pc-windows-gnu 2>/dev/null || true

echo "Building for Windows (x86_64)..."
if cargo build --release --target x86_64-pc-windows-gnu 2>/dev/null; then
    echo "✅ Windows build successful"
else
    echo "⚠️  Windows build failed - install mingw-w64:"
    echo "    sudo apt install gcc-mingw-w64-x86-64"
fi

echo ""
echo "ℹ️  macOS builds skipped (require macOS SDK on Apple hardware)"
echo "   For macOS binaries, build on a Mac with: cargo build --release"

echo "📁 Available binaries in target/*/release/"

echo ""
echo "🎉 BitchatX build complete!"
echo ""
echo "🔧 Usage:"
echo "  ./target/release/bitchatx                    # Run with ephemeral identity"
echo "  ./target/release/bitchatx --nsec nsec1...   # Run with your Nostr key"  
echo "  ./target/release/bitchatx --channel dr5reg   # Auto-join channel"
echo "  ./target/release/bitchatx --help            # Show all options"
echo ""