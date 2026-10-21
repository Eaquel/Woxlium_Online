//! JNI entry points for `com.woxlium.zombie.NativeBridge` (static methods).
#![allow(non_snake_case)]

use std::os::raw::c_void;

use crate::assets::android::{AAssetManager_fromJava, ApkSource};
use crate::engine::{self, lock, ENGINE, HUD, INPUT, SETTINGS};
use crate::game::HUD_LEN;
use crate::math::v2;

type Env = *mut c_void;
type Class = *mut c_void;
type JBool = u8;

/// Must be called once with the app's AssetManager (kept alive by the app).
#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeInit(env: Env, _: Class, asset_manager: *mut c_void) {
    let mgr = unsafe { AAssetManager_fromJava(env, asset_manager) };
    if !mgr.is_null() {
        engine::set_source(Box::new(ApkSource(mgr as usize)));
    }
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeStart(_: Env, _: Class, mode: i32, seed: i32) {
    engine::start_game(mode, seed as u32);
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeSurfaceCreated(_: Env, _: Class) -> JBool {
    engine::surface_created();
    let e = lock(&ENGINE);
    (e.renderer.is_some()) as JBool
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
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeLook(_: Env, _: Class, dx: f32, dy: f32) {
    lock(&INPUT).look(dx, dy);
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
    } else if index == HUD_LEN as i32 {
        lock(&ENGINE).fps
    } else {
        0.0
    }
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeSetMinimap(_: Env, _: Class, x: f32, y: f32, size: f32) {
    lock(&SETTINGS).minimap = [x, y, size];
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeSetMapExpanded(_: Env, _: Class, expanded: JBool) {
    lock(&SETTINGS).map_expanded = expanded != 0;
}

#[no_mangle]
pub extern "system" fn Java_com_woxlium_zombie_NativeBridge_nativeSetShadows(_: Env, _: Class, on: JBool) {
    lock(&SETTINGS).shadows = on != 0;
}
