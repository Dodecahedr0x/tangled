#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
pub fn home_dir() -> PathBuf {
    if let Ok(p) = std::env::var("WEFT_HOME") {
        return PathBuf::from(p);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(h) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        return PathBuf::from(h).join(".weft");
    }
    PathBuf::from(".weft")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn knot_path() -> PathBuf {
    home_dir().join("knot.txt")
}
