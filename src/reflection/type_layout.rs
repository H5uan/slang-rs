use super::{Type, Variable, VariableLayout, rcall};
use crate::{
	BindingType, ImageFormat, MatrixLayoutMode, ParameterCategory, ResourceAccess, ResourceShape,
	ScalarType, TypeKind, sys,
};

/// Reflection of a type laid out for a particular target.
///
/// Mirrors `TypeLayoutReflection` in slang.h.
#[repr(transparent)]
pub struct TypeLayout(sys::SlangReflectionTypeLayout);

impl TypeLayout {
	/// Returns the underlying type of this layout.
	pub fn ty(&self) -> Option<&Type> {
		rcall!(spReflectionTypeLayout_GetType(self) as Option<&Type>)
	}

	/// Returns the kind of the underlying type.
	pub fn kind(&self) -> TypeKind {
		rcall!(spReflectionTypeLayout_getKind(self))
	}

	/// Returns the size of this type layout in the given parameter category.
	///
	/// Returns `SLANG_UNBOUNDED_SIZE` for unbounded resources (e.g. unsized
	/// arrays) and `SLANG_UNKNOWN_SIZE` when the size depends on unresolved
	/// generic parameters or link-time constants.
	pub fn size(&self, category: ParameterCategory) -> usize {
		rcall!(spReflectionTypeLayout_GetSize(self, category))
	}

	/// Returns the stride of this type layout in the given parameter
	/// category, or `SLANG_UNBOUNDED_SIZE` / `SLANG_UNKNOWN_SIZE` on the
	/// same conditions as [`size`](Self::size).
	pub fn stride(&self, category: ParameterCategory) -> usize {
		rcall!(spReflectionTypeLayout_GetStride(self, category))
	}

	/// Returns the alignment of this type layout in the given parameter
	/// category.
	pub fn alignment(&self, category: ParameterCategory) -> i32 {
		rcall!(spReflectionTypeLayout_getAlignment(self, category))
	}

	/// Returns the number of fields of this type layout.
	pub fn field_count(&self) -> u32 {
		rcall!(spReflectionTypeLayout_GetFieldCount(self))
	}

	/// Returns the layout of the field at `index`, or `None` if `index` is
	/// out of range.
	pub fn field_by_index(&self, index: u32) -> Option<&VariableLayout> {
		rcall!(spReflectionTypeLayout_GetFieldByIndex(self, index) as Option<&VariableLayout>)
	}

	/// Iterates over the layouts of the fields of this type layout.
	pub fn fields(&self) -> impl ExactSizeIterator<Item = &VariableLayout> {
		(0..self.field_count()).map(|i| self.field_by_index(i).unwrap())
	}

	/// Finds the index of the field named `name`, or returns -1 if there is
	/// no such field.
	pub fn find_field_index_by_name(&self, name: &str) -> i64 {
		let (start, end) = (name.as_ptr(), unsafe { name.as_ptr().add(name.len()) });
		rcall!(spReflectionTypeLayout_findFieldIndexByName(
			self,
			start as *const _,
			end as *const _
		))
	}

	/// Returns the layout of the explicit counter variable associated with
	/// this type (e.g. for an `AppendStructuredBuffer`), or `None` if there
	/// is none.
	pub fn explicit_counter(&self) -> Option<&VariableLayout> {
		rcall!(spReflectionTypeLayout_GetExplicitCounter(self) as Option<&VariableLayout>)
	}

	/// Returns whether the underlying type is an array type.
	pub fn is_array(&self) -> bool {
		self.ty().map(|t| t.is_array()).unwrap_or(false)
	}

	/// Recursively unwraps nested array type layouts, returning the
	/// innermost non-array element type layout.
	pub fn unwrap_array(&self) -> &TypeLayout {
		let mut ty = self;
		while ty.is_array() {
			ty = match ty.element_type_layout() {
				Some(t) => t,
				None => break,
			};
		}
		ty
	}

	/// Returns the total number of elements in a (possibly multi-dimensional)
	/// array type layout, or 0 if the underlying type is not an array.
	pub fn total_array_element_count(&self) -> usize {
		self.ty()
			.map(|t| t.total_array_element_count())
			.unwrap_or(0)
	}

	/// Returns the number of elements of this array or vector type layout,
	/// or `None` if the layout has no underlying type.
	pub fn element_count(&self) -> Option<usize> {
		Some(self.ty()?.element_count())
	}

	/// Returns the stride between elements of this array type layout in the
	/// given parameter category, or `SLANG_UNBOUNDED_SIZE` /
	/// `SLANG_UNKNOWN_SIZE` on the same conditions as [`size`](Self::size).
	pub fn element_stride(&self, category: ParameterCategory) -> usize {
		rcall!(spReflectionTypeLayout_GetElementStride(self, category))
	}

	/// Returns the layout of the element type of this array type layout, or
	/// `None` if this is not an array type layout.
	pub fn element_type_layout(&self) -> Option<&TypeLayout> {
		rcall!(spReflectionTypeLayout_GetElementTypeLayout(self) as Option<&TypeLayout>)
	}

	/// Returns the variable layout of the elements of this array type
	/// layout, or `None` if there is none.
	pub fn element_var_layout(&self) -> Option<&VariableLayout> {
		rcall!(spReflectionTypeLayout_GetElementVarLayout(self) as Option<&VariableLayout>)
	}

	/// Returns the variable layout of the container holding this type's data
	/// (e.g. the constant buffer wrapping a structured buffer), or `None` if
	/// there is none.
	pub fn container_var_layout(&self) -> Option<&VariableLayout> {
		rcall!(spReflectionTypeLayout_getContainerVarLayout(self) as Option<&VariableLayout>)
	}

	/// Returns the parameter category that determines how this type is
	/// bound.
	pub fn parameter_category(&self) -> ParameterCategory {
		rcall!(spReflectionTypeLayout_GetParameterCategory(self))
	}

	/// Returns the number of parameter categories this type consumes.
	pub fn category_count(&self) -> u32 {
		rcall!(spReflectionTypeLayout_GetCategoryCount(self))
	}

	/// Returns the parameter category consumed at `index`.
	pub fn category_by_index(&self, index: u32) -> ParameterCategory {
		rcall!(spReflectionTypeLayout_GetCategoryByIndex(self, index))
	}

	/// Iterates over the parameter categories this type consumes.
	pub fn categories(&self) -> impl ExactSizeIterator<Item = ParameterCategory> {
		(0..self.category_count()).map(|i| self.category_by_index(i))
	}

	/// Returns the number of rows of this matrix type layout, or `None` if
	/// the layout has no underlying type.
	pub fn row_count(&self) -> Option<u32> {
		Some(self.ty()?.row_count())
	}

	/// Returns the number of columns of this vector or matrix type layout,
	/// or `None` if the layout has no underlying type.
	pub fn column_count(&self) -> Option<u32> {
		Some(self.ty()?.column_count())
	}

	/// Returns the scalar type this layout is built from, or `None` if the
	/// layout has no underlying type.
	pub fn scalar_type(&self) -> Option<ScalarType> {
		Some(self.ty()?.scalar_type())
	}

	/// Returns the result type of this resource type layout, or `None` if
	/// this is not a resource type layout.
	pub fn resource_result_type(&self) -> Option<&Type> {
		self.ty()?.resource_result_type()
	}

	/// Returns the shape of this resource type layout, or `None` if the
	/// layout has no underlying type.
	pub fn resource_shape(&self) -> Option<ResourceShape> {
		Some(self.ty()?.resource_shape())
	}

	/// Returns the access mode of this resource type layout, or `None` if
	/// the layout has no underlying type.
	pub fn resource_access(&self) -> Option<ResourceAccess> {
		Some(self.ty()?.resource_access())
	}

	/// Returns the name of the underlying type, or `None` if the layout has
	/// no underlying type or the type has no name.
	pub fn name(&self) -> Option<&str> {
		self.ty()?.name()
	}

	/// Returns the matrix layout mode (row- or column-major) of this type
	/// layout.
	pub fn matrix_layout_mode(&self) -> MatrixLayoutMode {
		rcall!(spReflectionTypeLayout_GetMatrixLayoutMode(self))
	}

	/// Returns the index of the generic parameter this type layout stands in
	/// for, or -1 if it is not a generic parameter layout.
	pub fn generic_param_index(&self) -> i32 {
		rcall!(spReflectionTypeLayout_getGenericParamIndex(self))
	}

	/// Deprecated in slang.h: pending type layout functionality has been
	/// removed; always returns `None`.
	pub fn pending_data_type_layout(&self) -> Option<&TypeLayout> {
		rcall!(spReflectionTypeLayout_getPendingDataTypeLayout(self) as Option<&TypeLayout>)
	}

	/// Deprecated in slang.h: pending type layout functionality has been
	/// removed; always returns `None`.
	pub fn specialized_type_pending_data_var_layout(&self) -> Option<&VariableLayout> {
		rcall!(
			spReflectionTypeLayout_getSpecializedTypePendingDataVarLayout(self)
				as Option<&VariableLayout>
		)
	}

	/// Returns the number of binding ranges introduced by this type layout.
	pub fn binding_range_count(&self) -> i64 {
		rcall!(spReflectionTypeLayout_getBindingRangeCount(self))
	}

	/// Returns the binding type of the binding range at `index`.
	pub fn binding_range_type(&self, index: i64) -> BindingType {
		rcall!(spReflectionTypeLayout_getBindingRangeType(self, index))
	}

	/// Returns whether the binding range at `index` can be specialized.
	pub fn is_binding_range_specializable(&self, index: i64) -> bool {
		rcall!(spReflectionTypeLayout_isBindingRangeSpecializable(
			self, index
		)) != 0
	}

	/// Returns the number of bindings in the binding range at `index`, or
	/// `SLANG_UNBOUNDED_SIZE` for unbounded resources / `SLANG_UNKNOWN_SIZE`
	/// when the count depends on unresolved generic parameters or link-time
	/// constants.
	pub fn binding_range_binding_count(&self, index: i64) -> i64 {
		rcall!(spReflectionTypeLayout_getBindingRangeBindingCount(
			self, index
		))
	}

	/// Returns the offset of the binding range used by the field at
	/// `field_index`.
	pub fn field_binding_range_offset(&self, field_index: i64) -> i64 {
		rcall!(spReflectionTypeLayout_getFieldBindingRangeOffset(
			self,
			field_index
		))
	}

	/// Returns the offset of the binding range used by the explicit counter
	/// of this type layout.
	pub fn explicit_counter_binding_range_offset(&self) -> i64 {
		rcall!(spReflectionTypeLayout_getExplicitCounterBindingRangeOffset(
			self
		))
	}

	/// Returns the layout of the leaf type of the binding range at `index`,
	/// or `None` if there is none.
	pub fn binding_range_leaf_type_layout(&self, index: i64) -> Option<&TypeLayout> {
		rcall!(
			spReflectionTypeLayout_getBindingRangeLeafTypeLayout(self, index)
				as Option<&TypeLayout>
		)
	}

	/// Returns the leaf variable of the binding range at `index`, or `None`
	/// if there is none.
	pub fn binding_range_leaf_variable(&self, index: i64) -> Option<&Variable> {
		rcall!(spReflectionTypeLayout_getBindingRangeLeafVariable(self, index) as Option<&Variable>)
	}

	/// Returns the image format of the binding range at `index`.
	pub fn binding_range_image_format(&self, index: i64) -> ImageFormat {
		rcall!(spReflectionTypeLayout_getBindingRangeImageFormat(
			self, index
		))
	}

	/// Returns the index of the descriptor set that the binding range at
	/// `index` belongs to.
	pub fn binding_range_descriptor_set_index(&self, index: i64) -> i64 {
		rcall!(spReflectionTypeLayout_getBindingRangeDescriptorSetIndex(
			self, index
		))
	}

	/// Returns the index of the first descriptor range used by the binding
	/// range at `index`.
	pub fn binding_range_first_descriptor_range_index(&self, index: i64) -> i64 {
		rcall!(spReflectionTypeLayout_getBindingRangeFirstDescriptorRangeIndex(self, index))
	}

	/// Returns the number of descriptor ranges used by the binding range at
	/// `index`.
	pub fn binding_range_descriptor_range_count(&self, index: i64) -> i64 {
		rcall!(spReflectionTypeLayout_getBindingRangeDescriptorRangeCount(
			self, index
		))
	}

	/// Returns the number of descriptor sets used by this type layout.
	pub fn descriptor_set_count(&self) -> i64 {
		rcall!(spReflectionTypeLayout_getDescriptorSetCount(self))
	}

	/// Returns the space offset of the descriptor set at `set_index`.
	pub fn descriptor_set_space_offset(&self, set_index: i64) -> i64 {
		rcall!(spReflectionTypeLayout_getDescriptorSetSpaceOffset(
			self, set_index
		))
	}

	/// Returns the number of descriptor ranges in the descriptor set at
	/// `set_index`.
	pub fn descriptor_set_descriptor_range_count(&self, set_index: i64) -> i64 {
		rcall!(spReflectionTypeLayout_getDescriptorSetDescriptorRangeCount(
			self, set_index
		))
	}

	/// Returns the index offset of the descriptor range at `range_index`
	/// within the descriptor set at `set_index`, or `SLANG_UNKNOWN_SIZE`
	/// when the offset depends on unresolved generic parameters or link-time
	/// constants.
	pub fn descriptor_set_descriptor_range_index_offset(
		&self,
		set_index: i64,
		range_index: i64,
	) -> i64 {
		rcall!(
			spReflectionTypeLayout_getDescriptorSetDescriptorRangeIndexOffset(
				self,
				set_index,
				range_index
			)
		)
	}

	/// Returns the number of descriptors in the descriptor range at
	/// `range_index` within the descriptor set at `set_index`, or
	/// `SLANG_UNBOUNDED_SIZE` for unbounded resources / `SLANG_UNKNOWN_SIZE`
	/// when the count depends on unresolved generic parameters or link-time
	/// constants.
	pub fn descriptor_set_descriptor_range_descriptor_count(
		&self,
		set_index: i64,
		range_index: i64,
	) -> i64 {
		rcall!(
			spReflectionTypeLayout_getDescriptorSetDescriptorRangeDescriptorCount(
				self,
				set_index,
				range_index
			)
		)
	}

	/// Returns the binding type of the descriptor range at `range_index`
	/// within the descriptor set at `set_index`.
	pub fn descriptor_set_descriptor_range_type(
		&self,
		set_index: i64,
		range_index: i64,
	) -> BindingType {
		rcall!(spReflectionTypeLayout_getDescriptorSetDescriptorRangeType(
			self,
			set_index,
			range_index
		))
	}

	/// Returns the parameter category of the descriptor range at
	/// `range_index` within the descriptor set at `set_index`.
	pub fn descriptor_set_descriptor_range_category(
		&self,
		set_index: i64,
		range_index: i64,
	) -> ParameterCategory {
		rcall!(
			spReflectionTypeLayout_getDescriptorSetDescriptorRangeCategory(
				self,
				set_index,
				range_index
			)
		)
	}

	/// Returns the number of sub-object ranges (e.g. entry points in a
	/// shader record) used by this type layout.
	pub fn sub_object_range_count(&self) -> i64 {
		rcall!(spReflectionTypeLayout_getSubObjectRangeCount(self))
	}

	/// Returns the index of the binding range that the sub-object range at
	/// `sub_object_range_index` belongs to.
	pub fn sub_object_range_binding_range_index(&self, sub_object_range_index: i64) -> i64 {
		rcall!(spReflectionTypeLayout_getSubObjectRangeBindingRangeIndex(
			self,
			sub_object_range_index
		))
	}

	/// Returns the space offset of the sub-object range at
	/// `sub_object_range_index`, or `SLANG_UNKNOWN_SIZE` when the offset
	/// depends on unresolved generic parameters or link-time constants.
	pub fn sub_object_range_space_offset(&self, sub_object_range_index: i64) -> i64 {
		rcall!(spReflectionTypeLayout_getSubObjectRangeSpaceOffset(
			self,
			sub_object_range_index
		))
	}

	/// Returns the variable layout describing where the data of the
	/// sub-object range at `sub_object_range_index` is stored.
	pub fn sub_object_range_offset(&self, sub_object_range_index: i64) -> Option<&VariableLayout> {
		rcall!(
			spReflectionTypeLayout_getSubObjectRangeOffset(self, sub_object_range_index)
				as Option<&VariableLayout>
		)
	}

	// Note: the remaining sub-object-range accessors declared in
	// slang-deprecated.h (`..._getSubObjectRangeObjectCount`,
	// `..._getSubObjectRangeTypeLayout`, and the five
	// `..._getSubObjectRangeDescriptorRange*` functions) are inside `#if 0`
	// blocks in both slang-deprecated.h and slang-reflection-api.cpp at Slang
	// v2026.14.1, so they are not exported by the Slang binaries and cannot
	// be bound.
}
