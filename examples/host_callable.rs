//! Host-callable example: compile a compute entry point to host machine code
//! (`CompileTarget::ShaderHostCallable`) and call it directly from Rust. The
//! ABI of the exported function is documented in Slang's `docs/cpu-target.md`:
//! the entry point is exported under its source name as
//! `void fn(ComputeVaryingInput*, UniformEntryPointParams*, UniformState*)`,
//! and `RWStructuredBuffer<T>` maps to `{ T* data; size_t count; }`.
//!
//! Requires a downstream CPU compiler: the slang-llvm JIT bundled with the
//! prebuilt Slang binaries, or a system C/C++ compiler (MSVC/gcc/clang).
//!
//! Run with: `cargo run --example host_callable`

use std::ffi::c_void;

use shader_slang_rs as slang;

const SOURCE: &str = r#"
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

type ComputeFunc = extern "C" fn(*const ComputeVaryingInput, *const c_void, *const UniformState);

fn main() {
	let global_session = slang::GlobalSession::new().unwrap();

	let target_desc = slang::TargetDesc::default().format(slang::CompileTarget::ShaderHostCallable);
	let targets = [target_desc];
	let session_desc = slang::SessionDesc::default().targets(&targets);
	let session = global_session.create_session(&session_desc).unwrap();

	let module = session
		.load_module_from_source_string("host_callable", "host_callable.slang", SOURCE)
		.unwrap();
	let entry_point = module.find_entry_point_by_name("computeMain").unwrap();
	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();

	let shared_library = program.entry_point_host_callable(0, 0).unwrap();
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
	// `[numthreads(4, 1, 1)]` with a single group covers all 4 elements.
	let varying_input = ComputeVaryingInput {
		start_group_id: [0, 0, 0],
		end_group_id: [1, 1, 1],
	};

	func(&varying_input, std::ptr::null(), &uniform_state);

	println!("input:  {input:?}");
	println!("output: {output:?}");
}
