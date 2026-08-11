// Cowboy Advance — grass.
//
// One opaque triangle per tuft, swayed and trampled entirely in the vertex
// shader. Designed for WebGL2 on low-end mobile, which drives three choices:
//
//   * OPAQUE, no alpha test and no discard. Alpha-tested grass cards defeat
//     early-Z and the hidden-surface removal that tile-based mobile GPUs rely
//     on; solid triangles are dramatically cheaper and suit the low-poly look.
//   * ONE texture fetch per vertex, and it is the only fetch in the shader.
//   * LIT PER VERTEX. Tufts are ~9x7 pixels on screen, so per-fragment
//     lighting is invisible but costs a full screen of shading. The fragment
//     shader is one instruction.
//
// The trample map is written by the CPU (see src/grass.rs):
//   R = flatten (springs back in a few seconds)
//   G = wear    (a slow-fading record of where the herd has been)
//   BA = the direction the grass was pushed, encoded to 0..1

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::mesh_view_bindings::{view, globals}

const TAU: f32 = 6.2831853;

struct GrassParams {
    // xy = field origin, zw = field size, for mapping world XZ to trample UV
    field: vec4<f32>,
    // xy = wind direction, z = wind speed, w = wind amplitude (radians)
    wind: vec4<f32>,
    // x = gust amplitude, y = gust speed, z = max flatten angle, w = unused
    gust: vec4<f32>,
    base_color: vec4<f32>,
    tip_color: vec4<f32>,
    dry_color: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> gp: GrassParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var trample_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var trample_sampler: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // Packed per-vertex data, Unorm8x4 so a vertex is only 16 bytes total:
    // x = height factor (0 at the base, 1 at the tip)
    // y = per-tuft sway phase
    // z = per-tuft brightness jitter
    // w = per-tuft dryness
    @location(8) data: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    let world_from_local = get_world_from_local(v.instance_index);
    var world = mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));

    let h = v.data.x;
    let phase = v.data.y;
    let tint = v.data.z;
    let dry = v.data.w;

    // --- trample state: the only texture fetch in the shader -------------
    // Grass grows past the play field, but the map only covers the field, and
    // the sampler clamps — without this mask the edge texels would smear
    // phantom trails outward forever.
    let uv = (world.xz - gp.field.xy) / gp.field.zw;
    let inside = f32(all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0)));
    let t = textureSampleLevel(trample_tex, trample_sampler, uv, 0.0) * inside;
    let flat = t.r;
    let wear = t.g;
    let push = t.ba * 2.0 - 1.0;

    let wdir = gp.wind.xy;
    let ph = globals.time * gp.wind.z + dot(world.xz, wdir) * 0.35 + phase * TAU;
    var sway = (sin(ph) + 0.35 * sin(ph * 2.3 + 1.7)) * gp.wind.w;
    let g = max(sin(globals.time * gp.gust.y - dot(world.xz, wdir) * 0.06), 0.0);
    sway = sway + gp.gust.x * g * g * sin(ph * 0.7);

    // Flattened grass is pinned and cannot sway. Killing the wind term as the
    // blade goes down is most of what makes trampling read as weight rather
    // than as a colour change.
    let angle = sway * (1.0 - flat) * h * h + gp.gust.z * flat * h;

    // Arc about the base rather than shearing. The field is flat at y = 0, so
    // a vertex's height above the ground IS its distance from the pivot.
    let up = world.y;
    let dir = normalize(mix(wdir, push, flat) + vec2<f32>(1e-4, 0.0));
    world.x = world.x + up * sin(angle) * dir.x;
    world.z = world.z + up * sin(angle) * dir.y;
    world.y = up * cos(angle);

    var col = mix(gp.base_color.rgb, gp.tip_color.rgb, h);
    col = col * mix(0.91, 1.09, tint);
    // Matted and yellowed where it has been walked on, and more so along the
    // trails the herd keeps using.
    col = mix(col, gp.dry_color.rgb, max(max(flat * 0.62, wear * 0.8), dry * 0.12));
    // Cheap directional feel: a blade lying over catches less light.
    col = col * clamp(0.62 + 0.38 * cos(angle), 0.0, 1.4);

    var out: VertexOutput;
    out.clip_position = view.clip_from_world * world;
    out.color = col;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
