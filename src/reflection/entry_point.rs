use super::{Function, TypeLayout, VariableLayout, rcall};
use crate::{Stage, sys};

/// Reflection of a shader entry point (`EntryPointReflection` in slang.h).
#[repr(transparent)]
pub struct EntryPoint(sys::SlangReflectionEntryPoint);

impl EntryPoint {
	/// Returns the name of the entry point, or `None` if unavailable.
	pub fn name(&self) -> Option<&str> {
		rcall!(spReflectionEntryPoint_getName(self) as Option<&str>)
	}

	/// Returns the name override of the entry point as specified in source, or
	/// `None` if the entry point uses its declared name.
	pub fn name_override(&self) -> Option<&str> {
		rcall!(spReflectionEntryPoint_getNameOverride(self) as Option<&str>)
	}

	/// Returns the number of parameters of the entry point.
	pub fn parameter_count(&self) -> u32 {
		rcall!(spReflectionEntryPoint_getParameterCount(self))
	}

	/// Returns the layout of the entry point parameter at `index`, or `None`
	/// if `index` is out of range.
	pub fn parameter_by_index(&self, index: u32) -> Option<&VariableLayout> {
		rcall!(spReflectionEntryPoint_getParameterByIndex(self, index) as Option<&VariableLayout>)
	}

	/// Returns an iterator over the layouts of the entry point's parameters.
	pub fn parameters(&self) -> impl ExactSizeIterator<Item = &VariableLayout> {
		(0..self.parameter_count()).map(|i| self.parameter_by_index(i).unwrap())
	}

	/// Returns the function reflection for this entry point, or `None` if
	/// unavailable.
	pub fn function(&self) -> Option<&Function> {
		rcall!(spReflectionEntryPoint_getFunction(self) as Option<&Function>)
	}

	/// Returns the pipeline stage this entry point targets.
	pub fn stage(&self) -> Stage {
		rcall!(spReflectionEntryPoint_getStage(self))
	}

	/// Returns the compute thread group size along each axis as `[x, y, z]`;
	/// axes without a specified size are 0.
	pub fn compute_thread_group_size(&self) -> [u64; 3] {
		let mut out_size = [0; 3];
		rcall!(spReflectionEntryPoint_getComputeThreadGroupSize(
			self,
			3,
			&mut out_size as *mut u64
		));
		out_size
	}

	/// Returns the compute wave size, or 0 if the entry point does not declare
	/// one.
	pub fn compute_wave_size(&self) -> u64 {
		let mut out_size = 0;
		rcall!(spReflectionEntryPoint_getComputeWaveSize(
			self,
			&mut out_size as *mut u64
		));
		out_size
	}

	/// Returns whether the entry point uses any sample-rate input.
	pub fn uses_any_sample_rate_input(&self) -> bool {
		rcall!(spReflectionEntryPoint_usesAnySampleRateInput(self)) != 0
	}

	/// Returns the variable layout for the entry point's parameters, or `None`
	/// if unavailable.
	pub fn var_layout(&self) -> Option<&VariableLayout> {
		rcall!(spReflectionEntryPoint_getVarLayout(self) as Option<&VariableLayout>)
	}

	/// Returns the type layout for the entry point's parameters, or `None` if
	/// unavailable.
	pub fn type_layout(&self) -> Option<&TypeLayout> {
		self.var_layout()?.type_layout()
	}

	/// Returns the variable layout of the entry point's result, or `None` if
	/// unavailable.
	pub fn result_var_layout(&self) -> Option<&VariableLayout> {
		rcall!(spReflectionEntryPoint_getResultVarLayout(self) as Option<&VariableLayout>)
	}

	/// Returns whether the entry point's uniform parameters are collected into
	/// a default constant buffer.
	pub fn has_default_constant_buffer(&self) -> bool {
		rcall!(spReflectionEntryPoint_hasDefaultConstantBuffer(self)) != 0
	}
}
