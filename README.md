<div align="center">

# shader-slang-rs
**Rust bindings for the [Slang](https://github.com/shader-slang/slang/) shader language compiler**

</div>

Supports both the modern compilation and reflection API.

## Example

```rust
let global_session = shader_slang_rs::GlobalSession::new().unwrap();

let search_path = std::ffi::CString::new("shaders/directory").unwrap();

// All compiler options are available through this builder.
let session_options = shader_slang_rs::CompilerOptions::default()
	.optimization(shader_slang_rs::OptimizationLevel::High)
	.matrix_layout_row(true);

let target_desc = shader_slang_rs::TargetDesc::default()
	.format(shader_slang_rs::CompileTarget::Spirv)
	.profile(global_session.find_profile("glsl_450"));

let targets = [target_desc];
let search_paths = [search_path.as_ptr()];

let session_desc = shader_slang_rs::SessionDesc::default()
	.targets(&targets)
	.search_paths(&search_paths)
	.options(&session_options);

let session = global_session.create_session(&session_desc).unwrap();
let module = session.load_module("filename.slang").unwrap();
let entry_point = module.find_entry_point_by_name("main").unwrap();

let program = session
	.create_composite_component_type(&[module.into(), entry_point.into()])
	.unwrap();

let linked_program = program.link().unwrap();

// Entry point to the reflection API.
let reflection = linked_program.layout(0).unwrap();

let shader_bytecode = linked_program.entry_point_code(0, 0).unwrap();
```

## Supported platforms

| Platform | Status |
| --- | --- |
| Windows x86_64 (MSVC) | Tested |
| Windows aarch64 | Prebuilt download available, untested |
| Linux x86_64 / aarch64 | Supported via official prebuilt binaries, not yet verified locally |
| macOS x86_64 / aarch64 | Supported via official prebuilt binaries, not yet verified locally |

## Installation

Add `shader-slang-rs` to the `[dependencies]` section of your `Cargo.toml`:

```toml
[dependencies]
shader-slang-rs = "0.1"
```

The low-level FFI bindings live in the separate `shader-slang-rs-sys` crate, which `shader-slang-rs` re-exports as needed; depend on it directly only if you want to work with the raw API.

By default, the build script automatically downloads the prebuilt Slang v2026.14.1 binaries for your platform from the [Slang releases page](https://github.com/shader-slang/slang/releases) and caches them in `target/slang-bin`. No manual setup is required: the runtime libraries (`slang.dll`, `libslang*.so*`, `libslang*.dylib*`, including versioned soname aliases) are copied next to your executables automatically, so `cargo build`/`cargo test`/`cargo run` work out of the box on every platform. Standalone binaries run outside of Cargo need `libslang` on the loader path (e.g. via rpath or `LD_LIBRARY_PATH`).

If you prefer to build Slang from source, enable the `source-build` feature. This builds the pinned `slang/` git submodule (v2026.14.1) with CMake:

```bash
cargo build --features source-build
```

Alternatively, point this library to an existing Slang installation by setting the `SLANG_DIR` environment variable to the path of your Slang directory. To specify the `include` and `lib` directories separately, set the `SLANG_INCLUDE_DIR` and `SLANG_LIB_DIR` environment variables. Environment variables take precedence over the other options. To compile to DXIL bytecode, copy `dxil.dll` and `dxcompiler.dll` from the [Microsoft DirectXShaderCompiler](https://github.com/microsoft/DirectXShaderCompiler/releases) to your executable's directory.

## Credits

Started from [FloatyMonkey/slang-rs](https://github.com/FloatyMonkey/slang-rs), originally maintained by Lauro Oyen ([@laurooyen](https://github.com/laurooyen)); the API surface has since been extended to cover Slang v2026.14.1.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
