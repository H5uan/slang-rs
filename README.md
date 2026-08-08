<div align="center">

# slang-rs
**Rust bindings for the [Slang](https://github.com/shader-slang/slang/) shader language compiler**

</div>

Supports both the modern compilation and reflection API.

## Example

```rust
let global_session = slang_rs::GlobalSession::new().unwrap();

let search_path = std::ffi::CString::new("shaders/directory").unwrap();

// All compiler options are available through this builder.
let session_options = slang_rs::CompilerOptions::default()
	.optimization(slang_rs::OptimizationLevel::High)
	.matrix_layout_row(true);

let target_desc = slang_rs::TargetDesc::default()
	.format(slang_rs::CompileTarget::Spirv)
	.profile(global_session.find_profile("glsl_450"));

let targets = [target_desc];
let search_paths = [search_path.as_ptr()];

let session_desc = slang_rs::SessionDesc::default()
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

## Installation

Add `slang-rs` to the `[dependencies]` section of your `Cargo.toml`.

By default, the build script automatically downloads the prebuilt Slang v2026.14.1 binaries from the [Slang releases page](https://github.com/shader-slang/slang/releases) and caches them in `target/slang-bin`. No manual setup is required; `slang.dll` is copied next to your executable automatically.

If you prefer to build Slang from source, enable the `source-build` feature. This builds the pinned `slang/` git submodule (v2026.14.1) with CMake:

```bash
cargo build --features source-build
```

Alternatively, point this library to an existing Slang installation by setting the `SLANG_DIR` environment variable to the path of your Slang directory. To specify the `include` and `lib` directories separately, set the `SLANG_INCLUDE_DIR` and `SLANG_LIB_DIR` environment variables. Environment variables take precedence over the other options. To compile to DXIL bytecode, copy `dxil.dll` and `dxcompiler.dll` from the [Microsoft DirectXShaderCompiler](https://github.com/microsoft/DirectXShaderCompiler/releases) to your executable's directory.

## Credits

Based on [FloatyMonkey/slang-rs](https://github.com/FloatyMonkey/slang-rs), originally maintained by Lauro Oyen ([@laurooyen](https://github.com/laurooyen)).

Licensed under MIT or Apache-2.0.
