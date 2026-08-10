//! Benchmarks for the shader-slang-rs high-level API.
//!
//! Measures the overhead of the Rust bindings compared to the Slang C++ API
//! by timing end-to-end compilation (load, link, codegen) and key operations.

use divan::Bencher;
use shader_slang_rs as slang;

fn main() {
	divan::main();
}

/// A shared GlobalSession used across all benchmarks. `GlobalSession` is
/// deliberately `!Sync`, so it cannot live in a `'static` static directly; the
/// session is leaked and its address stored behind a `Send + Sync` holder.
/// Benchmarks run single-threaded via `bench_local`, and the shared session is
/// only touched from setup code on the bench's own thread.
fn global_session() -> &'static slang::GlobalSession {
	static GS: std::sync::OnceLock<LeakedGlobalSession> = std::sync::OnceLock::new();
	GS.get_or_init(|| {
		let session = slang::GlobalSession::new().expect("GlobalSession::new failed");
		LeakedGlobalSession(Box::leak(Box::new(session)) as *const slang::GlobalSession)
	})
	.as_ref()
}

/// Holds a leaked `&'static` `GlobalSession` address for [`global_session`].
struct LeakedGlobalSession(*const slang::GlobalSession);

// SAFETY: the leaked session is only dereferenced from the benchmark's own
// (single) thread, never concurrently.
unsafe impl Send for LeakedGlobalSession {}
unsafe impl Sync for LeakedGlobalSession {}

impl LeakedGlobalSession {
	fn as_ref(&self) -> &'static slang::GlobalSession {
		// SAFETY: the pointer is non-null and to a `Box::leak`ed session that
		// outlives the process.
		unsafe { &*self.0 }
	}
}

/// Benchmark: create a GlobalSession.
#[divan::bench]
fn create_global_session() -> Option<slang::GlobalSession> {
	slang::GlobalSession::new()
}

/// Benchmark: create a Session with a SPIR-V target.
#[divan::bench]
fn create_session() -> slang::Session {
	let gs = global_session();
	let search_path = std::ffi::CString::new("shaders").unwrap();
	let search_paths = [search_path.as_ptr()];
	let target_desc = slang::TargetDesc::default()
		.format(slang::CompileTarget::Spirv)
		.profile(gs.find_profile("glsl_450"));
	let targets = [target_desc];
	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.search_paths(&search_paths);
	gs.create_session(&session_desc).unwrap()
}

/// Benchmark: load a module from source string.
#[divan::bench]
fn load_module(bencher: Bencher) {
	let session = create_session();
	bencher.bench_local(|| {
		let _module = session.load_module("test.slang");
	});
}

/// Benchmark: full compile pipeline (load, link, entry point code).
#[divan::bench]
fn full_compile(bencher: Bencher) {
	let session = create_session();
	bencher.bench_local(|| {
		let module = session.load_module("test.slang").unwrap();
		let entry_point = module.find_entry_point_by_name("main").unwrap();
		let program = session
			.create_composite_component_type(&[module.into(), entry_point.into()])
			.unwrap();
		let linked = program.link().unwrap();
		let _code = linked.entry_point_code(0, 0).unwrap();
	});
}

/// Benchmark: reflection layout (after linking).
#[divan::bench]
fn get_layout(bencher: Bencher) {
	let session = create_session();
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();
	let linked = program.link().unwrap();
	bencher.bench_local(|| {
		let _layout = linked.layout(0);
	});
}

/// Benchmark: module serialization + deserialization round trip.
#[divan::bench]
fn serialize_module(bencher: Bencher) {
	let session = create_session();
	let module = session.load_module("test.slang").unwrap();
	bencher.bench_local(|| {
		let _blob = module.serialize().unwrap();
	});
}

/// Benchmark: host callable compilation (when a CPU compiler is available).
#[divan::bench]
#[allow(unused_results)]
fn host_callable_compile(bencher: Bencher) {
	let gs = global_session();
	let has_cpu_compiler = [
		slang::PassThrough::Llvm,
		slang::PassThrough::VisualStudio,
		slang::PassThrough::Gcc,
		slang::PassThrough::Clang,
		slang::PassThrough::GenericCCpp,
	]
	.into_iter()
	.any(|compiler| gs.check_pass_through_support(compiler).is_ok());
	if !has_cpu_compiler {
		return;
	}

	let target_desc = slang::TargetDesc::default().format(slang::CompileTarget::ShaderHostCallable);
	let targets = [target_desc];
	let session_desc = slang::SessionDesc::default().targets(&targets);
	let session = gs.create_session(&session_desc).unwrap();

	let source = r#"
RWStructuredBuffer<int> inputBuffer;
RWStructuredBuffer<int> outputBuffer;

[shader("compute")]
[numthreads(4, 1, 1)]
void computeMain(uint3 dispatchThreadID : SV_DispatchThreadID) {
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

	bencher.bench_local(|| {
		let _lib = program.entry_point_host_callable(0, 0).unwrap();
	});
}
