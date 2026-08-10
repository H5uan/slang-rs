use crate as slang;

#[test]
fn compile() {
	let global_session = slang::GlobalSession::new().unwrap();

	let search_path = std::ffi::CString::new("shaders").unwrap();

	// All compiler options are available through this builder.
	let session_options = slang::CompilerOptions::default()
		.optimization(slang::OptimizationLevel::High)
		.matrix_layout_row(true);

	let target_desc = slang::TargetDesc::default()
		.format(slang::CompileTarget::Spirv)
		.profile(global_session.find_profile("glsl_450"));

	let targets = [target_desc];
	let search_paths = [search_path.as_ptr()];

	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.search_paths(&search_paths)
		.options(&session_options);

	let session = global_session.create_session(&session_desc).unwrap();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();

	let linked_program = program.link().unwrap();

	// Entry point to the reflection API.
	let reflection = linked_program.layout(0).unwrap();
	assert_eq!(reflection.entry_point_count(), 1);
	assert_eq!(reflection.parameter_count(), 3);

	let shader_bytecode = linked_program.entry_point_code(0, 0).unwrap();
	assert_ne!(shader_bytecode.as_slice().len(), 0);
}

/// A cached global session that is never dropped, to avoid triggering Slang's
/// global state cleanup on macOS (which can cause a flaky SIGBUS during
/// process exit).
///
/// Using `OnceLock` + `&*` ensures the value lives for the entire process
/// lifetime and is never destroyed.
fn global_session() -> &'static slang::GlobalSession {
	static GLOBAL_SESSION: std::sync::OnceLock<slang::GlobalSession> = std::sync::OnceLock::new();
	GLOBAL_SESSION.get_or_init(|| slang::GlobalSession::new().expect("GlobalSession::new failed"))
}

/// Creates a session with a SPIR-V target and the `shaders/` search path,
/// shared by the tests below.
fn create_test_session() -> slang::Session {
	let global_session = global_session();

	let search_path = std::ffi::CString::new("shaders").unwrap();

	let target_desc = slang::TargetDesc::default()
		.format(slang::CompileTarget::Spirv)
		.profile(global_session.find_profile("glsl_450"));

	let targets = [target_desc];
	let search_paths = [search_path.as_ptr()];

	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.search_paths(&search_paths);

	global_session.create_session(&session_desc).unwrap()
}

#[test]
fn load_module_error() {
	let session = create_test_session();

	match session.load_module("definitely_not_a_module_xyz") {
		Err(slang::Error::Blob(blob)) => {
			let message = blob.as_str().unwrap();
			assert!(
				message.contains("definitely_not_a_module_xyz"),
				"diagnostics should name the missing module, got: {message}"
			);
		}
		Err(slang::Error::Code(_)) => {
			// Acceptable per the null-diagnostics fallback, but Slang normally
			// provides a blob for a missing module.
		}
		Ok(_) => panic!("loading a nonexistent module should fail"),
	}
}

#[test]
fn load_module_from_source_string_syntax_error() {
	let session = create_test_session();

	let result = session.load_module_from_source_string(
		"syntax_error",
		"syntax_error.slang",
		"this is not valid slang source !!!",
	);

	match result {
		Err(slang::Error::Blob(blob)) => {
			assert!(
				blob.as_str().unwrap().contains("error"),
				"diagnostics should mention the error"
			);
		}
		Err(slang::Error::Code(_)) => {}
		Ok(_) => panic!("compiling invalid source should fail"),
	}
}

#[test]
fn find_entry_point_not_found() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();

	assert!(module.find_entry_point_by_name("main").is_some());
	assert!(
		module
			.find_entry_point_by_name("no_such_entry_point")
			.is_none()
	);
}

#[test]
fn load_module_from_source_string_success() {
	let session = create_test_session();

	let source = r#"
[shader("compute")]
[numthreads(4, 1, 1)]
void main(uint3 thread_id : SV_DispatchThreadID) {}
"#;

	let module = session
		.load_module_from_source_string("inline_test", "inline_test.slang", source)
		.unwrap();
	assert_eq!(module.name(), Some("inline_test"));

	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();

	let reflection = linked_program.layout(0).unwrap();
	let reflected_entry_point = reflection.entry_point_by_index(0).unwrap();
	assert_eq!(reflected_entry_point.compute_thread_group_size(), [4, 1, 1]);

	let code = linked_program.entry_point_code(0, 0).unwrap();
	assert!(!code.as_slice().is_empty());
}

#[test]
fn reflection_details() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let reflection = linked_program.layout(0).unwrap();

	// Parameter count and names.
	assert_eq!(reflection.parameter_count(), 3);
	let input_0 = reflection.parameter_by_index(0).unwrap();
	let input_1 = reflection.parameter_by_index(1).unwrap();
	let output = reflection.parameter_by_index(2).unwrap();
	assert_eq!(input_0.name(), Some("input_0"));
	assert_eq!(input_1.name(), Some("input_1"));
	assert_eq!(output.name(), Some("output"));

	// Binding locations.
	assert_eq!(input_0.binding_index(), 0);
	assert_eq!(input_1.binding_index(), 1);
	assert_eq!(output.binding_index(), 2);
	assert_eq!(input_0.binding_space(), 0);
	assert_eq!(output.binding_space(), 0);

	// Type layout of a StructuredBuffer parameter.
	let input_0_layout = input_0.type_layout().unwrap();
	assert_eq!(input_0_layout.kind(), slang::TypeKind::Resource);
	assert_eq!(
		input_0_layout.resource_shape(),
		Some(slang::ResourceShape::SlangStructuredBuffer)
	);
	assert_eq!(
		input_0_layout.resource_access(),
		Some(slang::ResourceAccess::Read)
	);
	assert_eq!(
		output.type_layout().unwrap().resource_access(),
		Some(slang::ResourceAccess::ReadWrite)
	);

	// Binding range of the StructuredBuffer.
	assert_eq!(input_0_layout.binding_range_count(), 1);
	assert_eq!(input_0_layout.binding_range_binding_count(0), 1);

	// Element type: a single 4-byte float.
	let element_layout = input_0_layout.element_type_layout().unwrap();
	assert_eq!(element_layout.kind(), slang::TypeKind::Scalar);
	assert_eq!(
		element_layout.scalar_type(),
		Some(slang::ScalarType::Float32)
	);
	assert_eq!(element_layout.size(slang::ParameterCategory::Uniform), 4);

	// Entry point: name, stage and compute thread group size.
	assert_eq!(reflection.entry_point_count(), 1);
	let reflected_entry_point = reflection.entry_point_by_index(0).unwrap();
	assert_eq!(reflected_entry_point.name(), Some("main"));
	assert_eq!(reflected_entry_point.stage(), slang::Stage::Compute);
	assert_eq!(reflected_entry_point.compute_thread_group_size(), [1, 1, 1]);
}

#[test]
fn shader_to_json() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let reflection = linked_program.layout(0).unwrap();

	let json = reflection.to_json().unwrap();
	assert!(
		json.contains("input_0"),
		"JSON reflection should name the input_0 parameter, got: {json}"
	);
	assert!(
		json.contains("output"),
		"JSON reflection should name the output parameter, got: {json}"
	);
}

#[test]
fn shader_bindless_space_index() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let reflection = linked_program.layout(0).unwrap();

	// slang.h documents that the index is a layout-time reservation and may
	// stay non-negative even when the final code uses no bindless heap; -1
	// would mean no space was reserved at all. Slang v2026.14.1 reserves
	// space 1 for this program.
	assert_eq!(reflection.bindless_space_index(), 1);
}

#[test]
fn variable_default_values() {
	let session = create_test_session();

	let source = r#"
float foo(float z, float x = 1.5, int y = 42) { return x + y + z; }
"#;

	let module = session
		.load_module_from_source_string("defaults", "defaults.slang", source)
		.unwrap();
	let program = slang::ComponentType::from(module);
	let reflection = program.layout(0).unwrap();

	let foo = reflection.find_function_by_name("foo").unwrap();
	let z = foo.parameter_by_index(0).unwrap();
	let x = foo.parameter_by_index(1).unwrap();
	let y = foo.parameter_by_index(2).unwrap();

	assert!(x.has_default_value());
	assert_eq!(x.default_value_float(), Some(1.5));
	assert!(y.has_default_value());
	assert_eq!(y.default_value_int(), Some(42));
	assert!(!z.has_default_value());
	assert_eq!(z.default_value_float(), None);
}

#[test]
fn decl_find_modifier() {
	let session = create_test_session();

	let source = r#"
static int g_counter = 0;
int bar() { return g_counter; }
"#;

	let module = session
		.load_module_from_source_string("modifiers", "modifiers.slang", source)
		.unwrap();
	let module_decl = module.module_reflection();

	let var_decl = module_decl
		.children()
		.find(|d| d.name() == Some("g_counter"))
		.unwrap();
	assert!(
		var_decl
			.find_modifier(slang::ModifierID::SlangModifierStatic)
			.is_some()
	);
	assert!(
		var_decl
			.find_modifier(slang::ModifierID::SlangModifierConst)
			.is_none()
	);

	let func_decl = module_decl
		.children()
		.find(|d| d.name() == Some("bar"))
		.unwrap();
	assert!(
		func_decl
			.find_modifier(slang::ModifierID::SlangModifierStatic)
			.is_none()
	);
}

#[test]
fn specialized_element_count() {
	let session = create_test_session();

	let module = session
		.load_module_from_source_string("arrays", "arrays.slang", "int g_data[8];\n")
		.unwrap();
	let program = slang::ComponentType::from(module.clone());
	let reflection = program.layout(0).unwrap();

	let module_decl = module.module_reflection();
	let var_decl = module_decl
		.children()
		.find(|d| d.name() == Some("g_data"))
		.unwrap();
	let ty = var_decl.as_variable().unwrap().ty().unwrap();

	assert_eq!(ty.kind(), slang::TypeKind::Array);
	assert_eq!(ty.element_count(), 8);
	// With no unresolved constants, the specialized count matches the plain
	// element count whether or not a program layout is provided.
	assert_eq!(ty.specialized_element_count(None), 8);
	assert_eq!(ty.specialized_element_count(Some(reflection)), 8);
}

#[test]
#[allow(deprecated)]
fn try_resolve_overloaded_function() {
	let session = create_test_session();

	let source = r#"
int foo(int x) { return x; }
float foo(float x) { return x; }
"#;

	let module = session
		.load_module_from_source_string("overloaded", "overloaded.slang", source)
		.unwrap();
	let program = slang::ComponentType::from(module);
	let reflection = program.layout(0).unwrap();

	let foo = reflection.find_function_by_name("foo").unwrap();
	assert!(foo.is_overloaded());
	assert_eq!(foo.overload_count(), 2);

	let overload_0 = foo.overload_by_index(0).unwrap();
	let overload_1 = foo.overload_by_index(1).unwrap();

	// A single concrete candidate resolves to itself.
	let resolved = reflection
		.try_resolve_overloaded_function(&[overload_0])
		.unwrap();
	assert!(std::ptr::eq(resolved, overload_0));

	// With several candidates slang resolves the overload group to a fresh
	// reflection object for the chosen overload (deprecated API; no call-site
	// context is taken into account).
	let resolved = reflection
		.try_resolve_overloaded_function(&[overload_0, overload_1])
		.unwrap();
	assert_eq!(resolved.name(), Some("foo"));
}

#[test]
fn specialize_generic_entry_point() {
	let session = create_test_session();

	let source = r#"
__generic<T : __BuiltinFloatingPointType>
[shader("compute")]
[numthreads(4, 1, 1)]
void main(RWStructuredBuffer<T> output, uint3 thread_id : SV_DispatchThreadID)
{
	output[thread_id.x] = T(1.5);
}
"#;

	let module = session
		.load_module_from_source_string("generic_ep", "generic_ep.slang", source)
		.unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	// The composite program exposes the entry point's generic parameter as a
	// specialization parameter.
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	assert_eq!(program.specialization_param_count(), 1);

	// Specializing with `float` resolves the buffer element type.
	let specialized = program
		.specialize(&[slang::SpecializationArg::from_expr("float")])
		.unwrap();
	assert_eq!(specialized.specialization_param_count(), 0);

	let linked_program = specialized.link().unwrap();

	let reflection = linked_program.layout(0).unwrap();
	let reflected_entry_point = reflection.entry_point_by_index(0).unwrap();
	let element_layout = reflected_entry_point
		.parameter_by_index(0)
		.unwrap()
		.type_layout()
		.unwrap()
		.element_type_layout()
		.unwrap();
	assert_eq!(
		element_layout.scalar_type(),
		Some(slang::ScalarType::Float32)
	);

	let code = linked_program.entry_point_code(0, 0).unwrap();
	assert!(!code.as_slice().is_empty());
}

#[test]
fn specialize_type() {
	let session = create_test_session();

	// `ISession::specializeType` specializes an existential (interface) type
	// with concrete type arguments; it is not for generic types like
	// `Pair<T>`.
	let source = r#"
interface IValue
{
	int get();
}

struct ConstantA : IValue
{
	int get() { return 1; }
}
"#;

	let module = session
		.load_module_from_source_string("specialize_type", "specialize_type.slang", source)
		.unwrap();
	let module_decl = module.module_reflection();

	let find_type = |name: &str| {
		module_decl
			.children()
			.find(|d| d.name() == Some(name))
			.unwrap()
			.ty()
			.unwrap()
	};
	let interface_type = find_type("IValue");
	let impl_type = find_type("ConstantA");

	// Note that `ISession::specializeType` only accepts `Kind::Type` args.
	let specialized = session
		.specialize_type(
			interface_type,
			&[slang::SpecializationArg::from_type(impl_type)],
		)
		.unwrap();
	assert_eq!(specialized.kind(), slang::TypeKind::Specialized);
}

#[test]
fn type_conformance_component_type() {
	let session = create_test_session();

	let source = r#"
interface IValue
{
	int get();
}

struct ConstantA : IValue
{
	int get() { return 1; }
}

struct ConstantB : IValue
{
	int get() { return 2; }
}

RWStructuredBuffer<int> output;

[shader("compute")]
[numthreads(1, 1, 1)]
void main(uniform IValue value)
{
	output[0] = value.get();
}
"#;

	let module = session
		.load_module_from_source_string("conformance", "conformance.slang", source)
		.unwrap();
	let module_decl = module.module_reflection();

	let find_type = |name: &str| {
		module_decl
			.children()
			.find(|d| d.name() == Some(name))
			.unwrap()
			.ty()
			.unwrap()
	};
	let interface_type = find_type("IValue");
	let impl_type = find_type("ConstantA");

	let conformance = session
		.create_type_conformance_component_type(impl_type, interface_type, -1)
		.unwrap();

	// Including the conformance component in the composite restricts the
	// dynamic dispatch to `ConstantA`; the program still links and compiles.
	let entry_point = module.find_entry_point_by_name("main").unwrap();
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into(), conformance.into()])
		.unwrap();
	let linked_program = program.link().unwrap();

	let code = linked_program.entry_point_code(0, 0).unwrap();
	assert!(!code.as_slice().is_empty());
}

#[test]
fn component_type_link_with_options_and_friends() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();

	// link_with_options smoke test.
	let options = slang::CompilerOptions::default().optimization(slang::OptimizationLevel::High);
	let linked_program = program.link_with_options(&options).unwrap();
	let code = linked_program.entry_point_code(0, 0).unwrap();
	assert!(!code.as_slice().is_empty());

	// The hash blob is a non-empty dependency hash usable as a cache key.
	let hash = linked_program.entry_point_hash(0, 0);
	assert!(!hash.as_slice().is_empty());

	// A component type hands out its owning session, which stays usable.
	let component_session = program.get_session();
	let module_again = component_session.load_module("test.slang").unwrap();
	assert_eq!(module_again.name(), Some("test.slang"));

	// rename_entry_point returns a new component type that links and compiles
	// (the new name affects the generated entry point symbol, not the
	// reflection of the original function).
	let renamed_linked = program
		.rename_entry_point("renamed_main")
		.unwrap()
		.link()
		.unwrap();
	let renamed_code = renamed_linked.entry_point_code(0, 0).unwrap();
	assert!(!renamed_code.as_slice().is_empty());
}

#[test]
fn query_interface_and_cast_as() {
	use slang::Interface;

	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	// Sub-interfaces query their base interface successfully; casts to
	// unrelated interfaces return `None`.
	assert!(
		module
			.as_unknown()
			.query_interface::<slang::ComponentType>()
			.is_some()
	);
	assert!(
		entry_point
			.as_unknown()
			.query_interface::<slang::ComponentType>()
			.is_some()
	);
	assert!(
		entry_point
			.as_unknown()
			.query_interface::<slang::Module>()
			.is_none()
	);
	assert!(
		module
			.as_unknown()
			.query_interface::<slang::GlobalSession>()
			.is_none()
	);

	// `castAs` on `IMetadata`: a self-cast succeeds, unrelated interfaces fail.
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let metadata = linked_program.target_metadata(0).unwrap();
	assert!(metadata.cast_as::<slang::Metadata>().is_some());
	assert!(metadata.cast_as::<slang::Blob>().is_none());
}

#[test]
fn ir_blob_round_trip() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();

	// Serialize to an IR blob and read its info back.
	let ir_blob = module.serialize().unwrap();
	let info = session.module_info_from_ir_blob(&ir_blob).unwrap();
	assert_eq!(info.name, Some("test.slang"));

	// A freshly serialized module is up-to-date against its own source.
	assert!(session.is_binary_module_up_to_date("shaders/test.slang", &ir_blob));

	// Load the module back from the blob and compile it end to end.
	let loaded = session
		.load_module_from_ir_blob("test_ir", "test_ir.slang", &ir_blob)
		.unwrap();
	let entry_point = loaded.find_entry_point_by_name("main").unwrap();
	let program = session
		.create_composite_component_type(&[loaded.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let code = linked_program.entry_point_code(0, 0).unwrap();
	assert!(!code.as_slice().is_empty());
}

#[test]
fn core_module_round_trip() {
	// Save the core module from a fully initialized global session.
	let global_session = slang::GlobalSession::new().unwrap();
	let core_blob = global_session
		.save_core_module(slang::ArchiveType::RiffLz4)
		.unwrap();
	assert!(!core_blob.as_slice().is_empty());

	// A global session created without the core module becomes usable after
	// loading the serialized core module.
	let bare_global_session = slang::GlobalSession::new_without_core_module().unwrap();
	bare_global_session
		.load_core_module(core_blob.as_slice())
		.unwrap();

	let target_desc = slang::TargetDesc::default().format(slang::CompileTarget::Spirv);
	let targets = [target_desc];
	let session_desc = slang::SessionDesc::default().targets(&targets);
	let session = bare_global_session.create_session(&session_desc).unwrap();
	let module = session
		.load_module_from_source_string("core_test", "core_test.slang", "int foo() { return 1; }\n")
		.unwrap();
	assert_eq!(module.name(), Some("core_test"));

	// `compile_core_module` is the alternative way to fill in a bare global
	// session, compiling the core module from embedded source.
	let bare_global_session = slang::GlobalSession::new_without_core_module().unwrap();
	bare_global_session.compile_core_module(0).unwrap();
	let session = bare_global_session.create_session(&session_desc).unwrap();
	let module = session
		.load_module_from_source_string(
			"core_test_2",
			"core_test_2.slang",
			"int bar() { return 2; }\n",
		)
		.unwrap();
	assert_eq!(module.name(), Some("core_test_2"));
}

#[test]
fn session_desc_preprocessor_macros() {
	let global_session = slang::GlobalSession::new().unwrap();

	let target_desc = slang::TargetDesc::default().format(slang::CompileTarget::Spirv);
	let targets = [target_desc];
	let macros = [slang::PreprocessorMacroDesc::new("M3B_WITH_OUTPUT", "1")];
	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.preprocessor_macros(&macros);
	let session = global_session.create_session(&session_desc).unwrap();

	let source = r#"
#ifdef M3B_WITH_OUTPUT
RWStructuredBuffer<int> output;
#endif

[shader("compute")]
[numthreads(1, 1, 1)]
void main() {}
"#;

	let module = session
		.load_module_from_source_string("macros", "macros.slang", source)
		.unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let reflection = linked_program.layout(0).unwrap();
	assert_eq!(reflection.parameter_count(), 1);
}

#[test]
fn session_desc_allow_glsl_syntax() {
	let global_session = slang::GlobalSession::new().unwrap();

	let target_desc = slang::TargetDesc::default().format(slang::CompileTarget::Spirv);
	let targets = [target_desc];
	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.allow_glsl_syntax(true);
	let session = global_session.create_session(&session_desc).unwrap();

	// Smoke: a session with GLSL syntax enabled still compiles plain Slang.
	let module = session
		.load_module_from_source_string(
			"glsl_smoke",
			"glsl_smoke.slang",
			"int foo() { return 1; }\n",
		)
		.unwrap();
	assert_eq!(module.name(), Some("glsl_smoke"));
}

#[test]
fn check_compile_target_and_pass_through_support() {
	let global_session = slang::GlobalSession::new().unwrap();

	// SPIR-V codegen is compiled into the library itself; DXIL depends on an
	// external DXC, so only SPIR-V is asserted here.
	assert!(
		global_session
			.check_compile_target_support(slang::CompileTarget::Spirv)
			.is_ok()
	);

	// The prebuilt Slang binaries ship slang-glslang.
	assert!(
		global_session
			.check_pass_through_support(slang::PassThrough::Glslang)
			.is_ok()
	);
}

#[test]
fn parse_command_line_arguments() {
	let global_session = slang::GlobalSession::new().unwrap();

	let parsed = global_session
		.parse_command_line_arguments(&["-target", "spirv"])
		.unwrap();
	assert_eq!(parsed.targetCount, 1);

	// The parsed desc drives session creation directly.
	let session = global_session.create_session(&parsed).unwrap();
	let module = session
		.load_module_from_source_string("parsed", "parsed.slang", "int foo() { return 1; }\n")
		.unwrap();
	assert_eq!(module.name(), Some("parsed"));
}

#[test]
fn loaded_modules() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();

	assert!(session.loaded_module_count() >= 1);
	let found = session
		.loaded_modules()
		.find(|m| m.name() == Some("test.slang"))
		.expect("the loaded module list should contain test.slang");
	assert_eq!(found.unique_identity(), module.unique_identity());

	// The session hands its global session back.
	let _global_session = session.get_global_session();
}

#[test]
fn module_serialize_write_disassemble_and_find_and_check() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();

	// write_to_file produces a non-empty binary module file.
	let path = std::env::temp_dir().join("shader_slang_rs_m3b_test.slang-module");
	let path_str = path.to_str().unwrap();
	module.write_to_file(path_str).unwrap();
	let bytes = std::fs::read(&path).unwrap();
	assert!(!bytes.is_empty());
	std::fs::remove_file(&path).unwrap();

	// disassemble produces human-readable IR text.
	let disassembly = module.disassemble().unwrap();
	assert!(
		disassembly.contains("main"),
		"disassembly should mention the main entry point, got: {disassembly}"
	);

	// find_and_check_entry_point validates functions that carry no entry point
	// attributes at all. (Note: `[numthreads]` alone already makes
	// `find_entry_point_by_name` succeed in Slang v2026.14.1 — it implicitly
	// designates a compute entry point.)
	let plain = session
		.load_module_from_source_string("plain_ep", "plain_ep.slang", "void cs_main() {}\n")
		.unwrap();
	assert!(plain.find_entry_point_by_name("cs_main").is_none());
	let entry_point = plain
		.find_and_check_entry_point("cs_main", slang::Stage::Compute)
		.unwrap();
	assert_eq!(entry_point.function_reflection().name(), Some("cs_main"));
}

#[test]
fn session_type_utilities() {
	let session = create_test_session();

	let source = r#"
interface IValue
{
	int get();
}

struct ConstantA : IValue
{
	int get() { return 1; }
}

struct Plain
{
	float x;
	int y;
}
"#;

	let module = session
		.load_module_from_source_string("type_utils", "type_utils.slang", source)
		.unwrap();
	let module_decl = module.module_reflection();

	let find_type = |name: &str| {
		module_decl
			.children()
			.find(|d| d.name() == Some(name))
			.unwrap()
			.ty()
			.unwrap()
	};
	let interface_type = find_type("IValue");
	let impl_type = find_type("ConstantA");
	let plain_type = find_type("Plain");

	// Standalone type layout.
	let layout = session
		.type_layout(plain_type, 0, slang::LayoutRules::Default)
		.unwrap();
	assert_eq!(layout.kind(), slang::TypeKind::Struct);
	assert_eq!(layout.field_count(), 2);

	// Container type: Plain -> StructuredBuffer<Plain>.
	let buffer_type = session
		.container_type(plain_type, slang::ContainerType::StructuredBuffer)
		.unwrap();
	assert_eq!(buffer_type.kind(), slang::TypeKind::Resource);

	// The __Dynamic type is available for specialization arguments.
	let _dynamic = session.dynamic_type();

	// RTTI / witness helpers.
	let rtti_name = session.type_rtti_mangled_name(impl_type).unwrap();
	assert!(!rtti_name.as_slice().is_empty());
	let witness_name = session
		.type_conformance_witness_mangled_name(impl_type, interface_type)
		.unwrap();
	assert!(!witness_name.as_slice().is_empty());
	let _witness_id = session
		.type_conformance_witness_sequential_id(impl_type, interface_type)
		.unwrap();
	let rtti_bytes = session
		.dynamic_object_rtti_bytes(impl_type, interface_type, 32)
		.unwrap();
	assert!(rtti_bytes.len() >= 4);

	// Source locations of declarations.
	let plain_decl = module_decl
		.children()
		.find(|d| d.name() == Some("Plain"))
		.unwrap();
	let location = session.decl_source_location(plain_decl).unwrap();
	assert!(
		location.file_path().unwrap().contains("type_utils.slang"),
		"unexpected source file: {:?}",
		location.file_path()
	);
	assert!(location.line() > 0);
}

#[test]
fn compile_results() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();

	// The linked component type implements IComponentType2 in Slang v2026.14.1.
	let component2 = linked_program
		.as_component_type2()
		.expect("linked component type should implement IComponentType2");

	let target_result = component2.target_compile_result(0).unwrap();
	assert!(target_result.item_count() > 0);
	// Item 0 is the base SPIR-V. Slang v2026.14.1 reports an additional slot
	// for the debug SPIR-V, which errors with `SLANG_E_NOT_FOUND` when no
	// separate debug compilation was requested.
	assert_eq!(target_result.item_count(), 2);
	assert!(!target_result.item_data(0).unwrap().as_slice().is_empty());
	assert!(target_result.item_data(1).is_err());
	let _metadata = target_result.metadata().unwrap();

	let entry_point_result = component2.entry_point_compile_result(0, 0).unwrap();
	assert!(entry_point_result.item_count() > 0);
	assert!(
		!entry_point_result
			.item_data(0)
			.unwrap()
			.as_slice()
			.is_empty()
	);
	let _metadata = entry_point_result.metadata().unwrap();

	// Out-of-range indices are reported as errors, not panics.
	assert!(target_result.item_data(target_result.item_count()).is_err());
}

#[test]
fn bindless_resource_metadata() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let metadata = linked_program.target_metadata(0).unwrap();

	// `IBindlessResourceMetadata` is always available on target metadata in
	// Slang v2026.14.1; test.slang uses no descriptor handles, so the
	// post-lowering usage signal is false.
	let bindless = metadata
		.cast_as::<slang::BindlessResourceMetadata>()
		.expect("target metadata should cast to IBindlessResourceMetadata");
	assert!(!bindless.uses_bindless_resource_heap());
}

#[test]
fn coverage_synthetic_and_cooperative_metadata() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let metadata = linked_program.target_metadata(0).unwrap();

	// In Slang v2026.14.1 these metadata interfaces are always implemented on
	// target metadata; a plain shader compiled without coverage
	// instrumentation simply reports zero counters/entries/resources.
	let coverage = metadata
		.cast_as::<slang::CoverageTracingMetadata>()
		.expect("target metadata should cast to ICoverageTracingMetadata");
	assert_eq!(coverage.counter_count(), 0);
	assert_eq!(coverage.entry_count(), 0);

	let synthetic = metadata
		.cast_as::<slang::SyntheticResourceMetadata>()
		.expect("target metadata should cast to ISyntheticResourceMetadata");
	assert_eq!(synthetic.resource_count(), 0);

	// `ICooperativeTypesMetadata` likewise reports empty lists when no
	// cooperative types survive lowering.
	let cooperative = metadata
		.cast_as::<slang::CooperativeTypesMetadata>()
		.expect("target metadata should cast to ICooperativeTypesMetadata");
	assert_eq!(cooperative.cooperative_matrix_type_count(), 0);
	assert_eq!(cooperative.cooperative_matrix_combination_count(), 0);
	assert_eq!(cooperative.cooperative_vector_type_count(), 0);
	assert_eq!(cooperative.cooperative_vector_combination_count(), 0);
}

#[test]
fn debug_build_identifier() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let metadata = linked_program.target_metadata(0).unwrap();

	// Smoke: without separate debug compilation Slang v2026.14.1 hands back an
	// empty identifier string; either way the call must succeed.
	let _ = metadata.get_debug_build_identifier();
}

// --- FileSystem (reverse COM) tests ---

/// In-memory [`slang::FileSystem`] for the tests below. Paths are matched by
/// their final component so the tests do not depend on how Slang
/// canonicalizes candidate paths.
struct VirtualFileSystem {
	files: std::collections::HashMap<String, Vec<u8>>,
	requested: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
	on_drop: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
}

impl VirtualFileSystem {
	fn new(files: &[(&str, &str)]) -> Self {
		Self {
			files: files
				.iter()
				.map(|(path, source)| (path.to_string(), source.as_bytes().to_vec()))
				.collect(),
			requested: Default::default(),
			on_drop: None,
		}
	}
}

impl slang::FileSystem for VirtualFileSystem {
	fn load_file(&self, path: &str) -> Result<Vec<u8>, slang::FileSystemError> {
		self.requested.lock().unwrap().push(path.to_string());
		let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
		self.files
			.get(name)
			.cloned()
			.ok_or(slang::FileSystemError::NotFound)
	}
}

impl Drop for VirtualFileSystem {
	fn drop(&mut self) {
		if let Some(counter) = &self.on_drop {
			counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
		}
	}
}

const VIRTUAL_SHADER: &str = "
RWStructuredBuffer<int> output;

[shader(\"compute\")]
[numthreads(1, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    output[id.x] = 42;
}
";

/// Creates a SPIR-V session that loads sources through `file_system`.
fn create_fs_session(file_system: &slang::FileSystemObject) -> slang::Session {
	let global_session = slang::GlobalSession::new().unwrap();

	let target_desc = slang::TargetDesc::default()
		.format(slang::CompileTarget::Spirv)
		.profile(global_session.find_profile("glsl_450"));

	let targets = [target_desc];

	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.file_system(file_system);

	global_session.create_session(&session_desc).unwrap()
}

/// The C++ -> Rust callback chain: a module that only exists in the virtual
/// file system must compile end to end.
#[test]
fn file_system_virtual_module() {
	let fs = VirtualFileSystem::new(&[("virtual_test.slang", VIRTUAL_SHADER)]);
	let requested = fs.requested.clone();
	let fs = slang::FileSystemObject::new(fs);
	let session = create_fs_session(&fs);

	let module = session.load_module("virtual_test").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let code = linked_program.entry_point_code(0, 0).unwrap();
	assert!(!code.as_slice().is_empty());

	// The module has no on-disk counterpart, so a successful load proves the
	// C++ side really called back into the Rust implementation.
	let requested = requested.lock().unwrap();
	assert!(
		requested
			.iter()
			.any(|path| path.ends_with("virtual_test.slang")),
		"Slang should have requested virtual_test.slang, got: {requested:?}"
	);
}

/// `import` resolution goes through the file system as well.
#[test]
fn file_system_import() {
	let fs = VirtualFileSystem::new(&[
		(
			"main.slang",
			"
import child;

RWStructuredBuffer<int> output;

[shader(\"compute\")]
[numthreads(1, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    output[id.x] = getValue();
}
",
		),
		(
			"child.slang",
			"
public int getValue()
{
    return 42;
}
",
		),
	]);
	let requested = fs.requested.clone();
	let fs = slang::FileSystemObject::new(fs);
	let session = create_fs_session(&fs);

	let module = session.load_module("main").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let code = linked_program.entry_point_code(0, 0).unwrap();
	assert!(!code.as_slice().is_empty());

	let requested = requested.lock().unwrap();
	assert!(
		requested.iter().any(|path| path.ends_with("child.slang")),
		"import resolution should have requested child.slang, got: {requested:?}"
	);
}

/// `FileSystemError::NotFound` propagates as a module load failure.
#[test]
fn file_system_not_found() {
	let fs = slang::FileSystemObject::new(VirtualFileSystem::new(&[]));
	let session = create_fs_session(&fs);

	assert!(
		session.load_module("missing_module").is_err(),
		"a module the file system cannot provide must fail to load"
	);
}

struct PanicFileSystem;

impl slang::FileSystem for PanicFileSystem {
	fn load_file(&self, _path: &str) -> Result<Vec<u8>, slang::FileSystemError> {
		panic!("deliberate panic in load_file");
	}
}

/// A panic in the callback must not unwind into C++; the load fails instead.
#[test]
fn file_system_panic_is_contained() {
	let fs = slang::FileSystemObject::new(PanicFileSystem);
	let session = create_fs_session(&fs);

	assert!(
		session.load_module("anything").is_err(),
		"a panicking file system must surface as a load error, not an abort"
	);
}

/// Slang `addRef`s the file system at `createSession` time, so the session
/// stays usable after the wrapper is dropped; the Rust object is reclaimed
/// exactly when the session's last reference goes away.
#[test]
fn file_system_outlives_wrapper() {
	use std::sync::Arc;
	use std::sync::atomic::{AtomicUsize, Ordering};

	let drops = Arc::new(AtomicUsize::new(0));
	let mut fs = VirtualFileSystem::new(&[("virtual_test.slang", VIRTUAL_SHADER)]);
	fs.on_drop = Some(drops.clone());
	let fs = slang::FileSystemObject::new(fs);

	let session = create_fs_session(&fs);

	// The session holds its own COM reference; dropping our handle must not
	// free the object.
	drop(fs);
	assert_eq!(drops.load(Ordering::SeqCst), 0);

	let module = session.load_module("virtual_test").unwrap();
	drop(module);
	drop(session);
	assert_eq!(
		drops.load(Ordering::SeqCst),
		1,
		"the Rust object should be reclaimed on the final release"
	);
}

/// The `queryInterface` thunk answers the interface's own IID from Rust.
#[test]
fn file_system_query_interface() {
	use slang::Interface;

	let fs = slang::FileSystemObject::new(VirtualFileSystem::new(&[]));
	assert!(
		fs.as_unknown()
			.query_interface::<slang::FileSystemObject>()
			.is_some()
	);
}

// --- FileSystemExt / WritableFileSystem (reverse COM) tests ---

/// A [`slang::FileSystemExt`] implementation over an in-memory file set,
/// counting Ext callbacks so the tests can prove Slang used the
/// implementation's path management directly instead of wrapping the object
/// in its `CacheFileSystem` emulation.
struct VirtualFileSystemExt {
	files: std::collections::HashMap<String, Vec<u8>>,
	ext_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl VirtualFileSystemExt {
	fn new(files: &[(&str, &str)]) -> Self {
		Self {
			files: files
				.iter()
				.map(|(path, source)| (path.to_string(), source.as_bytes().to_vec()))
				.collect(),
			ext_calls: Default::default(),
		}
	}

	/// The bare file name of `path` (the virtual file set is flat).
	fn file_name(path: &str) -> &str {
		path.rsplit(['/', '\\']).next().unwrap_or(path)
	}

	/// Naive path combining, sufficient for the flat virtual file set:
	/// resolves `path` relative to the directory of `from_path`.
	fn combine(from_path_type: slang::PathType, from_path: &str, path: &str) -> String {
		let directory = match from_path_type {
			slang::PathType::Directory => from_path,
			slang::PathType::File => from_path
				.rfind(['/', '\\'])
				.map(|index| &from_path[..index])
				.unwrap_or(""),
		};
		if directory.is_empty() {
			path.to_string()
		} else {
			format!("{directory}/{path}")
		}
	}
}

impl slang::FileSystem for VirtualFileSystemExt {
	fn load_file(&self, path: &str) -> Result<Vec<u8>, slang::FileSystemError> {
		self.files
			.get(Self::file_name(path))
			.cloned()
			.ok_or(slang::FileSystemError::NotFound)
	}
}

impl slang::FileSystemExt for VirtualFileSystemExt {
	fn file_unique_identity(&self, path: &str) -> Result<String, slang::FileSystemError> {
		self.ext_calls
			.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
		Ok(path.to_string())
	}

	fn calc_combined_path(
		&self,
		from_path_type: slang::PathType,
		from_path: &str,
		path: &str,
	) -> Result<String, slang::FileSystemError> {
		self.ext_calls
			.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
		Ok(Self::combine(from_path_type, from_path, path))
	}

	fn path_type(&self, path: &str) -> Result<slang::PathType, slang::FileSystemError> {
		self.ext_calls
			.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
		let name = Self::file_name(path);
		if self.files.contains_key(name) {
			Ok(slang::PathType::File)
		} else if path.is_empty() || !name.contains('.') {
			// Treat anything that does not look like a file (search paths,
			// ".", ...) as an existing directory.
			Ok(slang::PathType::Directory)
		} else {
			Err(slang::FileSystemError::NotFound)
		}
	}

	fn get_path(
		&self,
		_kind: slang::PathKind,
		path: &str,
	) -> Result<String, slang::FileSystemError> {
		// The identity transformation is a valid simplified path.
		Ok(path.to_string())
	}
}

/// A module that only exists in the virtual file system must compile end to
/// end with the Ext path management, and Slang must have called it directly:
/// with the `CacheFileSystem` wrapper (used for plain [`slang::FileSystem`]
/// objects) none of the Ext callbacks would fire.
#[test]
fn file_system_ext_path_management() {
	use std::sync::atomic::Ordering;

	let fs = VirtualFileSystemExt::new(&[("ext_test.slang", VIRTUAL_SHADER)]);
	let ext_calls = fs.ext_calls.clone();
	let fs = slang::FileSystemObject::new_ext(fs);
	let session = create_fs_session(&fs);

	let module = session.load_module("ext_test").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let code = linked_program.entry_point_code(0, 0).unwrap();
	assert!(!code.as_slice().is_empty());

	assert!(
		ext_calls.load(Ordering::SeqCst) > 0,
		"Slang should have used the Ext path management directly (no CacheFileSystem wrapper)"
	);
}

/// An in-memory [`slang::WritableFileSystem`] with file and directory
/// tracking, used to drive the reverse thunks through the forward
/// [`slang::MutableFileSystem`] wrapper.
#[derive(Default)]
struct VirtualWritableFileSystem {
	files: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
	directories: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl slang::FileSystem for VirtualWritableFileSystem {
	fn load_file(&self, path: &str) -> Result<Vec<u8>, slang::FileSystemError> {
		self.files
			.lock()
			.unwrap()
			.get(path)
			.cloned()
			.ok_or(slang::FileSystemError::NotFound)
	}
}

impl slang::FileSystemExt for VirtualWritableFileSystem {
	fn file_unique_identity(&self, path: &str) -> Result<String, slang::FileSystemError> {
		Ok(path.to_string())
	}

	fn calc_combined_path(
		&self,
		from_path_type: slang::PathType,
		from_path: &str,
		path: &str,
	) -> Result<String, slang::FileSystemError> {
		Ok(VirtualFileSystemExt::combine(
			from_path_type,
			from_path,
			path,
		))
	}

	fn path_type(&self, path: &str) -> Result<slang::PathType, slang::FileSystemError> {
		if self.files.lock().unwrap().contains_key(path) {
			Ok(slang::PathType::File)
		} else if self.directories.lock().unwrap().contains(path) {
			Ok(slang::PathType::Directory)
		} else {
			Err(slang::FileSystemError::NotFound)
		}
	}
}

impl slang::WritableFileSystem for VirtualWritableFileSystem {
	fn save_file(&self, path: &str, data: &[u8]) -> Result<(), slang::FileSystemError> {
		self.files
			.lock()
			.unwrap()
			.insert(path.to_string(), data.to_vec());
		Ok(())
	}

	fn remove(&self, path: &str) -> Result<(), slang::FileSystemError> {
		let removed = self.files.lock().unwrap().remove(path).is_some()
			|| self.directories.lock().unwrap().remove(path);
		if removed {
			Ok(())
		} else {
			Err(slang::FileSystemError::NotFound)
		}
	}

	fn create_directory(&self, path: &str) -> Result<(), slang::FileSystemError> {
		self.directories.lock().unwrap().insert(path.to_string());
		Ok(())
	}
}

/// `queryInterface` answers exactly the interface level the object was
/// created with.
#[test]
fn file_system_interface_levels() {
	use slang::Interface;

	let base = slang::FileSystemObject::new(VirtualFileSystem::new(&[]));
	let ext = slang::FileSystemObject::new_ext(VirtualFileSystemExt::new(&[]));
	let writable = slang::FileSystemObject::new_writable(VirtualWritableFileSystem::default());

	// Every level answers the base `ISlangFileSystem` IID.
	assert!(
		ext.as_unknown()
			.query_interface::<slang::FileSystemObject>()
			.is_some()
	);
	assert!(
		writable
			.as_unknown()
			.query_interface::<slang::FileSystemObject>()
			.is_some()
	);

	// Only a writable object answers `ISlangMutableFileSystem`.
	assert!(
		base.as_unknown()
			.query_interface::<slang::MutableFileSystem>()
			.is_none()
	);
	assert!(
		ext.as_unknown()
			.query_interface::<slang::MutableFileSystem>()
			.is_none()
	);
	assert!(
		writable
			.as_unknown()
			.query_interface::<slang::MutableFileSystem>()
			.is_some()
	);
}

/// Drives a Rust `WritableFileSystem` through the forward
/// [`slang::MutableFileSystem`] wrapper: every call crosses forward wrapper
/// -> COM vtable -> reverse thunk -> Rust implementation.
#[test]
fn file_system_writable_forward_roundtrip() {
	use slang::Interface;

	let fs = slang::FileSystemObject::new_writable(VirtualWritableFileSystem::default());
	let mutable = fs
		.as_unknown()
		.query_interface::<slang::MutableFileSystem>()
		.unwrap();

	mutable.create_directory("shaders").unwrap();
	assert_eq!(
		mutable.path_type("shaders").unwrap(),
		slang::PathType::Directory
	);

	mutable.save_file("shaders/a.bin", b"hello").unwrap();
	assert_eq!(
		mutable.path_type("shaders/a.bin").unwrap(),
		slang::PathType::File
	);
	assert_eq!(
		mutable.load_file("shaders/a.bin").unwrap().as_slice(),
		b"hello"
	);

	// Ext path resolution goes to the Rust implementation (naive join); the
	// returned blob holds the path zero terminated, as slang.h specifies.
	let combined = mutable
		.calc_combined_path(slang::PathType::File, "shaders/main.slang", "child.slang")
		.unwrap();
	assert_eq!(combined.as_slice(), b"shaders/child.slang\0");

	mutable.remove("shaders/a.bin").unwrap();
	assert!(mutable.load_file("shaders/a.bin").is_err());

	assert_eq!(mutable.os_path_kind(), slang::OSPathKind::None);
}

// --- get_result_as_file_system (forward) tests ---

/// The compilation outputs are exposed as files in an in-memory
/// `ISlangMutableFileSystem`: enumerate them, read the SPIR-V binary back,
/// and exercise the write operations of the result file system.
#[test]
fn result_as_file_system() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();

	let fs = linked_program.get_result_as_file_system(0, 0).unwrap();

	// The compiled entry point shows up as a file in the result file system.
	// (MemoryFileSystem's root directory is addressed as ".".)
	let mut entries = Vec::new();
	fs.enumerate_path_contents(".", |path_type, name| {
		entries.push((path_type, name.to_string()));
	})
	.unwrap();
	let spv = entries
		.iter()
		.find(|(path_type, name)| *path_type == slang::PathType::File && name.ends_with(".spv"))
		.map(|(_, name)| name.clone());
	let Some(spv) = spv else {
		panic!("no .spv file in the result file system, got: {entries:?}")
	};

	// Its contents are the SPIR-V binary (magic number 0x07230203, little
	// endian) that `entry_point_code` produces.
	let from_fs = fs.load_file(&spv).unwrap();
	let expected = linked_program.entry_point_code(0, 0).unwrap();
	assert_eq!(&from_fs.as_slice()[..4], &[0x03, 0x02, 0x23, 0x07]);
	assert_eq!(from_fs.as_slice(), expected.as_slice());

	// The result file system is fully mutable.
	fs.save_file("extra.bin", b"extra").unwrap();
	assert_eq!(fs.load_file("extra.bin").unwrap().as_slice(), b"extra");
	fs.remove("extra.bin").unwrap();
	assert!(fs.load_file("extra.bin").is_err());
}

/// End-to-end host-callable test: a compute entry point is compiled to host
/// machine code (`CompileTarget::ShaderHostCallable`) and called directly from
/// Rust. The ABI of the exported function follows Slang's
/// `docs/cpu-target.md` (and the `examples/cpu-hello-world` C++ sample):
///
/// - the entry point is exported under its source name with the signature
///   `void fn(ComputeVaryingInput*, UniformEntryPointParams*, UniformState*)`;
///   a single call executes the whole group range
///   `[startGroupID, endGroupID)`;
/// - `RWStructuredBuffer<T>` maps to `{ T* data; size_t count; }` and global
///   bindings land in `UniformState` in source declaration order;
/// - the entry point has no uniform parameters, so the second argument is
///   null.
#[test]
fn host_callable() {
	use std::ffi::c_void;

	let global_session = slang::GlobalSession::new().unwrap();

	// Host-callable compilation needs a downstream CPU compiler: the bundled
	// slang-llvm JIT (preferred), or a system C/C++ compiler. Skip when none
	// is available.
	let has_cpu_compiler = [
		slang::PassThrough::Llvm,
		slang::PassThrough::VisualStudio,
		slang::PassThrough::Gcc,
		slang::PassThrough::Clang,
		slang::PassThrough::GenericCCpp,
	]
	.into_iter()
	.any(|compiler| global_session.check_pass_through_support(compiler).is_ok());
	if !has_cpu_compiler {
		eprintln!("skipping host_callable: no downstream CPU compiler available");
		return;
	}

	let target_desc = slang::TargetDesc::default().format(slang::CompileTarget::ShaderHostCallable);
	let targets = [target_desc];
	let session_desc = slang::SessionDesc::default().targets(&targets);
	let session = global_session.create_session(&session_desc).unwrap();

	let source = r#"
RWStructuredBuffer<int> inputBuffer;
RWStructuredBuffer<int> outputBuffer;

[shader("compute")]
[numthreads(4, 1, 1)]
void computeMain(uint3 dispatchThreadID : SV_DispatchThreadID)
{
	uint tid = dispatchThreadID.x;
	outputBuffer[tid] = inputBuffer[tid] * 2 + 1;
}
"#;
	let module = session
		.load_module_from_source_string("host_callable", "host_callable.slang", source)
		.unwrap();
	let entry_point = module.find_entry_point_by_name("computeMain").unwrap();
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();

	let shared_library = program.entry_point_host_callable(0, 0).unwrap();

	// The ABI of the exported entry point, per docs/cpu-target.md.
	#[repr(C)]
	struct StructuredBuffer {
		data: *mut i32,
		count: usize,
	}

	#[repr(C)]
	struct UniformState {
		input_buffer: StructuredBuffer,
		output_buffer: StructuredBuffer,
	}

	#[repr(C)]
	struct ComputeVaryingInput {
		start_group_id: [u32; 3],
		end_group_id: [u32; 3],
	}

	type ComputeFunc =
		extern "C" fn(*const ComputeVaryingInput, *const c_void, *const UniformState);

	let symbol = shared_library.find_symbol("computeMain").unwrap();
	// SAFETY: `symbol` is the entry point function Slang compiled for the
	// host-callable target; its signature is the documented
	// `void fn(ComputeVaryingInput*, UniformEntryPointParams*, UniformState*)`
	// with C calling convention. The `SharedLibrary` (which owns the code)
	// outlives this call.
	let func: ComputeFunc = unsafe { std::mem::transmute(symbol) };

	let input = [1, 2, 3, 4];
	let mut output = [0; 4];

	let uniform_state = UniformState {
		input_buffer: StructuredBuffer {
			data: input.as_ptr() as *mut i32,
			count: input.len(),
		},
		output_buffer: StructuredBuffer {
			data: output.as_mut_ptr(),
			count: output.len(),
		},
	};
	let varying_input = ComputeVaryingInput {
		start_group_id: [0, 0, 0],
		end_group_id: [1, 1, 1],
	};

	// `[numthreads(4, 1, 1)]` with a single group covers all 4 elements. The
	// kernel only writes through `output_buffer` and only within its `count`,
	// so the raw pointers stay in bounds for the duration of the call.
	func(&varying_input, std::ptr::null(), &uniform_state);
	assert_eq!(output, [3, 5, 7, 9]);

	// Unknown symbols report `None`.
	assert!(shared_library.find_symbol("no_such_symbol").is_none());

	// `IComponentType2::getTargetHostCallable` exports the same entry point.
	let program2 = program.as_component_type2().unwrap();
	let target_library = program2.target_host_callable(0).unwrap();
	assert!(target_library.find_symbol("computeMain").is_some());
}

// --- M7c: misc remaining capability tests ---

#[test]
fn build_tag_string_free_function() {
	// The free function must agree with the global-session method (slang.h
	// documents them as returning exactly the same result).
	let tag = slang::build_tag_string().expect("build tag string should be available");
	assert!(!tag.is_empty());

	let global_session = slang::GlobalSession::new().unwrap();
	assert_eq!(Some(tag), global_session.build_tag_string());
}

#[test]
fn load_module_from_source_blob() {
	let session = create_test_session();

	let source = slang::Blob::new(
		b"int fortyTwo() { return 42; }\n[shader(\"compute\")][numthreads(1,1,1)] void main() {}\n",
	);
	let module = session
		.load_module_from_source("blob_source", "blob_source.slang", &source)
		.unwrap();
	assert_eq!(module.name(), Some("blob_source"));
	assert_eq!(module.entry_point_count(), 1);
}

#[test]
fn global_session_add_builtins() {
	let global_session = slang::GlobalSession::new().unwrap();
	global_session.add_builtins(
		"m7c_builtin.slang",
		"public int m7cBuiltinValue() { return 42; }\n",
	);

	let target_desc = slang::TargetDesc::default().format(slang::CompileTarget::Spirv);
	let targets = [target_desc];
	let session_desc = slang::SessionDesc::default().targets(&targets);
	let session = global_session.create_session(&session_desc).unwrap();

	// Declarations added through `addBuiltins` are in scope for modules loaded
	// into sessions created afterwards, without any `import`.
	let module = session
		.load_module_from_source_string(
			"uses_builtin",
			"uses_builtin.slang",
			"int getValue() { return m7cBuiltinValue(); }\n",
		)
		.unwrap();
	assert_eq!(module.name(), Some("uses_builtin"));
}

#[test]
fn global_session_downstream_compilers() {
	let global_session = slang::GlobalSession::new().unwrap();

	// Default downstream compiler round-trip.
	global_session
		.set_default_downstream_compiler(slang::SourceLanguage::Hlsl, slang::PassThrough::Dxc)
		.unwrap();
	assert_eq!(
		global_session.default_downstream_compiler(slang::SourceLanguage::Hlsl),
		slang::PassThrough::Dxc
	);

	// Downstream-compiler-for-transition round-trip.
	global_session.set_downstream_compiler_for_transition(
		slang::CompileTarget::Hlsl,
		slang::CompileTarget::Dxil,
		slang::PassThrough::Dxc,
	);
	assert_eq!(
		global_session.downstream_compiler_for_transition(
			slang::CompileTarget::Hlsl,
			slang::CompileTarget::Dxil
		),
		slang::PassThrough::Dxc
	);

	// Version query: the glslang family always reports (0, 0) per slang.h,
	// and a compiler that cannot be located reports an error. Either outcome
	// is fine here — the point is exercising the FFI path.
	let _ = global_session.downstream_compiler_version(slang::PassThrough::Glslang);
}

#[test]
fn builtin_module_round_trip() {
	let global_session = slang::GlobalSession::new().unwrap();

	// A fully initialized global session already has the core builtin module,
	// so compiling it again fails, but saving it succeeds.
	assert!(
		global_session
			.compile_builtin_module(slang::BuiltinModuleName::Core, 0)
			.is_err()
	);
	let blob = global_session
		.save_builtin_module(slang::BuiltinModuleName::Core, slang::ArchiveType::RiffLz4)
		.unwrap();
	assert!(!blob.as_slice().is_empty());
}

#[test]
fn metadata_info_methods_empty() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let metadata = linked_program.target_metadata(0).unwrap();

	// A plain shader compiled without coverage instrumentation reports zero
	// counters/entries/resources (see coverage_synthetic_and_cooperative_metadata);
	// the indexed accessors must reject out-of-range indices with an error
	// rather than panic.
	let coverage = metadata
		.cast_as::<slang::CoverageTracingMetadata>()
		.expect("target metadata should cast to ICoverageTracingMetadata");
	assert_eq!(coverage.entry_count(), 0);
	assert!(coverage.entry_info(0).is_err());
	// Without coverage there is no synthesized buffer; `getBufferInfo` reports
	// the not-assigned sentinels.
	let buffer_info = coverage.buffer_info().unwrap();
	assert_eq!(buffer_info.space, -1);
	assert_eq!(buffer_info.binding, -1);

	let synthetic = metadata
		.cast_as::<slang::SyntheticResourceMetadata>()
		.expect("target metadata should cast to ISyntheticResourceMetadata");
	assert_eq!(synthetic.resource_count(), 0);
	assert!(synthetic.resource_info(0).is_err());
	// Id 0 is the reserved "unassigned" sentinel and never resolves.
	assert!(synthetic.find_resource_index_by_id(0).is_err());

	let cooperative = metadata
		.cast_as::<slang::CooperativeTypesMetadata>()
		.expect("target metadata should cast to ICooperativeTypesMetadata");
	assert!(cooperative.cooperative_matrix_type_by_index(0).is_err());
	assert!(
		cooperative
			.cooperative_matrix_combination_by_index(0)
			.is_err()
	);
	assert!(cooperative.cooperative_vector_type_by_index(0).is_err());
	assert!(
		cooperative
			.cooperative_vector_combination_by_index(0)
			.is_err()
	);
}

#[test]
fn module_precompile_service() {
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();

	// Module COM objects implement IModulePrecompileService_Experimental in
	// Slang v2026.14.1 (slang-linkable.h).
	let service = module
		.precompile_service()
		.expect("module should implement IModulePrecompileService_Experimental");

	// test.slang imports nothing, so it has no module dependencies.
	assert_eq!(service.module_dependency_count(), 0);
	assert!(service.module_dependency(0).is_err());

	service
		.precompile_for_target(slang::CompileTarget::Spirv)
		.unwrap();
	let code = service
		.precompiled_target_code(slang::CompileTarget::Spirv)
		.unwrap();
	assert!(!code.as_slice().is_empty());
}

#[test]
fn byte_code_runner() {
	// Smoke: creating a runner and querying it without a loaded module must
	// not crash. Slang v2026.14.1 leaves the interpreter's module view
	// uninitialized until `loadModule` succeeds and reads it unconditionally
	// in these queries, so the wrapper short-circuits them itself until a
	// module has been loaded (see the `ByteCodeRunner` docs).
	let runner = slang::ByteCodeRunner::new().unwrap();
	assert_eq!(runner.find_function_by_name("main"), -1);
	assert!(runner.function_info(0).is_err());
	assert!(runner.select_function_by_index(0).is_err());
	// `getErrorString` only reads the runner's error string builder, so it is
	// safe (and useful) without a loaded module.
	let _ = runner.error_string();
	// There is no public way to produce a Slang bytecode module in
	// v2026.14.1, so `loadModule`/`execute` stay untested.
}

#[test]
fn multi_target_compilation() {
	// Compile to SPIR-V and DXIL simultaneously, verifying the output
	// from both targets.
	let gs = global_session();

	// Override targets: SPIR-V + DXIL.
	let spirv = slang::TargetDesc::default()
		.format(slang::CompileTarget::Spirv)
		.profile(gs.find_profile("glsl_450"));
	let dxil = slang::TargetDesc::default()
		.format(slang::CompileTarget::Dxil)
		.profile(gs.find_profile("sm_6_5"));
	let targets = [spirv, dxil];

	// Rebuild the session with both targets.
	let search_path = std::ffi::CString::new("shaders").unwrap();
	let search_paths = [search_path.as_ptr()];
	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.search_paths(&search_paths);
	let session = gs.create_session(&session_desc).unwrap();

	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();

	// Target 0: SPIR-V
	let spirv_code = linked_program.entry_point_code(0, 0).unwrap();
	assert!(!spirv_code.as_slice().is_empty());
	// SPIR-V magic number: 0x07230203
	let magic = u32::from_le_bytes(spirv_code.as_slice()[..4].try_into().unwrap());
	assert_eq!(magic, 0x07230203, "not valid SPIR-V");

	// Target 1: DXIL (if supported on this platform)
	if gs
		.check_compile_target_support(slang::CompileTarget::Dxil)
		.is_ok()
	{
		let dxil_code = linked_program.entry_point_code(0, 1).unwrap();
		assert!(!dxil_code.as_slice().is_empty());
		// DXIL starts with a DXBC header ("DXBC" magic).
		let header = &dxil_code.as_slice()[..4];
		assert_eq!(header, b"DXBC", "not valid DXIL");
	}
}

#[test]
fn multi_entry_point() {
	// Module with two entry points, both compiled and verified.
	let session = create_test_session();
	let source = r#"
struct VertexOutput { float4 position : SV_Position; };

[shader("vertex")]
VertexOutput vertexMain(uint vid : SV_VertexID) {
	VertexOutput output;
	output.position = float4(vid, 0, 0, 1);
	return output;
}

[shader("fragment")]
float4 fragmentMain(VertexOutput input) : SV_Target { return input.position; }
"#;
	let module = session
		.load_module_from_source_string("multi_ep", "multi_ep.slang", source)
		.unwrap();
	assert_eq!(module.entry_point_count(), 2);

	let ep_vertex = module.find_entry_point_by_name("vertexMain").unwrap();
	let ep_fragment = module.find_entry_point_by_name("fragmentMain").unwrap();
	let ep_names: Vec<_> = module
		.entry_points()
		.map(|ep| ep.function_reflection().name().unwrap_or("?").to_string())
		.collect();
	assert_eq!(ep_names, ["vertexMain", "fragmentMain"]);

	// Compile both entry points together.
	let program = session
		.create_composite_component_type(&[module.into(), ep_vertex.into(), ep_fragment.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let code_v = linked_program.entry_point_code(0, 0).unwrap();
	let code_f = linked_program.entry_point_code(1, 0).unwrap();
	assert!(!code_v.as_slice().is_empty());
	assert!(!code_f.as_slice().is_empty());
	// Both must be valid SPIR-V.
	let magic_v = u32::from_le_bytes(code_v.as_slice()[..4].try_into().unwrap());
	let magic_f = u32::from_le_bytes(code_f.as_slice()[..4].try_into().unwrap());
	assert_eq!(magic_v, 0x07230203);
	assert_eq!(magic_f, 0x07230203);
}

#[test]
fn compiler_options_comprehensive() {
	// Exercise all typed builder methods on CompilerOptions.
	let gs = global_session();
	let profile = gs.find_profile("glsl_450");
	let capability = gs.find_capability("spirv_1_5");

	let options = slang::CompilerOptions::default()
		.optimization(slang::OptimizationLevel::High)
		.matrix_layout_row(true)
		.matrix_layout_column(false)
		.profile(profile)
		.capability(capability)
		.language(slang::SourceLanguage::Slang)
		.macro_define("TEST_MACRO", "42")
		.include(".");

	let search_path = std::ffi::CString::new(".").unwrap();
	let search_paths = [search_path.as_ptr()];
	let target_desc = slang::TargetDesc::default()
		.format(slang::CompileTarget::Spirv)
		.profile(profile);
	let targets = [target_desc];
	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.search_paths(&search_paths)
		.options(&options);
	let session = gs.create_session(&session_desc).unwrap();

	let source = r#"
#if TEST_MACRO != 42
#error "TEST_MACRO not set"
#endif
[numthreads(1, 1, 1)]
[shader("compute")]
void main(uint3 tid : SV_DispatchThreadID) { }
"#;
	let module = session
		.load_module_from_source_string("opts_test", "opts_test.slang", source)
		.unwrap();
	assert!(module.find_entry_point_by_name("main").is_some());
}

#[test]
fn rename_entry_point_and_hash() {
	// Rename an entry point and verify the hash changes.
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();
	let comp: slang::ComponentType = entry_point.into();
	let hash_before = comp.entry_point_hash(0, 0);

	let renamed = comp.rename_entry_point("renamed_main").unwrap();
	let hash_after = renamed.entry_point_hash(0, 0);
	assert_ne!(
		hash_before.as_slice(),
		hash_after.as_slice(),
		"renamed entry point should produce a different hash",
	);

	// The renamed entry point still compiles valid SPIR-V.
	let program = session
		.create_composite_component_type(&[module.into(), renamed])
		.unwrap();
	let linked = program.link().unwrap();
	let code = linked.entry_point_code(0, 0).unwrap();
	assert!(!code.as_slice().is_empty());
}

#[test]
fn container_type_and_specialize() {
	// Create a container type and specialize a concrete type against it.
	let session = create_test_session();
	let module = session
		.load_module_from_source_string("cont", "cont.slang", "int g_data;\n")
		.unwrap();
	let program = slang::ComponentType::from(module);
	let reflection = program.layout(0).unwrap();

	let param = reflection.parameter_by_index(0).unwrap();
	let ty = param.ty().unwrap();

	// Wrap the scalar type in a container: UnsizedArray<int>.
	let element_ty = session
		.container_type(ty, slang::ContainerType::UnsizedArray)
		.unwrap();
	assert_eq!(element_ty.kind(), slang::TypeKind::Array);
	assert_eq!(element_ty.element_count(), 0); // unsized

	// Container type of a StructuredBuffer<float>.
	let float_module = session
		.load_module_from_source_string("float_mod", "float_mod.slang", "float f;\n")
		.unwrap();
	let float_program = slang::ComponentType::from(float_module);
	let float_reflection = float_program.layout(0).unwrap();
	let float_param = float_reflection.parameter_by_index(0).unwrap();
	let float_ty = float_param.ty().unwrap();
	let sb_ty = session
		.container_type(float_ty, slang::ContainerType::StructuredBuffer)
		.unwrap();
	assert_eq!(sb_ty.kind(), slang::TypeKind::Resource);
}

#[test]
fn module_info_and_binary_up_to_date() {
	// Serialize a module, read its info back, and check the up-to-date flag.
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let ir_blob = module.serialize().unwrap();

	let info = session.module_info_from_ir_blob(&ir_blob).unwrap();
	assert_eq!(info.name, Some("test.slang"));
	assert!(info.version > 0);

	// The serialized blob is derived from the current source, so it should
	// be considered up-to-date.
	let up_to_date = session.is_binary_module_up_to_date("shaders/test.slang", &ir_blob);
	assert!(up_to_date);
}

#[test]
fn spirv_validation() {
	// Verify that the SPIR-V output is valid by checking the magic number
	// and basic structure.
	let session = create_test_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked_program = program.link().unwrap();
	let code = linked_program.entry_point_code(0, 0).unwrap();
	let bytes = code.as_slice();

	// SPIR-V binary format:
	//   [0..4]   Magic number (0x07230203)
	//   [4..8]   Version number
	//   [8..12]  Generator's magic number
	//   [12..16] Bound (ID bound)
	//   [16..20] Schema (0 for standard SPIR-V)
	assert!(bytes.len() >= 20, "SPIR-V too short: {} bytes", bytes.len());
	let magic = u32::from_le_bytes(bytes[..4].try_into().unwrap());
	assert_eq!(magic, 0x07230203, "bad SPIR-V magic");
	let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
	// SPIR-V 1.0 = 0x10000, 1.1 = 0x10100, 1.3 = 0x10300, 1.5 = 0x10500
	assert!(
		(0x10000..=0x10600).contains(&version),
		"unexpected SPIR-V version: 0x{version:08x}",
	);
}
