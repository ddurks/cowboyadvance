// Cowboy Advance — dust puffs.
//
// Billboarded quads, built CPU-side and drawn in one call (see src/dust.rs).
// Designed for WebGL2 on low-end mobile:
//
//   * The sprite sheet's ALPHA is the only thing sampled — it's the puff shape.
//     The colour is a flat tint, so a grey/near-black sheet can never bleed dark
//     pixels onto the field the way a textured StandardMaterial did.
//   * One texture fetch, alpha-blended, no discard.
//   * Per-vertex alpha fades each puff over its life, so they dissolve instead
//     of popping.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::mesh_view_bindings::view

struct DustParams {
    // rgb = dust colour (linear); a = overall opacity scale
    tint: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> dp: DustParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var atlas_sampler: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // x,y = atlas UV of this frame; z = life fade (0..1)
    @location(8) data: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
}

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    let world_from_local = get_world_from_local(v.instance_index);
    let world = mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));

    var out: VertexOutput;
    out.clip_position = view.clip_from_world * world;
    out.uv = v.data.xy;
    out.alpha = v.data.z;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let shape = textureSample(atlas_tex, atlas_sampler, in.uv).a;
    return vec4<f32>(dp.tint.rgb, shape * in.alpha * dp.tint.a);
}
