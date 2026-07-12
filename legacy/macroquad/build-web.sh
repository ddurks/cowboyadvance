#!/usr/bin/env bash
# Build the wasm binary and drop it next to the web shell, then (optionally)
# serve it. Usage:
#   ./build-web.sh          # build + copy
#   ./build-web.sh serve    # build + copy + serve on http://localhost:8080
set -euo pipefail
cd "$(dirname "$0")"

cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/cowboyadvance.wasm web/cowboyadvance.wasm
echo "built -> web/cowboyadvance.wasm"

if [[ "${1:-}" == "serve" ]]; then
    echo "serving web/ at http://localhost:8080  (ctrl-c to stop)"
    python3 -m http.server 8080 --directory web
fi
