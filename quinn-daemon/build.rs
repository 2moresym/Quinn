use std::{env, path::PathBuf};

fn main() {
    if env::var_os("CARGO_FEATURE_VOICE").is_none() {
        return;
    }

    let lib_dir = env::var_os("QUINN_VOSK_LIB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../native/vosk"));

    println!("cargo:rerun-if-env-changed=QUINN_VOSK_LIB_DIR");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=vosk");

    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
}
