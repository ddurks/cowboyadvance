#!/usr/bin/env bash
# Build the Bevy wasm binary and generate the JS bindings into web/, then
# (optionally) serve. Usage:
#   ./build-web.sh          # optimized build + bindgen
#   ./build-web.sh dev      # incremental development build + bindgen
#   ./build-web.sh serve    # optimized build + serve (legacy convenience)
#
# Requires wasm-bindgen-cli whose version matches the `wasm-bindgen` crate Bevy
# resolves. Check with:  cargo tree -i wasm-bindgen --target wasm32-unknown-unknown
set -euo pipefail
cd "$(dirname "$0")"

mode="${1:-release}"
profile=release
if [[ "$mode" == "dev" ]]; then
    profile=debug
    cargo build --target wasm32-unknown-unknown
else
    cargo build --release --target wasm32-unknown-unknown
fi
wasm-bindgen --out-name cowboyadvance --out-dir web --target web \
    "target/wasm32-unknown-unknown/$profile/cowboyadvance.wasm"
echo "built -> web/cowboyadvance.js + web/cowboyadvance_bg.wasm"

# Sync runtime assets into web/ so the AssetServer can fetch them (Bevy on wasm
# loads from /assets/... relative to the page).
rsync -a --delete assets/ web/assets/ 2>/dev/null || { rm -rf web/assets && cp -R assets web/assets; }
echo "synced assets -> web/assets/"

# Optional size trim if wasm-opt (binaryen) is installed. Rust wasm uses several
# post-MVP features, so enable them or wasm-opt rejects the input.
if [[ "$mode" != "dev" ]] && command -v wasm-opt >/dev/null 2>&1; then
    wasm-opt -Oz \
        --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
        --enable-mutable-globals --enable-reference-types --enable-multivalue \
        -o web/cowboyadvance_bg.wasm web/cowboyadvance_bg.wasm
    echo "wasm-opt -Oz applied"
fi

# Stamp the build so index.html can load the js/wasm pair under versioned URLs.
# Safari (iOS especially) heuristically caches on plain http; a stale glue JS
# paired with a fresh wasm throws "__wasm_bindgen_func_elem_N is not a function"
# because every build renumbers those exports.
build_id="$(cat web/cowboyadvance_bg.wasm web/cowboyadvance.js | shasum | cut -c1-12)"
printf '%s' "$build_id" > web/build_id.txt
echo "build id: $build_id"

if [[ "$mode" == "serve" ]]; then
    # Reclaim the port: one canonical server on 8080, never a second on 8081.
    lsof -ti tcp:8080 | xargs kill 2>/dev/null || true
    echo "serving web/ at http://localhost:8080  (ctrl-c to stop)"
    # no-store keeps index.html itself from going heuristically stale on phones.
    python3 - 8080 "$PWD/web" <<'PY'
import http.server, os, sys

port = int(sys.argv[1])
os.chdir(sys.argv[2])

class Handler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()

http.server.ThreadingHTTPServer(('0.0.0.0', port), Handler).serve_forever()
PY
fi
