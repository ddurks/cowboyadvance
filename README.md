# Cowboy Advance

![A cattle drive on the open range: three riders, a column of cattle filing off
toward distant hills under a big cloudy sky](renders/painting.png)

<sup>Not concept art — a frame of the actual game engine. The riders, cattle and
grass are the game's own assets and systems; `cargo run --example painting` sets
up the landscape camera, renders it, and runs a painterly filter over the
result. See [Painting the box art](#painting-the-box-art).</sup>

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

Compiles today (`cargo build --release --target aarch64-apple-ios`). Running on
device is the one remaining packaging step: an Xcode/`bevy_main` wrapper + signing
(see Bevy's `mobile` example). No source changes needed.

## Architecture

Layered so input, simulation, and rendering stay independent — the key to easy
multiplayer later.

| File                   | Responsibility                                                             |
| ---------------------- | -------------------------------------------------------------------------- |
| `config.rs`            | Every tunable constant. **Engine-agnostic.**                               |
| `level.rs`             | Pen fences + gate geometry. **Engine-agnostic.**                           |
| `sim.rs`               | The boid herding simulation — pure functions, no ECS. **Engine-agnostic.** |
| `noise.rs`             | Seeded hashing + value noise. **Engine-agnostic.**                         |
| `components.rs`        | ECS components + resources.                                                |
| `setup.rs`             | Builds the 3D scene + cameras (game→viewport, UI→window).                  |
| `cabinet.rs`           | The arcade cabinet frame, deterministic layout, and the stick region.      |
| `input.rs`             | Keyboard + arcade stick → one `move_dir` per player.                       |
| `systems.rs`           | Per-frame systems: movement, boids, pen scoring, HUD.                      |
| `anim.rs`              | glTF characters: animation graphs, rider mounting, speed-driven gaits.     |
| `fence.rs`             | Runs the modelled post + rail along the pen and the field edge.            |
| `grass.rs`             | The grass field: wind, trampling, and worn trails.                         |
| `hide.rs`              | Per-cow splotches, so the herd reads as individuals.                       |
| `main.rs`              | App wiring + system schedule.                                              |
| `examples/painting.rs` | The header image: a landscape still, rendered by the game (see below).     |

**Simulation.** Runs on the X/Z ground plane with `Vec2`; rendering lifts to 3D.
Cows are [boids](https://www.red3d.com/cwr/boids/) (separation / alignment /
cohesion) plus **flee-from-players** (the herding force) and fence avoidance.
No physics engine — just circle-vs-segment resolution. The top three modules
depend only on glam math and were ported unchanged from an earlier macroquad
prototype (kept for reference in `legacy/macroquad/`).

**The cabinet.** The 3D game renders straight into a camera viewport set to the
cabinet's scene rect, with the bezel drawn over it by a UI `Camera2d`.
`cabinet.rs` computes a `CabinetLayout` from the window size each frame and
positions every element; the stick reads its centre and radius from that same
layout, so input and visuals never drift.

A portrait window takes its **own** aspect (floored at `CABINET_AR_MIN`), so the
cabinet fills a phone edge to edge. It used to be pinned to `CABINET_AR`, which
left wide black gutters on mobile — browser chrome eats height, that makes the
viewport squatter than the design ratio, and a contain-fit then limits the
cabinet by height and pads the sides. Windows wider than `CABINET_AR_MAX` are
landscape-ish and keep the design ratio, letterboxed, since stretching a
portrait cabinet across a desktop window would only inflate the frame art. The
bezel pieces are sized from the cabinet _width_ and the scene takes whatever
height is left, so this only ever stretches the play area, never the frame.
`cargo test` pins all of it.

**Characters and gaits.** Each entity is a bare gameplay transform with a
skinned glTF model hung off it as a child. The rigs are modelled facing −Y in
Blender and exported "+Y Up", which lands them facing +Z in glTF, so `MODEL_YAW`
in `config.rs` is the 180° that reconciles them with the game's −Z-forward
convention. The player's rider is parented to the horse's exported `saddle`
joint, so he bounces with whatever gait is playing underneath him.

Clip choice and playback rate both come from ground speed. Every locomotion clip
was animated with a known stride and duration, so `stride / duration` is the
speed at which its feet do not skate, and playing it at `speed / that` keeps
them planted. The field is arcade-scaled — roughly 14× the pace the gaits were
built for — so `GAIT_SPEED_SCALE` in `config.rs` scales every reference speed by
one shared factor; the authored walk:run relationship survives intact and only
the absolute pace is dialled back. Thresholds have separate enter/exit values so
an animal hovering on a boundary doesn't flicker, and dropping out of a run
fires the one-shot skid clip.

**Grass.** ~15k tufts in 15 chunks, one opaque triangle each, all swayed and
flattened in the vertex shader. Opaque rather than alpha-tested on purpose:
alpha foliage defeats early-Z and the hidden-surface removal that tile-based
mobile GPUs depend on. The fixed orthographic camera is worth two shortcuts —
every tuft sees the camera from the same angle, so the blade orientation is
baked at build time with no billboarding maths, and nothing shrinks with
distance, so there is no LOD.

Trample state lives in one 128×256 texture that the CPU stamps and the vertex
shader reads — the only texture fetch in the whole shader. `R` is flatten, which
springs back in about three seconds; `G` is wear, which fades over ninety, so a
herd driven over the same ground carves semi-permanent trails. Tramplers are
stamped as swept capsules rather than points, because at 19 units/second a
cowboy would otherwise leave a dotted line, and recovery only touches the dirty
rect. Density is one constant (`TUFTS_PER_UNIT2`) if a device struggles.

**Fencing.** `fence.rs` divides each run into a whole number of modules and
stretches the rails to absorb the rounding, so a run always ends on a post;
coincident posts at shared corners are deduplicated, and the gate posts are
built taller so the opening reads at a glance. Every piece shares one material,
so the couple of hundred of them batch down to a handful of draw calls. The wood
texture is generated rather than painted, from the same two browns and near-black
outline as the cabinet bezel — fence and bezel are literally the same timber.

**Cow hides.** Each cow gets its own dark-brown patches, baked into vertex
colours once when its model finishes loading, so there is no per-frame cost. The
hide meshes carry no UVs (the material is untextured, so the export strips them),
and a shader would need the rest position of a skinned vertex to keep patches
welded to the hide — vertex colours sidestep both, skin along with the mesh for
free, and compose with the penned-green tint. About one in five cows comes out
solid-coloured, as a real herd does.

**Multiplayer-ready.** Systems read `PlayerInputs(Vec<Vec2>)`, one entry per
`Player::index`. Today there's one local player; for 4-player coop you fill more
slots (some local, some networked) and the sim is unchanged. Keep the sim systems
deterministic to enable lockstep / host-authoritative netcode.

## Status

Done and verified in-browser:

- Real 3D — directional light, shadows, PBR, orthographic camera.
- Portrait level: herd up through a gate into the pen; live "penned" counter.
- Skinned glTF characters — cows, and a cowboy riding a horse — walking,
  running, skidding to a halt, each clip's tempo scaled to how fast the entity
  is actually travelling.
- Grass that sways, flattens under hooves, springs back, and keeps the trails
  the herd wears into the field.
- Randomised splotches per cow, so the herd reads as individuals.
- Modelled fencing around the pen and the field edge, textured with wood
  generated from the cabinet frame's own palette.
- Entities yaw to face their travel direction.
- Retro cabinet: bezeled screen, "COWBOY ADVANCE" title, arcade stick + A/B buttons.
- Mobile web: safe-area handling, `100dvh` sizing, and browser chrome tinted to
  the bezel so the strips above and below match the cabinet.
- Compiles for all three targets; clean console. The web bundle is ~37 MB, or
  ~27 MB when `wasm-opt` is available to `build-web.sh`.

## Roadmap

- **Cabinet art** — swap flat procedural colours for baked textures (9-slice the
  case/bezel, sprite the buttons/stick); optional CRT/scanline pass.
- **A/B buttons** — currently cosmetic; wire to a real action (whistle / sprint).
- **Stylized look** — cel/toon shading + outlines (`ExtendedMaterial` + outlines).
- **Worn ground** — sample the grass trample map in the ground material too, so
  dirt paths show through where the grass has been flattened.
- **Gameplay** — boid tuning for portrait, multiple gates, win/round flow.
- **Native iOS** — Xcode/`bevy_main` wrapper so it runs as an installed app.
- **Multiplayer** — 4-player coop over the `PlayerInputs` seam.

## Painting the box art

The image at the top of this README is not concept art and not a paint program —
it is rendered in the game:

```sh
cargo run --example painting     # -> renders/painting.png
```

## Notes

- **Assets are pre-built and committed.** `assets/models/*.glb` and
  `assets/textures/*` are generated from Blender sources kept outside the repo,
  so a clone builds and runs without Blender installed. Each locomotion clip's
  stride and non-skating speed are transcribed into the gait constants in
  `src/config.rs` — re-check those if a clip is ever re-exported.
- **WebGL2:** no compute shaders and no storage buffers, which is why the grass
  is driven by a CPU-written texture read in the vertex shader rather than a
  compute pass. Vertex texture fetch is the one piece worth re-checking on a
  real low-end device.
- **Shaders compile at runtime,** so `cargo check` will not catch a mistake in
  `assets/shaders/*.wgsl` — a change there needs an actual run.
- **Tuning:** almost everything — herd feel, camera, pen size, grass density,
  cabinet colours — is a named constant in `src/config.rs` or at the top of the
  module that owns it.
