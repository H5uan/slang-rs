use super::{Generic, Shader, UserAttribute, Variable, rcall};
use crate::{
	Blob, Error, IUnknown, ResourceAccess, ResourceShape, Result, ScalarType, TypeKind, cstring,
	succeeded, sys,
};

/// Reflection of a Slang type.
///
/// Mirrors `TypeReflection` in slang.h.
#[repr(transparent)]
pub struct Type(sys::SlangReflectionType);

impl Type {
	/// Returns the kind of this type.
	pub fn kind(&self) -> TypeKind {
		rcall!(spReflectionType_GetKind(self))
	}

	/// Returns the number of fields. Only useful when `kind()` is
	/// `TypeKind::Struct`.
	pub fn field_count(&self) -> u32 {
		rcall!(spReflectionType_GetFieldCount(self))
	}

	/// Returns the field at `index`, or `None` if `index` is out of range.
	pub fn field_by_index(&self, index: u32) -> Option<&Variable> {
		rcall!(spReflectionType_GetFieldByIndex(self, index) as Option<&Variable>)
	}

	/// Iterates over the fields of this type.
	pub fn fields(&self) -> impl ExactSizeIterator<Item = &Variable> {
		(0..self.field_count()).map(|i| self.field_by_index(i).unwrap())
	}

	/// Returns whether this type is an array type.
	pub fn is_array(&self) -> bool {
		self.kind() == TypeKind::Array
	}

	/// Recursively unwraps nested array types, returning the innermost
	/// non-array element type.
	pub fn unwrap_array(&self) -> &Type {
		let mut ty = self;
		while ty.is_array() {
			ty = match ty.element_type() {
				Some(t) => t,
				None => break,
			};
		}
		ty
	}

	/// Returns the total number of elements in a (possibly multi-dimensional)
	/// array type, or 0 if this is not an array type.
	pub fn total_array_element_count(&self) -> usize {
		if !self.is_array() {
			return 0;
		}
		let mut result = 1;
		let mut ty = Some(self);
		while let Some(t) = ty {
			if !t.is_array() {
				break;
			}
			result *= t.element_count();
			ty = t.element_type();
		}
		result
	}

	/// Returns the number of elements of this array or vector type, or
	/// `SLANG_UNBOUNDED_SIZE` for unbounded-size arrays.
	pub fn element_count(&self) -> usize {
		rcall!(spReflectionType_GetElementCount(self))
	}

	/// Get the number of elements in an array or vector type, using the
	/// program layout to resolve link-time constants when one is available.
	///
	/// Mirrors `TypeReflection::getElementCount(SlangReflection*)` in slang.h:
	/// returns `SLANG_UNBOUNDED_SIZE` for unbounded-size arrays and
	/// `SLANG_UNKNOWN_SIZE` when the size depends on unresolved generic
	/// parameters or link-time constants. Only useful when `kind()` is
	/// `TypeKind::Array` or `TypeKind::Vector`.
	pub fn specialized_element_count(&self, reflection: Option<&Shader>) -> usize {
		let reflection = reflection
			.map(|r| r as *const _ as *mut _)
			.unwrap_or(std::ptr::null_mut());
		rcall!(spReflectionType_GetSpecializedElementCount(
			self, reflection
		))
	}

	/// Returns the element type of this array, vector, or matrix type, or
	/// `None` if the type has no element type.
	pub fn element_type(&self) -> Option<&Type> {
		rcall!(spReflectionType_GetElementType(self) as Option<&Type>)
	}

	/// Returns the number of rows of this matrix type.
	pub fn row_count(&self) -> u32 {
		rcall!(spReflectionType_GetRowCount(self))
	}

	/// Returns the number of columns of this vector or matrix type.
	pub fn column_count(&self) -> u32 {
		rcall!(spReflectionType_GetColumnCount(self))
	}

	/// Returns the scalar type this type is built from.
	pub fn scalar_type(&self) -> ScalarType {
		rcall!(spReflectionType_GetScalarType(self))
	}

	/// Returns the result type of this resource type (e.g. the texel type of
	/// a texture), or `None` if this is not a resource type.
	pub fn resource_result_type(&self) -> Option<&Type> {
		rcall!(spReflectionType_GetResourceResultType(self) as Option<&Type>)
	}

	/// Returns the shape (and arrayed-ness) of this resource type.
	pub fn resource_shape(&self) -> ResourceShape {
		rcall!(spReflectionType_GetResourceShape(self))
	}

	/// Returns the access mode (read, read-write, ...) of this resource
	/// type.
	pub fn resource_access(&self) -> ResourceAccess {
		rcall!(spReflectionType_GetResourceAccess(self))
	}

	/// Returns the type's name without any type-level modifier wrappers
	/// (`no_diff`, `unorm`, `snorm`, ...), or `None` if the type has no
	/// name.
	pub fn name(&self) -> Option<&str> {
		rcall!(spReflectionType_GetName(self) as Option<&str>)
	}

	/// Returns the fully-qualified type name without any type-level modifier
	/// wrappers.
	///
	/// Returns `Err` if Slang fails to produce the name blob.
	pub fn full_name(&self) -> Result<Blob> {
		let mut name = std::ptr::null_mut();
		let result = rcall!(spReflectionType_GetFullName(self, &mut name));

		if succeeded(result) && !name.is_null() {
			Ok(Blob(IUnknown(
				std::ptr::NonNull::new(name as *mut _).unwrap(),
			)))
		} else {
			Err(Error::Code(result))
		}
	}

	/// Returns the number of user attributes attached to this type.
	pub fn user_attribute_count(&self) -> u32 {
		rcall!(spReflectionType_GetUserAttributeCount(self))
	}

	/// Returns the user attribute at `index`, or `None` if `index` is out of
	/// range.
	pub fn user_attribute_by_index(&self, index: u32) -> Option<&UserAttribute> {
		rcall!(spReflectionType_GetUserAttribute(self, index) as Option<&UserAttribute>)
	}

	/// Iterates over the user attributes attached to this type.
	pub fn user_attributes(&self) -> impl ExactSizeIterator<Item = &UserAttribute> {
		(0..self.user_attribute_count()).map(|i| self.user_attribute_by_index(i).unwrap())
	}

	/// Finds a user attribute by name, or returns `Ok(None)` if there is no
	/// such attribute. Returns `Err` when `name` contains an interior NUL
	/// byte.
	pub fn find_user_attribute_by_name(&self, name: &str) -> Result<Option<&UserAttribute>> {
		let name = cstring(name)?;
		Ok(rcall!(
			spReflectionType_FindUserAttributeByName(self, name.as_ptr()) as Option<&UserAttribute>
		))
	}

	/// Returns the generic declaration this type belongs to, or `None` if it
	/// is not nested inside a generic.
	pub fn generic_container(&self) -> Option<&Generic> {
		rcall!(spReflectionType_GetGenericContainer(self) as Option<&Generic>)
	}

	/// Applies the specializations of `generic` to this type, returning the
	/// specialized type, or `None` on failure.
	pub fn apply_specializations(&self, generic: &Generic) -> Option<&Type> {
		rcall!(
			spReflectionType_applySpecializations(self, generic as *const _ as *mut _)
				as Option<&Type>
		)
	}

	/// Returns the number of type arguments of this specialized type, or 0 if
	/// the type is not a specialization.
	pub fn specialized_type_arg_count(&self) -> i64 {
		rcall!(spReflectionType_getSpecializedTypeArgCount(self))
	}

	/// Returns the type argument at `index` of this specialized type, or
	/// `None` if `index` is out of range.
	pub fn specialized_type_arg_type(&self, index: i64) -> Option<&Type> {
		rcall!(spReflectionType_getSpecializedTypeArgType(self, index) as Option<&Type>)
	}
}
