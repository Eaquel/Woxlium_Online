"""Blender (headless) character pipeline for Woxlium.

  blender -b --python tools/blender/wxm_export.py -- <config.json>

For every character in the config it:
  * imports a Microsoft Rocketbox avatar (MIT) and strips facial bones,
  * retargets clips from Quaternius' Universal Animation Library (CC0, Rigify)
    and Rocketbox motion files onto the avatar's Biped skeleton using a
    rest-pose aligned world-space delta (keeps twist, fixes A-pose vs T-pose),
  * optionally keeps only arm geometry (first-person view model),
  * writes a compact .wxm binary (mesh + skeleton + baked 30 fps clips).

Coordinates are converted from Blender Z-up to game Y-up: (x, y, z) -> (x, z, -y).
"""
import json
import os
import struct
import sys

import bpy
from mathutils import Matrix, Quaternion, Vector

FACIAL = ("Eye", "Jaw", "Lip", "Tongue", "Mouth", "Masseter", "Caninus", "Blink", "Eyebrow", "Nose", "Cheek", "lip")

UAL_MAP = {
    "DEF-hips": "Bip01 Pelvis", "DEF-spine.001": "Bip01 Spine", "DEF-spine.002": "Bip01 Spine1",
    "DEF-spine.003": "Bip01 Spine2", "DEF-neck": "Bip01 Neck", "DEF-head": "Bip01 Head",
}
for s, b in (("L", "L"), ("R", "R")):
    UAL_MAP.update({
        f"DEF-shoulder.{s}": f"Bip01 {b} Clavicle", f"DEF-upper_arm.{s}": f"Bip01 {b} UpperArm",
        f"DEF-forearm.{s}": f"Bip01 {b} Forearm", f"DEF-hand.{s}": f"Bip01 {b} Hand",
        f"DEF-thigh.{s}": f"Bip01 {b} Thigh", f"DEF-shin.{s}": f"Bip01 {b} Calf",
        f"DEF-foot.{s}": f"Bip01 {b} Foot", f"DEF-toe.{s}": f"Bip01 {b} Toe0",
    })
    for i, f in enumerate(["thumb", "f_index", "f_middle", "f_ring", "f_pinky"]):
        for j, suffix in enumerate(["", "1", "2"]):
            UAL_MAP[f"DEF-{f}.0{j + 1}.{s}" if f != "thumb" else f"DEF-thumb.0{j + 1}.{s}"] = f"Bip01 {b} Finger{i}{suffix}"

# direction reference child for swing alignment of target (Biped) bones
CHILD = {"Bip01 Pelvis": "Bip01 Spine", "Bip01 Spine": "Bip01 Spine1", "Bip01 Spine1": "Bip01 Spine2",
         "Bip01 Spine2": "Bip01 Neck", "Bip01 Neck": "Bip01 Head"}
for b in ("L", "R"):
    CHILD.update({f"Bip01 {b} Clavicle": f"Bip01 {b} UpperArm", f"Bip01 {b} UpperArm": f"Bip01 {b} Forearm",
                  f"Bip01 {b} Forearm": f"Bip01 {b} Hand", f"Bip01 {b} Hand": f"Bip01 {b} Finger2",
                  f"Bip01 {b} Thigh": f"Bip01 {b} Calf", f"Bip01 {b} Calf": f"Bip01 {b} Foot",
                  f"Bip01 {b} Foot": f"Bip01 {b} Toe0"})
    for i in range(5):
        CHILD[f"Bip01 {b} Finger{i}"] = f"Bip01 {b} Finger{i}1"
        CHILD[f"Bip01 {b} Finger{i}1"] = f"Bip01 {b} Finger{i}2"

ARM_BONES = ("Clavicle", "UpperArm", "Forearm", "Hand", "Finger")


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def import_any(path):
    before = set(bpy.data.objects)
    if path.lower().endswith(".fbx"):
        bpy.ops.import_scene.fbx(filepath=path, use_anim=True)
    else:
        bpy.ops.import_scene.gltf(filepath=path)
    return [o for o in bpy.data.objects if o not in before]


def rot_of(m):
    return m.to_3x3().normalized().to_quaternion()


class Pose:
    """World-space rotations/positions of an armature's bones at the current frame."""

    def __init__(self, arm, rest=False):
        self.rot = {}
        self.pos = {}
        mw = arm.matrix_world
        for b in arm.data.bones:
            m = mw @ (b.matrix_local if rest else arm.pose.bones[b.name].matrix)
            self.rot[b.name] = rot_of(m)
            self.pos[b.name] = m.translation.copy()


def swing(a, b):
    a = a.normalized()
    b = b.normalized()
    return a.rotation_difference(b)


class Target:
    def __init__(self, fbx, keep_arms_only=False):
        objs = import_any(fbx)
        self.arm = [o for o in objs if o.type == 'ARMATURE'][0]
        self.meshes = [o for o in objs if o.type == 'MESH']
        if self.arm.animation_data:
            self.arm.animation_data.action = None
        bones = [b for b in self.arm.data.bones if not any(k in b.name for k in FACIAL) or b.name == "Bip01 Head"]
        self.bones = [b.name for b in bones]
        self.index = {n: i for i, n in enumerate(self.bones)}
        self.parent = []
        for b in bones:
            p = b.parent
            while p is not None and p.name not in self.index:
                p = p.parent
            self.parent.append(self.index[p.name] if p is not None else -1)
        self.rest = Pose(self.arm, rest=True)
        self.arms_only = keep_arms_only

    def kept_ancestor(self, name):
        b = self.arm.data.bones.get(name)
        while b is not None and b.name not in self.index:
            b = b.parent
        return self.index[b.name] if b is not None else 0


def retarget_frame(tgt, src_pose, src_rest, mapping, align, hip_scale, src_root, overrides=None):
    """Return world rotations + root position for target bones."""
    rot = {}
    for i, name in enumerate(tgt.bones):
        src = mapping.get(name)
        if overrides and name in overrides:
            # local rotation relative to the already-solved parent
            p = tgt.bones[tgt.parent[i]]
            rot[name] = rot[p] @ overrides[name]
        elif src is not None and src in src_pose.rot:
            rot[name] = src_pose.rot[src] @ src_rest.rot[src].inverted() @ align[name] @ tgt.rest.rot[name]
        else:
            pi = tgt.parent[i]
            if pi < 0:
                rot[name] = tgt.rest.rot[name]
            else:
                p = tgt.bones[pi]
                rot[name] = rot[p] @ tgt.rest.rot[p].inverted() @ tgt.rest.rot[name]
    root = tgt.bones[0]
    sr = mapping[root]
    pos = tgt.rest.pos[root] + (src_pose.pos[sr] - src_rest.pos[src_root]) * hip_scale
    return rot, pos


def compute_align(tgt, src_rest, mapping):
    align = {}
    for name in tgt.bones:
        src = mapping.get(name)
        child = CHILD.get(name)
        if src is None or child is None or child not in mapping or mapping[child] not in src_rest.pos:
            align[name] = Quaternion()
            continue
        td = tgt.rest.pos[child] - tgt.rest.pos[name]
        sd = src_rest.pos[mapping[child]] - src_rest.pos[src]
        if td.length < 1e-6 or sd.length < 1e-6:
            align[name] = Quaternion()
        else:
            align[name] = swing(td, sd)
    # bones without a usable child inherit the parent's alignment (hands, finger tips, head)
    for i, name in enumerate(tgt.bones):
        if align[name] == Quaternion() and CHILD.get(name) is None and tgt.parent[i] >= 0:
            align[name] = align[tgt.bones[tgt.parent[i]]]
    return align


def bake_clip(tgt, spec):
    """spec: {name, file, action?, mapping: 'ual'|'rb', start?, end?, speed?, arms_from?}"""
    objs = import_any(spec["file"])
    src = [o for o in objs if o.type == 'ARMATURE'][0]
    if spec.get("action"):
        act = bpy.data.actions[spec["action"]]
    else:
        act = src.animation_data.action if src.animation_data else None
        if act is None:
            act = [a for a in bpy.data.actions if a.users and "Footsteps" not in a.name][-1]
    if src.animation_data is None:
        src.animation_data_create()
    mapping = {v: k for k, v in UAL_MAP.items()} if spec["mapping"] == "ual" else {n: n for n in tgt.bones}
    src_rest = Pose(src, rest=True)
    if spec["mapping"] == "ual":
        src_root = "DEF-hips"
    else:
        src_root = "Bip01 Pelvis"
    align = compute_align(tgt, src_rest, mapping)
    hip_scale = (tgt.rest.pos[tgt.bones[0]].z) / max(src_rest.pos[src_root].z, 1e-3)

    overrides_by_frame = None
    if spec.get("arms_from"):
        overrides_by_frame = arms_pose(tgt, spec["arms_from"])

    src.animation_data.action = act
    f0, f1 = act.frame_range
    f0 = spec.get("start", f0)
    f1 = spec.get("end", f1)
    scene = bpy.context.scene
    fps_src = spec.get("src_fps", scene.render.fps)
    print("CLIP", spec["name"], act.name, f0, f1, "fps", fps_src)
    speed = spec.get("speed", 1.0)
    n = max(2, int(round((f1 - f0) / fps_src * 30.0 / speed)) + 1)
    frames = []
    for k in range(n):
        f = f0 + (f1 - f0) * k / (n - 1)
        scene.frame_set(int(f), subframe=f - int(f))
        pose = Pose(src)
        rot, pos = retarget_frame(tgt, pose, src_rest, mapping, align, hip_scale, src_root, overrides_by_frame)
        if spec.get("in_place"):
            rp = tgt.rest.pos[tgt.bones[0]]
            pos = Vector((rp.x, rp.y, pos.z))
        frames.append((rot, pos))
    # remove the source rig
    for o in objs:
        bpy.data.objects.remove(o, do_unlink=True)
    return {"name": spec["name"], "frames": frames, "loop": spec.get("loop", False)}


def arms_pose(tgt, spec):
    """Local (parent-relative) arm rotations taken from one frame of another clip."""
    clip = bake_clip(tgt, dict(spec, name="_arms"))
    rot, _ = clip["frames"][min(spec.get("frame", 0), len(clip["frames"]) - 1)]
    out = {}
    for i, name in enumerate(tgt.bones):
        if any(k in name for k in ARM_BONES) and tgt.parent[i] >= 0:
            p = tgt.bones[tgt.parent[i]]
            out[name] = rot[p].inverted() @ rot[name]
    return out


# ------------------------------------------------------------------ mesh
def yup(v):
    return (v[0], v[2], -v[1])


def qyup(q):
    return (q.w, q.x, q.z, -q.y)


def export_mesh(tgt, cfg):
    """Triangulated, loop-split vertices with top-4 skin weights."""
    verts = []
    vmap = {}
    subs = []
    indices = []
    arm_bone_ids = {i for i, n in enumerate(tgt.bones) if any(k in n for k in ARM_BONES[1:])}
    for obj in tgt.meshes:
        me = obj.data  # bind pose (armature deformation is a modifier)
        me.calc_loop_triangles()
        uv = me.uv_layers.active
        has_tan = uv is not None
        if has_tan:
            me.calc_tangents(uvmap=uv.name)
            uv = me.uv_layers.active  # the old reference is invalid after calc_tangents
        mw = obj.matrix_world
        nm = mw.to_3x3().inverted().transposed()
        group_bone = {}
        for g in obj.vertex_groups:
            if g.name in tgt.index:
                group_bone[g.index] = tgt.index[g.name]
            elif g.name in tgt.arm.data.bones:
                group_bone[g.index] = tgt.kept_ancestor(g.name)
        weights_cache = {}

        def weights(vi):
            if vi in weights_cache:
                return weights_cache[vi]
            acc = {}
            for ge in me.vertices[vi].groups:
                b = group_bone.get(ge.group)
                if b is not None and ge.weight > 0:
                    acc[b] = acc.get(b, 0) + ge.weight
            top = sorted(acc.items(), key=lambda kv: -kv[1])[:4]
            s = sum(w for _, w in top) or 1.0
            top = [(b, w / s) for b, w in top]
            while len(top) < 4:
                top.append((0, 0.0))
            weights_cache[vi] = top
            return top

        by_mat = {}
        for tri in me.loop_triangles:
            if tgt.arms_only:
                # keep triangles dominated by arm/hand bones (first-person view model)
                ok = all(weights(me.loops[l].vertex_index)[0][0] in arm_bone_ids for l in tri.loops)
                if not ok:
                    continue
            by_mat.setdefault(tri.material_index, []).append(tri)
        for mi, tris in sorted(by_mat.items()):
            mat = obj.material_slots[mi].material.name if mi < len(obj.material_slots) and obj.material_slots[mi].material else "default"
            if mat in cfg.get("skip_materials", []):
                continue
            start = len(indices)
            for tri in tris:
                for l in tri.loops:
                    loop = me.loops[l]
                    vi = loop.vertex_index
                    p = mw @ me.vertices[vi].co
                    n = (nm @ loop.normal).normalized()
                    t = (mw.to_3x3() @ loop.tangent).normalized() if has_tan else Vector((1, 0, 0))
                    sign = loop.bitangent_sign if has_tan else 1.0
                    u = uv.data[l].uv if uv else (0, 0)
                    w = weights(vi)
                    key = (obj.name, vi, round(n.x, 2), round(n.y, 2), round(n.z, 2), round(u[0], 4), round(u[1], 4))
                    idx = vmap.get(key)
                    if idx is None:
                        idx = len(verts)
                        vmap[key] = idx
                        verts.append((yup(p), yup(n), yup(t) + (sign,), (u[0], 1.0 - u[1]), w))
                    indices.append(idx)
            subs.append((mat, start, len(indices) - start))
    return verts, indices, subs


def write_wxm(path, tgt, verts, indices, subs, clips):
    def s8(x):
        return max(-127, min(127, int(round(x * 127))))

    def name(b):
        e = b.encode()
        return struct.pack("<B", len(e)) + e

    out = bytearray(b"WXM1")
    nb = len(tgt.bones)
    out += struct.pack("<I", nb)
    for i, bn in enumerate(tgt.bones):
        p = tgt.parent[i]
        rw = tgt.rest.rot[bn]
        tw = tgt.rest.pos[bn]
        if p >= 0:
            pn = tgt.bones[p]
            lr = tgt.rest.rot[pn].inverted() @ rw
            lt = tgt.rest.rot[pn].inverted() @ (tw - tgt.rest.pos[pn])
        else:
            lr, lt = rw, tw
        bind = (Matrix.Translation(Vector(yup(tw))) @ Quaternion(qyup(rw)).to_matrix().to_4x4()).inverted()
        out += name(bn) + struct.pack("<i", p) + struct.pack("<3f", *yup(lt)) + struct.pack("<4f", *qyup(lr))
        out += struct.pack("<16f", *[bind[r][c] for c in range(4) for r in range(4)])
    out += struct.pack("<I", len(subs))
    for mat, start, count in subs:
        out += name(mat) + struct.pack("<II", start, count)
    out += struct.pack("<I", len(verts))
    for p, n, t, uv, w in verts:
        ws = [int(round(x * 255)) for _, x in w]
        ws[0] += 255 - sum(ws)
        out += struct.pack("<3f", *p) + struct.pack("<4b", s8(n[0]), s8(n[1]), s8(n[2]), 0)
        out += struct.pack("<4b", s8(t[0]), s8(t[1]), s8(t[2]), 127 if t[3] >= 0 else -127)
        out += struct.pack("<2f", *uv) + struct.pack("<4B", *[b for b, _ in w]) + struct.pack("<4B", *ws)
    wide = len(verts) > 65535
    if wide:
        out[0:4] = b"WXM2"  # 32-bit indices
    out += struct.pack("<I", len(indices))
    out += struct.pack(f"<{len(indices)}{'I' if wide else 'H'}", *indices)
    out += struct.pack("<I", len(clips))
    for c in clips:
        frames = c["frames"]
        out += name(c["name"]) + struct.pack("<fIB", 30.0, len(frames), 1 if c["loop"] else 0)
        for rot, pos in frames:
            out += struct.pack("<3f", *yup(pos))
            for i, bn in enumerate(tgt.bones):
                p = tgt.parent[i]
                q = rot[bn] if p < 0 else rot[tgt.bones[p]].inverted() @ rot[bn]
                q = Quaternion(qyup(q.normalized()))
                if q.w < 0:
                    q = -q
                out += struct.pack("<4h", *[max(-32767, min(32767, int(round(x * 32767)))) for x in (q.w, q.x, q.y, q.z)])
    with open(path, "wb") as f:
        f.write(out)
    print(f"WROTE {path} bones={nb} verts={len(verts)} tris={len(indices)//3} clips={[c['name'] for c in clips]} bytes={len(out)}")


def export_textures(tgt, cfg, out_dir):
    """Save the avatar's colour/normal maps next to the .wxm (post-processed by build_assets.py)."""
    seen = {}
    for obj in tgt.meshes:
        for slot in obj.material_slots:
            m = slot.material
            if not m or not m.node_tree:
                continue
            for node in m.node_tree.nodes:
                if node.type == 'TEX_IMAGE' and node.image and node.image.filepath:
                    seen.setdefault(m.name, []).append(bpy.path.abspath(node.image.filepath))
    return seen


def main():
    cfg = json.load(open(sys.argv[sys.argv.index("--") + 1]))
    roots = {"RB": os.environ.get("ROCKETBOX", "Assets"), "UAL": os.environ.get("UAL_GLB", "")}

    def fix(spec):
        spec = dict(spec)
        for k in ("file", "avatar"):
            if k in spec:
                head, _, rest = spec[k].partition("/")
                spec[k] = os.path.join(roots[head], rest) if head == "RB" else (roots["UAL"] if spec[k] == "UAL" else spec[k])
        if "arms_from" in spec:
            spec["arms_from"] = fix(spec["arms_from"])
        return spec

    for ch in cfg["characters"]:
        if ch["clips"] == "ZOMBIE":
            ch["clips"] = cfg["zombie_clips"]
        ch.update(fix(ch))
        ch["clips"] = [fix(c) for c in ch["clips"]]
    only = os.environ.get("ONLY")
    if only:
        cfg["characters"] = [c for c in cfg["characters"] if c["name"] in only.split(",")]
    os.makedirs(cfg["out"], exist_ok=True)
    manifest = {}
    for ch in cfg["characters"]:
        reset()
        tgt = Target(ch["avatar"], ch.get("arms_only", False))
        clips = []
        for spec in ch["clips"]:
            clips.append(bake_clip(tgt, spec))
        verts, indices, subs = export_mesh(tgt, ch)
        write_wxm(os.path.join(cfg["out"], ch["name"] + ".wxm"), tgt, verts, indices, subs, clips)
        manifest[ch["name"]] = export_textures(tgt, ch, cfg["out"])
        json.dump(manifest, open(os.path.join(cfg["out"], "textures_%s.json" % ch["name"]), "w"), indent=1)


main()
