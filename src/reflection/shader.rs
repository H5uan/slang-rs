use super::{
	EntryPoint, Function, Generic, Type, TypeLayout, TypeParameter, Variable, VariableLayout, rcall,
};
use crate::{
	Blob, Error, GenericArg, GenericArgType, IUnknown, Interface, LayoutRules, Result, Session,
	cstring, succeeded, sys,
};

/// Reflection of an entire shader program's layout.
///
/// Mirrors `ShaderReflection` (a.k.a. `ProgramLayout`) in slang.h.
#[repr(transparent)]
pub struct Shader(sys::SlangReflection);

impl Shader {
	/// Returns the number of (global and entry-point) parameters of the
	/// program.
	pub fn parameter_count(&self) -> u32 {
		rcall!(spReflection_GetParameterCount(self))
	}

	/// Returns the layout of the parameter at `index`, or `None` if `index`
	/// is out of range.
	pub fn parameter_by_index(&self, index: u32) -> Option<&VariableLayout> {
		rcall!(spReflection_GetParameterByIndex(self, index) as Option<&VariableLayout>)
	}

	/// Iterates over the layouts of all parameters of the program.
	pub fn parameters(&self) -> impl ExactSizeIterator<Item = &VariableLayout> {
		(0..self.parameter_count()).map(|i| self.parameter_by_index(i).unwrap())
	}

	/// Returns the number of global generic type parameters of the program.
	pub fn type_parameter_count(&self) -> u32 {
		rcall!(spReflection_GetTypeParameterCount(self))
	}

	/// Returns the global generic type parameter at `index`, or `None` if
	/// `index` is out of range.
	pub fn type_parameter_by_index(&self, index: u32) -> Option<&TypeParameter> {
		rcall!(spReflection_GetTypeParameterByIndex(self, index) as Option<&TypeParameter>)
	}

	/// Iterates over the global generic type parameters of the program.
	pub fn type_parameters(&self) -> impl ExactSizeIterator<Item = &TypeParameter> {
		(0..self.type_parameter_count()).map(|i| self.type_parameter_by_index(i).unwrap())
	}

	/// Finds a global generic type parameter by name, or returns `Ok(None)`
	/// if there is no such parameter. Returns `Err` when `name` contains
	/// an interior NUL byte (which cannot be represented in a C string).
	pub fn find_type_parameter_by_name(&self, name: &str) -> Result<Option<&TypeParameter>> {
		let name = cstring(name)?;
		Ok(rcall!(
			spReflection_FindTypeParameter(self, name.as_ptr()) as Option<&TypeParameter>
		))
	}

	/// Returns the number of entry points in the program.
	pub fn entry_point_count(&self) -> u32 {
		rcall!(spReflection_getEntryPointCount(self)) as _
	}

	/// Returns the entry point at `index`, or `None` if `index` is out of
	/// range.
	pub fn entry_point_by_index(&self, index: u32) -> Option<&EntryPoint> {
		rcall!(spReflection_getEntryPointByIndex(self, index as _) as Option<&EntryPoint>)
	}

	/// Iterates over the entry points of the program.
	pub fn entry_points(&self) -> impl ExactSizeIterator<Item = &EntryPoint> {
		(0..self.entry_point_count()).map(|i| self.entry_point_by_index(i).unwrap())
	}

	/// Finds an entry point by name, or returns `Ok(None)` if there is no
	/// such entry point. Returns `Err` when `name` contains an interior
	/// NUL byte.
	pub fn find_entry_point_by_name(&self, name: &str) -> Result<Option<&EntryPoint>> {
		let name = cstring(name)?;
		Ok(rcall!(
			spReflection_findEntryPointByName(self, name.as_ptr()) as Option<&EntryPoint>
		))
	}

	/// Returns the binding index of the global constant buffer, or
	/// `SLANG_UNKNOWN_SIZE` when the binding depends on unresolved generic
	/// parameters or link-time constants.
	pub fn global_constant_buffer_binding(&self) -> u64 {
		rcall!(spReflection_getGlobalConstantBufferBinding(self))
	}

	/// Returns the size in bytes of the global constant buffer, or
	/// `SLANG_UNKNOWN_SIZE` when the size depends on unresolved generic
	/// parameters or link-time constants.
	pub fn global_constant_buffer_size(&self) -> usize {
		rcall!(spReflection_getGlobalConstantBufferSize(self))
	}

	/// Finds a type by name, or returns `Ok(None)` if there is no such
	/// type. Returns `Err` when `name` contains an interior NUL byte.
	pub fn find_type_by_name(&self, name: &str) -> Result<Option<&Type>> {
		let name = cstring(name)?;
		Ok(rcall!(
			spReflection_FindTypeByName(self, name.as_ptr()) as Option<&Type>
		))
	}

	/// Finds a function by name, or returns `Ok(None)` if there is no such
	/// function. Returns `Err` when `name` contains an interior NUL byte.
	pub fn find_function_by_name(&self, name: &str) -> Result<Option<&Function>> {
		let name = cstring(name)?;
		Ok(rcall!(
			spReflection_FindFunctionByName(self, name.as_ptr()) as Option<&Function>
		))
	}

	/// Finds a function by name among the members of `ty`, or returns
	/// `Ok(None)` if there is no such function. Returns `Err` when `name`
	/// contains an interior NUL byte.
	pub fn find_function_by_name_in_type(
		&self,
		ty: &Type,
		name: &str,
	) -> Result<Option<&Function>> {
		let name = cstring(name)?;
		Ok(rcall!(spReflection_FindFunctionByNameInType(
			self,
			ty as *const _ as *mut _,
			name.as_ptr()
		) as Option<&Function>))
	}

	/// Finds a variable by name among the members of `ty`, or returns
	/// `Ok(None)` if there is no such variable. Returns `Err` when `name`
	/// contains an interior NUL byte.
	pub fn find_var_by_name_in_type(&self, ty: &Type, name: &str) -> Result<Option<&Variable>> {
		let name = cstring(name)?;
		Ok(rcall!(
			spReflection_FindVarByNameInType(self, ty as *const _ as *mut _, name.as_ptr())
				as Option<&Variable>
		))
	}

	/// Returns the layout of `ty` under the given layout `rules`, or `None`
	/// if the type cannot be laid out.
	pub fn type_layout(&self, ty: &Type, rules: LayoutRules) -> Option<&TypeLayout> {
		rcall!(
			spReflection_GetTypeLayout(self, ty as *const _ as *mut _, rules)
				as Option<&TypeLayout>
		)
	}

	/// Specializes the generic type `ty` with the given type arguments.
	///
	/// Returns `None` if specialization fails. Mirrors
	/// `ShaderReflection::specializeType` in slang.h, passing a null
	/// diagnostics blob.
	pub fn specialize_type(&self, ty: &Type, specialization_args: &[&Type]) -> Option<&Type> {
		rcall!(spReflection_specializeType(
			self,
			ty as *const _ as *mut _,
			specialization_args.len() as i64,
			specialization_args.as_ptr() as *mut _,
			std::ptr::null_mut()
		) as Option<&Type>)
	}

	/// Specializes the generic declaration `generic` with the given argument
	/// types and values.
	///
	/// Returns `None` if specialization fails. Mirrors
	/// `ShaderReflection::specializeGeneric` in slang.h, passing a null
	/// diagnostics blob.
	pub fn specialize_generic(
		&self,
		generic: &Generic,
		specialization_arg_types: &[GenericArgType],
		specialization_arg_vals: &[GenericArg],
	) -> Option<&Generic> {
		rcall!(spReflection_specializeGeneric(
			self,
			generic as *const _ as *mut _,
			specialization_arg_types.len() as i64,
			specialization_arg_types.as_ptr() as *mut _,
			specialization_arg_vals.as_ptr() as *mut _,
			std::ptr::null_mut()
		) as Option<&Generic>)
	}

	/// Returns whether `sub_type` is a sub-type of `super_type`.
	pub fn is_sub_type(&self, sub_type: &Type, super_type: &Type) -> bool {
		rcall!(spReflection_isSubType(
			self,
			sub_type as *const _ as *mut _,
			super_type as *const _ as *mut _
		))
	}

	/// Returns the number of strings in the program's hashed string table.
	pub fn hashed_string_count(&self) -> u64 {
		rcall!(spReflection_getHashedStringCount(self))
	}

	/// Returns the hashed string at `index`, or `None` if `index` is out of
	/// range.
	pub fn hashed_string(&self, index: u64) -> Option<&str> {
		let mut len = 0;
		let result = rcall!(spReflection_getHashedString(self, index, &mut len));

		(!result.is_null()).then(|| {
			// SAFETY: `result` is non-null (checked above) and points to `len`
			// bytes of a hashed string owned by the reflection, which outlives
			// `&self`.
			let slice = unsafe { std::slice::from_raw_parts(result as *const u8, len) };
			std::str::from_utf8(slice).unwrap()
		})
	}

	/// Iterates over the strings in the program's hashed string table.
	pub fn hashed_strings(&self) -> impl ExactSizeIterator<Item = &str> {
		(0..self.hashed_string_count() as usize).map(|i| self.hashed_string(i as u64).unwrap())
	}

	/// Returns the type layout of the global-scope parameters, or `None` if
	/// the program has no global parameters.
	pub fn global_params_type_layout(&self) -> Option<&TypeLayout> {
		rcall!(spReflection_getGlobalParamsTypeLayout(self) as Option<&TypeLayout>)
	}

	/// Returns the variable layout of the global-scope parameters, or `None`
	/// if the program has no global parameters.
	pub fn global_params_var_layout(&self) -> Option<&VariableLayout> {
		rcall!(spReflection_getGlobalParamsVarLayout(self) as Option<&VariableLayout>)
	}

	/// Resolves an overloaded function to a single candidate.
	///
	/// Deprecated in slang.h (SLANG_DEPRECATED); prefer resolving overloads
	/// through `Function::overloads`.
	#[deprecated(note = "deprecated in slang.h; inspect Function::overloads instead")]
	pub fn try_resolve_overloaded_function(&self, candidates: &[&Function]) -> Option<&Function> {
		rcall!(spReflection_TryResolveOverloadedFunction(
			self,
			candidates.len() as u32,
			candidates.as_ptr() as *mut _
		) as Option<&Function>)
	}

	/// Serializes the program layout to a JSON string.
	///
	/// Mirrors `ShaderReflection::toJson` in slang.h, which passes a null
	/// compile request.
	pub fn to_json(&self) -> Result<String> {
		let mut json = std::ptr::null_mut();
		let result = rcall!(spReflection_ToJson(self, std::ptr::null_mut(), &mut json));

		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		// `outBlob` receives an added reference on success; wrap it immediately
		// so the reference is released when the `Blob` drops.
		let json = std::ptr::NonNull::new(json as *mut _).ok_or(Error::Code(sys::SLANG_FAIL))?;
		let json = Blob(IUnknown(json));
		Ok(String::from_utf8_lossy(json.as_slice()).into_owned())
	}

	/// Returns the descriptor set/space index reserved for the bindless
	/// resource heap, or -1 when no bindless heap space was reserved for the
	/// program layout.
	pub fn bindless_space_index(&self) -> i64 {
		rcall!(spReflection_getBindlessSpaceIndex(self))
	}

	/// Returns the session this program reflection belongs to
	/// (`spReflection_GetSession` in slang-deprecated.h).
	///
	/// The C function hands out a borrowed reference owned by the reflected
	/// program; this method adds a reference, so the returned [`Session`]
	/// keeps the session alive independently of this reflection.
	pub fn session(&self) -> Option<Session> {
		let session = rcall!(spReflection_GetSession(self));
		let session = Session(IUnknown(std::ptr::NonNull::new(session as *mut _)?));
		// SAFETY: `session` wraps a live `slang_ISession` pointer; adding a
		// reference turns the borrowed pointer into an owned one matching the
		// `IUnknown` RAII drop semantics (same pattern as `Session::load_module`).
		unsafe { (session.as_unknown().vtable().ISlangUnknown_addRef)(session.as_raw()) };
		Some(session)
	}
}
