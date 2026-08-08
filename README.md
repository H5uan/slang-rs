# shader-slang (Rust bindings for Slang)

Rust bindings for the [Slang](https://github.com/shader-slang/slang) shading language compiler.

This repository is a Cargo workspace with two crates:

- **`shader-slang-sys`** — low-level, `bindgen`-generated raw FFI bindings, plus the build-time logic for obtaining a Slang binary (either downloading an official prebuilt release, or building from the `slang` git submodule).
- **`shader-slang`** (this crate, at the repository root) — a safe, idiomatic Rust API built on top of `shader-slang-sys`.

## Overview

The `shader-slang` crate currently supports:

- Compiling Slang/HLSL source (as an in-memory string) to SPIRV
- Reading back diagnostics (errors/warnings) as readable text on compile failure

Shader reflection and additional compile targets (DXIL, GLSL, Metal, etc.) are planned but not yet implemented — see [`docs/vtable-layout.md`](docs/vtable-layout.md) for the details of how the safe API calls into Slang's COM-style C++ interfaces.

## Prerequisites

- Rust 1.70+ (2021 edition)
- A C/C++ toolchain (bindgen needs `libclang`)
- Either network access to GitHub Releases (`prebuilt` feature, default) or CMake + a full C++ toolchain to build Slang from source (`build-from-source` feature)

## Building

```bash
git clone --recursive https://github.com/yourusername/shader-slang-rs
cd shader-slang-rs
cargo build
```

By default this downloads the official Slang v2026.14.1 prebuilt binaries for your platform (Windows/Linux/macOS, x86_64/aarch64) and verifies them against a pinned SHA-256 checksum table in `shader-slang-sys/src/prebuilt_checksums.rs`.

To build Slang from source instead (e.g. to use a patched Slang, or a platform without prebuilt binaries):

```bash
cd shader-slang-sys/slang
cmake --preset default
cmake --build --preset release
cd ../..
cargo build --no-default-features --features build-from-source
```

## Usage

```rust,no_run
use shader_slang::GlobalSession;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let global = GlobalSession::new()?;
    let session = global.create_session(None)?; // None = default SPIR-V profile

    let module = session.load_module_from_source(
        "my_shader",
        "my_shader.slang",
        r#"
        [shader("compute")]
        [numthreads(1, 1, 1)]
        void main(uint3 tid : SV_DispatchThreadID) {}
        "#,
    )?;

    let entry_point = module.find_entry_point_by_name("main")?;
    let program = entry_point.link()?;
    let spirv_bytes = program.get_target_code(0)?;

    Ok(())
}
```

## Testing

```bash
cargo test                    # unit tests, no Slang binary required
cargo test -- --ignored       # integration tests against a real Slang (see tests/compile.rs)
```

## License

This project (`shader-slang` and `shader-slang-sys`) is dual-licensed under MIT or Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.

Slang itself is licensed under Apache-2.0 WITH LLVM-exception — see [`shader-slang-sys/THIRD_PARTY_LICENSES.md`](shader-slang-sys/THIRD_PARTY_LICENSES.md).
