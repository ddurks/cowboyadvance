#!/usr/bin/env bash
# Rebuild the web game on changes and serve it with live reload and caching
# disabled. The HTTP implementation uses only Python's standard library.
set -euo pipefail
cd "$(dirname "$0")/.."

port="${PORT:-8080}"

# Reclaim the port: one canonical server, never a fallback on a strange port.
lsof -ti tcp:"$port" | xargs kill 2>/dev/null || true

./build-web.sh dev

cleanup() {
    trap - EXIT INT TERM
    kill "$server_pid" "$watch_pid" 2>/dev/null || true
    wait "$server_pid" "$watch_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

python3 - "$port" "$PWD/web" <<'PY' &
import http.server
import os
import pathlib
import sys
import urllib.parse

port = int(sys.argv[1])
root = pathlib.Path(sys.argv[2])
os.chdir(root)

RELOAD_SCRIPT = b"""
<script>
(() => {
  let version;
  async function check() {
    try {
      const response = await fetch('/__build_id', { cache: 'no-store' });
      const next = await response.text();
      if (version !== undefined && next !== version) location.reload();
      version = next;
    } catch (_) {}
    setTimeout(check, 500);
  }
  check();
})();
</script>
"""

class DevHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Cache-Control', 'no-store, no-cache, must-revalidate, max-age=0')
        self.send_header('Pragma', 'no-cache')
        self.send_header('Expires', '0')
        super().end_headers()

    def do_GET(self):
        path = urllib.parse.urlsplit(self.path).path
        if path == '/__build_id':
            wasm = root / 'cowboyadvance_bg.wasm'
            stamp = str(wasm.stat().st_mtime_ns if wasm.exists() else 0).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain')
            self.send_header('Content-Length', str(len(stamp)))
            self.end_headers()
            self.wfile.write(stamp)
            return
        if path in ('/', '/index.html'):
            page = (root / 'index.html').read_bytes().replace(b'</body>', RELOAD_SCRIPT + b'</body>')
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(page)))
            self.end_headers()
            self.wfile.write(page)
            return
        super().do_GET()

print(f'web: http://0.0.0.0:{port} (live reload, cache disabled)', flush=True)
http.server.ThreadingHTTPServer(('0.0.0.0', port), DevHandler).serve_forever()
PY
server_pid=$!

cargo watch \
    -w src -w assets -w Cargo.toml -w Cargo.lock -w web/index.html \
    -s './build-web.sh dev' &
watch_pid=$!

# macOS still ships Bash 3.2, which does not support `wait -n`. The server is
# the foreground lifetime; the EXIT trap always tears down both children.
wait "$server_pid"
