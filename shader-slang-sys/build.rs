use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

include!("src/prebuilt_checksums.rs");

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=slang/include/slang.h");
    println!("cargo:rerun-if-changed=slang/include/slang-gfx.h");
    println!("cargo:rerun-if-changed=slang/include/slang-com-helper.h");
    println!("cargo:rerun-if-env-changed=SHADER_SLANG_SYS_CACHE_DIR");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    let use_prebuilt = cfg_feature("prebuilt") && !cfg_feature("build-from-source");

    let (include_dir, link_search_dir, static_ok) = if use_prebuilt {
        let dir = fetch_prebuilt(&target_os, &target_arch);
        (dir.join("include"), dir.join("lib"), false)
    } else {
        build_from_source()
    };

    println!(
        "cargo:rustc-link-search=native={}",
        link_search_dir.display()
    );

    let static_requested = cfg_feature("static-link") && !cfg_feature("dynamic-link");
    if static_requested && static_ok {
        println!("cargo:rustc-link-lib=static=slang");
    } else {
        println!("cargo:rustc-link-lib=dylib=slang");
    }

    let is_windows = target_os == "windows";
    let is_msvc = target_env == "msvc";
    if is_windows {
        println!("cargo:rustc-cfg=slang_stdcall");
    }

    generate_bindings(&include_dir, &target_arch, is_windows, is_msvc);

    println!("cargo:rustc-cfg=slang_bindings_generated");
}

fn cfg_feature(name: &str) -> bool {
    let env_name = format!("CARGO_FEATURE_{}", name.to_uppercase().replace('-', "_"));
    env::var(env_name).is_ok()
}

/// Builds Slang from the `slang/` git submodule via CMake. Returns
/// (include_dir, lib_dir, has_static_archive).
fn build_from_source() -> (PathBuf, PathBuf, bool) {
    let slang_dir = PathBuf::from("slang");
    let include_dir = slang_dir.join("include");

    let lib_paths = [
        slang_dir.join("build/Release/lib"),
        slang_dir.join("build/Debug/lib"),
    ];

    let lib_dir = lib_paths
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| {
            println!("cargo:warning=Slang library not found in expected locations under slang/build.");
            println!("cargo:warning=To fix this, build Slang first: cd slang && cmake --preset default && cmake --build --preset release");
            lib_paths[0].clone()
        });

    let has_static = lib_dir.join("libslang.a").exists() || lib_dir.join("slang.lib").exists();

    (include_dir, lib_dir, has_static)
}

/// Downloads and caches the official prebuilt Slang release archive for the
/// current target, verifying its SHA-256 against `prebuilt_checksums.rs`.
/// Returns the directory the archive was extracted into (containing
/// `include/` and `lib/`).
fn fetch_prebuilt(target_os: &str, target_arch: &str) -> PathBuf {
    let asset = lookup(target_os, target_arch).unwrap_or_else(|| {
        panic!(
            "no prebuilt Slang release asset is registered for os={target_os} arch={target_arch}; \
             enable the `build-from-source` feature instead, or add an entry to prebuilt_checksums.rs"
        )
    });

    if asset.sha256.starts_with("TODO_FILL_SHA256") {
        panic!(
            "prebuilt_checksums.rs still has a placeholder SHA-256 for {} — \
             run scripts/compute_checksums.sh and fill in the real digest before using the `prebuilt` feature",
            asset.asset_name
        );
    }

    let cache_dir = cache_dir()
        .join(SLANG_VERSION)
        .join(format!("{target_os}-{target_arch}"));
    let extracted_marker = cache_dir.join(".extracted-ok");
    if extracted_marker.exists() {
        return cache_dir;
    }

    fs::create_dir_all(&cache_dir).expect("failed to create cache dir");
    let archive_path = cache_dir.join(asset.asset_name);

    let url = format!(
        "https://github.com/shader-slang/slang/releases/download/v{}/{}",
        SLANG_VERSION, asset.asset_name
    );

    println!(
        "cargo:warning=Downloading Slang {} from {}",
        SLANG_VERSION, url
    );
    download(&url, &archive_path);

    let digest = sha256_of_file(&archive_path);
    if digest != asset.sha256 {
        let _ = fs::remove_file(&archive_path);
        panic!(
            "SHA-256 mismatch for {}: expected {}, got {}. Aborting — the download may be corrupted or tampered with.",
            asset.asset_name, asset.sha256, digest
        );
    }

    extract_zip(&archive_path, &cache_dir);

    fs::write(&extracted_marker, b"ok").expect("failed to write extraction marker");
    cache_dir
}

fn cache_dir() -> PathBuf {
    if let Ok(dir) = env::var("SHADER_SLANG_SYS_CACHE_DIR") {
        return PathBuf::from(dir);
    }
    dirs::cache_dir()
        .unwrap_or_else(env::temp_dir)
        .join("shader-slang-sys")
}

fn download(url: &str, dest: &Path) {
    let resp = ureq::get(url)
        .call()
        .unwrap_or_else(|e| panic!("failed to download {url}: {e}"));
    let mut reader = resp.into_reader();
    let mut file =
        fs::File::create(dest).unwrap_or_else(|e| panic!("failed to create {}: {e}", dest.display()));
    std::io::copy(&mut reader, &mut file)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", dest.display()));
}

fn sha256_of_file(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut file =
        fs::File::open(path).unwrap_or_else(|e| panic!("failed to open {}: {e}", path.display()));
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Extracts every entry in the archive into `dest`. All files are kept
/// (not just the main slang library) since the runtime may depend on
/// additional shared libraries shipped alongside it under `bin/`/`lib/`.
fn extract_zip(archive_path: &Path, dest: &Path) {
    let file = fs::File::open(archive_path)
        .unwrap_or_else(|e| panic!("failed to open {}: {e}", archive_path.display()));
    let mut zip = zip::ZipArchive::new(file)
        .unwrap_or_else(|e| panic!("failed to read zip {}: {e}", archive_path.display()));
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let Some(out_path) = entry.enclosed_name().map(|p| dest.join(p)) else {
            continue;
        };
        if entry.is_dir() {
            fs::create_dir_all(&out_path).unwrap();
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            let mut out_file = fs::File::create(&out_path).unwrap();
            std::io::copy(&mut entry, &mut out_file).unwrap();
        }
    }
}

fn generate_bindings(include_dir: &Path, target_arch: &str, is_windows: bool, is_msvc: bool) {
    let slang_dir = PathBuf::from("slang");

    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", slang_dir.display()))
        .clang_arg(format!("-I{}", include_dir.display()))
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-std=c++17")
        .clang_arg("-fno-exceptions")
        .clang_arg(format!(
            "-DSLANG_PTR_IS_64={}",
            if target_arch == "x86_64" || target_arch == "aarch64" {
                1
            } else {
                0
            }
        ))
        .allowlist_type("Slang.*")
        .allowlist_type("ISlang.*")
        .allowlist_function("slang.*")
        .allowlist_var("SLANG_.*")
        .allowlist_var("kIROp.*")
        .blocklist_type("std::.*")
        .blocklist_type("__gnu_cxx::.*")
        .blocklist_type("__std_.*")
        .use_core()
        .layout_tests(false)
        .generate_comments(true)
        .enable_cxx_namespaces()
        .derive_default(true)
        .derive_eq(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    if is_windows && is_msvc {
        builder = builder.clang_arg("-D_MSC_VER=1930").clang_arg("-D_WIN64");
    } else if is_windows {
        builder = builder
            .clang_arg("-D__MINGW32__")
            .clang_arg("-D_WIN64");
    } else {
        builder = builder
            .clang_arg("-D__linux__")
            .clang_arg("-DSLANG_LINUX=1");
    }

    let bindings = builder
        .generate()
        .expect("Unable to generate bindings - ensure Slang headers are present");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
