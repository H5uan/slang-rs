//! Integration tests exercising the full "source in, bytecode out" path
//! against a real Slang binary.
//!
//! These are marked `#[ignore]` because they need an actual Slang library
//! present — either built from source (`cargo build --no-default-features
//! --features build-from-source` after building `shader-slang-sys/slang`
//! via CMake) or downloaded via the `prebuilt` feature once
//! `prebuilt_checksums.rs` has real SHA-256 digests filled in. Run with:
//!
//! ```sh
//! cargo test --test compile -- --ignored
//! ```

use shader_slang::GlobalSession;

const VALID_COMPUTE_SHADER: &str = r#"
[shader("compute")]
[numthreads(1, 1, 1)]
void main(uint3 tid : SV_DispatchThreadID)
{
}
"#;

const SYNTAX_ERROR_SHADER: &str = r#"
[shader("compute")]
[numthreads(1, 1, 1)]
void main(uint3 tid : SV_DispatchThreadID)
{
    int x = ;
}
"#;

#[test]
#[ignore = "requires a real Slang library; see module docs"]
fn compiles_valid_compute_shader_to_spirv() {
    let global = GlobalSession::new().expect("create global session");
    let session = global.create_session(None).expect("create session");
    let module = session
        .load_module_from_source("test_module", "test_module.slang", VALID_COMPUTE_SHADER)
        .expect("load module");
    let entry_point = module
        .find_entry_point_by_name("main")
        .expect("find entry point");
    let program = entry_point.link().expect("link program");
    let code = program.get_target_code(0).expect("get target code");

    assert!(!code.is_empty(), "expected non-empty SPIRV bytecode");
    // SPIR-V binaries start with the magic number 0x07230203 (little-endian
    // on all platforms Slang currently targets).
    assert_eq!(&code[0..4], &[0x03, 0x02, 0x23, 0x07]);
}

#[test]
#[ignore = "requires a real Slang library; see module docs"]
fn reports_diagnostics_for_syntax_error() {
    let global = GlobalSession::new().expect("create global session");
    let session = global.create_session(None).expect("create session");

    let result = session.load_module_from_source("bad_module", "bad_module.slang", SYNTAX_ERROR_SHADER);

    let err = match result {
        Ok(_) => panic!("expected a compilation error for invalid syntax"),
        Err(err) => err,
    };

    let shader_slang::Error::Compilation(diagnostics) = err else {
        panic!("expected Error::Compilation, got {err:?}");
    };
    assert!(
        !diagnostics.is_empty(),
        "expected non-empty diagnostics text for a syntax error"
    );
}
