use std::env;
use std::path::{Path, PathBuf};

fn candidate_dirs(manifest_dir: &Path) -> [PathBuf; 4] {
    [
        manifest_dir.join("lib"),
        manifest_dir.join("deps"),
        manifest_dir
            .join("..")
            .join("HDR_DEV")
            .join("imgui-smash")
            .join("examples")
            .join("ingame-dev")
            .join("lib"),
        manifest_dir
            .join("..")
            .join("imgui-smash")
            .join("examples")
            .join("ingame-dev")
            .join("lib"),
    ]
}

fn emit_link_search(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    println!("cargo:rustc-link-search={}", dir.display());
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    if env::var_os("CARGO_FEATURE_CONSOLE").is_none() {
        return;
    }

    println!("cargo:rerun-if-env-changed=IMGUI_SMASH_LIB_DIR");

    if let Ok(dir) = env::var("IMGUI_SMASH_LIB_DIR") {
        let dir = PathBuf::from(dir);
        if dir.join("libimgui_smash.a").exists() {
            emit_link_search(&dir);
            return;
        }
        panic!(
            "IMGUI_SMASH_LIB_DIR was set to '{}' but libimgui_smash.a was not found there",
            dir.display()
        );
    }

    for dir in candidate_dirs(&manifest_dir) {
        if dir.join("libimgui_smash.a").exists() {
            emit_link_search(&dir);
            return;
        }
    }

    panic!(
        "could not find libimgui_smash.a; set IMGUI_SMASH_LIB_DIR or place the archive in ./lib"
    );
}
