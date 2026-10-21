//! Asset access: APK assets via AAssetManager on Android, a directory on desktop.

pub trait AssetSource: Send {
    fn read(&self, name: &str) -> Option<Vec<u8>>;
}

pub struct DirSource(pub std::path::PathBuf);

impl AssetSource for DirSource {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(name)).ok()
    }
}

#[cfg(target_os = "android")]
pub mod android {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_void};

    #[link(name = "android")]
    extern "C" {
        pub fn AAssetManager_fromJava(env: *mut c_void, mgr: *mut c_void) -> *mut c_void;
        fn AAssetManager_open(mgr: *mut c_void, name: *const c_char, mode: c_int) -> *mut c_void;
        fn AAsset_getLength(a: *mut c_void) -> i64;
        fn AAsset_read(a: *mut c_void, buf: *mut c_void, n: usize) -> c_int;
        fn AAsset_close(a: *mut c_void);
    }

    pub struct ApkSource(pub usize);

    impl super::AssetSource for ApkSource {
        fn read(&self, name: &str) -> Option<Vec<u8>> {
            let c = CString::new(name).ok()?;
            unsafe {
                let a = AAssetManager_open(self.0 as *mut c_void, c.as_ptr(), 2 /* AASSET_MODE_BUFFER */);
                if a.is_null() {
                    return None;
                }
                let n = AAsset_getLength(a).max(0) as usize;
                let mut v = vec![0u8; n];
                let mut got = 0usize;
                while got < n {
                    let r = AAsset_read(a, v[got..].as_mut_ptr() as *mut c_void, n - got);
                    if r <= 0 {
                        break;
                    }
                    got += r as usize;
                }
                AAsset_close(a);
                if got == n {
                    Some(v)
                } else {
                    None
                }
            }
        }
    }
}
