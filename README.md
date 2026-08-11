# Cowboy Advance

![A cattle drive on the open range: three riders, a column of cattle filing off
toward distant hills under a big cloudy sky](renders/painting.png)

<sup>rendered by `cargo run --example painting`.</sup>

Home, home on the range. A cattle-herding game demo, built in Rust with [Bevy](https://bevyengine.org)
0.19. One compiled codebase targets **desktop, web (WebAssembly), and iOS** —
Rust → native (Metal on iOS) and Rust → wasm (WebGL2), not transpiled JavaScript.

Herd the boid-driven cows up through the gate into the pen. Cows flee the cowboy,
so you steer the herd by where you stand. Portrait-oriented, viewed through an
angled orthographic camera.

## Controls

- **Desktop:** WASD / arrow keys.
- **Web / mobile:** the on-screen arcade stick (touch, or mouse-drag to test).

Cows turn green once penned; the HUD tracks the count.

## Running

### Desktop

```sh
cargo run              # debug (deps are optimized, so it runs smoothly)
cargo run --release
```

Running the built binary directly roots assets at the **executable**, not the
working directory, so either use `cargo run` or set `BEVY_ASSET_ROOT` to the
repo root.

### Web

Needs `wasm-bindgen-cli` matching the `wasm-bindgen` crate Bevy resolves:

```sh
cargo tree -i wasm-bindgen --target wasm32-unknown-unknown   # shows the version
cargo install wasm-bindgen-cli --version <that version>
rustup target add wasm32-unknown-unknown
```

Then build + serve (uses `wasm-opt` if [binaryen](https://github.com/WebAssembly/binaryen) is installed):

```sh
./build-web.sh serve   # -> http://localhost:8080
```

To test on a phone, browse to `http://<your-lan-ip>:8080` from the same network.

### iOS

Compiles with (`cargo build --release --target aarch64-apple-ios`). Running on
device is the one remaining packaging step: an Xcode/`bevy_main` wrapper + signing
(see Bevy's `mobile` example). No source changes needed.
