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
    let (executable_hash, license_hash) = if selection.source_candidate {
        (String::new(), String::new())
    } else {
        for resource in ["meowcal-core.exe", "meowcal-core.json", "LICENSE"] {
            println!(
                "cargo:rerun-if-changed={}",
                manifest_dir.join("resources/core").join(resource).display()
            );
        }
        for script in ["fetch-meowcal-core.ps1", "verify-core-package.ps1"] {
            println!(
                "cargo:rerun-if-changed={}",
                repository_root.join("scripts").join(script).display()
            );
        }
        println!("cargo:rerun-if-env-changed=MEOWCAL_CORE_ARCHIVE");
        let target = std::env::var("CARGO_CFG_TARGET_ARCH").expect("Cargo target architecture");
        let archive = std::env::var_os("MEOWCAL_CORE_ARCHIVE").map(std::path::PathBuf::from);
        core_version::prepare_reviewed_resource(repository_root, &target, archive.as_deref())
            .unwrap_or_else(|error| panic!("Meowcal Core release preparation failed: {error}"))
    };
    println!("cargo:rustc-env=MEOWCAL_EXPECTED_CORE_EXECUTABLE_SHA256={executable_hash}");
    println!("cargo:rustc-env=MEOWCAL_EXPECTED_CORE_LICENSE_SHA256={license_hash}");
    tauri_build::build()
}
