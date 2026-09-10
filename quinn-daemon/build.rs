use std::{env, path::PathBuf};

fn main() {
    if env::var_os("CARGO_FEATURE_VOICE").is_none() {
        return;
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=QUINN_VOSK_LIB_DIR");

    let candidates = [
        env::var_os("QUINN_VOSK_LIB_DIR").map(PathBuf::from),
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".local/share/quinn/vosk")),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../native/vosk")),
    ];

    let lib_dir = candidates
        .into_iter()
        .flatten()
        .find(|path| path.join("libvosk.so").is_file())
        .unwrap_or_else(|| {
            panic!(
                "could not find libvosk.so; set QUINN_VOSK_LIB_DIR or install it at ~/.local/share/quinn/vosk"
            )
        });

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=vosk");

    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-arg=-L{}", lib_dir.display());
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    }
}
