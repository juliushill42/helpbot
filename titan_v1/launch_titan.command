#!/bin/bash

# Titan Orchestration System Launcher
# Double-click this file to start Titan

clear
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║             TITAN ORCHESTRATION SYSTEM v0.2.0              ║"
echo "║          AI Security Framework - Production Ready          ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

# Source cargo environment if available
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

# Check if binary exists
if [ ! -f "./host/target/release/titan-host" ]; then
    echo "⚠️  Binary not found. Building Titan..."
    cd host
    
    # Check if cargo is available
    if ! command -v cargo &> /dev/null; then
        echo "❌ Cargo not found. Please install Rust:"
        echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        read -p "Press Enter to exit..."
        exit 1
    fi
    
    cargo build --release
    if [ $? -ne 0 ]; then
        echo "❌ Build failed. Please check errors above."
        read -p "Press Enter to exit..."
        exit 1
    fi
    cd ..
    echo "✅ Build complete!"
    echo ""
fi

# Check for OpenAI API key (optional)
if [ -z "$OPENAI_API_KEY" ]; then
    echo "⚠️  OPENAI_API_KEY not set. Using Ollama fallback."
    echo "   To use OpenAI, set: export OPENAI_API_KEY='sk-...'"
    echo ""
else
    echo "✅ OpenAI API key detected"
    echo ""
fi

# Launch Titan
echo "🚀 Starting Titan Core..."
echo "   Port 8080: JSON-RPC Bridge"
echo "   Press Ctrl+C to stop"
echo ""
echo "═══════════════════════════════════════════════════════════"
echo ""

cd host
./target/release/titan-host

# Keep terminal open on exit
echo ""
echo "Titan has stopped."
read -p "Press Enter to close..."
