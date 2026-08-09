//! Zero-overhead borrowed wrappers over Slang's program reflection API (the
//! `SlangReflection*` types and `spReflection*` functions in slang.h).

mod decl;
mod entry_point;
mod function;
mod generic;
mod shader;
mod ty;
mod type_layout;
mod type_parameter;
mod user_attribute;
mod variable;
mod variable_layout;

pub use decl::Decl;
pub use entry_point::EntryPoint;
pub use function::Function;
pub use generic::Generic;
pub use shader::Shader;
pub use ty::Type;
pub use type_layout::TypeLayout;
pub use type_parameter::TypeParameter;
pub use user_attribute::UserAttribute;
pub use variable::Variable;
pub use variable_layout::VariableLayout;

use super::{Modifier, sys};

/// Computes the hash Slang uses for a string, matching the hash embedded in
/// compiled modules (`spComputeStringHash` in slang.h).
pub fn compute_string_hash(string: &str) -> u32 {
	rcall!(spComputeStringHash(string, string.len()))
}

macro_rules! rcall {
	($f:ident($s:ident $(,$arg:expr)*)) => {
		// SAFETY: `$s` is a live reflection object and the remaining arguments
		// are valid at each call site.
		unsafe { sys::$f($s as *const _ as *mut _ $(,$arg)*) }
	};

	($f:ident($s:ident $(,$arg:expr)*) as Option<&str>) => {
		unsafe {
			let ptr = sys::$f($s as *const _ as *mut _ $(,$arg)*);
			(!ptr.is_null()).then(|| {
				// SAFETY: `ptr` is non-null (checked above) and points to a
				// NUL-terminated string owned by the reflection object, which
				// outlives the `&self` borrow this expands within.
				std::ffi::CStr::from_ptr(ptr).to_str().ok()
			}).flatten()
		}
	};

	($f:ident($s:ident $(,$arg:expr)*) as Option<&$cast:ty>) => {
		// SAFETY: `$s` is a live reflection object; the returned pointer (if
		// any) is owned by it, so tying the borrow to `$s` via `ref_from_ptr`
		// is sound.
		unsafe {
			let ptr = sys::$f($s as *const _ as *mut _ $(,$arg)*);
			super::ref_from_ptr::<_, $cast>(ptr)
		}
	};
}

pub(super) use rcall;

/// Trait to associate wrapper types with their underlying system types.
/// This ensures conversions from raw pointers to wrapper types, as performed by the rcall! macro, are type-safe.
///
/// # Safety
///
/// Implementors must be `repr(transparent)` newtypes over `SysType`, so that
/// casting a `*mut SysType` to a `*const Self` (as `ref_from_ptr` does)
/// produces a pointer with a valid layout.
pub(super) unsafe trait Wrapper {
	type SysType;
}

unsafe impl Wrapper for Decl {
	type SysType = sys::SlangReflectionDecl;
}
unsafe impl Wrapper for EntryPoint {
	type SysType = sys::SlangReflectionEntryPoint;
}
unsafe impl Wrapper for Function {
	type SysType = sys::SlangReflectionFunction;
}
unsafe impl Wrapper for Generic {
	type SysType = sys::SlangReflectionGeneric;
}
unsafe impl Wrapper for Modifier {
	type SysType = sys::SlangReflectionModifier;
}
unsafe impl Wrapper for Shader {
	type SysType = sys::SlangReflection;
}
unsafe impl Wrapper for Type {
	type SysType = sys::SlangReflectionType;
}
unsafe impl Wrapper for TypeLayout {
	type SysType = sys::SlangReflectionTypeLayout;
}
unsafe impl Wrapper for TypeParameter {
	type SysType = sys::SlangReflectionTypeParameter;
}
unsafe impl Wrapper for UserAttribute {
	type SysType = sys::SlangReflectionUserAttribute;
}
unsafe impl Wrapper for Variable {
	type SysType = sys::SlangReflectionVariable;
}
unsafe impl Wrapper for VariableLayout {
	type SysType = sys::SlangReflectionVariableLayout;
}

/// Converts a raw Slang reflection pointer into a borrowed wrapper reference.
///
/// This helper is deliberately private to the `reflection` module: its free
/// lifetime `'a` is only sound when the caller ties it to the object that owns
/// the reflection data. Every call site is a `&self` method on a reflection
/// wrapper (via `rcall!`), so lifetime elision binds `'a` to that `&self`
/// borrow and the returned reference can never outlive its owner.
unsafe fn ref_from_ptr<'a, S, W>(ptr: *mut S) -> Option<&'a W>
where
	W: Wrapper<SysType = S>,
{
	(!ptr.is_null()).then(|| {
		// SAFETY: `ptr` is non-null (checked above) and points to a live
		// `SysType`; `W` is `repr(transparent)` over `SysType` per the `Wrapper`
		// safety contract. The caller guarantees the pointee outlives `'a`.
		unsafe { &*(ptr as *const W) }
	})
}
