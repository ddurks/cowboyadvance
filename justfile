set shell := ["bash", "-cu"]

# Recompile and restart the desktop game when Rust or assets change.
dev:
    cargo watch -w src -w assets -x run

# Build, serve, and live-reload the web game. Override with `PORT=9000 just web`.
web:
    ./scripts/dev-web.sh

# One incremental web build without starting a server.
web-build:
    ./build-web.sh dev

# Optimized deployable output in web/.
web-release:
    ./build-web.sh

check:
    cargo check
    cargo test

# Re-render the mounted gallop and assemble the loading GIF from its frames.
loading-gif:
    cargo run --example render_loading_gif
    magick -delay 6 -loop 0 renders/loading/frame-*.png -resize 318x318 -layers Optimize assets/ui/loading-cowboy.gif

# Re-render the cowboy head-shot and cut the favicon set from it.
# Keying: the render clears to pure lime with MSAA off, so a corner floodfill
# with a whisker of fuzz lifts the whole backdrop without touching the subject.
favicon:
    cargo run --example render_favicon
    magick renders/favicon.png -alpha set -fuzz 4% -fill none \
        -draw "alpha 1,1 floodfill" -draw "alpha 510,1 floodfill" \
        -draw "alpha 1,510 floodfill" -draw "alpha 510,510 floodfill" \
        renders/favicon-keyed.png
    magick renders/favicon-keyed.png -resize 32x32 assets/ui/favicon-32.png
    magick renders/favicon-keyed.png -resize 16x16 assets/ui/favicon-16.png
    magick renders/favicon-keyed.png -background "#5e4123" -flatten -resize 180x180 assets/ui/apple-touch-icon.png
    magick renders/favicon-keyed.png -define icon:auto-resize=48,32,16 web/favicon.ico
