# shader-slang-rs

**Rust bindings for the [Slang](https://github.com/shader-slang/slang/) shader language compiler** — targeting v2026.14.1.

Safe, high-level wrappers over the Slang compilation and reflection API, with a
hand-written FFI layer for the COM-style interfaces. Two crates in one
workspace:

- **`shader-slang-rs`** — idiomatic Rust API (reference-counted RAII, builder
  descriptors, zero-overhead reflection wrappers, file system callbacks).
- **`shader-slang-rs-sys`** — raw `#[repr(C)]` FFI bindings (generated types +
  hand-written vtable structs). Re-exported by the high-level crate; depend on
  it directly only if you need the raw API.

## Quick start

```toml
[dependencies]
shader-slang-rs = "0.2"
```

```rust
let global_session = shader_slang_rs::GlobalSession::new().unwrap();

let session_desc = shader_slang_rs::SessionDesc::default()
    .targets(&[shader_slang_rs::TargetDesc::default()
        .format(shader_slang_rs::CompileTarget::Spirv)
        .profile(global_session.find_profile("glsl_450"))])
    .options(&shader_slang_rs::CompilerOptions::default()
        .optimization(shader_slang_rs::OptimizationLevel::High));

let session = global_session.create_session(&session_desc).unwrap();
let module = session.load_module("shader.slang").unwrap();
let entry_point = module.find_entry_point_by_name("main").unwrap();

let program = session
    .create_composite_component_type(&[module.into(), entry_point.into()])
    .unwrap()
    .link()
    .unwrap();

let reflection = program.layout(0).unwrap();
let bytecode = program.entry_point_code(0, 0).unwrap();
```

See the [examples](./examples) directory for more:

```bash
cargo run --example compile
cargo run --example reflect
cargo run --example host_callable
cargo run --example virtual_file_system
```

## Installation

### Default: automatic download

The build script downloads the prebuilt Slang v2026.14.1 binaries for your
platform from the [releases page](https://github.com/shader-slang/slang/releases)
and caches them in `target/slang-bin`. Runtime libraries are copied next to
your executables, so `cargo build` / `cargo test` / `cargo run` work out of the
box.

Standalone binaries need `libslang` on the loader path (rpath, `LD_LIBRARY_PATH`,
`DYLD_LIBRARY_PATH`, or `PATH`).

### Build from source

```bash
cargo build --features source-build
```

Requires CMake, a C++ compiler, and the `slang/` git submodule checked out
(pinned to v2026.14.1).

### Use an existing Slang installation

| Variable | Purpose |
|---|---|
| `SLANG_DIR` | Root of a Slang installation (looks for `include/` and `lib/` underneath) |
| `SLANG_LIB_DIR` | Path to `libslang` shared library |
| `SLANG_INCLUDE_DIR` | Path to `slang.h` |

Environment variables take precedence over the default download and
`source-build`.

### DXIL compilation

Copy `dxil.dll` and `dxcompiler.dll` from the
[DirectXShaderCompiler](https://github.com/microsoft/DirectXShaderCompiler/releases)
release to your executable's directory.

## Platform support

| Platform | Status |
|---|---|
| Windows x86_64 (MSVC) | Tested |
| Windows aarch64 | Prebuilt available, untested |
| Linux x86_64 / aarch64 | Prebuilt available, not yet verified locally |
| macOS x86_64 / aarch64 | Prebuilt available, not yet verified locally |

## Feature flags

| Feature | Description |
|---|---|
| `source-build` | Build Slang from the pinned `slang/` submodule instead of downloading prebuilt binaries |
| `serde` | Enable `Serialize`/`Deserialize` on select types |

## Project structure

| Path | Contents |
|---|---|
| [`src/`](./src) | High-level API crate: `GlobalSession`, `Session`, `Module`, `ComponentType`, `EntryPoint`, `Blob`, `Metadata`, `MutableFileSystem` |
| [`src/reflection/`](./src/reflection) | Reflection wrappers (zero-cost borrowed views into Slang's reflection data) |
| [`src/file_system.rs`](./src/file_system.rs) | Reverse COM interop — implement `ISlangFileSystem`/`ISlangMutableFileSystem` in Rust for C++ callbacks |
| [`src/tests.rs`](./src/tests.rs) | End-to-end integration tests (real compilation, no mocks) |
| [`examples/`](./examples) | Runnable examples: compile, reflect, host-callable, virtual file system |
| [`shader-slang-rs-sys/`](./shader-slang-rs-sys) | Raw FFI crate: bindgen types + hand-written `#[repr(C)]` COM vtable structs |
| [`slang/`](./slang) | Slang source submodule (pinned at v2026.14.1, only needed for `source-build`) |

## Upgrading Slang

When upgrading to a new Slang version, hand-written COM vtable structs in
`shader-slang-rs-sys/src/lib.rs` must be checked against the new `slang.h`
method order (Slang only appends methods at the end of each interface). Update
`SLANG_VERSION` in `shader-slang-rs-sys/build.rs`, the submodule tag, and the
version comment at the top of `shader-slang-rs-sys/src/lib.rs`.

## Credits

Started from [FloatyMonkey/slang-rs](https://github.com/FloatyMonkey/slang-rs)
by [Lauro Oyen](https://github.com/laurooyen). The API surface has since been
extended to cover Slang v2026.14.1.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
