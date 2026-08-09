use super::{Generic, Type, UserAttribute, rcall};
use crate::{GlobalSession, Interface, Modifier, ModifierID, succeeded, sys};

/// Reflection of a variable, such as a struct field, function parameter, or
/// global.
///
/// Corresponds to `VariableReflection` in slang.h.
#[repr(transparent)]
pub struct Variable(sys::SlangReflectionVariable);

impl Variable {
	/// Returns the name of the variable.
	pub fn name(&self) -> Option<&str> {
		rcall!(spReflectionVariable_GetName(self) as Option<&str>)
	}

	/// Returns the type of the variable.
	pub fn ty(&self) -> Option<&Type> {
		rcall!(spReflectionVariable_GetType(self) as Option<&Type>)
	}

	/// Finds the modifier with the given ID, returning `None` if the variable
	/// has no such modifier.
	pub fn find_modifier(&self, id: ModifierID) -> Option<&Modifier> {
		rcall!(spReflectionVariable_FindModifier(self, id) as Option<&Modifier>)
	}

	/// Returns the number of user-defined attributes on the variable.
	pub fn user_attribute_count(&self) -> u32 {
		rcall!(spReflectionVariable_GetUserAttributeCount(self))
	}

	/// Returns the user-defined attribute at `index`, or `None` if `index` is
	/// out of range.
	pub fn user_attribute_by_index(&self, index: u32) -> Option<&UserAttribute> {
		rcall!(spReflectionVariable_GetUserAttribute(self, index) as Option<&UserAttribute>)
	}

	/// Returns an iterator over the variable's user-defined attributes.
	pub fn user_attributes(&self) -> impl ExactSizeIterator<Item = &UserAttribute> {
		(0..self.user_attribute_count()).map(|i| self.user_attribute_by_index(i).unwrap())
	}

	/// Finds a user-defined attribute by name, returning `None` if the variable
	/// has no attribute with that name.
	///
	/// `global_session` must be the global session that produced this
	/// reflection.
	pub fn find_user_attribute_by_name(
		&self,
		global_session: &GlobalSession,
		name: &str,
	) -> Option<&UserAttribute> {
		let name = std::ffi::CString::new(name).unwrap();
		rcall!(spReflectionVariable_FindUserAttributeByName(
			self,
			global_session.as_raw(),
			name.as_ptr()
		) as Option<&UserAttribute>)
	}

	/// Returns whether the variable has a default value.
	///
	/// Deprecated in slang.h in favor of the (C++-only) `getDefaultValueBlob`.
	pub fn has_default_value(&self) -> bool {
		rcall!(spReflectionVariable_HasDefaultValue(self))
	}

	/// Gets an integer default value, returning `None` if no integer default
	/// value is available.
	///
	/// Deprecated in slang.h in favor of the (C++-only) `getDefaultValueBlob`.
	/// For specialized generic static constants, the semantic value is resolved
	/// under the current specialization first; literal initializers are used as
	/// a fallback when no integer value resolves.
	pub fn default_value_int(&self) -> Option<i64> {
		let mut value = 0;
		let result = rcall!(spReflectionVariable_GetDefaultValueInt(self, &mut value));
		if succeeded(result) { Some(value) } else { None }
	}

	/// Gets a floating-point default value from a literal initializer.
	///
	/// Deprecated in slang.h in favor of the (C++-only) `getDefaultValueBlob`;
	/// unlike `default_value_int`, this does not resolve specialized generic
	/// semantic values before checking the initializer.
	pub fn default_value_float(&self) -> Option<f32> {
		let mut value = 0.0;
		let result = rcall!(spReflectionVariable_GetDefaultValueFloat(self, &mut value));
		if succeeded(result) { Some(value) } else { None }
	}

	/// Returns the generic container this variable is declared in, or `None` if
	/// it is not part of a generic.
	pub fn generic_container(&self) -> Option<&Generic> {
		rcall!(spReflectionVariable_GetGenericContainer(self) as Option<&Generic>)
	}

	/// Returns a version of this variable with its generic parameters
	/// substituted according to `generic`.
	pub fn apply_specializations(&self, generic: &Generic) -> Option<&Variable> {
		rcall!(
			spReflectionVariable_applySpecializations(self, generic as *const _ as *mut _)
				as Option<&Variable>
		)
	}
}
