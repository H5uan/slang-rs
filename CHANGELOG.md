# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-08-09

### Remaining misc capabilities

- `ModulePrecompileService` (experimental `IModulePrecompileService_Experimental`): RAII wrapper obtained via `Module::precompile_service`, with `precompile_for_target`, `precompiled_target_code`, `module_dependency_count`, `module_dependency`.
- Metadata info accessors: `CoverageTracingMetadata::entry_info` / `buffer_info` (returning the new `CoverageEntryInfo` / `CoverageBufferInfo` structs plus `CoverageEntryKind` / `CoverageCounterMode` / `CoverageBranchArmKind` re-exports), `SyntheticResourceMetadata::resource_info` / `find_resource_index_by_id` (returning `SyntheticResourceInfo`, plus `SyntheticResourceScope` / `SyntheticResourceAccess` re-exports), and the four `CooperativeTypesMetadata::cooperative_*_by_index` methods (returning the re-exported `CooperativeMatrixType` / `CooperativeMatrixCombination` / `CooperativeVectorTypeUsageInfo` / `CooperativeVectorCombination` sys structs).
- `GlobalSession`: `add_builtins`, `set_default_downstream_compiler` / `default_downstream_compiler`, `set_downstream_compiler_for_transition` / `downstream_compiler_for_transition`, `downstream_compiler_version`, and the experimental builtin-module trio `compile_builtin_module` / `load_builtin_module` / `save_builtin_module` (with the `BuiltinModuleName` re-export).
- `Session::load_module_from_source`: loads a module from a `Blob` source; `Blob::new` builds a blob from raw bytes (`slang_createBlob`).
- `build_tag_string` free function (`spGetBuildTagString`): the Slang build tag without creating a global session.
- `ByteCodeRunner` (experimental `IByteCodeRunner`) and `disassemble_byte_code`: creation plus the module/function selection surface (`load_module`, `select_function_by_index`, `find_function_by_name`, `function_info`, `error_string`, with the `ByteCodeFuncInfo` re-export); the raw-pointer/callback-driven methods (`execute`, `registerExtCall`, ...) stay unwrapped. `select_function_by_index` / `find_function_by_name` / `function_info` are guarded by a loaded-state flag and short-circuit until `load_module` succeeds, because Slang v2026.14.1 reads an uninitialized module view in those methods on a fresh runner (undefined behavior; observed as a SIGBUS on macOS aarch64).
- sys crate: handwritten `IModulePrecompileServiceExperimentalVtable` / `IByteCodeRunnerVtable` (verified against slang.h by the ABI method-count test); `spGetBuildTagString` added to the bindgen allowlist.

### Extended file system interfaces

- `MutableFileSystem` (`ISlangMutableFileSystem`): RAII wrapper with full path management (`load_file`, `file_unique_identity`, `calc_combined_path`, `path_type`, `get_path`, `clear_cache`, `enumerate_path_contents`, `os_path_kind`) and write operations (`save_file`, `save_file_blob`, `remove`, `create_directory`). New `PathType` / `PathKind` / `OSPathKind` re-exports.
- `ComponentType::get_result_as_file_system`: exposes the compilation outputs (compiled code, diagnostics, source maps) as files in an in-memory file system.
- Reverse COM extensions for user file systems: the `FileSystemExt` trait (path management, exposed via `FileSystemObject::new_ext`) and the `WritableFileSystem` trait (write operations, exposed via `FileSystemObject::new_writable`). An Ext-level object answers `queryInterface(ISlangFileSystemExt)`, so Slang uses the implementation's path management directly instead of wrapping it in its `CacheFileSystem` emulation.
- `FileSystemError::NotImplemented`: new variant mapping to `SLANG_E_NOT_IMPLEMENTED`.
- sys crate: handwritten `ISlangFileSystemExtVtable` / `ISlangMutableFileSystemVtable` (verified against slang.h by the ABI method-count test), `SLANG_E_NOT_IMPLEMENTED`.

### Host-callable CPU execution

- `SharedLibrary` (`ISlangSharedLibrary`): RAII wrapper with `find_symbol` for looking up exported functions/variables in compiled CPU code.
- `ComponentType::entry_point_host_callable` and `ComponentType2::target_host_callable`: compile entry points to host machine code (e.g. `CompileTarget::ShaderHostCallable`) callable directly from Rust.
- New `examples/host_callable.rs` example: compiles a compute entry point and invokes the exported function with the documented `ComputeVaryingInput` / `UniformState` ABI.

## [0.1.0]

Initial release of `shader-slang-rs` / `shader-slang-rs-sys`, Rust bindings for the [Slang](https://github.com/shader-slang/slang/) shader language compiler, aligned with Slang **v2026.14.1**. Started from [FloatyMonkey/slang-rs](https://github.com/FloatyMonkey/slang-rs) (MIT OR Apache-2.0) with the API surface extended to the v2026.14.1 capability set.

### Core compilation flow

- `GlobalSession` / `Session` / `Module` / `ComponentType` / `EntryPoint` / `Blob` / `Metadata`: safe, reference-counted RAII wrappers over Slang's COM interfaces.
- Builder-style descriptors: `SessionDesc`, `TargetDesc`, `CompilerOptions`, `PreprocessorMacroDesc`.
- Full compilation pipeline: load module → find entry point → composite component types → link → target/entry-point code and metadata.
- Specialization: `SpecializationArg` (type / expression), component-type specialization, entry-point specialization.

### Reflection

- Zero-overhead borrowed wrappers in the `reflection` module: `Shader`, `EntryPoint`, `Variable`, `VariableLayout`, `Type`, `TypeLayout`, `Function`, `Decl`, `Generic`, `TypeParameter`, `UserAttribute`.
- JSON reflection output (`Shader::to_json`), string hashing (`spComputeStringHash`).

### v2026 API surface

- Command-line argument parsing (`parse_command_line_arguments`), session desc digest.
- Core module management: compile / load / save core module, global session without core module.
- Downstream compiler and pass-through support checks, language preludes, SPIR-V core grammar, compiler elapsed time.
- Generic interface specialization (`ComponentType::specialize`, linked specialization, conformance info), `bindless_space_index`, and other interfaces added up to v2026.14.1.

### Virtual file system

- Reverse COM: implement `FileSystem` in Rust and hand it to Slang via `FileSystemObject`, with handwritten vtable thunks and panic containment (`catch_unwind`) at the FFI boundary.

### Platforms

- Windows x86_64 (MSVC), tested.
- Windows aarch64, Linux x86_64 / aarch64, macOS x86_64 / aarch64 via official prebuilt binaries, verified in CI.
- Automatic download of the prebuilt Slang v2026.14.1 release by default; `source-build` feature builds the pinned `slang/` submodule with CMake; `SLANG_DIR` / `SLANG_INCLUDE_DIR` / `SLANG_LIB_DIR` overrides supported.
