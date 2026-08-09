use super::{Type, rcall};
use crate::sys;

/// Reflection of a user-defined attribute.
///
/// Corresponds to `Attribute` (aliased as `UserAttribute`) in slang.h.
#[repr(transparent)]
pub struct UserAttribute(sys::SlangReflectionUserAttribute);

impl UserAttribute {
	/// Returns the name of the attribute.
	pub fn name(&self) -> Option<&str> {
		rcall!(spReflectionUserAttribute_GetName(self) as Option<&str>)
	}

	/// Returns the number of arguments passed to the attribute.
	pub fn argument_count(&self) -> u32 {
		rcall!(spReflectionUserAttribute_GetArgumentCount(self))
	}

	/// Returns the type of the argument at `index`, or `None` if `index` is out
	/// of range.
	pub fn argument_type(&self, index: u32) -> Option<&Type> {
		rcall!(spReflectionUserAttribute_GetArgumentType(self, index) as Option<&Type>)
	}

	/// Returns the value of the argument at `index` as an integer, or `None` if
	/// the argument is not an integer constant or `index` is out of range.
	pub fn argument_value_int(&self, index: u32) -> Option<i32> {
		let mut out = 0;
		let result = rcall!(spReflectionUserAttribute_GetArgumentValueInt(
			self, index, &mut out
		));

		crate::succeeded(result).then_some(out)
	}

	/// Returns the value of the argument at `index` as a float, or `None` if
	/// the argument is not a floating-point constant or `index` is out of
	/// range.
	pub fn argument_value_float(&self, index: u32) -> Option<f32> {
		let mut out = 0.0;
		let result = rcall!(spReflectionUserAttribute_GetArgumentValueFloat(
			self, index, &mut out
		));

		crate::succeeded(result).then_some(out)
	}

	/// Returns the value of the argument at `index` as a string, or `None` if
	/// the argument is not a string constant or `index` is out of range.
	pub fn argument_value_string(&self, index: u32) -> Option<&str> {
		let mut len = 0;
		let result = rcall!(spReflectionUserAttribute_GetArgumentValueString(
			self, index, &mut len
		));

		(!result.is_null()).then(|| {
			// SAFETY: `result` is non-null (checked above) and points to `len`
			// bytes of a string owned by the user attribute, which outlives
			// `&self`.
			let slice = unsafe { std::slice::from_raw_parts(result as *const u8, len) };
			std::str::from_utf8(slice).unwrap()
		})
	}
}
