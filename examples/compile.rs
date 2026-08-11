//! Minimal end-to-end compilation example: global session -> session with a
//! SPIR-V target -> load `shaders/test.slang` -> find the `main` entry point
//! -> composite -> link -> print the size of the generated code.
//!
//! Run with: `cargo run --example compile`

use shader_slang_rs as slang;

fn main() {
	let global_session = slang::GlobalSession::new().unwrap();

	// Locate the workspace's shaders/ directory relative to this crate, so the
	// example works from any working directory.
	let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
		.unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string());
	let search_path = format!("{manifest_dir}/shaders");

	let target_desc = slang::TargetDesc::default()
		.format(slang::CompileTarget::Spirv)
		.profile(global_session.find_profile("glsl_450"));
	let targets = [target_desc];

	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.search_paths(&[&search_path])
		.unwrap();

	let session = global_session.create_session(&session_desc).unwrap();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();

	let spirv = linked_program.entry_point_code(0, 0).unwrap();
	println!("generated {} bytes of SPIR-V", spirv.as_slice().len());
}
