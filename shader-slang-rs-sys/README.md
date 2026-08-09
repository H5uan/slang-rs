# shader-slang-rs-sys

Low-level FFI bindings for the [Slang](https://github.com/shader-slang/slang/) shader language compiler (v2026.14.1).

bindgen generates the data types, enums and free functions from `slang.h`; the COM interface vtables are handwritten in `src/lib.rs` (`#[repr(C)]` structs, `_base` fields expressing inheritance). Most users should depend on the safe high-level API in [`shader-slang-rs`](https://crates.io/crates/shader-slang-rs) instead.

By default the build script downloads the official prebuilt Slang binaries for your platform; set `SLANG_DIR`/`SLANG_INCLUDE_DIR`/`SLANG_LIB_DIR` to use an existing installation, or enable the `source-build` feature to build the pinned `slang/` submodule with CMake.

Licensed under MIT or Apache-2.0.
