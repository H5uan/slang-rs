//! Reflection walkthrough: compiles `shaders/test.slang` and prints every
//! entry point and every global shader parameter with its name, binding, and
//! type kind, as reported by Slang's reflection API.
//!
//! Run with: `cargo run --example reflect`

use shader_slang_rs as slang;

fn main() {
	let global_session = slang::GlobalSession::new().unwrap();

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

	// Entry point to the reflection API.
	let reflection = linked_program.layout(0).unwrap();

	for entry_point in reflection.entry_points() {
		println!(
			"entry point: {} (stage: {:?})",
			entry_point.name().unwrap_or("<unnamed>"),
			entry_point.stage(),
		);
		for parameter in entry_point.parameters() {
			print_parameter(parameter);
		}
	}

	println!("global parameters:");
	for parameter in reflection.parameters() {
		print_parameter(parameter);
	}
}

fn print_parameter(parameter: &slang::reflection::VariableLayout) {
	let name = parameter.name().unwrap_or("<unnamed>");
	let binding = parameter.binding_index();
	let space = parameter.binding_space();
	let kind = parameter.type_layout().map(|layout| layout.kind());
	println!("  {name}: binding {binding}, space {space}, type kind {kind:?}");
}
