use super::{Function, Generic, Type, Variable, rcall};
use crate::{DeclKind, Modifier, ModifierID, sys};

/// Reflection of a declaration in shader code (`DeclReflection` in slang.h).
#[repr(transparent)]
pub struct Decl(sys::SlangReflectionDecl);

impl Decl {
	/// Returns the name of this declaration, or `None` if it is anonymous.
	pub fn name(&self) -> Option<&str> {
		let name = rcall!(spReflectionDecl_getName(self));
		(!name.is_null()).then(|| {
			// SAFETY: `name` is non-null (checked above) and points to a
			// NUL-terminated string owned by the declaration, which outlives
			// `&self`.
			unsafe { std::ffi::CStr::from_ptr(name) }.to_str().unwrap()
		})
	}

	/// Returns the kind of this declaration (`DeclReflection::Kind` in slang.h).
	pub fn kind(&self) -> DeclKind {
		rcall!(spReflectionDecl_getKind(self))
	}

	/// Returns the number of direct children of this declaration.
	pub fn child_count(&self) -> u32 {
		rcall!(spReflectionDecl_getChildrenCount(self))
	}

	/// Returns the child declaration at `index`, or `None` if `index` is out of
	/// range.
	pub fn child_by_index(&self, index: u32) -> Option<&Decl> {
		rcall!(spReflectionDecl_getChild(self, index) as Option<&Decl>)
	}

	/// Returns an iterator over the direct children of this declaration.
	pub fn children(&self) -> impl ExactSizeIterator<Item = &Decl> {
		(0..self.child_count()).map(|i| self.child_by_index(i).unwrap())
	}

	/// Returns the type introduced or used by this declaration
	/// (`spReflection_getTypeFromDecl` in slang.h), or `None` if the
	/// declaration has no associated type.
	pub fn ty(&self) -> Option<&Type> {
		rcall!(spReflection_getTypeFromDecl(self) as Option<&Type>)
	}

	/// Returns this declaration viewed as a variable, or `None` if it is not a
	/// variable declaration.
	pub fn as_variable(&self) -> Option<&Variable> {
		rcall!(spReflectionDecl_castToVariable(self) as Option<&Variable>)
	}

	/// Returns this declaration viewed as a function, or `None` if it is not a
	/// function declaration.
	pub fn as_function(&self) -> Option<&Function> {
		rcall!(spReflectionDecl_castToFunction(self) as Option<&Function>)
	}

	/// Returns this declaration viewed as a generic, or `None` if it is not a
	/// generic declaration.
	pub fn as_generic(&self) -> Option<&Generic> {
		rcall!(spReflectionDecl_castToGeneric(self) as Option<&Generic>)
	}

	/// Returns the parent of this declaration, or `None` if it has none.
	pub fn parent(&self) -> Option<&Decl> {
		rcall!(spReflectionDecl_getParent(self) as Option<&Decl>)
	}

	/// Finds a modifier on this declaration by ID, e.g. an `[unroll]` or
	/// `[shader(...)]` attribute. Returns `None` when the declaration does not
	/// carry the modifier.
	pub fn find_modifier(&self, id: ModifierID) -> Option<&Modifier> {
		rcall!(spReflectionDecl_findModifier(self, id) as Option<&Modifier>)
	}
}
