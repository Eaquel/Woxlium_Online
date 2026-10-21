//! Woxlium — 3D zombie brawler core (simulation + GLES2 renderer).
//! The Android app (Kotlin) owns the window, UI and audio; this library owns
//! gameplay and drawing, driven through the JNI entry points in `jni.rs`.

pub mod atlas_gen;
pub mod game;
pub mod math;
pub mod world;

#[cfg(any(target_os = "android", feature = "preview"))]
pub mod gl;
#[cfg(any(target_os = "android", feature = "preview"))]
pub mod render;
#[cfg(any(target_os = "android", feature = "preview"))]
pub mod engine;
#[cfg(target_os = "android")]
mod jni;

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
