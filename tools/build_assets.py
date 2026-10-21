#!/usr/bin/env python3
"""Woxlium asset pipeline (first-person 3D build).

Inputs
  build/wxm/*.wxm, textures_*.json      characters from tools/blender/wxm_export.py
  build/wxm_props/*.wxm, textures_props  props from tools/blender/props_export.py
  $ROCKETBOX                           Microsoft Rocketbox Assets/ dir (MIT) for avatar textures
  $PH_TEX                              Poly Haven texture downloads (CC0): ground/walls + sky.jpg
  art/source/*.jpg                     the provided concept sheets (VFX + UI)

Outputs
  app/src/main/assets/*.wxm            meshes, skeletons, animation clips
  app/src/main/assets/*.pvr            ETC2 (+mips) textures, compressed with etcpak
  app/src/main/res/drawable-nodpi/     UI buttons and icons
  rust/woxlium_core/src/assets_gen.rs  texture / material / model tables
  build/preview_assets/                identical files for the desktop harness
"""
import json
import os
import shutil
import subprocess
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import sheets  # noqa: E402

ROOT = sheets.ROOT
ASSETS = sheets.ASSETS
DRAWABLE = sheets.DRAWABLE
PREVIEW = sheets.PREVIEW
GEN = os.path.join(ROOT, "rust", "woxlium_core", "src", "assets_gen.rs")
WXM = os.path.join(ROOT, "build", "wxm")
WXM_PROPS = os.path.join(ROOT, "build", "wxm_props")
ROCKETBOX = os.environ.get("ROCKETBOX", os.path.join(ROOT, "build", "dl", "rocketbox", "Assets"))
PH_TEX = os.environ.get("PH_TEX", os.path.join(ROOT, "build", "dl", "tex"))
ETCPAK = os.environ.get("ETCPAK", "etcpak")
TMP = os.path.join(ROOT, "build", "tex_tmp")

AVATAR_DIRS = {"zombie_a": "Male_Adult_07", "zombie_b": "Male_Adult_12", "zombie_c": "Male_Adult_04",
               "zombie_d": "Male_Adult_18", "player_arms": "Male_Adult_05"}
PROPS = ["concrete_road_barrier", "covered_car", "street_lamp_01", "old_tyre", "trashbag", "metal_trash_can",
         "wooden_crate_01", "medical_box", "facade_a", "facade_b", "utility_box_01", "fire_hydrant",
         "dead_tree_trunk", "Barrel_01"]
SURFACES = {  # name -> (poly haven id, albedo size)
    "road": ("road_damaged", 1024), "sidewalk": ("concrete_pavement", 1024), "mud": ("muddy_tracks", 1024),
    "rubble": ("rubble", 1024), "grass": ("sparse_grass", 1024), "wall": ("cracked_concrete_wall", 1024),
    "wall2": ("concrete_wall_008", 1024), "plaster": ("worn_cracked_plaster", 1024),
}

textures = []   # (file, flags)
materials = []  # (name, albedo idx, normal idx, flags)
MAT_ALPHA = 1
MAT_EMISSIVE = 2
MAT_SKIN = 4


def tex_index(name):
    for i, (n, _) in enumerate(textures):
        if n == name:
            return i
    raise KeyError(name)


def write_pvr(img, name, rgba=False, mips=True):
    """Compress with etcpak (ETC2 RGB / RGBA + mip chain) into assets + preview."""
    os.makedirs(TMP, exist_ok=True)
    png = os.path.join(TMP, name + ".png")
    img.convert("RGBA" if rgba else "RGB").save(png)
    out = os.path.join(ASSETS, name + ".pvr")
    args = [ETCPAK, "-c", "etc2_rgba" if rgba else "etc2_rgb"] + (["-m"] if mips else []) + [png, out]
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    shutil.copy(out, os.path.join(PREVIEW, name + ".pvr"))
    textures.append((name + ".pvr", 1 if rgba else 0))
    return len(textures) - 1


def ao_mul(diff, arm_path, strength=0.85):
    if not arm_path or not os.path.exists(arm_path):
        return diff
    ao = np.asarray(Image.open(arm_path).convert("RGB").resize(diff.size, Image.LANCZOS)).astype(np.float32)[..., 0:1] / 255.0
    d = np.asarray(diff.convert("RGB")).astype(np.float32)
    return Image.fromarray((d * (1 - strength + strength * ao)).clip(0, 255).astype(np.uint8))


# ------------------------------------------------------------------ zombify
def skin_mask(a):
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    mx = np.maximum(np.maximum(r, g), b)
    mn = np.minimum(np.minimum(r, g), b)
    sat = (mx - mn) / np.maximum(mx, 1)
    m = (r > g) & (g > b * 0.95) & (sat > 0.18) & (sat < 0.72) & (mx > 60) & ((r - g) > 12)
    m = m.astype(np.float32)
    return np.asarray(Image.fromarray((m * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(3))).astype(np.float32) / 255.0


def blood_stamps():
    """Splatter shapes cut from the provided blood VFX sheet."""
    cells = sheets.blood_vfx_cells()
    out = []
    for row in (0, 2, 3):
        for c, a in sheets.extract_vfx("vfx_blood.jpg", cells[row], soft=110.0, label_safe=False)[1:6]:
            out.append((c, a))
    return out


def noise(size, seed, scale):
    rng = np.random.default_rng(seed)
    small = rng.random((max(2, size // scale), max(2, size // scale))).astype(np.float32)
    im = Image.fromarray((small * 255).astype(np.uint8)).resize((size, size), Image.BICUBIC).filter(ImageFilter.GaussianBlur(size / scale * 0.6))
    a = np.asarray(im).astype(np.float32) / 255.0
    return (a - a.min()) / max(a.max() - a.min(), 1e-3)


def zombify(img, seed, head=False, amount=1.0, blood=True):
    size = img.size[0]
    a = np.asarray(img.convert("RGB")).astype(np.float32)
    sk = skin_mask(a)[..., None] * amount
    lum = a.mean(-1, keepdims=True)
    # skin: pale, grey-green, bruised blotches and dark veins
    blot = noise(size, seed, 40)[..., None]
    vn = noise(size, seed + 1, 18)[..., None]
    veins = np.clip(1 - np.abs(vn - 0.5) * 18, 0, 1) * 0.28
    dead = lum * np.array([0.78, 0.86, 0.70]) * (0.85 + 0.3 * blot) + 12
    dead = dead * (1 - veins) + veins * np.array([60, 40, 60])
    bruise = np.clip((blot - 0.7) * 2.5, 0, 1) * 0.4
    dead = dead * (1 - bruise) + bruise * np.array([95, 70, 85])
    out = a * (1 - sk) + dead * sk
    # clothes: grime + fading
    grime = noise(size, seed + 2, 28)[..., None]
    out = out * (1 - (1 - sk) * (0.22 * grime + 0.06))
    out = out * 0.92 + out.mean(-1, keepdims=True) * 0.08
    rng = np.random.default_rng(seed)
    stamps = blood_stamps()
    res = Image.fromarray(out.clip(0, 255).astype(np.uint8))
    n = (5 if head else 14) if blood else 0
    for _ in range(n):
        c, al = stamps[rng.integers(len(stamps))]
        s = rng.uniform(0.10, 0.28) * size if not head else rng.uniform(0.08, 0.16) * size
        st = sheets.to_image(c * np.array([0.6, 0.15, 0.12]), al).resize((int(s), int(s * al.shape[0] / al.shape[1]) or 1), Image.LANCZOS)
        st = st.rotate(rng.uniform(0, 360), expand=True)
        if head:
            # around mouth / chin of the Rocketbox face layout
            x = int(size * rng.uniform(0.38, 0.6) - st.width / 2)
            y = int(size * rng.uniform(0.38, 0.52) - st.height / 2)
        else:
            x = int(rng.uniform(0, size - st.width))
            y = int(rng.uniform(0, size - st.height))
        res.alpha_composite(st, (x, y)) if res.mode == "RGBA" else res.paste(st, (x, y), st)
    if head:
        # milky dead eyes: whiten iris tile (bottom-centre of the head atlas)
        r = np.asarray(res).astype(np.float32)
        y0, y1, x0, x1 = int(size * 0.83), int(size * 0.98), int(size * 0.2), int(size * 0.36)
        tile = r[y0:y1, x0:x1]
        tl = tile.mean(-1, keepdims=True)
        sat = (tile.max(-1, keepdims=True) - tile.min(-1, keepdims=True)) / 255.0
        iris = np.clip((sat - 0.12) * 4 + (150 - tl) / 120, 0, 1)
        pale = tl * 0.35 + np.array([170, 172, 158])
        r[y0:y1, x0:x1] = tile * (1 - iris * 0.85) + pale * iris * 0.85
        # dark sockets
        yy, xx = np.mgrid[0:size, 0:size] / size
        for ex in (0.42, 0.58):
            d = ((xx - ex) / 0.07) ** 2 + ((yy - 0.27) / 0.035) ** 2
            k = np.clip(1.2 - d, 0, 1)[..., None] * 0.55
            r = r * (1 - k) + np.array([40, 25, 30]) * k
        res = Image.fromarray(r.clip(0, 255).astype(np.uint8))
    return res


# ------------------------------------------------------------------ builders
def build_characters():
    models = []
    for ch, avatar in AVATAR_DIRS.items():
        src = os.path.join(WXM, ch + ".wxm")
        shutil.copy(src, os.path.join(ASSETS, ch + ".wxm"))
        shutil.copy(src, os.path.join(PREVIEW, ch + ".wxm"))
        models.append(ch + ".wxm")
        tdir = os.path.join(ROCKETBOX, "Avatars", "Adults", avatar, "Textures")
        code = [f for f in os.listdir(tdir) if f.endswith("_body_color.tga")][0].split("_")[0]
        seed = sum(map(ord, ch))
        zombie = ch.startswith("zombie")
        for part, size, nsize in (("body", 1024, 1024), ("head", 512, 512)):
            col = Image.open(os.path.join(tdir, f"{code}_{part}_color.tga")).convert("RGB").resize((size, size), Image.LANCZOS)
            if zombie:
                col = zombify(col, seed + (7 if part == "head" else 0), head=part == "head")
            else:
                # survivor: same person, just dirtier knuckles / sleeves
                col = zombify(col, seed, head=False, amount=0.0, blood=False)
            ai = write_pvr(col, f"{ch}_{part}")
            nrm = Image.open(os.path.join(tdir, f"{code}_{part}_normal.tga")).convert("RGB").resize((nsize // 2, nsize // 2), Image.LANCZOS)
            ni = write_pvr(nrm, f"{ch}_{part}_n")
            materials.append((f"{code}_{part}", ai, ni, MAT_SKIN))
        opath = os.path.join(tdir, f"{code}_opacity_color.tga")
        if os.path.exists(opath):
            op = Image.open(opath).convert("RGBA").resize((256, 256), Image.LANCZOS)
            oi = write_pvr(op, f"{ch}_opacity", rgba=True)
            materials.append((f"{code}_opacity", oi, -1, MAT_ALPHA))
    return models


def build_props():
    meta = json.load(open(os.path.join(WXM_PROPS, "textures_props.json")))
    meta.update(json.load(open(os.path.join(WXM_PROPS, "textures_facades.json"))))
    models = []
    done = {}
    for p in PROPS:
        shutil.copy(os.path.join(WXM_PROPS, p + ".wxm"), os.path.join(ASSETS, p + ".wxm"))
        shutil.copy(os.path.join(WXM_PROPS, p + ".wxm"), os.path.join(PREVIEW, p + ".wxm"))
        models.append(p + ".wxm")
        for mat, files in meta[p]["materials"].items():
            diff = next((f for f in files if "_diff_" in f), None)
            nor = next((f for f in files if "_nor_gl_" in f), None)
            arm = next((f for f in files if "_arm_" in f), None)
            flags = MAT_EMISSIVE if ("bulb" in mat or "glass" in mat and "lamp" in mat) else 0
            key = (diff, nor)
            if key not in done:
                size = 1024 if p.startswith("facade") else 512
                base = os.path.basename(diff).replace("_diff_1k.jpg", "")
                d = ao_mul(Image.open(diff).convert("RGB").resize((size, size), Image.LANCZOS), arm)
                ai = write_pvr(d, "p_" + base)
                ni = write_pvr(Image.open(nor).convert("RGB").resize((size // 2, size // 2), Image.LANCZOS), "p_" + base + "_n")
                done[key] = (ai, ni)
            ai, ni = done[key]
            materials.append((mat, ai, ni, flags))
    return models, {p: meta[p]["size"] for p in PROPS}


def build_surfaces():
    for name, (pid, size) in SURFACES.items():
        d = Image.open(os.path.join(PH_TEX, f"{pid}_Diffuse.jpg")).convert("RGB").resize((size, size), Image.LANCZOS)
        d = ao_mul(d, os.path.join(PH_TEX, f"{pid}_AO.jpg"), 0.7)
        ai = write_pvr(d, "s_" + name)
        n = Image.open(os.path.join(PH_TEX, f"{pid}_nor_gl.jpg")).convert("RGB").resize((size // 2, size // 2), Image.LANCZOS)
        ni = write_pvr(n, "s_" + name + "_n")
        materials.append(("surface_" + name, ai, ni, 0))
    sky = Image.open(os.path.join(PH_TEX, "sky.jpg")).convert("RGB")
    # night grade: darker, cold, keep the overcast structure
    s = np.asarray(sky).astype(np.float32)
    s = s * np.array([0.42, 0.46, 0.55]) + 4
    write_pvr(Image.fromarray(s.clip(0, 255).astype(np.uint8)), "sky", mips=False)


def build_vfx():
    """Blood / impact / dust / shockwave flipbooks from the provided sheets."""
    anims = []
    names = ["V_SPARK", "V_SLASH", "V_DUST", "V_SWEAT", "V_SHOCK", "V_SHIELD", "V_HEAL", "V_SMOKE"]
    for name, cells in zip(names, sheets.combat_vfx_cells()):
        if name in ("V_SWEAT", "V_HEAL"):
            continue
        fr = sheets.extract_vfx("vfx_combat.jpg", cells)
        anims.append((name, [sheets.to_image(c, a) for c, a in fr], 80))
    for name, cells in zip(["V_BLOOD", "V_CLAW", "V_POOL", "V_SPRAY", "V_GAS", "V_PUKE"], sheets.blood_vfx_cells()):
        if name in ("V_GAS", "V_PUKE"):
            continue
        fr = sheets.extract_vfx("vfx_blood.jpg", cells, soft=110.0, label_safe=False)
        anims.append((name, [sheets.to_image(c, a) for c, a in fr], 100))
    anims.append(("G_DISC", [sheets.soft_disc(64)], 64))
    anims.append(("G_RING", [sheets.ring(64)], 64))
    anims.append(("G_ARROW", [sheets.arrow(32)], 32))
    anims.append(("G_WHITE", [Image.new("RGBA", (8, 8), (255, 255, 255, 255))], 8))
    size = 1024
    atlas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    pk = sheets.Packer(size)
    items = []
    for ai, (name, imgs, px) in enumerate(anims):
        ref = max(max(i.width, i.height) for i in imgs)
        scaled = [i.resize((max(1, int(i.width * px / ref)), max(1, int(i.height * px / ref))), Image.LANCZOS) if ref > px else i for i in imgs]
        anims[ai] = (name, scaled, max(max(i.width, i.height) for i in scaled))
        for fi, im in enumerate(scaled):
            items.append((im.height, ai, fi))
    items.sort(reverse=True)
    pos = {}
    for _, ai, fi in items:
        im = anims[ai][1][fi]
        pos[(ai, fi)] = pk.place(im.width, im.height)
        atlas.paste(im, pos[(ai, fi)])
    # premultiply (GL blending is ONE, ONE_MINUS_SRC_ALPHA)
    a = np.asarray(atlas).astype(np.float32)
    a[..., :3] *= a[..., 3:4] / 255.0
    write_pvr(Image.fromarray(a.astype(np.uint8), "RGBA"), "vfx", rgba=True)
    lines = ["pub const VFX_FRAMES: &[[f32; 6]] = &["]
    consts = []
    off = 0
    for ai, (name, imgs, ref) in enumerate(anims):
        consts.append(f"pub const {name}: (usize, usize) = ({off}, {len(imgs)});")
        for fi, im in enumerate(imgs):
            x, y = pos[(ai, fi)]
            lines.append(f"    [{(x + .5) / size:.5f}, {(y + .5) / size:.5f}, {(x + im.width - .5) / size:.5f}, {(y + im.height - .5) / size:.5f}, {im.width / ref:.4f}, {im.height / ref:.4f}],")
        off += len(imgs)
    lines.append("];")
    return consts + lines


UI_BUTTONS = {
    "btn_play": (27, 60, 362, 136), "btn_endless": (29, 234, 284, 63), "btn_settings": (367, 332, 316, 59),
    "btn_quests": (766, 327, 260, 63), "btn_exit": (1086, 328, 263, 60),
}
UI_ICONS = {
    "ic_punch": (746, 43, 59, 60), "ic_dodge": (912, 41, 60, 62), "ic_block": (1154, 43, 61, 60),
    "ic_pause": (1236, 39, 63, 64), "ic_resume": (48, 706, 55, 55), "ic_restart": (154, 704, 56, 57),
    "ic_check": (745, 611, 59, 59), "ic_lobby": (521, 608, 57, 60), "ic_gear": (1240, 233, 59, 62),
    "ic_scroll": (1317, 233, 63, 63),
}


def build_ui():
    for f in os.listdir(DRAWABLE):
        os.remove(os.path.join(DRAWABLE, f))
    ui = Image.open(os.path.join(sheets.SRC, "ui_buttons.jpg")).convert("RGB")
    bg = np.array([89, 122, 92], np.float32)
    for name, (x, y, w, h) in UI_BUTTONS.items():
        sub = np.asarray(ui.crop((x - 4, y - 4, x + w + 4, y + h + 4))).astype(np.float32)
        col, alpha = sheets.key_character(sub, bg)
        alpha = sheets.strip_small(alpha, 0.3)
        sheets.to_image(*sheets.trim(col, alpha, 1)).save(os.path.join(DRAWABLE, name + ".webp"), "WEBP", quality=88, method=6)
    for name, rect in UI_ICONS.items():
        img = sheets.icon_from_ui(ui, rect)
        img.resize((128, int(128 * img.height / img.width)), Image.LANCZOS).save(os.path.join(DRAWABLE, name + ".webp"), "WEBP", quality=88, method=6)
    emb = ui.crop((40, 68, 158, 186)).resize((150, 150), Image.LANCZOS)
    icon = Image.new("RGBA", (192, 192), (0, 0, 0, 0))
    ImageDraw.Draw(icon).rounded_rectangle((4, 4, 188, 188), radius=40, fill=(20, 26, 20, 255), outline=(120, 150, 90, 255), width=5)
    mask = Image.new("L", (150, 150), 0)
    ImageDraw.Draw(mask).ellipse((2, 2, 148, 148), fill=255)
    icon.paste(emb, (21, 21), mask)
    icon.save(os.path.join(DRAWABLE, "ic_launcher.webp"), "WEBP", quality=90, method=6)


def write_gen(models, prop_sizes, vfx_lines):
    with open(os.path.join(ASSETS, "textures.txt"), "w") as f:
        f.write("\n".join(n for n, _ in textures) + "\n")
    L = ["// @generated by tools/build_assets.py — do not edit.", "#![allow(dead_code)]", ""]
    L.append("/// Texture files (asset names) in slot order.")
    L.append("pub const TEXTURES: &[&str] = &[" + ", ".join(f'"{n}"' for n, _ in textures) + "];")
    L.append("pub const MAT_ALPHA: u32 = 1;\npub const MAT_EMISSIVE: u32 = 2;\npub const MAT_SKIN: u32 = 4;")
    L.append("/// (material name, albedo slot, normal slot or -1, flags)")
    L.append("pub const MATERIALS: &[(&str, i32, i32, u32)] = &[")
    for n, a, nn, fl in materials:
        L.append(f'    ("{n}", {a}, {nn}, {fl}),')
    L.append("];")
    L.append("pub const MODELS: &[&str] = &[" + ", ".join(f'"{m}"' for m in models) + "];")
    for name, (_, _) in SURFACES.items():
        L.append(f'pub const SURF_{name.upper()}: &str = "surface_{name}";')
    L.append(f'pub const SKY_TEXTURE: &str = "sky.pvr";\npub const VFX_TEXTURE: &str = "vfx.pvr";')
    L.append("/// Prop sizes in metres (x, y, z) from Blender.")
    L.append("pub const PROP_SIZES: &[(&str, [f32; 3])] = &[" + ", ".join(f'("{k}", [{v[0]:.3f}, {v[1]:.3f}, {v[2]:.3f}])' for k, v in prop_sizes.items()) + "];")
    L += vfx_lines
    with open(GEN, "w") as f:
        f.write("\n".join(L) + "\n")


if __name__ == "__main__":
    for d in (ASSETS, PREVIEW, DRAWABLE):
        os.makedirs(d, exist_ok=True)
    for f in os.listdir(ASSETS):
        os.remove(os.path.join(ASSETS, f))
    models = build_characters()
    pm, sizes = build_props()
    build_surfaces()
    vfx = build_vfx()
    build_ui()
    write_gen(models + pm, sizes, vfx)
    total = sum(os.path.getsize(os.path.join(ASSETS, f)) for f in os.listdir(ASSETS))
    print(f"textures={len(textures)} materials={len(materials)} models={len(models) + len(pm)} assets={total / 1e6:.1f} MB")
