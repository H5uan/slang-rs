//! Virtual file system example: serves shader source code from memory through
//! a custom [`slang::FileSystem`] implementation, so the module compiles
//! without any on-disk file. Every file request Slang makes is printed.
//!
//! Run with: `cargo run --example virtual_file_system`

use shader_slang_rs as slang;

const SOURCE: &str = "
RWStructuredBuffer<int> output;

[shader(\"compute\")]
[numthreads(1, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    output[id.x] = 42;
}
";

/// An in-memory file system holding a single file, `virtual.slang`.
struct MemoryFileSystem;

impl slang::FileSystem for MemoryFileSystem {
	fn load_file(&self, path: &str) -> Result<Vec<u8>, slang::FileSystemError> {
		println!("slang requested: {path}");
		// Match on the final path component: Slang probes several candidate
		// paths whose exact shape depends on the search path configuration.
		let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
		if name == "virtual.slang" {
			Ok(SOURCE.as_bytes().to_vec())
		} else {
			Err(slang::FileSystemError::NotFound)
		}
	}
}

fn main() {
	let global_session = slang::GlobalSession::new().unwrap();

	let target_desc = slang::TargetDesc::default()
		.format(slang::CompileTarget::Spirv)
		.profile(global_session.find_profile("glsl_450"));
	let targets = [target_desc];

	let file_system = slang::FileSystemObject::new(MemoryFileSystem);

	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.file_system(&file_system);

	let session = global_session.create_session(&session_desc).unwrap();

	// There is no `virtual.slang` on disk; loading it proves the C++ side
	// called back into the Rust `FileSystem` implementation above.
	let module = session.load_module("virtual").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();

	let spirv = linked_program.entry_point_code(0, 0).unwrap();
	println!("generated {} bytes of SPIR-V", spirv.as_slice().len());
}
