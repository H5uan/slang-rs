use super::{Decl, Type, Variable, rcall};
use crate::{DeclKind, sys};

/// Reflection of a generic declaration or specialization
/// (`GenericReflection` in slang.h).
#[repr(transparent)]
pub struct Generic(sys::SlangReflectionGeneric);

impl Generic {
	/// Returns this generic viewed as a declaration, or `None` if unavailable.
	pub fn as_decl(&self) -> Option<&Decl> {
		rcall!(spReflectionGeneric_asDecl(self) as Option<&Decl>)
	}

	/// Returns the name of the generic declaration, or `None` if unavailable.
	pub fn name(&self) -> Option<&str> {
		rcall!(spReflectionGeneric_GetName(self) as Option<&str>)
	}

	/// Returns the number of type parameters of the generic.
	pub fn type_parameter_count(&self) -> u32 {
		rcall!(spReflectionGeneric_GetTypeParameterCount(self))
	}

	/// Returns the type parameter at `index`, or `None` if `index` is out of
	/// range.
	pub fn type_parameter_by_index(&self, index: u32) -> Option<&Variable> {
		rcall!(spReflectionGeneric_GetTypeParameter(self, index) as Option<&Variable>)
	}

	/// Returns an iterator over the type parameters of the generic.
	pub fn type_parameters(&self) -> impl ExactSizeIterator<Item = &Variable> {
		(0..self.type_parameter_count()).map(|i| self.type_parameter_by_index(i).unwrap())
	}

	/// Returns the number of value parameters of the generic.
	pub fn value_parameter_count(&self) -> u32 {
		rcall!(spReflectionGeneric_GetValueParameterCount(self))
	}

	/// Returns the value parameter at `index`, or `None` if `index` is out of
	/// range.
	pub fn value_parameter_by_index(&self, index: u32) -> Option<&Variable> {
		rcall!(spReflectionGeneric_GetValueParameter(self, index) as Option<&Variable>)
	}

	/// Returns an iterator over the value parameters of the generic.
	pub fn value_parameters(&self) -> impl ExactSizeIterator<Item = &Variable> {
		(0..self.value_parameter_count()).map(|i| self.value_parameter_by_index(i).unwrap())
	}

	/// Returns the number of constraints declared on the given type parameter.
	pub fn type_parameter_constraint_count(&self, type_param: &Variable) -> u32 {
		rcall!(spReflectionGeneric_GetTypeParameterConstraintCount(
			self,
			type_param as *const _ as *mut _
		))
	}

	/// Returns the constraint type at `index` for the given type parameter, or
	/// `None` if `index` is out of range.
	pub fn type_parameter_constraint_by_index(
		&self,
		type_param: &Variable,
		index: u32,
	) -> Option<&Type> {
		rcall!(spReflectionGeneric_GetTypeParameterConstraintType(
			self,
			type_param as *const _ as *mut _,
			index
		) as Option<&Type>)
	}

	/// Returns the declaration nested inside the generic, or `None` if
	/// unavailable.
	pub fn inner_decl(&self) -> Option<&Decl> {
		rcall!(spReflectionGeneric_GetInnerDecl(self) as Option<&Decl>)
	}

	/// Returns the kind of the declaration nested inside the generic.
	pub fn inner_kind(&self) -> DeclKind {
		rcall!(spReflectionGeneric_GetInnerKind(self))
	}

	/// Returns the enclosing generic, or `None` if this generic is not nested
	/// in another generic.
	pub fn outer_generic_container(&self) -> Option<&Generic> {
		rcall!(spReflectionGeneric_GetOuterGenericContainer(self) as Option<&Generic>)
	}

	/// Returns the concrete type substituted for the given type parameter in a
	/// specialized generic, or `None` if the parameter is unbound.
	pub fn concrete_type(&self, type_param: &Variable) -> Option<&Type> {
		rcall!(
			spReflectionGeneric_GetConcreteType(self, type_param as *const _ as *mut _)
				as Option<&Type>
		)
	}

	/// Returns the concrete integer value substituted for the given value
	/// parameter in a specialized generic.
	pub fn concrete_int_val(&self, value_param: &Variable) -> i64 {
		rcall!(spReflectionGeneric_GetConcreteIntVal(
			self,
			value_param as *const _ as *mut _
		))
	}

	/// Returns this generic with the specializations of `generic` applied, or
	/// `None` if the specializations do not apply.
	pub fn apply_specializations(&self, generic: &Generic) -> Option<&Generic> {
		rcall!(
			spReflectionGeneric_applySpecializations(self, generic as *const _ as *mut _)
				as Option<&Generic>
		)
	}
}
