# Cowboy Advance

A cattle-herding game demo in **real 3D**, built in Rust with [Bevy](https://bevyengine.org)
0.19. One compiled codebase targets **desktop, web (WebAssembly), and iOS** —
Rust → native (Metal on iOS) and Rust → wasm (WebGL2), not transpiled JavaScript.

Herd the boid-driven cows up through the gate into the pen. Cows flee the cowboy,
so you steer the herd by where you stand. Portrait-oriented, viewed through an
angled orthographic camera, and presented inside a Game-Boy×arcade **cabinet**.

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

### iOS
Compiles today (`cargo build --release --target aarch64-apple-ios`). Running on
device is the one remaining packaging step: an Xcode/`bevy_main` wrapper + signing
(see Bevy's `mobile` example). No source changes needed.

## Architecture

Layered so input, simulation, and rendering stay independent — the key to easy
multiplayer later.

| File | Responsibility |
| --- | --- |
| `config.rs` | Every tunable constant. **Engine-agnostic.** |
| `level.rs` | Pen fences + gate geometry. **Engine-agnostic.** |
| `sim.rs` | The boid herding simulation — pure functions, no ECS. **Engine-agnostic.** |
| `components.rs` | ECS components + resources. |
| `setup.rs` | Builds the 3D scene + cameras (game→texture, UI→window). |
| `cabinet.rs` | The arcade cabinet frame, deterministic layout, and the stick region. |
| `input.rs` | Keyboard + arcade stick → one `move_dir` per player. |
| `systems.rs` | Per-frame systems: movement, boids, pen scoring, HUD. |
| `main.rs` | App wiring + system schedule. |

**Simulation.** Runs on the X/Z ground plane with `Vec2`; rendering lifts to 3D.
Cows are [boids](https://www.red3d.com/cwr/boids/) (separation / alignment /
cohesion) plus **flee-from-players** (the herding force) and fence avoidance.
No physics engine — just circle-vs-segment resolution. The top three modules
depend only on glam math and were ported unchanged from an earlier macroquad
prototype (kept for reference in `legacy/macroquad/`).

**The cabinet (identical framing on every device).** The 3D game renders to a
fixed-resolution offscreen texture (`Camera3d` + `RenderTarget::Image`), shown as
a UI `ImageNode` inside a cabinet drawn by a second `Camera2d`. So the game's
framing is pixel-identical on native iOS, Safari, and desktop; the cabinet scales
around it and gutters on wide screens. `cabinet.rs` computes a `CabinetLayout`
from window size each frame and positions every element; the stick reads its
centre/radius from that same layout, so input and visuals never drift.

**Multiplayer-ready.** Systems read `PlayerInputs(Vec<Vec2>)`, one entry per
`Player::index`. Today there's one local player; for 4-player coop you fill more
slots (some local, some networked) and the sim is unchanged. Keep the sim systems
deterministic to enable lockstep / host-authoritative netcode.

## Status

Done and verified in-browser:

- Real 3D — directional light, shadows, PBR, orthographic camera.
- Portrait level: herd up through a gate into the pen; live "penned" counter.
- Entities yaw to face their travel direction (maps to future character models).
- Retro cabinet: bezeled screen, "COWBOY ADVANCE" title, arcade stick + A/B buttons.
- Compiles for all three targets; web bundle ~27 MB; clean console.

## Roadmap

- **Assets** — replace placeholder cubes with glTF models (cow, cowboy). See
  `blender/` for a scale-accurate reference scene, and wire up glTF loading.
- **Cabinet art** — swap flat procedural colours for Blender-baked textures
  (9-slice the case/bezel, sprite the buttons/stick); optional CRT/scanline pass.
- **A/B buttons** — currently cosmetic; wire to a real action (whistle / sprint).
- **Stylized look** — cel/toon shading + outlines (`ExtendedMaterial` + outlines).
- **Mobile polish** — safe-area insets for notches/home indicator.
- **Gameplay** — boid tuning for portrait, multiple gates, win/round flow.
- **Native iOS** — Xcode/`bevy_main` wrapper so it runs as an installed app.
- **Multiplayer** — 4-player coop over the `PlayerInputs` seam.

## Blender reference

`blender/cowboy_advance_reference.py` builds a scene matching the game's exact
orthographic camera, with reference cubes at the in-game placeholder sizes, so you
can model assets to scale and preview the framing. Run it in Blender (Scripting →
Run) or `blender --background --python blender/cowboy_advance_reference.py`.
Export glTF with **"+Y Up"** and model **facing −Y** so assets land correctly.

## Notes

- **WebGL2:** the render-target `Image` uses a single `Rgba8UnormSrgb` format
  with no separate view format — a separate sRGB view needs the `VIEW_FORMATS`
  capability that WebGL2 (browser + iOS Safari) lacks.
- **Tuning:** almost everything — herd feel, camera, pen size, cabinet colours —
  is a named constant in `src/config.rs`.
