# Cowboy Advance — working guidelines

A cattle-herding game demo in real 3D. Rust + Bevy 0.19, one codebase compiled to desktop, web (wasm/WebGL2), and native iOS (Metal via wgpu) — actually compiled, not transpiled JS. Herd boid-driven cows through the gate into the pen; portrait, angled orthographic camera, wrapped in an arcade cabinet. Single local player today; **4-player coop is a design goal**, so keep the seams for it intact.

See `README.md` for the overview and how to run. Every tunable lives in `src/config.rs`.

## Architecture — respect the layering

Input → simulation → rendering are kept independent; that separation is the whole reason coop and netcode stay tractable. Don't collapse it for convenience.

- **Engine-agnostic core** — `config.rs`, `level.rs`, `sim.rs`, `noise.rs` depend only on glam math (`bevy::math`), never on ECS, rendering, or Bevy systems. Keep them that way: it makes the sim unit-testable and portable (it was lifted from an earlier macroquad prototype unchanged). New pure gameplay / geometry / math logic belongs here, not in a system.
- **Bevy glue** — `components.rs`, `setup.rs`, `input.rs`, `systems.rs`, `anim.rs`, `cabinet.rs`, `fence.rs`, `grass.rs`, `hide.rs`, `main.rs`. Systems collect entity state into plain slices, call the core, and write the results back.
- **Determinism** — the sim is pure functions and entity spawns are hash-based (no RNG). Keep it deterministic so lockstep / host-authoritative netcode stays possible.
- **Multiplayer seam** — systems read `PlayerInputs(Vec<Vec2>)` indexed by `Player::index`, and forces loop over all players. There's one local player now (`vec![Vec2::ZERO]`). Never hardcode "player 0" or bake in a single-player assumption.
- **Tuning** — add a named constant in `config.rs` (or at the top of the module that owns it) with a provenance / why note. Don't scatter bare magic numbers through logic.

## Conventions

- World is the X/Z ground plane, Y up; **forward is −Z**. The sim works in `Vec2` = (X, Z); rendering lifts to 3D `Transform`s.
- An entity is a bare gameplay transform with the skinned glTF model hung off it as a child; `MODEL_YAW` reconciles the rig's exported facing with the −Z-forward convention.
- Colors are stored as sRGB `(f32, f32, f32)` tuples in `config.rs` and converted to `Color` at spawn.

## Comments — WHY only

The code should be self-explanatory; comments carry what the code cannot. Write a comment only when it says something the reader can't get from the code itself:

- **Why** a thing is done this way (rationale, trade-off, intent).
- **Provenance** of a magic number (where a constant / index / measurement comes from).
- A non-obvious **convention** (coordinate frame, units, sign, axis mapping).
- A **gotcha / workaround / constraint** (engine quirk, export-pipeline coupling, ordering that matters).

Do not add comments that restate what the code does or echo a name. Prefer a clearer name or structure over a comment. When a comment would just narrate the next line, delete it.

## Code style

- Make the code self-explanatory: reach for a clear name or a small helper before a comment.
- Match the surrounding file's style and idiom.
- Factor shared logic instead of duplicating it (e.g. `resolve_fences` / `clamp_field` serve both players and cows).
- Keep changes minimal and focused; don't reformat or churn code you aren't changing.

## Bevy 0.19 (this repo pins 0.19 — older tutorials differ)

- No bundles — spawn tuples of components (`Mesh3d`, `MeshMaterial3d`, `Transform`, …).
- `DirectionalLight { shadow_maps_enabled: true, .. }` — not `shadows_enabled`. Ambient light is the `GlobalAmbientLight` resource, not a component.
- `BorderRadius` is a `Node` field, not a standalone component.
- `time.delta_secs()`; `Query::single()` returns a `Result`.
- Disjoint queries need `Without<…>`; the O(n²) boid pass snapshots with `.iter()` then updates with `.iter_mut().enumerate()` (sequential borrows, no `ParamSet`).
- `ScalingMode` lives in `bevy::camera`, not the prelude.

## Targets, web & size

- One codebase must keep compiling for desktop, `wasm32-unknown-unknown` (WebGL2), and `aarch64-apple-ios`. Don't add anything that breaks a target.
- **WebGL2 has no compute shaders and no storage buffers** — that's why grass is driven by a CPU-written texture read in the vertex shader. Render-target `Image`s must use `Rgba8UnormSrgb` with `view_format: None`; a separate sRGB view format needs `VIEW_FORMATS`, which WebGL2 lacks, and the whole render aborts to the clear color.
- Keep the wasm bundle small: Bevy features are trimmed (`default-features = false`, `3d` / `ui` / `bevy_gltf` / `bevy_animation`), the release profile is size-tuned, and `build-web.sh` runs `wasm-opt`. Don't pull in heavy dependencies casually.

## Verifying changes

- `cargo check` / `cargo build` won't catch WGSL errors — shaders compile at runtime, so a shader change needs an actual run.
- `cargo test` pins the cabinet layout math; keep it green.
- **Don't trust the Codex-in-Chrome automation browser for web render checks** — it renders file-loaded texture `ImageNode`s (frame, joystick) blank even though they load and render fine in a real browser. Verify web rendering in the user's real Chrome, or with a native in-engine screenshot (`Screenshot::primary_window()` + `save_to_disk`).
- Assets (`assets/models/*.glb`, `assets/textures/*`) are pre-built and committed from Blender sources kept outside the repo, so a clone builds without Blender. If a locomotion clip is re-exported, re-check the gait constants in `config.rs`.
