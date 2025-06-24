#!/bin/bash
# Build script for clearmodel

set -e

echo "Building clearmodel..."

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "Error: Rust/Cargo is not installed. Please install Rust first:"
    echo "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Build in release mode
echo "Running cargo build --release..."
cargo build --release

if [ $? -eq 0 ]; then
    echo "✅ Build successful!"
    echo "Binary location: ./target/release/clearmodel"
    echo ""
    echo "To run:"
    echo "  ./target/release/clearmodel --help"
    echo "  ./target/release/clearmodel --dry-run --verbose"
else
    echo "❌ Build failed!"
    exit 1
fi 