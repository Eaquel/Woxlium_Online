//! Woxlium — first-person 3D zombie brawler core.
//! Simulation, skeletal animation and an OpenGL ES 3.0 renderer; the Android
//! app (Kotlin) owns the window, UI and audio and drives this through `jni.rs`.

pub mod anim;
pub mod assets;
pub mod assets_gen;
pub mod game;
pub mod level;
pub mod math;
pub mod wxm;

#[cfg(any(target_os = "android", feature = "preview"))]
pub mod engine;
#[cfg(any(target_os = "android", feature = "preview"))]
pub mod gl;
#[cfg(target_os = "android")]
mod jni;
#[cfg(any(target_os = "android", feature = "preview"))]
pub mod render;
#[cfg(any(target_os = "android", feature = "preview"))]
pub mod texture;

pub fn log(msg: &str) {
    #[cfg(target_os = "android")]
    {
        extern "C" {
            fn __android_log_write(prio: i32, tag: *const u8, text: *const u8) -> i32;
        }
        let text = std::ffi::CString::new(msg.replace('\0', " ")).unwrap_or_default();
        unsafe { __android_log_write(5, b"Woxlium\0".as_ptr(), text.as_ptr() as *const u8) };
    }
    #[cfg(not(target_os = "android"))]
    eprintln!("[woxlium] {msg}");
}
