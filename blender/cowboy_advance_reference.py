"""
Cowboy Advance — Blender reference scene.

Builds a scene that matches the game's orthographic camera and shows reference
cubes at the EXACT size of the in-game placeholder cubes, so you can model your
cow / cowboy / props to the correct scale and preview how they'll frame in-game.

HOW TO RUN
  - In Blender: Scripting workspace -> Open -> pick this file -> Run Script (Alt-P).
  - Or headless:  blender --python cowboy_advance_reference.py
  Then press Numpad-0 to look through "GameCamera", and F12 to render (the render
  resolution is set to the game's 880x1280 screen, so the render == the in-game view).

COORDINATE SYSTEMS  (this is the important bit)
  Bevy is Y-up / -Z forward; Blender is Z-up. This script converts game (Bevy)
  coordinates to Blender with:  blender = (bevy.x, -bevy.z, bevy.y).
  That's the SAME mapping Blender's glTF exporter uses (with "+Y Up" enabled),
  so if you model against these reference cubes and export glTF with the default
  "+Y Up" setting, your asset lands in Bevy at the correct size and orientation.

MODELLING TIPS
  - Model your asset FACING -Y in Blender (front view, Numpad-1). In-game, entities
    turn so their local -Z (Blender -Y after export) points along their travel
    direction, so -Y-facing == "forward".
  - Keep the base sitting on Z = 0 (the ground). The reference cubes already do.
  - Match a cow to CowCube (1.4 x 1.4 x 1.4) and the player to PlayerCube
    (1.8 wide x 1.8 deep x 2.6 tall). Sizes are pulled from src/config.rs.

Verified against the game's config.rs / setup.rs:
  CAM_TARGET (0,-2) -> world (0,0,-2);  CAM_OFFSET (0,40,40) -> cam at (0,40,38)
  Ortho AutoMin { min_width: 44, min_height: 64 };  screen texture 880x1280.
"""

import bpy
from mathutils import Vector

# ---------------------------------------------------------------------------
# Game constants (keep in sync with src/config.rs)
# ---------------------------------------------------------------------------
COW_SIZE = (1.4, 1.4, 1.4)          # Bevy (x=width, y=height, z=depth)
PLAYER_SIZE = (1.8, 2.6, 1.8)       # Bevy (x=width, y=height, z=depth)

CAM_TARGET_XZ = (0.0, -2.0)         # CAM_TARGET (x, z)
CAM_OFFSET = (0.0, 40.0, 40.0)      # CAM_OFFSET (x, y, z)
CAM_MIN_HEIGHT = 64.0               # AutoMin min_height -> ortho vertical extent

SCREEN_RES_W, SCREEN_RES_H = 880, 1280

FIELD_HALF_X, FIELD_HALF_Z = 22.0, 40.0  # ground extents

SKY_SRGB = (0.55, 0.75, 0.95)
GROUND_SRGB = (0.34, 0.56, 0.28)
COW_SRGB = (0.58, 0.42, 0.30)
PLAYER_SRGB = (0.25, 0.55, 0.95)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
def bevy_to_blender(x, y, z):
    """Bevy (Y-up, -Z fwd) -> Blender (Z-up). Same as glTF '+Y Up' export."""
    return Vector((x, -z, y))


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def make_material(name, srgb, roughness):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    r, g, b = (srgb_to_linear(c) for c in srgb)
    bsdf.inputs["Base Color"].default_value = (r, g, b, 1.0)
    bsdf.inputs["Roughness"].default_value = roughness
    return mat


def add_box(name, bevy_size, bevy_center, mat):
    """bevy_size (w,h,d) at bevy_center (x,y,z); placed + scaled in Blender space."""
    w, h, d = bevy_size
    loc = bevy_to_blender(*bevy_center)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=loc)
    obj = bpy.context.active_object
    obj.name = name
    # Bevy (w=x, h=y-up, d=z) -> Blender dimensions (x=w, y=d, z=h).
    obj.scale = (w, d, h)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(mat)
    return obj


# ---------------------------------------------------------------------------
# Build the scene
# ---------------------------------------------------------------------------
def build():
    # Clear everything for a clean, reproducible scene.
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)

    scene = bpy.context.scene

    # --- Ground plane (Z = 0), sized to the play field ---------------------
    ground_mat = make_material("Ground", GROUND_SRGB, 1.0)
    bpy.ops.mesh.primitive_plane_add(size=1.0, location=bevy_to_blender(0, 0, 0))
    ground = bpy.context.active_object
    ground.name = "Ground"
    ground.scale = (FIELD_HALF_X * 2.0, FIELD_HALF_Z * 2.0, 1.0)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    ground.data.materials.append(ground_mat)

    # --- Reference cubes (sit on the ground, near the camera target) -------
    # Bottom on Z=0  ->  center height = size.y / 2.
    add_box("CowCube", COW_SIZE, (-2.0, COW_SIZE[1] / 2.0, -2.0),
            make_material("Cow", COW_SRGB, 0.8))
    add_box("PlayerCube", PLAYER_SIZE, (2.0, PLAYER_SIZE[1] / 2.0, -2.0),
            make_material("Player", PLAYER_SRGB, 0.6))

    # --- Camera: match the game's orthographic camera exactly --------------
    cam_bevy = (CAM_TARGET_XZ[0] + CAM_OFFSET[0],
                0.0 + CAM_OFFSET[1],
                CAM_TARGET_XZ[1] + CAM_OFFSET[2])          # (0, 40, 38)
    target_bevy = (CAM_TARGET_XZ[0], 0.0, CAM_TARGET_XZ[1])  # (0, 0, -2)
    cam_loc = bevy_to_blender(*cam_bevy)
    target_loc = bevy_to_blender(*target_bevy)

    bpy.ops.object.camera_add(location=cam_loc)
    cam = bpy.context.active_object
    cam.name = "GameCamera"
    cam.data.type = 'ORTHO'
    cam.data.sensor_fit = 'VERTICAL'      # ortho_scale == vertical world extent
    cam.data.ortho_scale = CAM_MIN_HEIGHT  # 64 units tall, matching AutoMin
    cam.data.clip_start = 0.1
    cam.data.clip_end = 1000.0
    cam.rotation_euler = (target_loc - cam_loc).to_track_quat('-Z', 'Y').to_euler()
    scene.camera = cam

    # --- Key light: match the game's directional light ---------------------
    bpy.ops.object.light_add(type='SUN', location=bevy_to_blender(20, 40, 20))
    sun = bpy.context.active_object
    sun.name = "KeyLight"
    sun.data.energy = 4.0
    sun.data.use_shadow = True
    # Game light shines from (20,40,20) toward origin.
    shine = bevy_to_blender(-20, -40, -20)
    sun.rotation_euler = shine.to_track_quat('-Z', 'Y').to_euler()

    # --- World / ambient (soft sky, roughly matching the game) -------------
    world = scene.world or bpy.data.worlds.new("World")
    scene.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes.get("Background")
    sc = [srgb_to_linear(c) for c in SKY_SRGB]
    bg.inputs[0].default_value = (sc[0], sc[1], sc[2], 1.0)
    bg.inputs[1].default_value = 0.5  # strength

    # --- Render settings: the game's screen resolution ---------------------
    scene.render.resolution_x = SCREEN_RES_W
    scene.render.resolution_y = SCREEN_RES_H
    scene.render.resolution_percentage = 100
    for engine in ("BLENDER_EEVEE_NEXT", "BLENDER_EEVEE"):
        try:
            scene.render.engine = engine
            break
        except (TypeError, ValueError):
            continue

    print("Cowboy Advance reference scene built.")
    print(f"  Camera 'GameCamera' at Blender {tuple(round(v, 2) for v in cam_loc)} "
          f"(ortho, scale {CAM_MIN_HEIGHT}, {SCREEN_RES_W}x{SCREEN_RES_H}).")
    print("  Reference cubes: CowCube (1.4^3), PlayerCube (1.8x1.8x2.6). "
          "Press Numpad-0 to view, F12 to render.")


if __name__ == "__main__":
    build()
