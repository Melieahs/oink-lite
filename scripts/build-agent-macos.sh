#!/usr/bin/env bash
# Build the Oink agent on macOS 10.15 (Catalina) without the Tauri shell.
# Run this on the Catalina machine; paste the first error back if it fails.
set -euo pipefail
cd "$(dirname "$0")/.."

export MACOSX_DEPLOYMENT_TARGET=10.15

echo "== environment =="
sw_vers
xcode-select -p
rustc -vV | sed -n '1,3p'
command -v cmake || echo "cmake: MISSING (brew install cmake)"

echo "== building oink-agent =="
cargo build -p oink-agent --release
