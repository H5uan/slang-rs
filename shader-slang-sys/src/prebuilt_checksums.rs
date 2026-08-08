// Version and checksum metadata for the official Slang prebuilt release
// archives that `build.rs` downloads when the `prebuilt` feature is active.
// This file is shared between the `shader-slang-sys` crate (via `include!`
// from lib.rs) and `build.rs` (via `include!` directly), so it must not use
// crate-level (`//!`) doc comments.
//
// `SHA256_TODO_*` entries are placeholders: this SHA-256 could not be
// computed in the environment that generated this table because outbound
// network access to GitHub's release-asset CDN was unavailable. Before
// shipping a release that depends on the `prebuilt` feature, replace every
// `SHA256_TODO_*` placeholder with the real digest — see
// `scripts/compute_checksums.sh` for a helper that downloads each asset and
// prints its SHA-256.
//
// `build.rs` panics at build time if it encounters an unreplaced
// placeholder, so a stale table fails loudly instead of silently skipping
// verification.

pub const SLANG_VERSION: &str = "2026.14.1";

pub const SHA256_TODO_WINDOWS_X86_64: &str = "TODO_FILL_SHA256_windows-x86_64";
pub const SHA256_TODO_LINUX_X86_64: &str = "TODO_FILL_SHA256_linux-x86_64";
pub const SHA256_TODO_LINUX_AARCH64: &str = "TODO_FILL_SHA256_linux-aarch64";
pub const SHA256_TODO_MACOS_X86_64: &str = "TODO_FILL_SHA256_macos-x86_64";
pub const SHA256_TODO_MACOS_AARCH64: &str = "TODO_FILL_SHA256_macos-aarch64";

/// One entry per supported (os, arch) target. `asset_name` must match the
/// filename of the corresponding GitHub release asset exactly.
pub struct PrebuiltAsset {
    pub os: &'static str,
    pub arch: &'static str,
    pub asset_name: &'static str,
    pub sha256: &'static str,
}

pub const PREBUILT_ASSETS: &[PrebuiltAsset] = &[
    PrebuiltAsset {
        os: "windows",
        arch: "x86_64",
        asset_name: "slang-2026.14.1-windows-x86_64.zip",
        sha256: SHA256_TODO_WINDOWS_X86_64,
    },
    PrebuiltAsset {
        os: "linux",
        arch: "x86_64",
        asset_name: "slang-2026.14.1-linux-x86_64.zip",
        sha256: SHA256_TODO_LINUX_X86_64,
    },
    PrebuiltAsset {
        os: "linux",
        arch: "aarch64",
        asset_name: "slang-2026.14.1-linux-aarch64.zip",
        sha256: SHA256_TODO_LINUX_AARCH64,
    },
    PrebuiltAsset {
        os: "macos",
        arch: "x86_64",
        asset_name: "slang-2026.14.1-macos-x86_64.zip",
        sha256: SHA256_TODO_MACOS_X86_64,
    },
    PrebuiltAsset {
        os: "macos",
        arch: "aarch64",
        asset_name: "slang-2026.14.1-macos-aarch64.zip",
        sha256: SHA256_TODO_MACOS_AARCH64,
    },
];

pub fn lookup(os: &str, arch: &str) -> Option<&'static PrebuiltAsset> {
    PREBUILT_ASSETS
        .iter()
        .find(|a| a.os == os && a.arch == arch)
}
