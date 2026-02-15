#[cfg(target_os = "macos")]
fn main() {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};

    // The `screencapturekit` crate uses Swift bridging code which introduces a runtime dependency
    // on `@rpath/libswift_Concurrency.dylib`. On macOS, Swift runtime libs live under
    // `/usr/lib/swift` (often in the dyld shared cache), but `@rpath` won't resolve unless we add
    // an rpath entry. Prefer the system Swift runtime: bundling a single Swift dylib can lead to
    // duplicate-class crashes if other Swift dylibs are loaded from the system.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    let out_dir = match env::var("OUT_DIR") {
        Ok(v) => PathBuf::from(v),
        Err(_) => return,
    };

    // OUT_DIR looks like: target/{profile}/build/{crate-hash}/out
    // We want: target/{profile}
    let Some(profile_dir) = out_dir.ancestors().nth(3).map(Path::to_path_buf) else {
        return;
    };

    // If an older build copied Swift dylibs into `target/{profile}`, remove them to avoid
    // duplicate Swift runtime images being loaded.
    let stale_main = profile_dir.join("libswift_Concurrency.dylib");
    let stale_deps = profile_dir.join("deps").join("libswift_Concurrency.dylib");
    let _ = fs::remove_file(&stale_main);
    let _ = fs::remove_file(&stale_deps);

    println!("cargo:rerun-if-changed=build.rs");
}

#[cfg(not(target_os = "macos"))]
fn main() {}
