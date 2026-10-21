"""Assemble apartment facades from Poly Haven's modular_urban_apartments_facade kit (CC0).

  blender -b --python tools/blender/facade_build.py -- <kit_dir> <out_dir>

The kit ships loose modules laid out side by side. This script stacks them
into 8-column (24 m), 4-storey building fronts with a cornice band over the
ground floor, a crown at the roof line and a plinth, then exports each
variant as a static .wxm via props_export's writer.
"""
import json
import os
import random
import sys

import bpy
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:]
kit_dir, out_dir = argv[0], argv[1]
os.makedirs(out_dir, exist_ok=True)
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "wxm_export.py")).read().split("\ndef main():")[0])

COL = 3.0
STOREY = 3.0
COLS = 8
FLOORS = 4  # ground + 3


def load_kit():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    f = [x for x in os.listdir(kit_dir) if x.endswith(".gltf")][0]
    bpy.ops.import_scene.gltf(filepath=os.path.join(kit_dir, f))
    kit = {}
    for o in bpy.data.objects:
        if o.type == 'MESH':
            o.hide_set(True)
            kit[o.name] = o
    return kit


def bbox(o):
    pts = [o.matrix_world @ v.co for v in o.data.vertices]
    return Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts))), Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))


def place(kit, name, anchor_name, x, z, out):
    """Copy module `name` so that the anchor module's (min x, min z) lands at (x, z)."""
    if name not in kit:
        return
    src = kit[name]
    amin, _ = bbox(kit[anchor_name])
    o = src.copy()
    o.data = src.data.copy()
    bpy.context.scene.collection.objects.link(o)
    o.hide_set(False)
    o.location = src.location + Vector((x - amin.x, 0.0, z - amin.z))
    out.append(o)


def build(seed, name):
    kit = load_kit()
    rng = random.Random(seed)
    parts = []
    window_styles = ["centered_large", "centered_small", "centered_double", "offset_small"]
    ground_styles = ["door_centered_large", "door_centered_small", "standard_standard", "door_offset_small"]
    for c in range(COLS):
        x = c * COL
        style = rng.choice(window_styles)
        g = rng.choice(ground_styles) if c % 3 != 1 else "standard_standard"
        # ground floor
        if g == "standard_standard":
            place(kit, "wall_standard_standard_01", "wall_standard_standard_01", x, 0.0, parts)
            place(kit, "dado_standard_standard_01", "wall_standard_standard_01", x, 1.0, parts)
        else:
            wall = f"wall_{g}_01"
            place(kit, wall, wall, x, 0.0, parts)
            place(kit, f"{g.split('_', 1)[0]}_{g.split('_', 1)[1]}_01".replace("door_", "door_", 1), wall, x, 0.0, parts)
            place(kit, f"dado_{g}_01", wall, x, 1.0, parts)
        place(kit, "base_standard_standard_01", "wall_standard_standard_01", x, 1.6, parts)
        place(kit, "cornice_standard_standard_01", "wall_standard_standard_01", x, -0.5, parts)
        # upper floors: same window style in a column, top floor uses the _03 / _05 variants
        for f in range(1, FLOORS):
            idx = "01" if f == 1 else "02"
            if f == FLOORS - 1:
                idx = {"offset_small": "05"}.get(style, "03")
            wall = f"wall_window_{style}_{idx}"
            if wall not in kit:
                wall = f"wall_window_{style}_01"
            win = wall.replace("wall_window_", "window_")
            z = f * STOREY
            place(kit, wall, wall, x, z, parts)
            place(kit, win, wall, x, z, parts)
        place(kit, "crown_standard_standard_01", "crown_standard_standard_01", x, FLOORS * STOREY, parts)
    # end caps
    place(kit, "crown_end_01", "crown_end_01", -0.43, FLOORS * STOREY, parts)
    for o in list(kit.values()):
        bpy.data.objects.remove(o, do_unlink=True)
    for o in parts:
        o.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    tris0 = sum(len(p.vertices) - 2 for p in obj.data.polygons)
    mod = obj.modifiers.new("planar", 'DECIMATE')
    mod.decimate_type = 'DISSOLVE'
    mod.angle_limit = 0.02
    mod.delimit = {'UV', 'MATERIAL'}
    bpy.ops.object.modifier_apply(modifier="planar")
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.mesh.quads_convert_to_tris()
    bpy.ops.object.mode_set(mode='OBJECT')
    tris1 = sum(len(p.vertices) - 2 for p in obj.data.polygons)
    budget = 26000
    if tris1 > budget:
        mod = obj.modifiers.new("dec", 'DECIMATE')
        mod.ratio = budget / tris1
        bpy.ops.object.modifier_apply(modifier="dec")
    print("FACADE", name, tris0, tris1)

    class T:
        meshes = [obj]
        bones = []
        index = {}
        parent = []
        arms_only = False

    verts, indices, subs = export_mesh(T, {})
    write_wxm(os.path.join(out_dir, name + ".wxm"), T, verts, indices, subs, [])
    mats = {}
    for slot in obj.material_slots:
        m = slot.material
        if m and m.node_tree:
            mats[m.name] = [bpy.path.abspath(n.image.filepath) for n in m.node_tree.nodes if n.type == 'TEX_IMAGE' and n.image]
    return mats


manifest = {}
for seed, name in ((3, "facade_a"), (11, "facade_b")):
    manifest[name] = {"materials": build(seed, name), "size": [COLS * COL, FLOORS * STOREY + 1, 0.5]}
json.dump(manifest, open(os.path.join(out_dir, "textures_facades.json"), "w"), indent=1)
