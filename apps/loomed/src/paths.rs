use std::path::PathBuf;

pub fn home_dir() -> PathBuf {
    if let Ok(p) = std::env::var("LOOMED_HOME") {
        return PathBuf::from(p);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(h) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        return PathBuf::from(h).join(".loomed");
    }
    PathBuf::from(".loomed")
}

pub fn keys_path() -> PathBuf {
    home_dir().join("keys.json")
}

pub fn knot_path() -> PathBuf {
    home_dir().join("knot.txt")
}

pub fn replica_dir() -> PathBuf {
    home_dir().join("replica")
}
