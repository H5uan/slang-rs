extern crate bindgen;

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The Slang release this crate binds against. This is the canonical Slang
/// version for the whole workspace; the version comment at the top of
/// `src/lib.rs` and the vtable signature snapshot header are checked against
/// it by tests. The handwritten vtables in `src/lib.rs` must match this
/// version's `slang.h` exactly.
const SLANG_VERSION: &str = "2026.14.1";

fn main() {
	println!("cargo:rerun-if-env-changed=SLANG_DIR");
	println!("cargo:rerun-if-env-changed=SLANG_INCLUDE_DIR");
	println!("cargo:rerun-if-env-changed=SLANG_LIB_DIR");

	let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
	let workspace_dir = manifest_dir.parent().unwrap().to_path_buf();

	// --- Locate the Slang library ---
	//
	// Priority: SLANG_LIB_DIR / SLANG_DIR env > `source-build` feature (cmake
	// build of the slang/ submodule) > download the prebuilt release archive.
	let lib_dir = if let Ok(dir) = env::var("SLANG_LIB_DIR") {
		PathBuf::from(dir)
	} else if let Ok(dir) = env::var("SLANG_DIR") {
		PathBuf::from(format!("{dir}/lib"))
	} else if env::var("CARGO_FEATURE_SOURCE_BUILD").is_ok() {
		build_from_source(&workspace_dir)
	} else {
		ensure_prebuilt(&workspace_dir)
	};

	// --- Locate the Slang headers ---
	//
	// The slang/ submodule (pinned to the matching release tag) is the
	// authoritative header source; the prebuilt archive's include/ directory
	// serves as fallback.
	let prebuilt_include = lib_dir
		.parent()
		.map(|p| p.join("include"))
		.unwrap_or_default();
	let include_dir = if let Ok(dir) = env::var("SLANG_INCLUDE_DIR") {
		PathBuf::from(dir)
	} else if let Ok(dir) = env::var("SLANG_DIR") {
		PathBuf::from(format!("{dir}/include"))
	} else if workspace_dir.join("slang/include/slang.h").exists() {
		workspace_dir.join("slang/include")
	} else if prebuilt_include.join("slang.h").exists() {
		prebuilt_include
	} else {
		panic!(
			"Could not locate slang.h (checked SLANG_INCLUDE_DIR, SLANG_DIR, slang/ submodule, prebuilt archive)"
		);
	};

	println!(
		"cargo:rerun-if-changed={}",
		include_dir.join("slang.h").display()
	);
	// Export the header location so the vtable ABI test in src/lib.rs can
	// cross-check the handwritten vtables against the exact slang.h in use.
	println!(
		"cargo:rustc-env=SHADER_SLANG_RS_SYS_SLANG_INCLUDE_DIR={}",
		include_dir.display()
	);

	println!("cargo:rustc-link-search=native={}", lib_dir.display());
	println!("cargo:rustc-link-lib=dylib=slang");

	// Runtime library lookup: Cargo only puts the profile directory and its
	// deps/ subdirectory on the loader search path (LD_LIBRARY_PATH /
	// DYLD_FALLBACK_LIBRARY_PATH on Unix); the rustc-link-search directory is
	// NOT searched at runtime, so the shared libraries must be copied next to
	// the executables on every platform. Downstream binaries running outside
	// Cargo must arrange their own rpath / loader path (or set SLANG_DIR and
	// handle it themselves).
	copy_runtime_libs_to_profile_dir(&lib_dir);

	let out_dir = env::var("OUT_DIR").expect("Couldn't determine output directory.");

	bindgen::builder()
		.header(include_dir.join("slang.h").to_str().unwrap())
		.clang_arg("-v")
		.clang_arg("-xc++")
		.clang_arg("-std=c++17")
		.allowlist_function("spReflection.*")
		.allowlist_function("spComputeStringHash")
		.allowlist_function("spGetBuildTagString")
		.allowlist_function("slang_.*")
		.allowlist_type("slang.*")
		// Types referenced by the handwritten ISlangFileSystemExt /
		// ISlangMutableFileSystem vtables in src/lib.rs.
		.allowlist_type("SlangPathType.*")
		.allowlist_type(".*PathKind")
		.allowlist_type("FileSystemContentsCallBack")
		.allowlist_var("SLANG_.*")
		.with_codegen_config(
			bindgen::CodegenConfig::FUNCTIONS
				| bindgen::CodegenConfig::TYPES
				| bindgen::CodegenConfig::VARS,
		)
		.parse_callbacks(Box::new(ParseCallback {}))
		.default_enum_style(bindgen::EnumVariation::Rust {
			non_exhaustive: false,
		})
		.constified_enum("SlangProfileID")
		.constified_enum("SlangCapabilityID")
		.vtable_generation(true)
		.layout_tests(false)
		.derive_copy(true)
		.generate()
		.expect("Couldn't generate bindings.")
		.write_to_file(format!("{out_dir}/bindings.rs").as_str())
		.expect("Couldn't write bindings.");
}

/// Download and unpack the prebuilt Slang release into
/// `target/slang-bin/v<SLANG_VERSION>`, returning its `lib` directory.
/// Cached across builds. The cache directory is keyed on `SLANG_VERSION` so
/// a version bump never reuses stale binaries.
fn ensure_prebuilt(workspace_dir: &Path) -> PathBuf {
	let cache_dir = workspace_dir
		.join("target/slang-bin")
		.join(format!("v{SLANG_VERSION}"));
	let lib_dir = cache_dir.join("lib");

	if lib_dir.join(import_lib_name()).exists() {
		return lib_dir;
	}

	// Artifact names follow
	// `slang-{version}-{os}-{arch}.{zip,tar.gz}` (see the v2026.14.1 release
	// page). The plain `linux-*` archives target a current glibc baseline;
	// upstream additionally publishes `-glibc-2.27`/`-glibc-2.28` variants
	// for older systems, which can be pointed at manually via SLANG_DIR.
	// The Linux and macOS branches are written against the official release
	// artifacts but have not been verified on a local machine.
	let (os, arch, ext) = match (env::consts::OS, env::consts::ARCH) {
		("windows", "x86_64") => ("windows", "x86_64", "zip"),
		("windows", "aarch64") => ("windows", "aarch64", "zip"),
		("linux", "x86_64") => ("linux", "x86_64", "tar.gz"),
		("linux", "aarch64") => ("linux", "aarch64", "tar.gz"),
		("macos", "x86_64") => ("macos", "x86_64", "tar.gz"),
		("macos", "aarch64") => ("macos", "aarch64", "tar.gz"),
		(other_os, other_arch) => panic!(
			"No prebuilt Slang archive for {other_os}/{other_arch}. \
			Set SLANG_DIR or use --features source-build instead."
		),
	};

	let url = format!(
		"https://github.com/shader-slang/slang/releases/download/v{SLANG_VERSION}/slang-{SLANG_VERSION}-{os}-{arch}.{ext}"
	);
	let archive_name = format!("slang-{SLANG_VERSION}-{os}-{arch}.{ext}");
	let archive_path = cache_dir.join(&archive_name);

	std::fs::create_dir_all(&cache_dir).expect("Couldn't create slang-bin cache directory.");

	// Download to a temporary file and rename into place only after the
	// download succeeded, so an interrupted download cannot poison the cache.
	if !archive_path.exists() {
		let tmp_archive_path = cache_dir.join(format!("{archive_name}.tmp"));
		run(Command::new("curl")
			.args(["-sL", "-f", "-o"])
			.arg(&tmp_archive_path)
			.arg(&url));
		std::fs::rename(&tmp_archive_path, &archive_path)
			.expect("Couldn't move the downloaded Slang archive into the cache.");
	}

	// Extract into a staging directory and move the contents into the cache
	// only after extraction succeeded and the expected import library is
	// present, so a partial extraction cannot poison the cache either.
	let staging_dir = cache_dir.join(".staging");
	if staging_dir.exists() {
		std::fs::remove_dir_all(&staging_dir)
			.expect("Couldn't clean up slang-bin staging directory.");
	}
	std::fs::create_dir_all(&staging_dir).expect("Couldn't create slang-bin staging directory.");

	// bsdtar (Windows) and GNU/BSD tar (Linux/macOS) all auto-detect the
	// zip/gzip format, so a single extraction path covers every platform.
	run(Command::new("tar")
		.arg("-xf")
		.arg(&archive_path)
		.arg("-C")
		.arg(&staging_dir));

	assert!(
		staging_dir.join("lib").join(import_lib_name()).exists(),
		"Prebuilt Slang archive did not contain expected lib/{}",
		import_lib_name()
	);

	for entry in std::fs::read_dir(&staging_dir)
		.expect("Couldn't read slang-bin staging directory.")
		.flatten()
	{
		// share/ is ~24 MB of documentation we never consume, and its doc tree
		// contains symlinks that dangle after extraction on unix, which makes
		// actions/cache fail to save this directory in CI.
		if entry.file_name() == "share" {
			continue;
		}
		let dest = cache_dir.join(entry.file_name());
		if dest.is_dir() {
			std::fs::remove_dir_all(&dest).expect("Couldn't replace stale slang-bin cache entry.");
		} else if dest.exists() {
			std::fs::remove_file(&dest).expect("Couldn't replace stale slang-bin cache entry.");
		}
		std::fs::rename(entry.path(), &dest)
			.expect("Couldn't move extracted Slang files into the cache.");
	}
	std::fs::remove_dir_all(&staging_dir).ok();

	lib_dir
}

/// Build the slang/ submodule with cmake, returning the library directory.
fn build_from_source(workspace_dir: &Path) -> PathBuf {
	let slang_dir = workspace_dir.join("slang");
	let build_dir = slang_dir.join("build");

	assert!(
		slang_dir.join("CMakeLists.txt").exists(),
		"slang/ submodule is not initialized; run `git submodule update --init`"
	);

	// The compiler library needs these bundled dependencies; tools and tests
	// are disabled, so their externals (slang-rhi, imgui, ...) are skipped.
	run(Command::new("git")
		.arg("-C")
		.arg(&slang_dir)
		.args(["submodule", "update", "--init"])
		.args([
			"external/spirv-tools",
			"external/spirv-headers",
			"external/glslang",
			"external/miniz",
			"external/lz4",
			"external/unordered_dense",
			"external/fast_float",
			"external/mimalloc",
			"external/vulkan",
			"external/cmark",
			"external/lua",
		]));

	if !build_dir.join("CMakeCache.txt").exists() {
		let mut configure = Command::new("cmake");
		configure
			.arg("-S")
			.arg(&slang_dir)
			.arg("-B")
			.arg(&build_dir)
			.arg("-DCMAKE_BUILD_TYPE=Release")
			// Only the compiler library is needed; skip tools, tests and
			// optional dependencies to keep the build small.
			.arg("-DSLANG_SLANG_LLVM_FLAVOR=DISABLE")
			.arg("-DSLANG_ENABLE_SLANG_RHI=OFF")
			.arg("-DSLANG_ENABLE_GFX=OFF")
			.arg("-DSLANG_ENABLE_TESTS=OFF")
			.arg("-DSLANG_ENABLE_EXAMPLES=OFF")
			.arg("-DSLANG_ENABLE_SLANGD=OFF")
			.arg("-DSLANG_ENABLE_SLANGI=OFF")
			.arg("-DSLANG_ENABLE_SLANGRT=OFF")
			.arg("-DSLANG_ENABLE_REPLAYER=OFF");
		if cfg!(windows) {
			// Avoid C4819 (code page) warnings being escalated to errors on
			// non-UTF-8 locales (MSVC-only flag).
			configure
				.arg("-DCMAKE_C_FLAGS=/utf-8")
				.arg("-DCMAKE_CXX_FLAGS=/utf-8");
		}
		run(&mut configure);
	}

	run(Command::new("cmake")
		.arg("--build")
		.arg(&build_dir)
		.arg("--config")
		.arg("Release")
		.arg("--target")
		.arg("slang")
		.arg("--parallel"));

	// The main target produces slang-compiler.dll; on Windows the `slang`
	// import library/DLL is a separately-built proxy that forwards to it.
	if cfg!(windows) {
		run(Command::new("cmake")
			.arg("--build")
			.arg(&build_dir)
			.arg("--config")
			.arg("Release")
			.arg("--target")
			.arg("slang-proxy")
			.arg("--parallel"));
	}

	// Multi-config generators (Visual Studio) nest under Release/, single-config do not.
	for candidate in [build_dir.join("Release/lib"), build_dir.join("lib")] {
		if candidate.join(import_lib_name()).exists() {
			return candidate;
		}
	}

	panic!(
		"Slang source build finished but no {} found under {}",
		import_lib_name(),
		build_dir.display()
	);
}

/// Import library (MSVC) or shared library name used to sanity-check lib dirs.
fn import_lib_name() -> &'static str {
	if cfg!(windows) {
		"slang.lib"
	} else if cfg!(target_os = "macos") {
		"libslang.dylib"
	} else {
		"libslang.so"
	}
}

/// Copy the Slang runtime libraries next to the test/binary executables so
/// `cargo test` and `cargo run` can load them without extra setup.
///
/// Slang's companion libraries use versioned sonames
/// (`libslang-compiler.so.0.2026.14.1`, `libslang-compiler.0.2026.14.1.dylib`),
/// so every alias must be present under its own name. Symlinks are recreated
/// as symlinks to avoid duplicating the (large) library contents.
fn copy_runtime_libs_to_profile_dir(lib_dir: &Path) {
	let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
	// OUT_DIR is <profile>/build/<pkg>-<hash>/out.
	let Some(profile_dir) = out_dir.ancestors().nth(3).map(|p| p.to_path_buf()) else {
		return;
	};

	let bin_dir = lib_dir.parent().map(|p| p.join("bin"));
	let lib_dirs = [Some(lib_dir), bin_dir.as_deref().filter(|p| p.exists())];

	for dir in lib_dirs.into_iter().flatten() {
		for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
			let path = entry.path();
			let name = path.file_name().unwrap().to_str().unwrap_or_default();
			let is_runtime_lib = if cfg!(windows) {
				path.extension().is_some_and(|ext| ext == "dll")
			} else {
				name.starts_with("libslang") && (name.contains(".so") || name.contains(".dylib"))
			};
			if !is_runtime_lib {
				continue;
			}
			for dest_dir in [&profile_dir, &profile_dir.join("deps")] {
				if !dest_dir.exists() {
					continue;
				}
				copy_preserving_symlinks(&path, &dest_dir.join(name));
			}
		}
	}
}

/// Copy `src` to `dest`; on Unix a symlink is recreated as a symlink
/// pointing at the same (relative) target instead of duplicating contents.
fn copy_preserving_symlinks(src: &Path, dest: &Path) {
	#[cfg(unix)]
	if std::fs::symlink_metadata(src).is_ok_and(|m| m.file_type().is_symlink()) {
		if let Ok(target) = std::fs::read_link(src) {
			let _ = std::fs::remove_file(dest);
			let _ = std::os::unix::fs::symlink(target, dest);
			return;
		}
	}
	let _ = std::fs::copy(src, dest);
}

fn run(command: &mut Command) {
	let status = command
		.status()
		.unwrap_or_else(|e| panic!("Failed to run {command:?}: {e}"));
	assert!(status.success(), "Command {command:?} failed with {status}");
}

#[derive(Debug)]
struct ParseCallback {}

impl bindgen::callbacks::ParseCallbacks for ParseCallback {
	fn enum_variant_name(
		&self,
		enum_name: Option<&str>,
		original_variant_name: &str,
		_variant_value: bindgen::callbacks::EnumVariantValue,
	) -> Option<String> {
		let enum_name = enum_name?;

		// Map enum names to the part of their variant names that needs to be trimmed.
		// When an enum name is not in this map the code below will try to trim the enum name itself.
		let mut map = std::collections::HashMap::new();
		map.insert("SlangMatrixLayoutMode", "SlangMatrixLayout");
		map.insert("SlangCompileTarget", "Slang");

		let trim = map.get(enum_name).unwrap_or(&enum_name);
		let new_variant_name = pascal_case_from_snake_case(original_variant_name);
		let new_variant_name = new_variant_name.trim_start_matches(trim);
		Some(new_variant_name.to_string())
	}

	#[cfg(feature = "serde")]
	fn add_derives(&self, info: &bindgen::callbacks::DeriveInfo<'_>) -> Vec<String> {
		if info.name.starts_with("Slang") && info.kind == bindgen::callbacks::TypeKind::Enum {
			return vec!["serde::Serialize".into(), "serde::Deserialize".into()];
		}

		// All-scalar structs used in the public high-level API.
		if matches!(
			info.name.as_ref(),
			"slang_ByteCodeFuncInfo" | "slang_CoverageBufferInfo"
		) {
			return vec!["serde::Serialize".into(), "serde::Deserialize".into()];
		}

		vec![]
	}
}

/// Converts `snake_case` or `SNAKE_CASE` to `PascalCase`.
/// If the input is already in `PascalCase` it will be returned as is.
fn pascal_case_from_snake_case(snake_case: &str) -> String {
	let mut result = String::new();

	let should_lower = snake_case
		.chars()
		.filter(|c| c.is_alphabetic())
		.all(|c| c.is_uppercase());

	for part in snake_case.split('_') {
		for (i, c) in part.chars().enumerate() {
			if i == 0 {
				result.push(c.to_ascii_uppercase());
			} else if should_lower {
				result.push(c.to_ascii_lowercase());
			} else {
				result.push(c);
			}
		}
	}

	result
}
