#!/usr/bin/env bash
# Build the Bevy wasm binary and generate the JS bindings into web/, then
# (optionally) serve. Usage:
#   ./build-web.sh          # build + bindgen
#   ./build-web.sh serve    # build + bindgen + serve on http://localhost:8080
#
# Requires wasm-bindgen-cli whose version matches the `wasm-bindgen` crate Bevy
# resolves. Check with:  cargo tree -i wasm-bindgen --target wasm32-unknown-unknown
set -euo pipefail
cd "$(dirname "$0")"

cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --out-name cowboyadvance --out-dir web --target web \
    target/wasm32-unknown-unknown/release/cowboyadvance.wasm
echo "built -> web/cowboyadvance.js + web/cowboyadvance_bg.wasm"

# Optional size trim if wasm-opt (binaryen) is installed. Rust wasm uses several
# post-MVP features, so enable them or wasm-opt rejects the input.
if command -v wasm-opt >/dev/null 2>&1; then
    wasm-opt -Oz \
        --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
        --enable-mutable-globals --enable-reference-types --enable-multivalue \
        -o web/cowboyadvance_bg.wasm web/cowboyadvance_bg.wasm
    echo "wasm-opt -Oz applied"
fi

if [[ "${1:-}" == "serve" ]]; then
    echo "serving web/ at http://localhost:8080  (ctrl-c to stop)"
    python3 -m http.server 8080 --directory web
fi
