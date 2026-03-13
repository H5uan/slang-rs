# Slang Rust Bindings

Rust bindings for the [Slang](https://github.com/shader-slang/slang) shading language compiler.

## Overview

This crate provides Rust bindings to the Slang compiler, enabling:

- Compilation of HLSL/Slang shaders to multiple targets (SPIRV, DXIL, GLSL, Metal, etc.)
- Shader reflection and introspection
- Cross-platform shader development

## Prerequisites

- Rust 1.70+ (2021 edition)
- Slang library binaries
- C++ compiler (for building bindgen dependencies)

## Building

1. Clone this repository with the Slang submodule:
   ```bash
   git clone --recursive https://github.com/yourusername/slang-rs
   cd slang-rs
   ```

2. Build the project:
   ```bash
   cargo build
   ```

3. Run tests:
   ```bash
   cargo test
   ```

## Usage

```rust
use slang_rs::GlobalSession;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a global session
    let session = GlobalSession::new()?;

    // Create a compilation session with target settings
    let compile_session = session.create_session(&SessionDesc {
        targets: &[CompileTarget::Spirv],
        ..Default::default()
    })?;

    // Load a shader module
    let module = compile_session.load_module("shader.slang")?;

    // Get compiled code for an entry point
    let code = module.get_entry_point_code("main")?;

    Ok(())
}
```

## Project Structure

```
slang-rs/
├── Cargo.toml          # Package manifest
├── build.rs            # Build script for bindgen
├── wrapper.h           # C header wrapper for bindgen
├── README.md           # This file
├── src/
│   ├── lib.rs          # Main library entry point
│   └── ffi/            # FFI bindings module
│       └── mod.rs      # FFI module (low-level bindings)
└── slang/              # Slang library headers
    ├── slang.h         # Core Slang C API
    └── slang-gfx.h     # Graphics-specific API
```

## License

This project is licensed under the MIT License. See LICENSE for details.

The Slang compiler itself is licensed under the Apache 2.0 license with LLVM exceptions.
