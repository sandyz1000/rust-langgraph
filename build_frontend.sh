#!/bin/bash

# Build script for the LangGraph Observability frontend
# This builds the Dioxus frontend as WASM and places it in the static directory

set -e

echo "🔧 Building LangGraph Observability Frontend..."

# Install wasm-pack if not present
if ! command -v wasm-pack &> /dev/null; then
    echo "Installing wasm-pack..."
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

# Build the WASM package
echo "📦 Building WASM package..."
wasm-pack build --target web --out-dir ../static/pkg --features frontend

echo "✅ Frontend build complete!"
echo "   WASM files are in crates/langgraph-observability/static/pkg/"
echo "   Start the server with: cargo run --example observability_demo"
