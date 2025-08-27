# Building BitchatX

BitchatX can be built for multiple platforms using different methods.

## Quick Start (Local)

```bash
# Build for current platform
cargo build --release

# Or use the build script
chmod +x build.sh
./build.sh
```

## Cross-Platform Builds

### GitHub Actions (Recommended)

The project includes GitHub Actions workflows that automatically build for all supported platforms:

- **Linux x86_64** (`x86_64-unknown-linux-gnu`)
- **Windows x86_64** (`x86_64-pc-windows-gnu`) 
- **macOS x86_64** (`x86_64-apple-darwin`) - Intel Macs
- **macOS ARM64** (`aarch64-apple-darwin`) - Apple Silicon Macs

Builds are triggered on:
- Push to `main` branch
- Pull requests to `main` branch  
- Git tags starting with `v*` (creates releases)

### Manual Cross-Compilation

#### Windows (from Linux)
```bash
# Install Windows toolchain
sudo apt install gcc-mingw-w64-x86-64

# Add target and build
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

#### macOS
macOS cross-compilation requires the macOS SDK and is best done on actual Mac hardware or through GitHub Actions.

## Build Artifacts

### Local Builds
- Native binary: `target/release/bitchatx` (or `.exe` on Windows)
- Cross-compiled: `target/<target>/release/bitchatx`

### GitHub Actions
- Artifacts are uploaded for each platform as `.tar.gz` files
- Tagged releases automatically create GitHub releases with all platform binaries

## Platform Support

| Platform | Architecture | Status | Notes |
|----------|--------------|--------|-------|
| Linux    | x86_64      | ✅ Supported | Native and cross-compile |
| Windows  | x86_64      | ✅ Supported | Cross-compile with mingw-w64 |
| macOS    | x86_64      | ✅ Supported | GitHub Actions only |
| macOS    | ARM64       | ✅ Supported | GitHub Actions only |

## Dependencies

### Runtime Dependencies
- None! BitchatX is statically linked and self-contained.

### Build Dependencies
- Rust 1.70+ (latest stable recommended)
- For Windows cross-compilation: `gcc-mingw-w64-x86-64`
- For macOS: macOS SDK (only available on macOS)

## Release Process

1. Tag a release: `git tag v0.5.0 && git push origin v0.5.0`
2. GitHub Actions automatically builds all platforms
3. Creates a GitHub release with all binaries attached
4. Users can download platform-specific binaries directly

## Troubleshooting

### Cross-compilation Issues
- **Windows**: Install `gcc-mingw-w64-x86-64` package
- **macOS**: Use GitHub Actions - local cross-compilation not supported
- **Link errors**: Update Rust to latest stable version

### Build Speed
- Use `cargo build --release` for optimized binaries
- Enable `sccache` for faster rebuilds
- GitHub Actions includes automatic caching