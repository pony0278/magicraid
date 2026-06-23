#!/usr/bin/env bash
# 構建 WASM 並放到 www/(供靜態伺服)。需 rustup target add wasm32-unknown-unknown。
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p magicraid-web --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/magicraid_web.wasm web/www/
echo "✓ web/www/magicraid_web.wasm 已更新($(du -k web/www/magicraid_web.wasm | cut -f1) KB)"
echo "  本機試玩:cd web/www && python3 -m http.server 8080 → http://localhost:8080"
