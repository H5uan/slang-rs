use super::{Generic, Type, UserAttribute, Variable, rcall};
use crate::{GlobalSession, Interface, Modifier, ModifierID, sys};

/// Reflection of a function in shader code (`FunctionReflection` in slang.h).
#[repr(transparent)]
pub struct Function(sys::SlangReflectionFunction);

impl Function {
	/// Returns the name of the function, or `None` if unavailable.
	pub fn name(&self) -> Option<&str> {
		rcall!(spReflectionFunction_GetName(self) as Option<&str>)
	}

	/// Returns the function's return type, or `None` if unavailable.
	pub fn return_type(&self) -> Option<&Type> {
		rcall!(spReflectionFunction_GetResultType(self) as Option<&Type>)
	}

	/// Returns the number of parameters of the function.
	pub fn parameter_count(&self) -> u32 {
		rcall!(spReflectionFunction_GetParameterCount(self))
	}

	/// Returns the parameter at `index`, or `None` if `index` is out of range.
	pub fn parameter_by_index(&self, index: u32) -> Option<&Variable> {
		rcall!(spReflectionFunction_GetParameter(self, index) as Option<&Variable>)
	}

	/// Returns an iterator over the function's parameters.
	pub fn parameters(&self) -> impl ExactSizeIterator<Item = &Variable> {
		(0..self.parameter_count()).map(|i| self.parameter_by_index(i).unwrap())
	}

	/// Returns the number of user-defined attributes on the function.
	pub fn user_attribute_count(&self) -> u32 {
		rcall!(spReflectionFunction_GetUserAttributeCount(self))
	}

	/// Returns the user-defined attribute at `index`, or `None` if `index` is
	/// out of range.
	pub fn user_attribute_by_index(&self, index: u32) -> Option<&UserAttribute> {
		rcall!(spReflectionFunction_GetUserAttribute(self, index) as Option<&UserAttribute>)
	}

	/// Returns an iterator over the user-defined attributes on the function.
	pub fn user_attributes(&self) -> impl ExactSizeIterator<Item = &UserAttribute> {
		(0..self.user_attribute_count()).map(|i| self.user_attribute_by_index(i).unwrap())
	}

	/// Finds a user-defined attribute on the function by name, or returns
	/// `None` if there is none. Panics if `name` contains an interior NUL
	/// byte.
	pub fn find_user_attribute_by_name(
		&self,
		global_session: &GlobalSession,
		name: &str,
	) -> Option<&UserAttribute> {
		let name = std::ffi::CString::new(name).unwrap();
		rcall!(spReflectionFunction_FindUserAttributeByName(
			self,
			global_session.as_raw(),
			name.as_ptr()
		) as Option<&UserAttribute>)
	}

	/// Finds a modifier on the function by ID, e.g. an `[unroll]` attribute.
	/// Returns `None` when the function does not carry the modifier.
	pub fn find_modifier(&self, id: ModifierID) -> Option<&Modifier> {
		rcall!(spReflectionFunction_FindModifier(self, id) as Option<&Modifier>)
	}

	/// Returns the generic declaration this function is nested in, or `None`
	/// if the function is not generic.
	pub fn generic_container(&self) -> Option<&Generic> {
		rcall!(spReflectionFunction_GetGenericContainer(self) as Option<&Generic>)
	}

	/// Returns the function with the specializations of `generic` applied, or
	/// `None` if the specializations do not apply.
	pub fn apply_specializations(&self, generic: &Generic) -> Option<&Function> {
		rcall!(
			spReflectionFunction_applySpecializations(self, generic as *const _ as *mut _)
				as Option<&Function>
		)
	}

	/// Returns the function specialized with the given argument types, or
	/// `None` if specialization fails.
	pub fn specialize_with_arg_types(&self, types: &[&Type]) -> Option<&Function> {
		rcall!(spReflectionFunction_specializeWithArgTypes(
			self,
			types.len() as i64,
			types.as_ptr() as *mut _
		) as Option<&Function>)
	}

	/// Returns whether this reflection stands for an overloaded function name
	/// rather than a single function.
	pub fn is_overloaded(&self) -> bool {
		rcall!(spReflectionFunction_isOverloaded(self))
	}

	/// Returns the number of overloads of an overloaded function name.
	pub fn overload_count(&self) -> u32 {
		rcall!(spReflectionFunction_getOverloadCount(self))
	}

	/// Returns the overload at `index`, or `None` if `index` is out of range.
	pub fn overload_by_index(&self, index: u32) -> Option<&Function> {
		rcall!(spReflectionFunction_getOverload(self, index) as Option<&Function>)
	}

	/// Returns an iterator over the overloads of an overloaded function name.
	pub fn overloads(&self) -> impl ExactSizeIterator<Item = &Function> {
		(0..self.overload_count()).map(|i| self.overload_by_index(i).unwrap())
	}
}
