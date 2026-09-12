#[path = "build_support/core_version.rs"]
mod core_version;

fn main() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repository_root = manifest_dir
        .parent()
        .expect("src-tauri must have the repository root as its parent");
    println!(
        "cargo:rerun-if-changed={}",
        repository_root
            .join("config/meowcal-core.lock.json")
            .display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        repository_root.join("core/Cargo.toml").display()
    );
    let selection = core_version::resolve(repository_root, cfg!(feature = "core-source-candidate"))
        .unwrap_or_else(|error| panic!("Meowcal Core version selection failed: {error}"));
    println!(
        "cargo:rustc-env=MEOWCAL_EXPECTED_CORE_VERSION={}",
        selection.expected_version
    );
    println!("cargo:rustc-check-cfg=cfg(meowcal_core_source_candidate)");
    if selection.source_candidate {
        println!("cargo:rustc-cfg=meowcal_core_source_candidate");
    }
    tauri_build::build()
}
