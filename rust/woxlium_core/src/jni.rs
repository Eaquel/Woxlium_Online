//! JNI entry points for `com.woxlium.zombie.NativeBridge` (all static methods).
//! Only primitive arguments are used, so no JNIEnv calls are needed.
#![allow(non_snake_case)]

use std::os::raw::c_void;

use crate::engine::{self, lock, HUD, INPUT, SETTINGS};
use crate::game::HUD_LEN;
use crate::math::v2;
use crate::render::TEX_SLOTS;

type Env = *mut c_void;
type Class = *mut c_void;
type JBool = u8;

static TEX: std::sync::Mutex<[u32; TEX_SLOTS]> = std::sync::Mutex::new([0; TEX_SLOTS]);

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeStart(_: Env, _: Class, mode: i32, skin: i32, seed: i32) {
    engine::start_game(mode, skin, seed as u32);
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeSetTexture(_: Env, _: Class, slot: i32, id: i32) {
    if (0..TEX_SLOTS as i32).contains(&slot) {
        lock(&TEX)[slot as usize] = id as u32;
    }
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeSurfaceCreated(_: Env, _: Class) {
    let slots = *lock(&TEX);
    engine::surface_created(&slots);
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeSurfaceChanged(_: Env, _: Class, w: i32, h: i32) {
    engine::surface_changed(w, h);
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeFrame(_: Env, _: Class, dt: f32) -> i32 {
    engine::frame(dt) as i32
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeMove(_: Env, _: Class, x: f32, y: f32) {
    lock(&INPUT).mv = v2(x, y);
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeButton(_: Env, _: Class, id: i32, down: JBool) {
    lock(&INPUT).button(id, down != 0);
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativePause(_: Env, _: Class, paused: JBool) {
    engine::set_paused(paused != 0);
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeHud(_: Env, _: Class, index: i32) -> f32 {
    if (0..HUD_LEN as i32).contains(&index) {
        lock(&HUD)[index as usize]
    } else {
        0.0
    }
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeSetZoom(_: Env, _: Class, zoom: f32) {
    lock(&SETTINGS).zoom = zoom.clamp(0.7, 1.5);
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeSetMinimap(_: Env, _: Class, x: f32, y: f32, size: f32) {
    lock(&SETTINGS).minimap = [x, y, size];
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeSetMapExpanded(_: Env, _: Class, expanded: JBool) {
    lock(&SETTINGS).map_expanded = expanded != 0;
}
