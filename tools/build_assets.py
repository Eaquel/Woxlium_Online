#!/usr/bin/env python3
"""Woxlium asset pipeline.

Turns the concept sheets in art/source (green-screen sprite sheets) into the
optimized runtime assets used by the game:

  app/src/main/assets/sprites.webp   one 2048x2048 atlas: characters, VFX, decals
  app/src/main/assets/tex_*.webp     256x256 seamless ground / wall textures
  app/src/main/res/drawable-nodpi/   UI buttons and HUD icons (WebP)
  rust/woxlium_core/src/atlas_gen.rs frame table consumed by the Rust renderer
  build/preview_assets/*.rgba        raw copies for the desktop preview harness

Run: python3 tools/build_assets.py
"""
import os
import sys

import numpy as np
from PIL import Image, ImageFilter
from scipy import ndimage as nd

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "art", "source")
ASSETS = os.path.join(ROOT, "app", "src", "main", "assets")
DRAWABLE = os.path.join(ROOT, "app", "src", "main", "res", "drawable-nodpi")
RUST_GEN = os.path.join(ROOT, "rust", "woxlium_core", "src", "atlas_gen.rs")
PREVIEW = os.path.join(ROOT, "build", "preview_assets")

ATLAS = 2048
CHAR_REF_PX = 190  # standing character height inside the atlas


def load(name):
    return np.asarray(Image.open(os.path.join(SRC, name)).convert("RGB")).astype(np.float32)


def sample_bg(a):
    # median of the outermost pixel ring = backdrop colour
    ring = np.concatenate([a[2, :], a[-3, :], a[:, 2], a[:, -3]])
    return np.median(ring, axis=0)


def key_character(a, bg):
    """Green-screen key tuned for opaque characters (keeps grey / olive cloth)."""
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    dist = np.abs(a - bg).sum(-1)
    green = g - np.maximum(r, b)
    bg_green = bg[1] - max(bg[0], bg[2])
    alpha = np.maximum(dist / 70.0, (bg_green - green) / 14.0)
    alpha = np.clip(alpha - 0.25, 0, 1) / 0.75
    # despill: pull green down towards the other channels on soft edges
    out = a.copy()
    lim = np.maximum(r, b) + 6
    out[..., 1] = np.where(alpha < 0.98, np.minimum(g, lim), g)
    return out, alpha


def key_vfx(a, bg, soft=150.0):
    """Difference matte for glowing / translucent effects, un-premultiplied."""
    dist = np.abs(a - bg).sum(-1)
    alpha = np.clip((dist - 18) / soft, 0, 1)
    safe = np.maximum(alpha, 1e-3)[..., None]
    col = (a - bg[None, None, :] * (1 - alpha[..., None])) / safe
    return np.clip(col, 0, 255), alpha


def strip_small(alpha, min_frac=0.02, thr=0.35):
    """Drop tiny islands (frame numbers, border remnants)."""
    m = alpha > thr
    lab, n = nd.label(m)
    if n == 0:
        return alpha
    sizes = nd.sum(m, lab, range(1, n + 1))
    keep_ids = [i + 1 for i, s in enumerate(sizes) if s >= max(sizes) * min_frac and s > 40]
    keep = np.isin(lab, keep_ids)
    keep = nd.binary_dilation(keep, iterations=3)
    return alpha * keep


def trim(col, alpha, pad=2):
    ys, xs = np.where(alpha > 0.04)
    if len(xs) == 0:
        return None
    x0, x1 = max(xs.min() - pad, 0), min(xs.max() + pad + 1, alpha.shape[1])
    y0, y1 = max(ys.min() - pad, 0), min(ys.max() + pad + 1, alpha.shape[0])
    return col[y0:y1, x0:x1], alpha[y0:y1, x0:x1]


def to_image(col, alpha):
    rgba = np.dstack([col, alpha * 255]).clip(0, 255).astype(np.uint8)
    return Image.fromarray(rgba, "RGBA")


def grid(xs, ys, inset=6):
    cells = []
    for (y0, y1) in ys:
        for (x0, x1) in xs:
            cells.append((x0 + inset, y0 + inset, x1 - inset, y1 - inset))
    return cells


def extract_chars(sheet, cells, pick=None):
    a = load(sheet)
    bg = sample_bg(a)
    out = []
    for i, (x0, y0, x1, y1) in enumerate(cells):
        if pick is not None and i not in pick:
            continue
        sub = a[y0:y1, x0:x1]
        col, alpha = key_character(sub, bg)
        alpha[:30, :34] = 0  # frame numbers live in the top-left corner
        alpha = strip_small(alpha, 0.04)
        t = trim(col, alpha)
        out.append(t)
    if pick is not None:
        order = {p: k for k, p in enumerate(pick)}
        idx = sorted(range(len(out)), key=lambda k: k)
        out = [out[i] for i in idx]
    return out


def extract_vfx(sheet, cells, soft=150.0, label_safe=True):
    a = load(sheet)
    bg = sample_bg(a)
    out = []
    for (x0, y0, x1, y1) in cells:
        col, alpha = key_vfx(a[y0:y1, x0:x1], bg, soft)
        if label_safe:
            # soft edge fade hides frame numbers / row titles without hard notches
            hh, ww = alpha.shape
            yy, xx = np.mgrid[0:hh, 0:ww]
            edge = np.minimum(np.minimum(xx, ww - 1 - xx), np.minimum(yy, hh - 1 - yy))
            alpha = alpha * np.clip((edge - 10) / 16.0, 0, 1)
        t = trim(col, alpha, pad=3)
        if t is not None:
            out.append(t)
    return out


# ---------------------------------------------------------------- sheets
Z_COLS = [(17, 237), (248, 469), (478, 699), (708, 929), (938, 1159), (1170, 1390)]
Z_ROWS = [(16, 192), (206, 383), (397, 575), (575, 766)]
P_COLS = [(21, 264), (275, 499), (509, 716), (729, 943), (953, 1172), (1185, 1391)]
P_ROWS = [(14, 388), (397, 765)]

ZG = grid(Z_COLS, Z_ROWS)
PG = grid(P_COLS, P_ROWS)


def death_cells():
    c = P_COLS
    cells = [(c[i][0], 14, c[i][1], 388) for i in range(4)]
    cells += [(c[4][0], 14, c[4][1], 199), (c[5][0], 14, c[5][1], 199)]
    cells += [(c[4][0], 207, c[4][1], 388), (c[5][0], 207, c[5][1], 388)]
    cells += [(x0, 397, x1, 579) for (x0, x1) in c]
    cells += [(x0, 588, x1, 765) for (x0, x1) in c]
    return [(x0 + 6, y0 + 6, x1 - 6, y1 - 6) for (x0, y0, x1, y1) in cells]


def combat_vfx_cells():
    """Rows of the combat VFX sheet; first sequence (left half) of each row."""
    a = load("vfx_combat.jpg")
    lum = a.mean(-1)
    bands = [(30, 108), (128, 206), (226, 301), (321, 394), (411, 486), (491, 579), (584, 672), (678, 765)]
    rows = []
    for (y0, y1) in bands:
        band = lum[y0 + 4:y1 - 4, :714]
        dark = (band < 50).mean(0) > 0.6
        xs = np.where(dark)[0]
        lines = []
        for x in xs:
            if not lines or x - lines[-1][-1] > 2:
                lines.append([x])
            else:
                lines[-1].append(x)
        lines = [int(np.mean(l)) for l in lines]
        if lines[0] > 30:
            lines = [21] + lines
        if lines[-1] < 700:
            lines.append(714)
        cells = []
        for l0, l1 in zip(lines, lines[1:]):
            if l1 - l0 > 40:
                cells.append((l0 + 5, y0 + 5, l1 - 5, y1 - 5))
        rows.append(cells)
    return rows


def blood_vfx_cells():
    bands = [(0, 150), (150, 270), (270, 390), (390, 500), (500, 655), (655, 768)]
    xs = [(i * 170, i * 170 + 170) for i in range(6)] + [(1020, 1250)]
    return [[(x0, y0, x1, y1) for (x0, x1) in xs] for (y0, y1) in bands]


# ---------------------------------------------------------------- packing
class Packer:
    def __init__(self, size):
        self.size = size
        self.x = self.y = self.row_h = 0
        self.pad = 2

    def place(self, w, h):
        if self.x + w + self.pad > self.size:
            self.x = 0
            self.y += self.row_h + self.pad
            self.row_h = 0
        if self.y + h > self.size:
            raise RuntimeError("atlas overflow")
        p = (self.x, self.y)
        self.x += w + self.pad
        self.row_h = max(self.row_h, h)
        return p


def scaled(frames, scale):
    res = []
    for col, alpha in frames:
        img = to_image(col, alpha)
        w = max(1, int(round(img.width * scale)))
        h = max(1, int(round(img.height * scale)))
        res.append(img.resize((w, h), Image.LANCZOS))
    return res


def anchor_x(img, lower=True):
    a = np.asarray(img)[..., 3].astype(np.float32)
    if lower:
        a = a[int(a.shape[0] * 0.55):, :]
    s = a.sum()
    if s <= 0:
        return 0.5
    return float((a.sum(0) * np.arange(a.shape[1])).sum() / s / img.width)


def soft_disc(size):
    yy, xx = np.mgrid[0:size, 0:size]
    r = np.hypot(xx - size / 2 + 0.5, yy - size / 2 + 0.5) / (size / 2)
    al = np.clip(1 - r, 0, 1) ** 1.6
    rgba = np.dstack([np.full_like(al, 255)] * 3 + [al * 255]).astype(np.uint8)
    return Image.fromarray(rgba, "RGBA")


def ring(size):
    yy, xx = np.mgrid[0:size, 0:size]
    r = np.hypot(xx - size / 2 + 0.5, yy - size / 2 + 0.5) / (size / 2)
    al = np.clip(1 - np.abs(r - 0.82) / 0.12, 0, 1)
    rgba = np.dstack([np.full_like(al, 255)] * 3 + [al * 255]).astype(np.uint8)
    return Image.fromarray(rgba, "RGBA")


def arrow(size):
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    from PIL import ImageDraw
    d = ImageDraw.Draw(img)
    d.polygon([(size / 2, 2), (size - 6, size - 4), (size / 2, size * 0.7), (6, size - 4)], fill=(255, 255, 255, 255))
    return img


def build_sprites():
    anims = []  # (name, kind, [images], [anchors])

    def char_anim(name, frames, ref_h, lower=True):
        imgs = scaled(frames, CHAR_REF_PX / ref_h)
        anims.append((name, "char", imgs, [anchor_x(i, lower) for i in imgs], CHAR_REF_PX))

    # ---- player
    pt = extract_chars("player_turnaround.jpg", PG)
    ref_p = pt[0][1].shape[0]
    char_anim("P_TURN", pt, ref_p)
    pp = extract_chars("player_punch.jpg", PG)
    char_anim("P_PUNCH", pp, pp[0][1].shape[0])
    pd = extract_chars("player_death.jpg", death_cells())
    pd = [pd[i] for i in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14, 15, 16, 18]]
    char_anim("P_DEATH", pd, ref_p * 1.0, lower=False)

    # ---- zombie
    zt = extract_chars("zombie_turnaround.jpg", grid(
        [(0, 235), (235, 470), (470, 705), (705, 940), (940, 1175), (1175, 1408)], [(0, 384), (384, 768)], inset=2))
    ref_z = zt[0][1].shape[0]
    char_anim("Z_TURN", zt, ref_z)
    zw = extract_chars("zombie_walk.jpg", ZG)
    ref_zw = zw[0][1].shape[0]
    walk = [zw[i] for i in [1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 13, 14, 15, 16, 17, 19, 20, 21, 22, 23]][::2]
    char_anim("Z_WALK", walk, ref_zw)
    char_anim("Z_IDLE", [zw[0]], ref_zw)
    zr = extract_chars("zombie_run.jpg", ZG)
    char_anim("Z_RUN", zr[1:][::2], zr[0][1].shape[0])
    zl = extract_chars("zombie_lunge.jpg", ZG)
    char_anim("Z_LUNGE", [zl[i] for i in [1, 2, 3, 4, 5, 6, 10, 12, 13, 14, 15, 16, 18, 20, 22]], zl[0][1].shape[0])
    zd = extract_chars("zombie_death.jpg", ZG)
    char_anim("Z_DEATH", [zd[i] for i in [1, 2, 4, 6, 8, 10, 12, 13, 14, 15, 16, 17, 18, 23]], zd[0][1].shape[0], lower=False)

    # ---- vfx
    names = ["V_SPARK", "V_SLASH", "V_DUST", "V_SWEAT", "V_SHOCK", "V_SHIELD", "V_HEAL", "V_SMOKE"]
    for name, cells in zip(names, combat_vfx_cells()):
        fr = extract_vfx("vfx_combat.jpg", cells)
        imgs = [to_image(c, a) for c, a in fr]
        ref = max(max(i.width, i.height) for i in imgs)
        imgs = [i.resize((max(1, int(i.width * 96 / ref)), max(1, int(i.height * 96 / ref))), Image.LANCZOS) for i in imgs]
        anims.append((name, "vfx", imgs, [0.5] * len(imgs), 96))
    bnames = ["V_BLOOD", "V_CLAW", "V_POOL", "V_SPRAY", "V_GAS", "V_PUKE"]
    for name, cells in zip(bnames, blood_vfx_cells()):
        fr = extract_vfx("vfx_blood.jpg", cells, soft=110.0, label_safe=False)
        imgs = [to_image(c, a) for c, a in fr]
        ref = max(max(i.width, i.height) for i in imgs)
        imgs = [i.resize((max(1, int(i.width * 110 / ref)), max(1, int(i.height * 110 / ref))), Image.LANCZOS) for i in imgs]
        anims.append((name, "vfx", imgs, [0.5] * len(imgs), 110))

    # ---- procedural helpers
    anims.append(("G_DISC", "vfx", [soft_disc(64)], [0.5], 64))
    anims.append(("G_RING", "vfx", [ring(64)], [0.5], 64))
    anims.append(("G_ARROW", "vfx", [arrow(32)], [0.5], 32))
    white = Image.new("RGBA", (8, 8), (255, 255, 255, 255))
    anims.append(("G_WHITE", "vfx", [white], [0.5], 8))

    # ---- pickup icon (medkit) from the UI sheet
    ui = Image.open(os.path.join(SRC, "ui_buttons.jpg")).convert("RGB")
    med = icon_from_ui(ui, (1079, 424, 65, 60))
    anims.append(("I_MEDKIT", "vfx", [med.resize((64, 60), Image.LANCZOS)], [0.5], 64))

    # ---- pack (tallest first gives tight shelves)
    atlas = Image.new("RGBA", (ATLAS, ATLAS), (0, 0, 0, 0))
    items = []
    for ai, (name, kind, imgs, anch, ref) in enumerate(anims):
        for fi, img in enumerate(imgs):
            items.append((img.height, ai, fi))
    items.sort(reverse=True)
    pk = Packer(ATLAS)
    pos = {}
    for h, ai, fi in items:
        img = anims[ai][2][fi]
        pos[(ai, fi)] = pk.place(img.width, img.height)
        atlas.paste(img, pos[(ai, fi)])
    print("atlas used rows up to", pk.y + pk.row_h, "of", ATLAS)

    # bleed colour into transparent texels so bilinear filtering has no dark halos
    arr = np.asarray(atlas).astype(np.float32)
    al = arr[..., 3:4] / 255.0
    col = arr[..., :3]
    blur_c = np.asarray(Image.fromarray((col * al).astype(np.uint8)).filter(ImageFilter.BoxBlur(2))).astype(np.float32)
    blur_a = np.asarray(Image.fromarray((al[..., 0] * 255).astype(np.uint8)).filter(ImageFilter.BoxBlur(2))).astype(np.float32)[..., None] / 255.0
    fill = blur_c / np.maximum(blur_a, 1e-3)
    col = np.where(al > 0.02, col, fill)
    atlas = Image.fromarray(np.dstack([col, arr[..., 3]]).clip(0, 255).astype(np.uint8), "RGBA")

    os.makedirs(ASSETS, exist_ok=True)
    atlas.save(os.path.join(ASSETS, "sprites.webp"), "WEBP", quality=88, method=6, alpha_quality=90)
    save_preview_raw(atlas, "sprites")

    # ---- rust frame table
    lines = ["// @generated by tools/build_assets.py — do not edit.", "",
             "/// One atlas sub-image. u/v in [0,1]; w/h relative to the animation reference size;",
             "/// ax = horizontal anchor (0 = left edge, 1 = right edge), feet/centre at the anchor.",
             "#[derive(Clone, Copy, Debug)]",
             "pub struct Frame { pub u0: f32, pub v0: f32, pub u1: f32, pub v1: f32, pub w: f32, pub h: f32, pub ax: f32 }",
             "", "pub const ATLAS_SIZE: u32 = %d;" % ATLAS, ""]
    offset = 0
    frames = []
    for ai, (name, kind, imgs, anch, ref) in enumerate(anims):
        lines.append("pub const %s: (usize, usize) = (%d, %d);" % (name, offset, len(imgs)))
        for fi, img in enumerate(imgs):
            x, y = pos[(ai, fi)]
            frames.append("    Frame { u0: %.6f, v0: %.6f, u1: %.6f, v1: %.6f, w: %.4f, h: %.4f, ax: %.4f }," % (
                (x + 0.5) / ATLAS, (y + 0.5) / ATLAS, (x + img.width - 0.5) / ATLAS, (y + img.height - 0.5) / ATLAS,
                img.width / ref, img.height / ref, anch[fi]))
        offset += len(imgs)
    lines.append("")
    lines.append("pub static FRAMES: [Frame; %d] = [" % offset)
    lines += frames
    lines.append("];")
    with open(RUST_GEN, "w") as f:
        f.write("\n".join(lines) + "\n")
    print("frames:", offset)


def icon_from_ui(ui, rect, circle=True):
    x, y, w, h = rect
    pad = 3
    sub = np.asarray(ui.crop((x - pad, y - pad, x + w + pad, y + h + pad))).astype(np.float32)
    bg = np.array([89, 122, 92], np.float32)
    col, alpha = key_character(sub, bg)
    alpha = strip_small(alpha, 0.2)
    if circle:
        hh, ww = alpha.shape
        yy, xx = np.mgrid[0:hh, 0:ww]
        r = np.hypot((xx - ww / 2) / (ww / 2), (yy - hh / 2) / (hh / 2))
        alpha = alpha * np.clip((1.0 - r) * 12, 0, 1)
    return to_image(col, alpha)


# ---------------------------------------------------------------- textures
def seamless(img):
    a = np.asarray(img).astype(np.float32)
    h, w = a.shape[:2]
    sh = np.roll(np.roll(a, h // 2, 0), w // 2, 1)
    yy, xx = np.mgrid[0:h, 0:w]
    wx = 1 - np.abs(xx - w / 2) / (w / 2)
    wy = 1 - np.abs(yy - h / 2) / (h / 2)
    m = np.clip(np.minimum(wx, wy) * 2.2, 0, 1)[..., None]
    return Image.fromarray((a * m + sh * (1 - m)).clip(0, 255).astype(np.uint8))


def build_textures():
    env = Image.open(os.path.join(SRC, "env_ground_tiles.jpg")).convert("RGB")
    cols = [(21, 346), (361, 699), (713, 1051), (1066, 1391)]
    rows = [(30, 204), (230, 401), (430, 604), (638, 765)]
    picks = {
        "asphalt": (0, 0, 0.0), "asphalt_hole": (0, 1, 0.0), "grass": (1, 0, 0.0), "mud": (1, 1, 0.0),
        "dirt": (1, 2, 0.0), "concrete": (2, 0, 0.0), "concrete_cracked": (2, 1, 0.0), "concrete_holes": (2, 2, 0.0),
    }
    for name, (r, c, _) in picks.items():
        x0, x1 = cols[c]
        y0, y1 = rows[r]
        # keep clear of the text label strip at the bottom of each tile
        box = (x0 + 6, y0 + 6, x1 - 6, y0 + int((y1 - y0) * 0.78))
        img = env.crop(box)
        if name == "asphalt_hole":
            continue  # potholes are drawn as soft dark decals instead
        side = min(img.width, img.height)
        img = img.crop(((img.width - side) // 2, 0, (img.width - side) // 2 + side, side))
        img = seamless(img.resize((256, 256), Image.LANCZOS))
        save_tex(img, name)


def save_tex(img, name, keep=False):
    img.save(os.path.join(ASSETS, "tex_%s.webp" % name), "WEBP", quality=82, method=6)
    save_preview_raw(img.convert("RGBA"), "tex_" + name)


def save_preview_raw(img, name):
    os.makedirs(PREVIEW, exist_ok=True)
    a = np.asarray(img.convert("RGBA")).astype(np.float32)
    a[..., :3] *= a[..., 3:4] / 255.0  # premultiplied like Android's GLUtils upload
    with open(os.path.join(PREVIEW, name + ".rgba"), "wb") as f:
        f.write(np.array([img.width, img.height], "<u4").tobytes())
        f.write(a.astype(np.uint8).tobytes())


# ---------------------------------------------------------------- UI
UI_BUTTONS = {
    "btn_play": (27, 60, 362, 136),
    "btn_story": (412, 75, 272, 98),
    "btn_endless": (29, 234, 284, 63),
    "btn_character": (367, 231, 316, 67),
    "btn_settings": (367, 332, 316, 59),
    "btn_quests": (766, 327, 260, 63),
    "btn_exit": (1086, 328, 263, 60),
}
UI_ICONS = {
    "ic_punch": (746, 43, 59, 60), "ic_kick": (831, 39, 58, 64), "ic_dodge": (912, 41, 60, 62),
    "ic_heal": (1075, 43, 58, 60), "ic_block": (1154, 43, 61, 60), "ic_pause": (1236, 39, 63, 64),
    "ic_resume": (48, 706, 55, 55), "ic_restart": (154, 704, 56, 57),
    "ic_check": (745, 611, 59, 59), "ic_lobby": (521, 608, 57, 60), "ic_gear": (1240, 233, 59, 62), "ic_scroll": (1317, 233, 63, 63),
    
}


def build_ui():
    os.makedirs(DRAWABLE, exist_ok=True)
    ui = Image.open(os.path.join(SRC, "ui_buttons.jpg")).convert("RGB")
    bg = np.array([89, 122, 92], np.float32)
    for name, (x, y, w, h) in UI_BUTTONS.items():
        sub = np.asarray(ui.crop((x - 4, y - 4, x + w + 4, y + h + 4))).astype(np.float32)
        col, alpha = key_character(sub, bg)
        alpha = strip_small(alpha, 0.3)
        t = trim(col, alpha, 1)
        to_image(*t).save(os.path.join(DRAWABLE, name + ".webp"), "WEBP", quality=88, method=6)
    for name, rect in UI_ICONS.items():
        img = icon_from_ui(ui, rect)
        img = img.resize((128, int(128 * img.height / img.width)), Image.LANCZOS)
        img.save(os.path.join(DRAWABLE, name + ".webp"), "WEBP", quality=88, method=6)
    # portraits for the character screen
    for name, sheet in [("portrait_soldier", "player_turnaround.jpg"), ("portrait_leather", "player_leather_turnaround.jpg"),
                        ("portrait_zombie", "zombie_turnaround.jpg")]:
        a = load(sheet)
        cell = (P_COLS[0][0] + 6, 20, P_COLS[0][1] - 6, 382) if "zombie" not in sheet else (10, 10, 230, 380)
        sub = a[cell[1]:cell[3], cell[0]:cell[2]]
        bg_c = sample_bg(sub) if "leather" not in sheet else np.array([128, 128, 128], np.float32)
        col, alpha = key_character(sub, bg_c) if "leather" not in sheet else key_gray(sub, bg_c)
        alpha = strip_small(alpha)
        t = trim(col, alpha)
        img = to_image(*t)
        img = img.resize((int(300 * img.width / img.height), 300), Image.LANCZOS)
        img.save(os.path.join(DRAWABLE, name + ".webp"), "WEBP", quality=85, method=6)
    # launcher icon: radiation emblem from the OYNA button on a dark plate
    emb = ui.crop((40, 68, 158, 186)).resize((150, 150), Image.LANCZOS)
    icon = Image.new("RGBA", (192, 192), (0, 0, 0, 0))
    from PIL import ImageDraw
    ImageDraw.Draw(icon).rounded_rectangle((4, 4, 188, 188), radius=40, fill=(20, 26, 20, 255), outline=(120, 150, 90, 255), width=5)
    mask = Image.new("L", (150, 150), 0)
    ImageDraw.Draw(mask).ellipse((2, 2, 148, 148), fill=255)
    icon.paste(emb, (21, 21), mask)
    icon.save(os.path.join(DRAWABLE, "ic_launcher.webp"), "WEBP", quality=90, method=6)
    # menu background: blurred, darkened environment collage
    env = Image.open(os.path.join(SRC, "env_ground_tiles.jpg")).convert("RGB").crop((361, 30, 1051, 401))
    env = env.resize((640, 344), Image.LANCZOS).filter(ImageFilter.GaussianBlur(3))
    env = Image.fromarray((np.asarray(env).astype(np.float32) * 0.45).astype(np.uint8))
    env.save(os.path.join(DRAWABLE, "menu_bg.webp"), "WEBP", quality=70, method=6)


def key_gray(a, bg):
    dist = np.abs(a - bg).sum(-1)
    alpha = np.clip((dist - 12) / 40.0, 0, 1)
    return a, alpha


if __name__ == "__main__":
    build_textures()
    build_sprites()
    build_ui()
    print("done")
