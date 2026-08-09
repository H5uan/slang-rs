use super::{Type, TypeLayout, Variable, rcall};
use crate::{ImageFormat, Modifier, ModifierID, ParameterCategory, Stage, sys};

/// Reflection of a variable together with its binding and layout information.
///
/// Corresponds to `VariableLayoutReflection` in slang.h.
#[repr(transparent)]
pub struct VariableLayout(sys::SlangReflectionVariableLayout);

impl VariableLayout {
	/// Returns the variable this layout describes.
	pub fn variable(&self) -> Option<&Variable> {
		rcall!(spReflectionVariableLayout_GetVariable(self) as Option<&Variable>)
	}

	/// Returns the name of the variable.
	pub fn name(&self) -> Option<&str> {
		self.variable()?.name()
	}

	/// Finds the modifier with the given ID on the variable, returning `None`
	/// if the variable has no such modifier.
	pub fn find_modifier(&self, id: ModifierID) -> Option<&Modifier> {
		self.variable().and_then(|v| v.find_modifier(id))
	}

	/// Returns the layout of the variable's type.
	pub fn type_layout(&self) -> Option<&TypeLayout> {
		rcall!(spReflectionVariableLayout_GetTypeLayout(self) as Option<&TypeLayout>)
	}

	/// Returns the parameter category of the variable's type layout, or `None`
	/// if the variable has no type layout.
	pub fn category(&self) -> Option<ParameterCategory> {
		Some(self.type_layout()?.parameter_category())
	}

	/// Returns the number of parameter categories the variable's type layout
	/// consumes.
	pub fn category_count(&self) -> u32 {
		self.type_layout().map_or(0, |tl| tl.category_count())
	}

	/// Returns the parameter category at `index`, or `None` if the variable has
	/// no type layout.
	pub fn category_by_index(&self, index: u32) -> Option<ParameterCategory> {
		Some(self.type_layout()?.category_by_index(index))
	}

	/// Returns an iterator over the parameter categories the variable's type
	/// layout consumes.
	pub fn categories(&self) -> impl ExactSizeIterator<Item = ParameterCategory> {
		(0..self.category_count()).map(|i| self.category_by_index(i).unwrap())
	}

	/// Returns the offset of the variable in the given parameter category.
	///
	/// Returns `SLANG_UNKNOWN_SIZE` when the offset depends on unresolved
	/// generic parameters or link-time constants.
	pub fn offset(&self, category: ParameterCategory) -> usize {
		rcall!(spReflectionVariableLayout_GetOffset(self, category))
	}

	/// Returns the type of the variable.
	pub fn ty(&self) -> Option<&Type> {
		self.variable()?.ty()
	}

	/// Returns the binding index of the variable.
	///
	/// Returns `SLANG_UNKNOWN_SIZE` when the index depends on unresolved
	/// generic parameters or link-time constants.
	pub fn binding_index(&self) -> u32 {
		rcall!(spReflectionParameter_GetBindingIndex(self))
	}

	/// Returns the binding space (register space / descriptor set) of the
	/// variable.
	///
	/// Returns `SLANG_UNKNOWN_SIZE` when the space depends on unresolved
	/// generic parameters or link-time constants.
	pub fn binding_space(&self) -> u32 {
		rcall!(spReflectionParameter_GetBindingSpace(self))
	}

	/// Returns the register space/set of the variable in the given parameter
	/// category.
	///
	/// Returns `SLANG_UNKNOWN_SIZE` when the space depends on unresolved
	/// generic parameters or link-time constants.
	pub fn binding_space_with_category(&self, category: ParameterCategory) -> usize {
		rcall!(spReflectionVariableLayout_GetSpace(self, category))
	}

	/// Returns the image format of a resource-typed variable.
	pub fn image_format(&self) -> ImageFormat {
		rcall!(spReflectionVariableLayout_GetImageFormat(self))
	}

	/// Returns the name of the semantic applied to the variable, or `None` if
	/// it has none.
	pub fn semantic_name(&self) -> Option<&str> {
		rcall!(spReflectionVariableLayout_GetSemanticName(self) as Option<&str>)
	}

	/// Returns the index of the semantic applied to the variable.
	pub fn semantic_index(&self) -> usize {
		rcall!(spReflectionVariableLayout_GetSemanticIndex(self))
	}

	/// Returns the shader stage the variable is used in.
	pub fn stage(&self) -> Stage {
		rcall!(spReflectionVariableLayout_getStage(self))
	}

	/// Deprecated in slang.h: the pending type layout functionality has been
	/// removed and this always returns `None`.
	pub fn pending_data_layout(&self) -> Option<&VariableLayout> {
		rcall!(spReflectionVariableLayout_getPendingDataLayout(self) as Option<&VariableLayout>)
	}
}
