#!/bin/bash

# LangGraph Observability Demo Script
# This script runs the observability example and provides instructions

echo "🔍 LangGraph Observability Demo"
echo "==============================="
echo ""

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "❌ Cargo not found. Please install Rust first:"
    echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

echo "✅ Rust/Cargo found"

# Navigate to the project directory
cd "$(dirname "$0")"

echo "📁 Current directory: $(pwd)"

# Build the project first
echo "🔨 Building the project..."
if cargo build --example observability_demo; then
    echo "✅ Build successful"
else
    echo "❌ Build failed"
    exit 1
fi

echo ""
echo "🚀 Starting LangGraph Observability Demo..."
echo ""
echo "This demo will:"
echo "  1. Initialize the observability system"
echo "  2. Start a web dashboard on http://localhost:3000"
echo "  3. Run multiple graph executions with different scenarios"
echo "  4. Collect tracing, metrics, and prompt analysis data"
echo "  5. Keep the dashboard running for 5 minutes"
echo ""
echo "📝 What to expect:"
echo "  • Real-time graph execution monitoring"
echo "  • Detailed run traces and performance metrics"
echo "  • Prompt analysis with optimization suggestions"
echo "  • Live event streaming via WebSocket"
echo ""
echo "🌐 Once started, open http://localhost:3000 in your browser"
echo ""

read -p "Press Enter to start the demo..."

# Run the observability demo
cargo run --example observability_demo

echo ""
echo "✅ Demo completed!"
echo ""
echo "🎯 Key Features Demonstrated:"
echo "  ✓ Distributed tracing with OpenTelemetry"
echo "  ✓ Real-time metrics collection"
echo "  ✓ Interactive web dashboard"
echo "  ✓ Prompt analysis and optimization"
echo "  ✓ Live event streaming"
echo "  ✓ Flexible storage backends"
echo ""
echo "📚 Next Steps:"
echo "  • Explore the source code in crates/langgraph-observability/"
echo "  • Check out the comprehensive README"
echo "  • Integrate observability into your own LangGraph applications"
echo "  • Customize the dashboard and metrics for your use case"
echo ""
echo "🤝 Thank you for trying LangGraph Observability!"
