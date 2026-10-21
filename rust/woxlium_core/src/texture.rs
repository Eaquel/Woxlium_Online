//! PVR v3 container (written by etcpak) -> GL compressed texture with mips.

use crate::gl::*;

pub struct PvrInfo {
    pub width: u32,
    pub height: u32,
    pub rgba: bool,
    pub levels: Vec<(u32, u32, usize, usize)>, // w, h, offset, size
}

pub fn parse_pvr(b: &[u8]) -> Result<PvrInfo, String> {
    if b.len() < 52 {
        return Err("pvr too small".into());
    }
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    if u32_at(0) != 0x0352_5650 {
        return Err("bad pvr magic".into());
    }
    let fmt = u64::from_le_bytes(b[8..16].try_into().unwrap());
    let rgba = match fmt {
        22 => false, // ETC2 RGB
        23 => true,  // ETC2 RGBA
        _ => return Err(format!("unsupported pvr format {fmt}")),
    };
    let height = u32_at(24);
    let width = u32_at(28);
    let mips = u32_at(44).max(1);
    let meta = u32_at(48) as usize;
    let mut off = 52 + meta;
    let mut levels = Vec::new();
    let (mut w, mut h) = (width, height);
    for _ in 0..mips {
        let blocks = w.div_ceil(4) as usize * h.div_ceil(4) as usize;
        let size = blocks * if rgba { 16 } else { 8 };
        if off + size > b.len() {
            break;
        }
        levels.push((w, h, off, size));
        off += size;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    Ok(PvrInfo { width, height, rgba, levels })
}

/// Upload into a new GL texture. `repeat` for tiling surfaces.
pub fn upload_pvr(b: &[u8], repeat: bool) -> Result<GLuint, String> {
    let info = parse_pvr(b)?;
    let mut id = 0;
    unsafe {
        glGenTextures(1, &mut id);
        glBindTexture(TEXTURE_2D, id);
        let fmt = if info.rgba { COMPRESSED_RGBA8_ETC2_EAC } else { COMPRESSED_RGB8_ETC2 };
        for (lvl, &(w, h, off, size)) in info.levels.iter().enumerate() {
            glCompressedTexImage2D(TEXTURE_2D, lvl as i32, fmt, w as i32, h as i32, 0, size as i32, b[off..].as_ptr() as *const _);
        }
        let wrap = if repeat { REPEAT } else { CLAMP_TO_EDGE };
        glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_S, wrap);
        glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_T, wrap);
        glTexParameteri(TEXTURE_2D, TEXTURE_MAG_FILTER, LINEAR);
        glTexParameteri(TEXTURE_2D, TEXTURE_MAX_LEVEL, info.levels.len() as i32 - 1);
        glTexParameteri(TEXTURE_2D, TEXTURE_MIN_FILTER, if info.levels.len() > 1 { LINEAR_MIPMAP_LINEAR } else { LINEAR });
    }
    Ok(id)
}
