#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

# Clone llama.cpp if missing
if [ ! -d llama.cpp ]; then
  git clone https://github.com/ggerganov/llama.cpp.git
fi

# Build llama.cpp (CPU-only)
cmake -S llama.cpp -B llama.cpp/build -DGGML_CPU=ON -DGGML_CUDA=OFF -DGGML_METAL=OFF -DGGML_OPENCL=OFF -DCMAKE_BUILD_TYPE=Release
cmake --build llama.cpp/build --config Release -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu)"

# Bridge build
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu)"

echo "titan_bridge built at $ROOT/build/titan_bridge"
