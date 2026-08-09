use super::{Type, rcall};
use crate::sys;

/// Reflection of a generic type parameter.
///
/// Corresponds to `TypeParameterReflection` in slang.h.
#[repr(transparent)]
pub struct TypeParameter(sys::SlangReflectionTypeParameter);

impl TypeParameter {
	/// Returns the name of the type parameter.
	pub fn name(&self) -> Option<&str> {
		rcall!(spReflectionTypeParameter_GetName(self) as Option<&str>)
	}

	/// Returns the index of the type parameter within its generic container.
	pub fn index(&self) -> u32 {
		rcall!(spReflectionTypeParameter_GetIndex(self))
	}

	/// Returns the number of constraints on the type parameter.
	pub fn constraint_count(&self) -> u32 {
		rcall!(spReflectionTypeParameter_GetConstraintCount(self))
	}

	/// Returns the constraint at `index`, or `None` if `index` is out of range.
	pub fn constraint_by_index(&self, index: u32) -> Option<&Type> {
		rcall!(spReflectionTypeParameter_GetConstraintByIndex(self, index) as Option<&Type>)
	}

	/// Returns an iterator over the type parameter's constraints.
	pub fn constraints(&self) -> impl ExactSizeIterator<Item = &Type> {
		(0..self.constraint_count()).map(|i| self.constraint_by_index(i).unwrap())
	}
}
