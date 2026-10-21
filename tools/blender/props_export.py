"""Static prop pipeline (Poly Haven CC0 glTF -> .wxm without skeleton).

  blender -b --python tools/blender/props_export.py -- <polyhaven_dir> <out_dir> name[:max_tris] ...

Each prop is joined, scaled to metres, decimated to the triangle budget and
written as a skeleton-less .wxm (one submesh per material). Texture paths are
listed in textures_props.json for build_assets.py.
"""
import json
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(__file__))

argv = sys.argv[sys.argv.index("--") + 1:]
src_dir, out_dir = argv[0], argv[1]
os.makedirs(out_dir, exist_ok=True)
exec(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "wxm_export.py")).read().split("\ndef main():")[0])


class StaticTarget:
    """Minimal stand-in for Target: no bones, every vertex bound to joint 0."""

    def __init__(self, meshes):
        self.meshes = meshes
        self.bones = []
        self.index = {}
        self.parent = []
        self.arms_only = False


manifest = {}
for spec in argv[2:]:
    name, _, budget = spec.partition(":")
    budget = int(budget or 3000)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    folder = os.path.join(src_dir, name)
    gltf = [f for f in os.listdir(folder) if f.endswith(".gltf")][0]
    bpy.ops.import_scene.gltf(filepath=os.path.join(folder, gltf))
    meshes = [o for o in bpy.data.objects if o.type == 'MESH']
    for o in meshes:
        o.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    tris = sum(len(p.vertices) - 2 for p in obj.data.polygons)
    if tris > budget * 2:
        # planar dissolve first: flat walls lose triangles without changing shape
        mod = obj.modifiers.new("planar", 'DECIMATE')
        mod.decimate_type = 'DISSOLVE'
        mod.angle_limit = 0.02
        mod.delimit = {'UV', 'MATERIAL'}
        bpy.ops.object.modifier_apply(modifier="planar")
        bpy.ops.object.mode_set(mode='EDIT')
        bpy.ops.mesh.select_all(action='SELECT')
        bpy.ops.mesh.quads_convert_to_tris()
        bpy.ops.object.mode_set(mode='OBJECT')
        tris = sum(len(p.vertices) - 2 for p in obj.data.polygons)
        print("PLANAR", name, tris)
    if tris > budget:
        mod = obj.modifiers.new("dec", 'DECIMATE')
        mod.ratio = budget / tris
        bpy.ops.object.modifier_apply(modifier="dec")
    tgt = StaticTarget([obj])
    verts, indices, subs = export_mesh(tgt, {})
    write_wxm(os.path.join(out_dir, name + ".wxm"), tgt, verts, indices, subs, [])
    mats = {}
    for slot in obj.material_slots:
        m = slot.material
        if m and m.node_tree:
            mats[m.name] = [bpy.path.abspath(n.image.filepath) for n in m.node_tree.nodes if n.type == 'TEX_IMAGE' and n.image]
    dims = obj.dimensions
    manifest[name] = {"materials": mats, "size": [dims.x, dims.z, dims.y], "src_tris": tris}
json.dump(manifest, open(os.path.join(out_dir, "textures_props.json"), "w"), indent=1)
