#!/bin/bash
# Installation script for clearmodel

set -e

INSTALL_DIR="${HOME}/.local/bin"
BINARY_NAME="clearmodel"

echo "Installing clearmodel..."

# Check if the binary exists
if [ ! -f "./target/release/clearmodel" ]; then
    echo "Error: Binary not found. Please run ./build.sh first."
    exit 1
fi

# Create install directory if it doesn't exist
mkdir -p "$INSTALL_DIR"

# Copy binary
echo "Installing to $INSTALL_DIR/$BINARY_NAME"
cp "./target/release/clearmodel" "$INSTALL_DIR/$BINARY_NAME"

# Make it executable
chmod +x "$INSTALL_DIR/$BINARY_NAME"

echo "✅ Installation successful!"
echo ""
echo "Make sure $INSTALL_DIR is in your PATH:"
echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
echo ""
echo "Add this to your shell profile (.bashrc, .zshrc, etc.) to make it permanent."
echo ""
echo "You can now run: clearmodel --help" 