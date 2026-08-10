//! Rust bindings for the [Slang](https://github.com/shader-slang/slang/) shader
//! language compiler, covering the compilation and reflection APIs of Slang
//! v2026.14.1.
//!
//! Slang exposes a COM-style API (`slang.h`); this crate wraps it in safe,
//! reference-counted RAII types ([`GlobalSession`], [`Session`], [`Module`],
//! [`ComponentType`], [`EntryPoint`], [`Blob`], [`SharedLibrary`]) plus a
//! zero-cost [`reflection`] module for inspecting compiled shaders. All
//! wrapper types own a COM reference and release it on drop.
//!
//! # Quick start
//!
//! Compile a Slang module to SPIR-V:
//!
//! ```no_run
//! let global_session = shader_slang_rs::GlobalSession::new().unwrap();
//!
//! let target_desc = shader_slang_rs::TargetDesc::default()
//!     .format(shader_slang_rs::CompileTarget::Spirv)
//!     .profile(global_session.find_profile("glsl_450"));
//! let targets = [target_desc];
//!
//! let session_desc = shader_slang_rs::SessionDesc::default().targets(&targets);
//! let session = global_session.create_session(&session_desc).unwrap();
//!
//! let module = session.load_module("my-shader.slang").unwrap();
//! let entry_point = module.find_entry_point_by_name("main").unwrap();
//!
//! let program = session
//!     .create_composite_component_type(&[module.into(), entry_point.into()])
//!     .unwrap();
//! let linked_program = program.link().unwrap();
//!
//! let spirv = linked_program.entry_point_code(0, 0).unwrap();
//! println!("generated {} bytes of SPIR-V", spirv.as_slice().len());
//! ```
//!
//! The example is marked `no_run` because it requires the Slang shared library
//! (`slang.dll` / `libslang.so`) and a shader module on disk at run time; see
//! the `examples/` directory for runnable end-to-end programs.
//!
//! By default the build downloads the prebuilt Slang v2026.14.1 binaries for
//! your platform; see the README for build options (`source-build` feature,
//! `SLANG_DIR` overrides).

#![warn(missing_docs)]

/// Zero-cost wrappers around Slang's reflection API (`SlangReflection*` in
/// slang.h) for inspecting compiled shaders: types, variables, entry points,
/// and layout information.
pub mod reflection;

mod file_system;
pub use file_system::{
	FileSystem, FileSystemError, FileSystemExt, FileSystemObject, WritableFileSystem,
};

#[cfg(test)]
mod tests;

use std::ffi::{CStr, CString, c_char, c_void};
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) use shader_slang_rs_sys as sys;

pub use sys::{
	OSPathKind, PathKind, SlangArchiveType as ArchiveType, SlangBindingType as BindingType,
	SlangCompileTarget as CompileTarget, SlangDebugInfoLevel as DebugInfoLevel,
	SlangDeclKind as DeclKind, SlangFloatingPointMode as FloatingPointMode,
	SlangImageFormat as ImageFormat, SlangLayoutRules as LayoutRules,
	SlangLineDirectiveMode as LineDirectiveMode, SlangMatrixLayoutMode as MatrixLayoutMode,
	SlangModifierID as ModifierID, SlangOptimizationLevel as OptimizationLevel,
	SlangParameterCategory as ParameterCategory, SlangPassThrough as PassThrough,
	SlangPathType as PathType, SlangReflectionGenericArg as GenericArg,
	SlangReflectionGenericArgType as GenericArgType, SlangResourceAccess as ResourceAccess,
	SlangResourceShape as ResourceShape, SlangScalarType as ScalarType,
	SlangSourceLanguage as SourceLanguage, SlangStage as Stage, SlangTargetFlags as TargetFlags,
	SlangTypeKind as TypeKind, SlangUUID as UUID, slang_BuiltinModuleName as BuiltinModuleName,
	slang_ByteCodeFuncInfo as ByteCodeFuncInfo,
	slang_CompileCoreModuleFlag_Enum as CompileCoreModuleFlag,
	slang_CompileCoreModuleFlags as CompileCoreModuleFlags,
	slang_CompilerOptionName as CompilerOptionName, slang_ContainerType as ContainerType,
	slang_CooperativeMatrixCombination as CooperativeMatrixCombination,
	slang_CooperativeMatrixType as CooperativeMatrixType,
	slang_CooperativeVectorCombination as CooperativeVectorCombination,
	slang_CooperativeVectorTypeUsageInfo as CooperativeVectorTypeUsageInfo,
	slang_CoverageBranchArmKind as CoverageBranchArmKind,
	slang_CoverageCounterMode as CoverageCounterMode, slang_CoverageEntryKind as CoverageEntryKind,
	slang_Modifier as Modifier, slang_SessionFlags as SessionFlags,
	slang_SyntheticResourceAccess as SyntheticResourceAccess,
	slang_SyntheticResourceScope as SyntheticResourceScope,
};

macro_rules! vcall {
	($self:expr, $method:ident($($args:expr),*)) => {
		// SAFETY: the call goes through the COM vtable of a live interface
		// pointer whose layout is guaranteed by the `Interface` safety
		// contract; argument validity is the call site's responsibility.
		unsafe { ($self.vtable().$method)($self.as_raw(), $($args),*) }
	};
	($self:expr, $($base:ident).+, $method:ident($($args:expr),*)) => {
		// SAFETY: the call goes through the COM vtable of a live interface
		// pointer whose layout is guaranteed by the `Interface` safety
		// contract; the method lives on a base interface reached through the
		// `_base` chain. Argument validity is the call site's responsibility.
		unsafe { ($self.vtable()$(.$base)+.$method)($self.as_raw(), $($args),*) }
	};
}

pub(crate) const fn uuid(uuid: u128) -> UUID {
	UUID {
		data1: (uuid >> 96) as u32,
		data2: ((uuid >> 80) & 0xffff) as u16,
		data3: ((uuid >> 64) & 0xffff) as u16,
		data4: (uuid as u64).to_be_bytes(),
	}
}

/// Error type returned by fallible operations in this crate.
pub enum Error {
	/// A raw Slang result code (`SlangResult`) when Slang reported failure
	/// without a diagnostics blob.
	Code(sys::SlangResult),
	/// A Slang diagnostics blob holding the compiler's error messages.
	Blob(Blob),
}

impl std::fmt::Debug for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Error::Code(code) => write!(f, "{}", code),
			Error::Blob(blob) => write!(f, "{}", blob.as_str().unwrap_or_default()),
		}
	}
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Debug::fmt(self, f)
	}
}

unsafe impl Send for Error {}
unsafe impl Sync for Error {}
impl std::error::Error for Error {}

/// Result type used throughout this crate; see [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn succeeded(result: sys::SlangResult) -> bool {
	result >= 0
}

fn result_from_blob(code: sys::SlangResult, blob: *mut sys::slang_IBlob) -> Result<()> {
	// Take ownership of the diagnostics blob regardless of the result: Slang
	// may hand back a non-null blob even on success (warnings), and it must
	// always be released. Warnings are silently discarded for now.
	let blob = std::ptr::NonNull::new(blob as *mut _).map(|ptr| Blob(IUnknown(ptr)));

	if code < 0 {
		Err(match blob {
			Some(blob) => Error::Blob(blob),
			None => Error::Code(code),
		})
	} else {
		Ok(())
	}
}

/// Builds an error from a Slang diagnostics blob pointer, falling back to a
/// generic failure code when Slang did not provide diagnostics.
fn error_from_diagnostics(diagnostics: *mut sys::slang_IBlob) -> Error {
	match std::ptr::NonNull::new(diagnostics as *mut _) {
		Some(diagnostics) => Error::Blob(Blob(IUnknown(diagnostics))),
		None => Error::Code(sys::SLANG_FAIL),
	}
}

/// Converts a possibly-null C string pointer returned by Slang into an
/// `Option<&str>`. Non-UTF-8 strings also yield `None`.
///
/// SAFETY: `ptr` must be null or point to a valid NUL-terminated string that
/// outlives `'a`.
pub(crate) unsafe fn str_from_slang<'a>(ptr: *const i8) -> Option<&'a str> {
	if ptr.is_null() {
		return None;
	}
	// SAFETY: upheld by the caller.
	unsafe { CStr::from_ptr(ptr) }.to_str().ok()
}

/// Gets the build version tag string of this Slang build, as produced by
/// `git describe --tags` (`spGetBuildTagString` in slang.h). Unlike
/// [`GlobalSession::build_tag_string`], this does not require creating a
/// global session. Returns `None` when the string is unavailable or not valid
/// UTF-8.
pub fn build_tag_string() -> Option<&'static str> {
	// SAFETY: `spGetBuildTagString` returns a pointer to a static string owned
	// by the Slang library, valid for as long as the library stays loaded
	// (which outlives any use through this crate).
	unsafe { str_from_slang(sys::spGetBuildTagString()) }
}

/// The internal ID of a compilation profile, as looked up by
/// [`GlobalSession::find_profile`] (`SlangProfileID` in slang.h).
///
/// Profile IDs are not guaranteed to be stable across versions of the Slang
/// library, so look profiles up by name at runtime instead of hardcoding IDs.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProfileID(sys::SlangProfileID);

impl ProfileID {
	/// The unknown profile ID (`SlangProfileUnknown` in slang.h).
	pub const UNKNOWN: ProfileID = ProfileID(sys::SlangProfileID_SlangProfileUnknown);

	/// Returns whether this is the unknown profile ID.
	pub fn is_unknown(&self) -> bool {
		self.0 == sys::SlangProfileID_SlangProfileUnknown
	}
}

/// The internal ID of a capability, as looked up by
/// [`GlobalSession::find_capability`] (`SlangCapabilityID` in slang.h).
#[derive(Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CapabilityID(sys::SlangCapabilityID);

impl CapabilityID {
	/// The unknown capability ID (`SlangCapabilityUnknown` in slang.h).
	pub const UNKNOWN: CapabilityID = CapabilityID(sys::SlangCapabilityID_SlangCapabilityUnknown);

	/// Returns whether this is the unknown capability ID.
	pub fn is_unknown(&self) -> bool {
		self.0 == sys::SlangCapabilityID_SlangCapabilityUnknown
	}
}

/// Trait implemented by the RAII wrappers around Slang's COM interfaces.
///
/// # Safety
///
/// Implementors must be `repr(transparent)` newtypes over a (possibly nested)
/// non-null COM interface pointer whose object exposes a vtable with exactly
/// the layout of `Self::Vtable` — the same methods, in declaration order, as
/// the corresponding interface in slang.h. `IID` must be the interface's
/// canonical UUID, and the `Clone` implementation must perform the COM
/// `addRef` that balances the `release` in `Drop`.
pub unsafe trait Interface: Sized + Clone {
	#[doc(hidden)]
	type Vtable;

	/// The canonical UUID of the COM interface, used by
	/// [`IUnknown::query_interface`].
	const IID: UUID;

	#[doc(hidden)]
	#[inline(always)]
	unsafe fn vtable(&self) -> &Self::Vtable {
		// SAFETY: per the `Interface` safety contract, the wrapped COM object
		// exposes a vtable with exactly the layout of `Self::Vtable`.
		unsafe { &**(self.as_raw::<*mut Self::Vtable>()) }
	}

	#[doc(hidden)]
	#[inline(always)]
	unsafe fn as_raw<T>(&self) -> *mut T {
		// SAFETY: per the `Interface` safety contract, `Self` is
		// `repr(transparent)` over the COM interface pointer, so copying its
		// bits out as a raw pointer is valid; `self` keeps owning the
		// reference.
		unsafe { std::mem::transmute_copy(self) }
	}

	/// Views this interface wrapper as the base [`IUnknown`]. Every Slang COM
	/// interface inherits `ISlangUnknown`, so this conversion always succeeds.
	fn as_unknown(&self) -> &IUnknown {
		// SAFETY: It is always safe to treat an `Interface` as an `IUnknown`.
		unsafe { std::mem::transmute(self) }
	}
}

/// An owned reference to a Slang COM object (`ISlangUnknown` in slang.h),
/// released on drop. This is the base of every interface wrapper in this
/// crate; use [`IUnknown::query_interface`] to cast to another interface.
#[repr(transparent)]
pub struct IUnknown(std::ptr::NonNull<std::ffi::c_void>);

unsafe impl Interface for IUnknown {
	type Vtable = sys::ISlangUnknown__bindgen_vtable;
	const IID: UUID = uuid(0x0000_0000_0000_0000_c000_0000_0000_0046);
}

impl Clone for IUnknown {
	fn clone(&self) -> Self {
		vcall!(self, ISlangUnknown_addRef());
		Self(self.0)
	}
}

impl Drop for IUnknown {
	fn drop(&mut self) {
		vcall!(self, ISlangUnknown_release());
	}
}

// SAFETY: `IUnknown` wraps a COM interface pointer. COM's `AddRef`/`Release`
// and `QueryInterface` are specified as thread-safe. Slang's own interface
// implementations are likewise safe to use from shared `&` references.
// Ownership of the pointer can be transferred between threads.
unsafe impl Send for IUnknown {}
unsafe impl Sync for IUnknown {}

impl IUnknown {
	/// Queries this object for the interface `T`, returning an owned reference
	/// on success. Mirrors COM `IUnknown::QueryInterface`: returns `None` when
	/// the object does not implement `T` or Slang reports failure.
	///
	/// The result is deliberately an `Option` rather than infallible: a failed
	/// cast is an ordinary outcome, not an error worth a diagnostics blob, and
	/// Slang signals it with a null out-pointer / `SLANG_E_NOT_AVAILABLE`.
	pub fn query_interface<T: Interface>(&self) -> Option<T> {
		let mut object = null_mut();
		let result = vcall!(self, ISlangUnknown_queryInterface(&T::IID, &mut object));
		if !succeeded(result) || object.is_null() {
			return None;
		}
		// `queryInterface` hands out a new reference; the returned wrapper takes
		// ownership of it. Slang guarantees the returned object exposes the
		// interface identified by `T::IID` at the returned pointer value.
		let object = IUnknown(std::ptr::NonNull::new(object)?);
		// SAFETY: see above.
		Some(unsafe { upcast(object) })
	}
}

/// Upcasts an interface wrapper to a wrapper around another interface the
/// underlying COM object exposes at the same pointer value (e.g.
/// `EntryPoint` -> `ComponentType`), transferring ownership of the reference.
///
/// SAFETY: the caller must guarantee that the COM object behind `value`
/// exposes `O`'s interface at the same pointer value — typically because
/// `I`'s interface inherits `O`'s interface in slang.h. Both wrappers are
/// `repr(transparent)` over the interface pointer per the `Interface` safety
/// contract.
unsafe fn upcast<I: Interface, O: Interface>(value: I) -> O {
	// `ManuallyDrop` keeps the source wrapper from releasing the reference;
	// ownership moves into the returned wrapper instead.
	let value = std::mem::ManuallyDrop::new(value);
	// SAFETY: per the function's safety contract.
	unsafe { std::mem::transmute_copy(&value) }
}

/// An owned reference to a Slang blob (`ISlangBlob` in slang.h): an immutable
/// byte buffer produced by the compiler, e.g. compiled code or diagnostics.
#[repr(transparent)]
#[derive(Clone)]
pub struct Blob(IUnknown);

unsafe impl Interface for Blob {
	type Vtable = sys::IBlobVtable;
	const IID: UUID = uuid(0x8ba5_fb08_5195_40e2_ac58_0d98_9c3a_0102);
}

impl Blob {
	/// Creates a Slang blob owning a copy of `data` (`slang_createBlob`).
	/// Panics when Slang fails to allocate the blob.
	pub fn new(data: &[u8]) -> Blob {
		// SAFETY: `data` points to `data.len()` readable bytes;
		// `slang_createBlob` copies them into the new blob and returns a new
		// reference owned by the caller.
		let blob = unsafe { sys::slang_createBlob(data.as_ptr() as *const c_void, data.len()) };
		Blob(IUnknown(
			std::ptr::NonNull::new(blob as *mut _).expect("slang_createBlob returned null"),
		))
	}

	/// Returns the blob's contents as a byte slice.
	pub fn as_slice(&self) -> &[u8] {
		let ptr = vcall!(self, getBufferPointer());
		let size = vcall!(self, getBufferSize());
		// SAFETY: a live `ISlangBlob` guarantees `getBufferPointer` points to
		// `getBufferSize` readable bytes owned by the blob, which outlives
		// `&self`.
		unsafe { std::slice::from_raw_parts(ptr as *const u8, size) }
	}

	/// Returns the blob's contents as a string slice. Returns
	/// `Err(Utf8Error)` when the contents are not valid UTF-8.
	pub fn as_str(&self) -> std::result::Result<&str, std::str::Utf8Error> {
		std::str::from_utf8(self.as_slice())
	}
}

/// An owned reference to a Slang shared library (`ISlangSharedLibrary` in
/// slang.h): an interface to executable code, produced by compiling with a
/// CPU target such as [`CompileTarget::ShaderHostCallable`]. Obtained from
/// [`ComponentType::entry_point_host_callable`] or
/// [`ComponentType2::target_host_callable`]; the compiled code stays valid
/// for as long as this object is alive.
#[repr(transparent)]
#[derive(Clone)]
pub struct SharedLibrary(IUnknown);

unsafe impl Interface for SharedLibrary {
	type Vtable = sys::ISlangSharedLibraryVtable;
	const IID: UUID = uuid(0x70db_c7c4_dc3b_4a07_ae7e_752a_f6a8_1555);
}

impl SharedLibrary {
	/// Finds the address of a symbol (a function or variable) exported by the
	/// compiled code (`ISlangSharedLibrary::findSymbolAddressByName`; slang.h
	/// also offers the `findFuncByName` convenience wrapper, which is an
	/// inline cast of this method). Returns `None` when no symbol with `name`
	/// exists. The returned pointer is valid for as long as this
	/// `SharedLibrary` is alive; interpreting it (e.g. casting to a function
	/// pointer with the correct ABI) is up to the caller.
	pub fn find_symbol(&self, name: &str) -> Option<*mut std::ffi::c_void> {
		let name = CString::new(name).unwrap();
		let symbol = vcall!(self, findSymbolAddressByName(name.as_ptr()));
		(!symbol.is_null()).then_some(symbol)
	}
}

/// An owned reference to a Slang clonable object (`ISlangClonable` in slang.h),
/// which can clone itself for any supported interface GUID.
#[repr(transparent)]
#[derive(Clone)]
pub struct Clonable(IUnknown);

unsafe impl Interface for Clonable {
	type Vtable = sys::ISlangClonableVtable;
	const IID: UUID = uuid(0x1ec3_6168_e9f4_430d_bb17_048a_8046_b31f);
}

impl Clonable {
	/// Clones the object for the specified interface `T` (`ISlangClonable::clone`).
	/// Returns `None` when the object does not support the requested interface.
	pub fn clone_as<T: Interface>(&self) -> Option<T> {
		let object = vcall!(self, clone(&T::IID));
		let object = std::ptr::NonNull::new(object)?;
		// `clone` returns a non-refcounted (borrowed) pointer; take a reference
		// so the returned wrapper owns it.
		let object = IUnknown(object);
		// SAFETY: the object is guaranteed by Slang to be a valid `T` pointer.
		unsafe { (object.vtable().ISlangUnknown_addRef)(object.as_raw()) };
		Some(unsafe { std::ptr::read(&object as *const _ as *const T) })
	}
}

/// An owned reference to a Slang writer stream (`ISlangWriter` in slang.h),
/// used for outputting diagnostic and other information.
#[repr(transparent)]
#[derive(Clone)]
pub struct Writer(IUnknown);

unsafe impl Interface for Writer {
	type Vtable = sys::ISlangWriterVtable;
	const IID: UUID = uuid(0xec45_7f0e_9add_4e6b_851c_d7fa_716d_15fd);
}

impl Writer {
	/// Begins an append buffer (`ISlangWriter::beginAppendBuffer`).
	/// Only one append buffer can be active at a time.
	pub fn begin_append_buffer(&self, max_num_chars: usize) -> *mut u8 {
		vcall!(self, beginAppendBuffer(max_num_chars)) as *mut u8
	}

	/// Ends the append buffer and writes its content (`ISlangWriter::endAppendBuffer`).
	pub fn end_append_buffer(&self, buffer: &[u8]) -> Result<()> {
		let result = vcall!(self, endAppendBuffer(buffer.as_ptr() as *mut i8, buffer.len()));
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Writes text to the writer (`ISlangWriter::write`).
	pub fn write(&self, chars: &[u8]) -> Result<()> {
		let result = vcall!(self, write(chars.as_ptr() as *const i8, chars.len()));
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Flushes any content to the output (`ISlangWriter::flush`).
	pub fn flush(&self) {
		vcall!(self, flush());
	}

	/// Returns whether this writer is a console writer (`ISlangWriter::isConsole`).
	pub fn is_console(&self) -> bool {
		vcall!(self, isConsole())
	}

	/// Sets the mode for the writer (`ISlangWriter::setMode`).
	pub fn set_mode(&self, mode: sys::SlangWriterMode) -> Result<()> {
		let result = vcall!(self, setMode(mode));
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}
}

/// An owned reference to a Slang profiler (`ISlangProfiler` in slang.h),
/// providing performance profiling data from the compiler.
#[repr(transparent)]
#[derive(Clone)]
pub struct Profiler(IUnknown);

unsafe impl Interface for Profiler {
	type Vtable = sys::ISlangProfilerVtable;
	const IID: UUID = uuid(0x1977_72c7_0155_4b91_84e8_6668_baff_0619);
}

impl Profiler {
	/// Gets the number of profiling entries (`ISlangProfiler::getEntryCount`).
	pub fn entry_count(&self) -> usize {
		vcall!(self, getEntryCount())
	}

	/// Gets the name of the profiling entry at `index` (`ISlangProfiler::getEntryName`).
	/// Returns `None` when the index is out of range or the name is not valid UTF-8.
	pub fn entry_name(&self, index: u32) -> Option<&str> {
		let name = vcall!(self, getEntryName(index));
		unsafe { str_from_slang(name) }
	}

	/// Gets the time in milliseconds for the profiling entry at `index`
	/// (`ISlangProfiler::getEntryTimeMS`).
	pub fn entry_time_ms(&self, index: u32) -> i32 {
		vcall!(self, getEntryTimeMS(index))
	}

	/// Gets the number of invocation times for the profiling entry at `index`
	/// (`ISlangProfiler::getEntryInvocationTimes`).
	pub fn entry_invocation_times(&self, index: u32) -> u32 {
		vcall!(self, getEntryInvocationTimes(index))
	}
}

/// An owned reference to a Slang mutable file system
/// (`ISlangMutableFileSystem` in slang.h): a (real or virtual) file system
/// with path management (`ISlangFileSystemExt`) and write operations.
/// Obtained from [`ComponentType::get_result_as_file_system`], which exposes
/// the compilation outputs as files in an in-memory file system.
///
/// All paths are UTF-8 strings; path-valued results are returned as [`Blob`]s
/// holding the zero-terminated string, as slang.h specifies.
#[repr(transparent)]
#[derive(Clone)]
pub struct MutableFileSystem(IUnknown);

unsafe impl Interface for MutableFileSystem {
	type Vtable = sys::ISlangMutableFileSystemVtable;
	// `ISlangMutableFileSystem` IID from slang.h:
	// A058675C-1D65-452A-8458-CCDED1427105 (note the final byte is 0x05).
	const IID: UUID = uuid(0xa058_675c_1d65_452a_8458_ccde_d142_7105);
}

impl MutableFileSystem {
	/// Loads the file at `path` and returns its exact bytes
	/// (`ISlangFileSystem::loadFile`).
	pub fn load_file(&self, path: &str) -> Result<Blob> {
		let path = CString::new(path).unwrap();
		let mut blob = null_mut();
		let result = vcall!(self, _base._base, loadFile(path.as_ptr(), &mut blob));
		// `loadFile` takes no diagnostics out-pointer; the result code is the
		// only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(blob as *mut _).unwrap(),
		)))
	}

	/// Returns a string that uniquely identifies the object at `path`
	/// (`ISlangFileSystemExt::getFileUniqueIdentity`). Two paths may only
	/// report the same identity when their contents are identical; Slang uses
	/// the identity for source caching and `#pragma once`.
	pub fn file_unique_identity(&self, path: &str) -> Result<Blob> {
		let path = CString::new(path).unwrap();
		let mut identity = null_mut();
		let result = vcall!(
			self,
			_base,
			getFileUniqueIdentity(path.as_ptr(), &mut identity)
		);
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(identity as *mut _).unwrap(),
		)))
	}

	/// Combines `from_path` with `path` into a single path
	/// (`ISlangFileSystemExt::calcCombinedPath`). `from_path_type` tells the
	/// file system whether to interpret `from_path` as a file (combine
	/// relative to its directory) or as a directory.
	pub fn calc_combined_path(
		&self,
		from_path_type: PathType,
		from_path: &str,
		path: &str,
	) -> Result<Blob> {
		let from_path = CString::new(from_path).unwrap();
		let path = CString::new(path).unwrap();
		let mut combined = null_mut();
		let result = vcall!(
			self,
			_base,
			calcCombinedPath(
				from_path_type,
				from_path.as_ptr(),
				path.as_ptr(),
				&mut combined
			)
		);
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(combined as *mut _).unwrap(),
		)))
	}

	/// Returns whether `path` names a file or a directory
	/// (`ISlangFileSystemExt::getPathType`). Returns `Err` when the path does
	/// not exist on this file system.
	pub fn path_type(&self, path: &str) -> Result<PathType> {
		let path = CString::new(path).unwrap();
		let mut path_type = PathType::File;
		let result = vcall!(self, _base, getPathType(path.as_ptr(), &mut path_type));
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(path_type)
	}

	/// Returns `path` converted to the requested `kind`
	/// (`ISlangFileSystemExt::getPath`), e.g. simplified or canonicalized.
	/// Returns `Err` when the file system does not support the conversion.
	pub fn get_path(&self, kind: PathKind, path: &str) -> Result<Blob> {
		let path = CString::new(path).unwrap();
		let mut out_path = null_mut();
		let result = vcall!(self, _base, getPath(kind, path.as_ptr(), &mut out_path));
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(out_path as *mut _).unwrap(),
		)))
	}

	/// Clears any cached path information the file system holds
	/// (`ISlangFileSystemExt::clearCache`).
	pub fn clear_cache(&self) {
		vcall!(self, _base, clearCache());
	}

	/// Enumerates the entries of the directory at `path`
	/// (`ISlangFileSystemExt::enumeratePathContents`), invoking `callback`
	/// with the type and bare name of each entry. Returns `Err` when the file
	/// system does not support enumeration.
	///
	/// A panic inside `callback` never unwinds across the FFI boundary into
	/// C++: it is caught at the callback trampoline (the panic message still
	/// reaches the default panic hook) and enumeration continues with the
	/// next entry.
	pub fn enumerate_path_contents(
		&self,
		path: &str,
		callback: impl FnMut(PathType, &str),
	) -> Result<()> {
		let path = CString::new(path).unwrap();
		let mut callback = callback;
		let mut callback: &mut dyn FnMut(PathType, &str) = &mut callback;

		unsafe extern "C" fn forward(
			path_type: sys::SlangPathType,
			name: *const c_char,
			user_data: *mut c_void,
		) {
			// SAFETY: `user_data` is the `&mut dyn FnMut(PathType, &str)`
			// passed to `enumeratePathContents` below together with this
			// trampoline, valid for the duration of the call.
			let callback = unsafe { &mut *(user_data as *mut &mut dyn FnMut(PathType, &str)) };
			// SAFETY: a non-null `name` from Slang is a NUL-terminated string
			// valid for the duration of the callback.
			let name = unsafe { str_from_slang(name) };
			if let Some(name) = name {
				// A panic in the caller's closure must not unwind into C++.
				let _ = catch_unwind(AssertUnwindSafe(|| callback(path_type, name)));
			}
		}

		let result = vcall!(
			self,
			_base,
			enumeratePathContents(
				path.as_ptr(),
				Some(forward),
				&mut callback as *mut _ as *mut c_void
			)
		);
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Returns how paths used with this file system map to the operating
	/// system's file system (`ISlangFileSystemExt::getOSPathKind`).
	pub fn os_path_kind(&self) -> OSPathKind {
		vcall!(self, _base, getOSPathKind())
	}

	/// Writes `data` to `path` (`ISlangMutableFileSystem::saveFile`),
	/// replacing any existing file.
	pub fn save_file(&self, path: &str, data: &[u8]) -> Result<()> {
		let path = CString::new(path).unwrap();
		let result = vcall!(
			self,
			saveFile(path.as_ptr(), data.as_ptr() as *const c_void, data.len())
		);
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Writes the contents of `data` to `path`
	/// (`ISlangMutableFileSystem::saveFileBlob`). Depending on the
	/// implementation this can be cheaper than [`MutableFileSystem::save_file`];
	/// the blob is treated as immutable.
	pub fn save_file_blob(&self, path: &str, data: &Blob) -> Result<()> {
		let path = CString::new(path).unwrap();
		let result = vcall!(self, saveFileBlob(path.as_ptr(), data.as_raw()));
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Removes the file or empty directory at `path`
	/// (`ISlangMutableFileSystem::remove`).
	pub fn remove(&self, path: &str) -> Result<()> {
		let path = CString::new(path).unwrap();
		let result = vcall!(self, remove(path.as_ptr()));
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Creates the directory at `path`
	/// (`ISlangMutableFileSystem::createDirectory`). The parent path must
	/// exist.
	pub fn create_directory(&self, path: &str) -> Result<()> {
		let path = CString::new(path).unwrap();
		let result = vcall!(self, createDirectory(path.as_ptr()));
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}
}

/// An owned reference to a Slang global session (`IGlobalSession` in
/// slang.h): the root object of the Slang API, from which compilation
/// [`Session`]s are created. Expensive to create — keep one around and reuse
/// it.
#[repr(transparent)]
#[derive(Clone)]
pub struct GlobalSession(IUnknown);

unsafe impl Interface for GlobalSession {
	type Vtable = sys::IGlobalSessionVtable;
	const IID: UUID = uuid(0xc140_b5fd_0c78_452e_ba7c_1a1e_70c7_f71c);
}

impl GlobalSession {
	/// Creates a global session with the core module available
	/// (`slang_createGlobalSession` in slang.h). Returns `None` when Slang
	/// fails to create the session.
	pub fn new() -> Option<GlobalSession> {
		let mut global_session = null_mut();
		// SAFETY: `global_session` is a valid out-pointer; on success it
		// receives a new reference owned by the caller.
		unsafe { sys::slang_createGlobalSession(sys::SLANG_API_VERSION as _, &mut global_session) };
		Some(GlobalSession(IUnknown(std::ptr::NonNull::new(
			global_session as *mut _,
		)?)))
	}

	/// Creates a global session without loading the core module
	/// (`slang_createGlobalSessionWithoutCoreModule` in slang.h). The session
	/// is not usable for compilation until the core module is provided via
	/// [`GlobalSession::compile_core_module`] or
	/// [`GlobalSession::load_core_module`]. Returns `None` when Slang fails to
	/// create the session.
	///
	/// NOTE: this API is experimental in Slang.
	pub fn new_without_core_module() -> Option<GlobalSession> {
		let mut global_session = null_mut();
		// SAFETY: `global_session` is a valid out-pointer; on success it
		// receives a new reference owned by the caller.
		unsafe {
			sys::slang_createGlobalSessionWithoutCoreModule(
				sys::SLANG_API_VERSION as _,
				&mut global_session,
			)
		};
		Some(GlobalSession(IUnknown(std::ptr::NonNull::new(
			global_session as *mut _,
		)?)))
	}

	/// Returns a lazily-initialized, process-wide singleton `GlobalSession`.
	///
	/// The session is created on first access and **never dropped**, which
	/// avoids triggering Slang's global state cleanup at process exit (a
	/// common source of flaky SIGBUS crashes on macOS). All callers share
	/// the same underlying Slang `IGlobalSession` object.
	///
	/// Panics if Slang fails to create the global session.
	pub fn global() -> &'static GlobalSession {
		static GLOBAL: std::sync::OnceLock<GlobalSession> = std::sync::OnceLock::new();
		GLOBAL.get_or_init(|| GlobalSession::new().expect("GlobalSession::new failed"))
	}

	/// Creates a new session for loading and compiling code
	/// (`IGlobalSession::createSession`). Returns `None` when Slang fails to
	/// create the session.
	pub fn create_session(&self, desc: &SessionDesc) -> Option<Session> {
		let mut session = null_mut();
		vcall!(self, createSession(&**desc, &mut session));
		Some(Session(IUnknown(std::ptr::NonNull::new(
			session as *mut _,
		)?)))
	}

	/// Looks up the internal ID of a profile by its `name`, e.g. `"glsl_450"`
	/// (`IGlobalSession::findProfile`). Returns [`ProfileID::UNKNOWN`] when no
	/// profile with that name exists.
	pub fn find_profile(&self, name: &str) -> ProfileID {
		let name = CString::new(name).unwrap();
		ProfileID(vcall!(self, findProfile(name.as_ptr())))
	}

	/// Looks up the internal ID of a capability by its `name`
	/// (`IGlobalSession::findCapability`). Returns [`CapabilityID::UNKNOWN`]
	/// when no capability with that name exists.
	pub fn find_capability(&self, name: &str) -> CapabilityID {
		let name = CString::new(name).unwrap();
		CapabilityID(vcall!(self, findCapability(name.as_ptr())))
	}

	/// Gets the build version tag string of this Slang build, as produced by
	/// `git describe --tags` (`IGlobalSession::getBuildTagString`). Returns
	/// `None` when the string is unavailable or not valid UTF-8.
	pub fn build_tag_string(&self) -> Option<&str> {
		let tag = vcall!(self, getBuildTagString());
		// SAFETY: the string returned by `getBuildTagString` is owned by the
		// global session and outlives `&self`.
		unsafe { str_from_slang(tag) }
	}

	/// Sets the path that downstream compilers (aka back end compilers) are
	/// looked up from. For back ends that are dlls/shared libraries, the path
	/// is prefixed when calling into the shared library loader; for
	/// executables, they are looked up along the path.
	pub fn set_downstream_compiler_path(&self, pass_through: PassThrough, path: &str) {
		let path = CString::new(path).unwrap();
		vcall!(self, setDownstreamCompilerPath(pass_through, path.as_ptr()));
	}

	/// Sets the prelude text that is pre-pended verbatim before generated
	/// source code for `source_language`. Preludes apply to code generation
	/// only, not to pass-through usage.
	pub fn set_language_prelude(&self, source_language: SourceLanguage, prelude_text: &str) {
		let prelude_text = CString::new(prelude_text).unwrap();
		vcall!(
			self,
			setLanguagePrelude(source_language, prelude_text.as_ptr())
		);
	}

	/// Gets the prelude associated with `source_language`, if any.
	pub fn language_prelude(&self, source_language: SourceLanguage) -> Option<Blob> {
		let mut prelude = null_mut();
		vcall!(self, getLanguagePrelude(source_language, &mut prelude));
		Some(Blob(IUnknown(std::ptr::NonNull::new(prelude as *mut _)?)))
	}

	/// Returns `Ok(())` if the compilation target is supported by this build
	/// of Slang and the resources it requires (e.g. downstream compiler shared
	/// libraries) can be found.
	pub fn check_compile_target_support(&self, target: CompileTarget) -> Result<()> {
		let result = vcall!(self, checkCompileTargetSupport(target));
		// `checkCompileTargetSupport` takes no diagnostics out-pointer; the
		// result code is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Returns `Ok(())` if the pass-through compiler is supported by this
	/// build of Slang and can be located.
	pub fn check_pass_through_support(&self, pass_through: PassThrough) -> Result<()> {
		let result = vcall!(self, checkPassThroughSupport(pass_through));
		// `checkPassThroughSupport` takes no diagnostics out-pointer; the
		// result code is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Compiles the core module from embedded source, making a global session
	/// created with [`GlobalSession::new_without_core_module`] usable. Fails
	/// if a core module is already available.
	///
	/// `flags` is a bitmask of [`CompileCoreModuleFlag`] values (currently
	/// only `WriteDocumentation`); pass `0` for the default behavior.
	///
	/// NOTE: this API is experimental in Slang.
	pub fn compile_core_module(&self, flags: CompileCoreModuleFlags) -> Result<()> {
		let result = vcall!(self, compileCoreModule(flags));
		// `compileCoreModule` takes no diagnostics out-pointer; the result
		// code is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Loads a serialized core module (as produced by
	/// [`GlobalSession::save_core_module`]) into a global session created with
	/// [`GlobalSession::new_without_core_module`].
	///
	/// NOTE: this API is experimental in Slang.
	pub fn load_core_module(&self, data: &[u8]) -> Result<()> {
		let result = vcall!(self, loadCoreModule(data.as_ptr() as _, data.len()));
		// `loadCoreModule` takes no diagnostics out-pointer; the result code
		// is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Serializes the core module of this global session into a blob, suitable
	/// for a later [`GlobalSession::load_core_module`] call.
	///
	/// NOTE: this API is experimental in Slang.
	pub fn save_core_module(&self, archive_type: ArchiveType) -> Result<Blob> {
		let mut blob = null_mut();
		let result = vcall!(self, saveCoreModule(archive_type, &mut blob));
		// `saveCoreModule` takes no diagnostics out-pointer; the result code
		// is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(blob as *mut _).unwrap(),
		)))
	}

	/// Returns the time in seconds spent in the Slang compiler and in
	/// downstream compilers, as `(total, downstream)`.
	pub fn compiler_elapsed_time(&self) -> (f64, f64) {
		let mut total = 0.0;
		let mut downstream = 0.0;
		vcall!(self, getCompilerElapsedTime(&mut total, &mut downstream));
		(total, downstream)
	}

	/// Specifies a `spirv.core.grammar.json` file to load and use when parsing
	/// and checking any SPIR-V code.
	pub fn set_spirv_core_grammar(&self, json_path: &str) -> Result<()> {
		let json_path = CString::new(json_path).unwrap();
		let result = vcall!(self, setSPIRVCoreGrammar(json_path.as_ptr()));
		// `setSPIRVCoreGrammar` takes no diagnostics out-pointer; the result
		// code is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Parses slangc-style command line options into a [`SessionDesc`] that can
	/// be used to create a session with all the specified compiler options.
	pub fn parse_command_line_arguments(&self, args: &[&str]) -> Result<ParsedCommandLine> {
		let arg_strings: Vec<CString> =
			args.iter().map(|arg| CString::new(*arg).unwrap()).collect();
		let argv: Vec<*const i8> = arg_strings.iter().map(|arg| arg.as_ptr()).collect();

		let mut desc = SessionDesc::default();
		let mut aux = null_mut();
		let result = vcall!(
			self,
			parseCommandLineArguments(argv.len() as _, argv.as_ptr(), &mut desc.inner, &mut aux)
		);
		// `parseCommandLineArguments` takes no diagnostics out-pointer; the
		// result code is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		let Some(aux) = std::ptr::NonNull::new(aux as *mut _) else {
			// Slang is expected to always hand back an aux allocation on
			// success; guard anyway so a null pointer cannot leak into the
			// owning wrapper.
			return Err(Error::Code(result));
		};
		Ok(ParsedCommandLine {
			desc,
			_aux: IUnknown(aux),
		})
	}

	/// Computes a digest that uniquely identifies the session description.
	pub fn session_desc_digest(&self, desc: &SessionDesc) -> Result<Blob> {
		let mut blob = null_mut();
		let result = vcall!(self, getSessionDescDigest(&**desc, &mut blob));
		// `getSessionDescDigest` takes no diagnostics out-pointer; the result
		// code is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(blob as *mut _).unwrap(),
		)))
	}

	/// Adds new builtin declarations, given as Slang source, to be used in
	/// subsequent compiles on sessions created from this global session
	/// (`IGlobalSession::addBuiltins`). `source_path` is used for diagnostics.
	pub fn add_builtins(&self, source_path: &str, source_string: &str) {
		let source_path = CString::new(source_path).unwrap();
		let source_string = CString::new(source_string).unwrap();
		vcall!(
			self,
			addBuiltins(source_path.as_ptr(), source_string.as_ptr())
		);
	}

	/// Sets the default downstream compiler for a source language
	/// (`IGlobalSession::setDefaultDownstreamCompiler`). The default is only
	/// used when Slang cannot pick a better compiler for the requested target.
	pub fn set_default_downstream_compiler(
		&self,
		source_language: SourceLanguage,
		default_compiler: PassThrough,
	) -> Result<()> {
		let result = vcall!(
			self,
			setDefaultDownstreamCompiler(source_language, default_compiler)
		);
		// `setDefaultDownstreamCompiler` takes no diagnostics out-pointer; the
		// result code is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Gets the default downstream compiler for a source language
	/// (`IGlobalSession::getDefaultDownstreamCompiler`).
	pub fn default_downstream_compiler(&self, source_language: SourceLanguage) -> PassThrough {
		vcall!(self, getDefaultDownstreamCompiler(source_language))
	}

	/// Sets the downstream compiler to use for a transition from the `source`
	/// code gen target to the `target` code gen target
	/// (`IGlobalSession::setDownstreamCompilerForTransition`).
	pub fn set_downstream_compiler_for_transition(
		&self,
		source: CompileTarget,
		target: CompileTarget,
		compiler: PassThrough,
	) {
		vcall!(
			self,
			setDownstreamCompilerForTransition(source, target, compiler)
		);
	}

	/// Gets the downstream compiler used for a transition from the `source`
	/// code gen target to the `target` code gen target
	/// (`IGlobalSession::getDownstreamCompilerForTransition`). Returns
	/// [`PassThrough::None`] when no compiler is defined for the transition.
	pub fn downstream_compiler_for_transition(
		&self,
		source: CompileTarget,
		target: CompileTarget,
	) -> PassThrough {
		vcall!(self, getDownstreamCompilerForTransition(source, target))
	}

	/// Gets the version of the downstream compiler that Slang will actually
	/// load and use for `pass_through`
	/// (`IGlobalSession::getDownstreamCompilerVersion`), applying the same lazy
	/// discovery used during compilation. The first call for a given
	/// `pass_through` loads the downstream library into the process, so this is
	/// not a cheap accessor. Some downstream compilers (e.g. the glslang
	/// family) always report `(0, 0)`.
	///
	/// Returns `Err` when the compiler cannot be located or loaded; the result
	/// code alone does not distinguish an invalid `pass_through` from a
	/// compiler that is simply not installed.
	pub fn downstream_compiler_version(&self, pass_through: PassThrough) -> Result<(i32, i32)> {
		let mut major = 0;
		let mut minor = 0;
		let result = vcall!(
			self,
			getDownstreamCompilerVersion(pass_through, &mut major, &mut minor)
		);
		// `getDownstreamCompilerVersion` takes no diagnostics out-pointer; the
		// result code is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok((major, minor))
	}

	/// Compiles a builtin module from embedded source
	/// (`IGlobalSession::compileBuiltinModule`). Fails if the builtin module is
	/// already available on this global session.
	///
	/// NOTE: this API is experimental in Slang.
	pub fn compile_builtin_module(
		&self,
		module: BuiltinModuleName,
		flags: CompileCoreModuleFlags,
	) -> Result<()> {
		let result = vcall!(self, compileBuiltinModule(module, flags));
		// `compileBuiltinModule` takes no diagnostics out-pointer; the result
		// code is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Loads a serialized builtin module, as produced by
	/// [`GlobalSession::save_builtin_module`]
	/// (`IGlobalSession::loadBuiltinModule`).
	///
	/// NOTE: this API is experimental in Slang.
	pub fn load_builtin_module(&self, module: BuiltinModuleName, data: &[u8]) -> Result<()> {
		let result = vcall!(
			self,
			loadBuiltinModule(module, data.as_ptr() as _, data.len())
		);
		// `loadBuiltinModule` takes no diagnostics out-pointer; the result code
		// is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Serializes a builtin module of this global session into a blob, suitable
	/// for a later [`GlobalSession::load_builtin_module`] call
	/// (`IGlobalSession::saveBuiltinModule`).
	///
	/// NOTE: this API is experimental in Slang.
	pub fn save_builtin_module(
		&self,
		module: BuiltinModuleName,
		archive_type: ArchiveType,
	) -> Result<Blob> {
		let mut blob = null_mut();
		let result = vcall!(self, saveBuiltinModule(module, archive_type, &mut blob));
		// `saveBuiltinModule` takes no diagnostics out-pointer; the result code
		// is the only signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(blob as *mut _).unwrap(),
		)))
	}
}

/// The result of [`GlobalSession::parse_command_line_arguments`]: a
/// [`SessionDesc`] together with the auxiliary allocation that owns the memory
/// the desc points into. Dereferences to the desc, so it can be passed
/// directly to [`GlobalSession::create_session`].
pub struct ParsedCommandLine {
	// `desc` borrows strings and arrays owned by `_aux`; `_aux` is declared
	// after `desc` so it is dropped after it (struct fields drop in
	// declaration order), keeping the desc valid for the struct's lifetime.
	// The `'static` lifetime parameter never escapes this struct except
	// through `Deref`, which keeps the borrow tied to `&self` at use sites.
	desc: SessionDesc<'static>,
	_aux: IUnknown,
}

impl std::ops::Deref for ParsedCommandLine {
	type Target = SessionDesc<'static>;

	fn deref(&self) -> &Self::Target {
		&self.desc
	}
}

/// Argument used for specialization to types/values; see
/// `slang::SpecializationArg` in slang.h.
pub struct SpecializationArg {
	inner: sys::slang_SpecializationArg,
	// Keeps the expression string alive for `Kind::Expr` arguments; the sys
	// struct only borrows it.
	_expr: Option<CString>,
}

impl SpecializationArg {
	/// Specialize to a type.
	pub fn from_type(type_: &reflection::Type) -> Self {
		Self {
			inner: sys::slang_SpecializationArg {
				kind: sys::slang_SpecializationArg_Kind::Type,
				__bindgen_anon_1: sys::slang_SpecializationArg__bindgen_ty_1 {
					// `reflection::Type` wraps `SlangReflectionType`, which
					// slang.h treats as pointer-interchangeable with
					// `slang::TypeReflection`.
					type_: type_ as *const _ as *mut _,
				},
			},
			_expr: None,
		}
	}

	/// Specialize to an expression in Slang syntax (a type or a value), e.g.
	/// `"float"` or `"3"`.
	pub fn from_expr(expr: &str) -> Self {
		let expr = CString::new(expr).unwrap();
		let expr_ptr = expr.as_ptr();
		Self {
			inner: sys::slang_SpecializationArg {
				kind: sys::slang_SpecializationArg_Kind::Expr,
				__bindgen_anon_1: sys::slang_SpecializationArg__bindgen_ty_1 { expr: expr_ptr },
			},
			_expr: Some(expr),
		}
	}
}

/// Collects the raw sys arguments for a specialize call. The returned vector
/// borrows the strings owned by the input slice, which must outlive it.
fn specialization_args_as_sys(args: &[SpecializationArg]) -> Vec<sys::slang_SpecializationArg> {
	args.iter().map(|arg| arg.inner).collect()
}

/// An owned reference to a Slang session (`ISession` in slang.h): a scope for
/// loading and compiling code, with its own search paths, preprocessor
/// definitions, and code generation targets (see [`SessionDesc`]).
///
/// Code loaded and compiled within a session is owned by the session and
/// remains resident in memory until the session is released.
#[repr(transparent)]
#[derive(Clone)]
pub struct Session(IUnknown);

unsafe impl Interface for Session {
	type Vtable = sys::ISessionVtable;
	const IID: UUID = uuid(0x6761_8701_d116_468f_ab3b_474b_edce_0e3d);
}

impl Session {
	/// Loads a module by name, as code using `import` would
	/// (`ISession::loadModule`). Returns `Err` when the module cannot be found
	/// or fails to compile; the error carries the compiler diagnostics blob
	/// when Slang produced one.
	pub fn load_module(&self, name: &str) -> Result<Module> {
		let name = CString::new(name).unwrap();
		let mut diagnostics = null_mut();

		let module = vcall!(self, loadModule(name.as_ptr(), &mut diagnostics));

		if module.is_null() {
			// The module pointer is the only error signal here; Slang usually
			// fills the diagnostics blob but is not guaranteed to, so fall
			// back to a generic failure code instead of dereferencing null.
			Err(error_from_diagnostics(diagnostics))
		} else {
			let module = Module(IUnknown(std::ptr::NonNull::new(module as *mut _).unwrap()));
			// SAFETY: `module` is non-null, so the vtable call is valid.
			// `loadModule`/`loadModuleFromSourceString`/`loadModuleFromIRBlob`
			// return a borrowed reference — code loaded in a session is owned
			// by the session (see the ISession documentation in slang.h:
			// "Code loaded and compiled within a session is owned by the
			// session"). Adding a reference turns it into an owned pointer
			// matching the `IUnknown` RAII drop semantics.
			unsafe { (module.as_unknown().vtable().ISlangUnknown_addRef)(module.as_raw()) };
			Ok(module)
		}
	}

	/// Loads a module from an in-memory Slang source string
	/// (`ISession::loadModuleFromSourceString`). `module_name` is the name the
	/// module is registered under; `path` is used for diagnostics and for
	/// resolving relative paths. Returns `Err` when the source fails to
	/// compile; the error carries the compiler diagnostics blob when Slang
	/// produced one.
	pub fn load_module_from_source_string(
		&self,
		module_name: &str,
		path: &str,
		source: &str,
	) -> Result<Module> {
		let module_name = CString::new(module_name).unwrap();
		let path = CString::new(path).unwrap();
		let source = CString::new(source).unwrap();
		let mut diagnostics = null_mut();

		let module = vcall!(
			self,
			loadModuleFromSourceString(
				module_name.as_ptr(),
				path.as_ptr(),
				source.as_ptr(),
				&mut diagnostics
			)
		);

		if module.is_null() {
			// The module pointer is the only error signal here; Slang usually
			// fills the diagnostics blob but is not guaranteed to, so fall
			// back to a generic failure code instead of dereferencing null.
			Err(error_from_diagnostics(diagnostics))
		} else {
			let module = Module(IUnknown(std::ptr::NonNull::new(module as *mut _).unwrap()));
			// SAFETY: `module` is non-null, so the vtable call is valid.
			// `loadModule`/`loadModuleFromSourceString`/`loadModuleFromIRBlob`
			// return a borrowed reference — code loaded in a session is owned
			// by the session (see the ISession documentation in slang.h:
			// "Code loaded and compiled within a session is owned by the
			// session"). Adding a reference turns it into an owned pointer
			// matching the `IUnknown` RAII drop semantics.
			unsafe { (module.as_unknown().vtable().ISlangUnknown_addRef)(module.as_raw()) };
			Ok(module)
		}
	}

	/// Loads a module from an in-memory Slang source blob
	/// (`ISession::loadModuleFromSource`). Unlike
	/// [`Session::load_module_from_source_string`], the source does not have to
	/// be valid UTF-8; a blob can be built from raw bytes with [`Blob::new`].
	/// `module_name` is the name the module is registered under; `path` is used
	/// for diagnostics and for resolving relative paths. Returns `Err` when the
	/// source fails to compile; the error carries the compiler diagnostics blob
	/// when Slang produced one.
	pub fn load_module_from_source(
		&self,
		module_name: &str,
		path: &str,
		source: &Blob,
	) -> Result<Module> {
		let module_name = CString::new(module_name).unwrap();
		let path = CString::new(path).unwrap();
		let mut diagnostics = null_mut();

		let module = vcall!(
			self,
			loadModuleFromSource(
				module_name.as_ptr(),
				path.as_ptr(),
				source.as_raw(),
				&mut diagnostics
			)
		);

		if module.is_null() {
			// The module pointer is the only error signal here; Slang usually
			// fills the diagnostics blob but is not guaranteed to, so fall
			// back to a generic failure code instead of dereferencing null.
			Err(error_from_diagnostics(diagnostics))
		} else {
			let module = Module(IUnknown(std::ptr::NonNull::new(module as *mut _).unwrap()));
			// SAFETY: `module` is non-null, so the vtable call is valid.
			// `loadModule`/`loadModuleFromSource`/`loadModuleFromSourceString`/
			// `loadModuleFromIRBlob` return a borrowed reference — code loaded
			// in a session is owned by the session (see the ISession
			// documentation in slang.h: "Code loaded and compiled within a
			// session is owned by the session"). Adding a reference turns it
			// into an owned pointer matching the `IUnknown` RAII drop
			// semantics.
			unsafe { (module.as_unknown().vtable().ISlangUnknown_addRef)(module.as_raw()) };
			Ok(module)
		}
	}

	/// Loads a module from a serialized module blob, as produced by
	/// [`Module::serialize`] (`ISession::loadModuleFromIRBlob`). Returns `Err`
	/// when the blob is not a valid serialized module; the error carries the
	/// compiler diagnostics blob when Slang produced one.
	pub fn load_module_from_ir_blob(
		&self,
		module_name: &str,
		path: &str,
		ir_blob: &Blob,
	) -> Result<Module> {
		let module_name = CString::new(module_name).unwrap();
		let path = CString::new(path).unwrap();
		let mut diagnostics = null_mut();

		let module = vcall!(
			self,
			loadModuleFromIRBlob(
				module_name.as_ptr(),
				path.as_ptr(),
				ir_blob.as_raw(),
				&mut diagnostics
			)
		);

		if module.is_null() {
			// The module pointer is the only error signal here; Slang usually
			// fills the diagnostics blob but is not guaranteed to, so fall
			// back to a generic failure code instead of dereferencing null.
			Err(error_from_diagnostics(diagnostics))
		} else {
			let module = Module(IUnknown(std::ptr::NonNull::new(module as *mut _).unwrap()));
			// SAFETY: `module` is non-null, so the vtable call is valid.
			// `loadModule`/`loadModuleFromSourceString`/`loadModuleFromIRBlob`
			// return a borrowed reference — code loaded in a session is owned
			// by the session (see the ISession documentation in slang.h:
			// "Code loaded and compiled within a session is owned by the
			// session"). Adding a reference turns it into an owned pointer
			// matching the `IUnknown` RAII drop semantics.
			unsafe { (module.as_unknown().vtable().ISlangUnknown_addRef)(module.as_raw()) };
			Ok(module)
		}
	}

	/// Combines multiple component types into a composite component type
	/// (`ISession::createCompositeComponentType`). All components must have
	/// been loaded or created using this session. The shader parameters,
	/// specialization parameters, and entry points of the composite are the
	/// union of those in `components`, following the order of `components`.
	///
	/// Returns `Err` when composition fails (e.g. a single module is
	/// aggregated more than once); the error carries the diagnostics blob when
	/// Slang produced one.
	pub fn create_composite_component_type(
		&self,
		components: &[ComponentType],
	) -> Result<ComponentType> {
		let mut composite_component_type = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				createCompositeComponentType(
					components.as_ptr() as _,
					components.len() as _,
					&mut composite_component_type,
					&mut diagnostics
				)
			),
			diagnostics,
		)?;

		Ok(ComponentType(IUnknown(
			std::ptr::NonNull::new(composite_component_type as *mut _).unwrap(),
		)))
	}

	/// Specializes an existential (interface) type by plugging in concrete
	/// type arguments, e.g. specializing `IMaterial` with `Diffuse`. The
	/// returned reflection is owned by the session.
	///
	/// Note: Slang only accepts arguments built with
	/// `SpecializationArg::from_type` here — `from_expr` arguments make the
	/// call fail.
	pub fn specialize_type(
		&self,
		type_: &reflection::Type,
		specialization_args: &[SpecializationArg],
	) -> Result<&reflection::Type> {
		let raw_args = specialization_args_as_sys(specialization_args);
		let mut diagnostics = null_mut();

		let specialized = vcall!(
			self,
			specializeType(
				type_ as *const _ as *mut _,
				raw_args.as_ptr(),
				raw_args.len() as _,
				&mut diagnostics
			)
		);

		if specialized.is_null() {
			// A null type is the only error signal here; fall back to a generic
			// failure code when Slang did not fill the diagnostics blob.
			Err(error_from_diagnostics(diagnostics))
		} else {
			// SAFETY: `specialized` is non-null and points to a reflection
			// object owned by the session, which outlives `&self`.
			Ok(unsafe { &*(specialized as *const _) })
		}
	}

	/// Creates a `TypeConformance` component type representing `type_`'s
	/// conformance to `interface_type`. Pass `conformance_id_override = -1` to
	/// let Slang assign the dispatch ID automatically.
	pub fn create_type_conformance_component_type(
		&self,
		type_: &reflection::Type,
		interface_type: &reflection::Type,
		conformance_id_override: i64,
	) -> Result<TypeConformance> {
		let mut conformance = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				createTypeConformanceComponentType(
					type_ as *const _ as *mut _,
					interface_type as *const _ as *mut _,
					&mut conformance,
					conformance_id_override,
					&mut diagnostics
				)
			),
			diagnostics,
		)?;

		Ok(TypeConformance(IUnknown(
			std::ptr::NonNull::new(conformance as *mut _).unwrap(),
		)))
	}

	/// Gets the global session that was used to create this session.
	pub fn get_global_session(&self) -> GlobalSession {
		let global_session = vcall!(self, getGlobalSession());
		let global_session = GlobalSession(IUnknown(
			std::ptr::NonNull::new(global_session as *mut _).unwrap(),
		));
		// SAFETY: `global_session` is non-null, so the vtable call is valid.
		// `getGlobalSession` returns a borrowed reference — the global session
		// owns the session. Adding a reference turns it into an owned pointer
		// matching the `IUnknown` RAII drop semantics.
		unsafe {
			(global_session.as_unknown().vtable().ISlangUnknown_addRef)(global_session.as_raw())
		};
		global_session
	}

	/// Gets the layout of `type_` on the chosen `target_index` under the given
	/// layout `rules`. The returned reflection is owned by the session.
	pub fn type_layout(
		&self,
		type_: &reflection::Type,
		target_index: i64,
		rules: LayoutRules,
	) -> Result<&reflection::TypeLayout> {
		// The COM ABI takes slang's C++ `LayoutRules` enum class; the public
		// re-export is the C `SlangLayoutRules` enum. Both share the same
		// values (slang.h defines one in terms of the other).
		let rules = match rules {
			LayoutRules::Default => sys::slang_LayoutRules::Default,
			LayoutRules::MetalArgumentBufferTier2 => {
				sys::slang_LayoutRules::MetalArgumentBufferTier2
			}
			LayoutRules::DefaultStructuredBuffer => sys::slang_LayoutRules::DefaultStructuredBuffer,
			LayoutRules::DefaultConstantBuffer => sys::slang_LayoutRules::DefaultConstantBuffer,
		};
		let mut diagnostics = null_mut();
		let layout = vcall!(
			self,
			getTypeLayout(
				type_ as *const _ as *mut _,
				target_index,
				rules,
				&mut diagnostics
			)
		);

		if layout.is_null() {
			// A null layout is the only error signal here; fall back to a
			// generic failure code when Slang did not fill the diagnostics blob.
			Err(error_from_diagnostics(diagnostics))
		} else {
			// SAFETY: `layout` is non-null and points to a reflection object
			// owned by the session, which outlives `&self`.
			Ok(unsafe { &*(layout as *const _) })
		}
	}

	/// Gets a container type wrapping `element_type`, e.g. given `T` returns a
	/// type that represents `StructuredBuffer<T>` for
	/// [`ContainerType::StructuredBuffer`]. The returned reflection is owned by
	/// the session.
	pub fn container_type(
		&self,
		element_type: &reflection::Type,
		container_type: ContainerType,
	) -> Result<&reflection::Type> {
		let mut diagnostics = null_mut();
		let container = vcall!(
			self,
			getContainerType(
				element_type as *const _ as *mut _,
				container_type,
				&mut diagnostics
			)
		);

		if container.is_null() {
			// A null type is the only error signal here; fall back to a generic
			// failure code when Slang did not fill the diagnostics blob.
			Err(error_from_diagnostics(diagnostics))
		} else {
			// SAFETY: `container` is non-null and points to a reflection object
			// owned by the session, which outlives `&self`.
			Ok(unsafe { &*(container as *const _) })
		}
	}

	/// Returns a type that represents the `__Dynamic` type, usable as a
	/// specialization argument to indicate dynamic dispatch. The returned
	/// reflection is owned by the session.
	pub fn dynamic_type(&self) -> &reflection::Type {
		let dynamic = vcall!(self, getDynamicType());
		// SAFETY: `getDynamicType` returns a non-null reflection object owned
		// by the session, which outlives `&self`.
		unsafe { &*(dynamic as *const _) }
	}

	/// Gets the mangled name for a type RTTI object.
	pub fn type_rtti_mangled_name(&self, type_: &reflection::Type) -> Result<Blob> {
		let mut name = null_mut();
		let result = vcall!(
			self,
			getTypeRTTIMangledName(type_ as *const _ as *mut _, &mut name)
		);
		// `getTypeRTTIMangledName` takes no diagnostics out-pointer; the result
		// code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(name as *mut _).unwrap(),
		)))
	}

	/// Gets the mangled name for a type witness of `type_`'s conformance to
	/// `interface_type`.
	pub fn type_conformance_witness_mangled_name(
		&self,
		type_: &reflection::Type,
		interface_type: &reflection::Type,
	) -> Result<Blob> {
		let mut name = null_mut();
		let result = vcall!(
			self,
			getTypeConformanceWitnessMangledName(
				type_ as *const _ as *mut _,
				interface_type as *const _ as *mut _,
				&mut name
			)
		);
		// `getTypeConformanceWitnessMangledName` takes no diagnostics
		// out-pointer; the result code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(name as *mut _).unwrap(),
		)))
	}

	/// Gets the sequential ID used to identify a type witness in a dynamic
	/// object. The sequential ID is part of the RTTI bytes returned by
	/// [`Session::dynamic_object_rtti_bytes`].
	pub fn type_conformance_witness_sequential_id(
		&self,
		type_: &reflection::Type,
		interface_type: &reflection::Type,
	) -> Result<u32> {
		let mut id = 0;
		let result = vcall!(
			self,
			getTypeConformanceWitnessSequentialID(
				type_ as *const _ as *mut _,
				interface_type as *const _ as *mut _,
				&mut id
			)
		);
		// `getTypeConformanceWitnessSequentialID` takes no diagnostics
		// out-pointer; the result code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(id)
	}

	/// Gets the 16-byte RTTI header to fill into a dynamic object holding a
	/// `type_` value dispatched through `interface_type`. The returned vector
	/// holds the buffer as `u32` words; `buffer_size_in_bytes` must be greater
	/// than 16 and is rounded up to a multiple of 4.
	pub fn dynamic_object_rtti_bytes(
		&self,
		type_: &reflection::Type,
		interface_type: &reflection::Type,
		buffer_size_in_bytes: u32,
	) -> Result<Vec<u32>> {
		let mut buffer = vec![0u32; (buffer_size_in_bytes as usize).div_ceil(4)];
		let result = vcall!(
			self,
			getDynamicObjectRTTIBytes(
				type_ as *const _ as *mut _,
				interface_type as *const _ as *mut _,
				buffer.as_mut_ptr(),
				buffer_size_in_bytes
			)
		);
		// `getDynamicObjectRTTIBytes` takes no diagnostics out-pointer; the
		// result code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(buffer)
	}

	/// Gets the number of modules loaded in this session.
	pub fn loaded_module_count(&self) -> i64 {
		vcall!(self, getLoadedModuleCount())
	}

	/// Gets a module already loaded in this session by index.
	pub fn loaded_module(&self, index: i64) -> Option<Module> {
		let module = vcall!(self, getLoadedModule(index));
		let module = Module(IUnknown(std::ptr::NonNull::new(module as *mut _)?));
		// SAFETY: `module` is non-null, so the vtable call is valid.
		// `getLoadedModule` returns a borrowed reference — loaded modules are
		// owned by the session. Adding a reference turns it into an owned
		// pointer matching the `IUnknown` RAII drop semantics.
		unsafe { (module.as_unknown().vtable().ISlangUnknown_addRef)(module.as_raw()) };
		Some(module)
	}

	/// Iterates over the modules loaded in this session.
	pub fn loaded_modules(&self) -> impl ExactSizeIterator<Item = Module> + '_ {
		(0..self.loaded_module_count() as usize).map(|i| self.loaded_module(i as _).unwrap())
	}

	/// Checks if a precompiled binary module is up-to-date with the current
	/// compiler option settings and the source file contents. See the
	/// `ISession::isBinaryModuleUpToDate` documentation in slang.h for the
	/// exact staleness rules (e.g. modules whose primary source file cannot be
	/// located on the search paths are treated as up-to-date).
	pub fn is_binary_module_up_to_date(&self, module_path: &str, binary_module: &Blob) -> bool {
		let module_path = CString::new(module_path).unwrap();
		vcall!(
			self,
			isBinaryModuleUpToDate(module_path.as_ptr(), binary_module.as_raw())
		)
	}

	/// Reads module info (name and version) from a serialized module blob, as
	/// produced by [`Module::serialize`]. The returned strings borrow from the
	/// session.
	pub fn module_info_from_ir_blob(&self, ir_blob: &Blob) -> Result<ModuleInfo<'_>> {
		let mut version = 0;
		let mut compiler_version = null();
		let mut name = null();
		let result = vcall!(
			self,
			loadModuleInfoFromIRBlob(
				ir_blob.as_raw(),
				&mut version,
				&mut compiler_version,
				&mut name
			)
		);
		// `loadModuleInfoFromIRBlob` takes no diagnostics out-pointer; the
		// result code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		// SAFETY: per slang.h, the returned string pointers are valid for as
		// long as the session, so tying them to `&self` is sound.
		Ok(ModuleInfo {
			version,
			compiler_version: unsafe { str_from_slang(compiler_version) },
			name: unsafe { str_from_slang(name) },
		})
	}

	/// Gets the source location of a declaration. The returned location
	/// borrows from the session.
	pub fn decl_source_location(&self, decl: &reflection::Decl) -> Result<SourceLocation<'_>> {
		// SAFETY: `slang_SourceLocation` is a C struct of scalars and a
		// pointer; an all-zero value is a valid instance.
		let mut location: sys::slang_SourceLocation = unsafe { std::mem::zeroed() };
		let result = vcall!(
			self,
			getDeclSourceLocation(decl as *const _ as *mut _, &mut location)
		);
		// `getDeclSourceLocation` takes no diagnostics out-pointer; the result
		// code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(SourceLocation {
			inner: location,
			_phantom: PhantomData,
		})
	}
}

/// Module metadata read from a serialized module blob by
/// [`Session::module_info_from_ir_blob`]. Borrows strings owned by the
/// session.
pub struct ModuleInfo<'a> {
	/// The version of the serialized module format.
	pub version: i64,
	/// The version of the Slang compiler that produced the blob, if recorded.
	pub compiler_version: Option<&'a str>,
	/// The name of the module, if recorded.
	pub name: Option<&'a str>,
}

/// The source location of a declaration, as returned by
/// [`Session::decl_source_location`]. Borrows the file path string owned by
/// the session.
#[repr(transparent)]
pub struct SourceLocation<'a> {
	inner: sys::slang_SourceLocation,
	_phantom: PhantomData<&'a ()>,
}

impl SourceLocation<'_> {
	/// The path of the source file the declaration was defined in, if known.
	pub fn file_path(&self) -> Option<&str> {
		// SAFETY: per slang.h, the file path pointer returned by
		// `getDeclSourceLocation` is valid for as long as the session, which
		// outlives the `&self` borrow the lifetime of this struct is tied to.
		unsafe { str_from_slang(self.inner.filePath) }
	}

	/// The line number of the declaration (1-based), or `-1` if unknown.
	pub fn line(&self) -> i64 {
		self.inner.line
	}

	/// The column number of the declaration (1-based), or `-1` if unknown.
	pub fn column(&self) -> i64 {
		self.inner.column
	}
}

/// An owned reference to Slang metadata about a compiled program or entry
/// point (`IMetadata` in slang.h), obtained from
/// [`ComponentType::target_metadata`] / [`ComponentType::entry_point_metadata`]
/// or [`CompileResult::metadata`]. Use [`Metadata::cast_as`] to query
/// extension interfaces such as [`BindlessResourceMetadata`].
#[repr(transparent)]
#[derive(Clone)]
pub struct Metadata(IUnknown);

unsafe impl Interface for Metadata {
	type Vtable = sys::IMetadataVtable;
	const IID: UUID = uuid(0x8044_a8a3_ddc0_4b7f_af8e_026e_905d_7332);
}

impl Metadata {
	/// Casts this object to another interface using Slang's `ISlangCastable`
	/// RTTI. Unlike `IUnknown::query_interface`, `castAs` can also hand out
	/// internal types that are not `ISlangUnknown`-derived; the raw result is a
	/// borrowed pointer, so this method takes an additional reference before
	/// wrapping it. Returns `None` when the cast is not supported.
	pub fn cast_as<T: Interface>(&self) -> Option<T> {
		// SAFETY: the call goes through the COM vtable of a live `IMetadata`
		// interface pointer; `castAs` lives on the `ISlangCastable` base of the
		// vtable. `&T::IID` is a valid UUID pointer.
		let object = unsafe { (self.vtable()._base.castAs)(self.as_raw(), &T::IID) };
		let object = std::ptr::NonNull::new(object)?;
		// `castAs` returns a non-refcounted (borrowed) pointer that stays valid
		// as long as `self` is alive. `clone` adds a reference that the returned
		// wrapper owns; `ManuallyDrop` ensures the borrowed pointer itself is
		// never released.
		let borrowed = std::mem::ManuallyDrop::new(IUnknown(object));
		let owned = (*borrowed).clone();
		// SAFETY: `castAs` guarantees the returned object exposes the interface
		// identified by `T::IID` at the returned pointer value.
		Some(unsafe { upcast(owned) })
	}

	/// Returns whether a resource parameter at the specified binding location
	/// is actually used in the compiled shader
	/// (`IMetadata::isParameterLocationUsed`). `category` selects the register
	/// class (e.g. `t` vs `s`); `space_index`/`register_index` are the
	/// space/register for D3D12 and the set/binding for Vulkan. Returns `None`
	/// when the query itself fails.
	pub fn is_parameter_location_used(
		&self,
		category: ParameterCategory,
		space_index: u64,
		register_index: u64,
	) -> Option<bool> {
		let mut used = false;
		let result = vcall!(
			self,
			isParameterLocationUsed(category, space_index, register_index, &mut used)
		);
		succeeded(result).then_some(used)
	}

	/// Returns the debug build identifier for a base and debug SPIR-V pair,
	/// as produced by separate debug compilation. Returns `None` when this
	/// metadata does not carry one (e.g. no separate debug data was emitted).
	pub fn get_debug_build_identifier(&self) -> Option<&str> {
		let identifier = vcall!(self, getDebugBuildIdentifier());
		// SAFETY: the string returned by `getDebugBuildIdentifier` is owned by
		// the metadata object and outlives `&self`.
		unsafe { str_from_slang(identifier) }
	}
}

/// Compile result for storing and retrieving multiple output blobs
/// (`ICompileResult` in slang.h), e.g. the base and debug SPIR-V produced by
/// separate debug compilation. Obtained from
/// [`ComponentType2::target_compile_result`] /
/// [`ComponentType2::entry_point_compile_result`].
#[repr(transparent)]
#[derive(Clone)]
pub struct CompileResult(IUnknown);

unsafe impl Interface for CompileResult {
	type Vtable = sys::ICompileResultVtable;
	const IID: UUID = uuid(0x5fa9_380e_b62f_41e5_9f12_4bad_4d9e_aae4);
}

impl CompileResult {
	/// The number of output blobs stored in this compile result.
	pub fn item_count(&self) -> u32 {
		vcall!(self, getItemCount())
	}

	/// Gets the output blob at `index`, which must be in
	/// `0..self.item_count()`.
	pub fn item_data(&self, index: u32) -> Result<Blob> {
		let mut blob = null_mut();
		let result = vcall!(self, getItemData(index, &mut blob));
		// `getItemData` takes no diagnostics out-pointer; the result code is
		// the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(blob as *mut _).unwrap(),
		)))
	}

	/// Gets the metadata associated with this compile result, e.g. the debug
	/// build identifier of a base/debug SPIR-V pair.
	pub fn metadata(&self) -> Result<Metadata> {
		let mut metadata = null_mut();
		let result = vcall!(self, getMetadata(&mut metadata));
		// `getMetadata` takes no diagnostics out-pointer; the result code is
		// the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Metadata(IUnknown(
			std::ptr::NonNull::new(metadata as *mut _).unwrap(),
		)))
	}
}

/// Extension interface of [`ComponentType`] for getting separate debug data
/// and host-callable code (`IComponentType2` in slang.h). Obtain it with
/// [`ComponentType::as_component_type2`].
///
/// Note: `IComponentType2` inherits `ISlangUnknown` directly in slang.h, not
/// `IComponentType`, so this is a `query_interface` cast rather than an
/// upcast.
#[repr(transparent)]
#[derive(Clone)]
pub struct ComponentType2(IUnknown);

unsafe impl Interface for ComponentType2 {
	type Vtable = sys::IComponentType2Vtable;
	const IID: UUID = uuid(0x9c2a_4b3d_7f68_4e91_a52c_8b19_3e45_7a9f);
}

impl ComponentType2 {
	/// Gets the compile result of the target at `target_index`, holding the
	/// base and debug output blobs and their metadata.
	pub fn target_compile_result(&self, target_index: i64) -> Result<CompileResult> {
		let mut compile_result = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				getTargetCompileResult(target_index, &mut compile_result, &mut diagnostics)
			),
			diagnostics,
		)?;

		Ok(CompileResult(IUnknown(
			std::ptr::NonNull::new(compile_result as *mut _).unwrap(),
		)))
	}

	/// Gets the compile result of the entry point at `entry_point_index` for
	/// the chosen `target_index`.
	pub fn entry_point_compile_result(
		&self,
		entry_point_index: i64,
		target_index: i64,
	) -> Result<CompileResult> {
		let mut compile_result = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				getEntryPointCompileResult(
					entry_point_index,
					target_index,
					&mut compile_result,
					&mut diagnostics
				)
			),
			diagnostics,
		)?;

		Ok(CompileResult(IUnknown(
			std::ptr::NonNull::new(compile_result as *mut _).unwrap(),
		)))
	}

	/// Compiles all entry points for the chosen `target_index` into
	/// host-machine code and returns them as a [`SharedLibrary`] whose
	/// exported symbols can be called directly from the application
	/// (`IComponentType2::getTargetHostCallable`). Like
	/// [`ComponentType::entry_point_host_callable`], but covers the whole
	/// target rather than a single entry point.
	///
	/// Returns `Err` when compilation fails; the error carries the diagnostics
	/// blob when Slang produced one.
	pub fn target_host_callable(&self, target_index: i32) -> Result<SharedLibrary> {
		let mut shared_library = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				getTargetHostCallable(target_index, &mut shared_library, &mut diagnostics)
			),
			diagnostics,
		)?;

		Ok(SharedLibrary(IUnknown(
			std::ptr::NonNull::new(shared_library as *mut _).unwrap(),
		)))
	}
}

/// Bindless resource metadata produced for a compiled target
/// (`IBindlessResourceMetadata` in slang.h). Cast from a [`Metadata`] object
/// with [`Metadata::cast_as`].
#[repr(transparent)]
#[derive(Clone)]
pub struct BindlessResourceMetadata(IUnknown);

unsafe impl Interface for BindlessResourceMetadata {
	type Vtable = sys::IBindlessResourceMetadataVtable;
	const IID: UUID = uuid(0xeafa_96d3_2352_4bf4_8864_3228_a407_7a83);
}

impl BindlessResourceMetadata {
	/// Returns true when the compiled target IR still contains a bindless
	/// descriptor-heap/resource-handle path after target-specific lowering.
	pub fn uses_bindless_resource_heap(&self) -> bool {
		vcall!(self, usesBindlessResourceHeap())
	}
}

/// Coverage tracing metadata produced when a shader coverage mode is active
/// (`ICoverageTracingMetadata` in slang.h). Cast from a [`Metadata`] object
/// with [`Metadata::cast_as`].
#[repr(transparent)]
#[derive(Clone)]
pub struct CoverageTracingMetadata(IUnknown);

unsafe impl Interface for CoverageTracingMetadata {
	type Vtable = sys::ICoverageTracingMetadataVtable;
	const IID: UUID = uuid(0x7c9f_1d50_1e4a_4b9c_8e21_3f7b_82a3_d951);
}

impl CoverageTracingMetadata {
	/// Number of runtime counter slots in the synthesized coverage buffer.
	pub fn counter_count(&self) -> u32 {
		vcall!(self, getCounterCount())
	}

	/// Number of source coverage entries.
	pub fn entry_count(&self) -> u32 {
		vcall!(self, getEntryCount())
	}

	/// Gets the attribution info of the source coverage entry at `index`
	/// (`ICoverageTracingMetadata::getEntryInfo`). Valid indices are
	/// `0..self.entry_count()`; an out-of-range index returns `Err`.
	pub fn entry_info(&self, index: u32) -> Result<CoverageEntryInfo<'_>> {
		let mut info = sys::slang_CoverageEntryInfo {
			structSize: std::mem::size_of::<sys::slang_CoverageEntryInfo>(),
			// SAFETY: `slang_CoverageEntryInfo` is a C struct of scalars and
			// pointers; an all-zero value is a valid instance.
			..unsafe { std::mem::zeroed() }
		};
		let result = vcall!(self, getEntryInfo(index, &mut info));
		// `getEntryInfo` takes no diagnostics out-pointer; the result code is
		// the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		// SAFETY: per slang.h, the string pointers returned through
		// `CoverageEntryInfo` remain valid for the lifetime of the metadata
		// object, which outlives the `&self` borrow the result is tied to.
		Ok(unsafe {
			CoverageEntryInfo {
				file: str_from_slang(info.file),
				line: info.line,
				counter_index: info.counterIndex,
				kind: info.kind,
				counter_mode: info.counterMode,
				start_column: info.startColumn,
				end_line: info.endLine,
				end_column: info.endColumn,
				function_name: str_from_slang(info.functionName),
				function_mangled_name: str_from_slang(info.functionMangledName),
				branch_site_id: info.branchSiteID,
				branch_arm_id: info.branchArmID,
				branch_arm_kind: info.branchArmKind,
			}
		})
	}

	/// Gets the descriptor binding of the synthesized coverage buffer
	/// (`ICoverageTracingMetadata::getBufferInfo`). Kept by Slang for
	/// compatibility; new integrations should prefer
	/// [`SyntheticResourceMetadata`], which also reports CPU/CUDA marshaling
	/// locations.
	pub fn buffer_info(&self) -> Result<CoverageBufferInfo> {
		let mut info = sys::slang_CoverageBufferInfo {
			structSize: std::mem::size_of::<sys::slang_CoverageBufferInfo>(),
			// SAFETY: `slang_CoverageBufferInfo` is a C struct of scalars; an
			// all-zero value is a valid instance.
			..unsafe { std::mem::zeroed() }
		};
		let result = vcall!(self, getBufferInfo(&mut info));
		// `getBufferInfo` takes no diagnostics out-pointer; the result code is
		// the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(CoverageBufferInfo {
			space: info.space,
			binding: info.binding,
			element_byte_width: info.elementByteWidth,
		})
	}
}

/// Per-coverage-entry attribution returned by
/// [`CoverageTracingMetadata::entry_info`] (`slang::CoverageEntryInfo` in
/// slang.h, minus the leading `structSize` ABI field). Borrows strings owned
/// by the metadata object.
pub struct CoverageEntryInfo<'a> {
	/// Source file for this coverage entry, if it could be attributed to a
	/// real source file.
	pub file: Option<&'a str>,
	/// 1-based source line for this entry, or 0 when unattributed.
	pub line: u32,
	/// Counter slot used by this entry, or the invalid-counter sentinel when
	/// the entry has no runtime counter.
	pub counter_index: u32,
	/// Semantic kind of this source coverage entry.
	pub kind: CoverageEntryKind,
	/// Runtime accumulation mode for `counter_index`.
	pub counter_mode: CoverageCounterMode,
	/// 1-based inclusive start column, or 0 when unavailable.
	pub start_column: u32,
	/// 1-based end line of this entry's half-open end coordinate, or 0 when
	/// the exact range is unavailable.
	pub end_line: u32,
	/// 1-based exclusive end column, or 0 when unavailable.
	pub end_column: u32,
	/// Function display name for function coverage entries, when applicable.
	pub function_name: Option<&'a str>,
	/// Stable mangled function name for function coverage entries, when
	/// applicable.
	pub function_mangled_name: Option<&'a str>,
	/// Stable branch-site identifier within the metadata object, or 0 when not
	/// applicable.
	pub branch_site_id: u32,
	/// Stable branch-arm identifier within `branch_site_id`, or 0 when not
	/// applicable.
	pub branch_arm_id: u32,
	/// Branch arm semantic for branch coverage entries.
	pub branch_arm_kind: CoverageBranchArmKind,
}

/// Coverage-buffer descriptor binding info returned by
/// [`CoverageTracingMetadata::buffer_info`] (`slang::CoverageBufferInfo` in
/// slang.h, minus the leading `structSize` ABI field). A `-1` sentinel means
/// the value is not reported for the current target.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CoverageBufferInfo {
	/// Register space the coverage buffer is bound to (D3D12 `space`, Vulkan
	/// descriptor set), or -1 if not assigned for this target.
	pub space: i32,
	/// Binding index the coverage buffer is bound at (D3D12 `register`, Vulkan
	/// `binding`), or -1 if not assigned for this target.
	pub binding: i32,
	/// Byte width of one counter slot in the synthesized buffer (4 for
	/// `RWStructuredBuffer<uint>`, 8 for `RWStructuredBuffer<uint64_t>`).
	pub element_byte_width: u32,
}

/// Metadata for compiler-synthesized bindable resources
/// (`ISyntheticResourceMetadata` in slang.h). Cast from a [`Metadata`] object
/// with [`Metadata::cast_as`].
#[repr(transparent)]
#[derive(Clone)]
pub struct SyntheticResourceMetadata(IUnknown);

unsafe impl Interface for SyntheticResourceMetadata {
	type Vtable = sys::ISyntheticResourceMetadataVtable;
	const IID: UUID = uuid(0x47a3_3723_181b_4d2b_b89e_2154_95bb_388b);
}

impl SyntheticResourceMetadata {
	/// Number of synthetic bindable resources reported by this metadata
	/// object.
	pub fn resource_count(&self) -> u32 {
		vcall!(self, getResourceCount())
	}

	/// Gets the info of the synthetic resource at `index`
	/// (`ISyntheticResourceMetadata::getResourceInfo`). Valid indices are
	/// `0..self.resource_count()`; an out-of-range index returns `Err`.
	pub fn resource_info(&self, index: u32) -> Result<SyntheticResourceInfo<'_>> {
		let mut info = sys::slang_SyntheticResourceInfo {
			structSize: std::mem::size_of::<sys::slang_SyntheticResourceInfo>(),
			// SAFETY: `slang_SyntheticResourceInfo` is a C struct of scalars
			// and pointers; an all-zero value is a valid instance.
			..unsafe { std::mem::zeroed() }
		};
		let result = vcall!(self, getResourceInfo(index, &mut info));
		// `getResourceInfo` takes no diagnostics out-pointer; the result code
		// is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		// SAFETY: per slang.h, the string pointers returned through
		// `SyntheticResourceInfo` remain valid for the lifetime of the metadata
		// object, which outlives the `&self` borrow the result is tied to.
		Ok(unsafe {
			SyntheticResourceInfo {
				id: info.id,
				// SAFETY: `slang::BindingType` (the enum class stored in the sys
				// struct) and the C `SlangBindingType` are both u32 enums whose
				// values slang.h defines one in terms of the other, so the bit
				// patterns are interchangeable.
				binding_type: std::mem::transmute::<sys::slang_BindingType, BindingType>(
					info.bindingType,
				),
				array_size: info.arraySize,
				scope: info.scope,
				access: info.access,
				entry_point_index: info.entryPointIndex,
				space: info.space,
				binding: info.binding,
				uniform_offset: info.uniformOffset,
				uniform_stride: info.uniformStride,
				debug_name: str_from_slang(info.debugName),
			}
		})
	}

	/// Finds the index of the synthetic resource with the given `id`
	/// (`ISyntheticResourceMetadata::findResourceIndexByID`). Returns `Err`
	/// (with `SLANG_E_NOT_FOUND`) when no resource carries that id; id 0 is
	/// reserved as the "unassigned" sentinel and always fails.
	pub fn find_resource_index_by_id(&self, id: u32) -> Result<u32> {
		let mut index = 0;
		let result = vcall!(self, findResourceIndexByID(id, &mut index));
		// `findResourceIndexByID` takes no diagnostics out-pointer; the result
		// code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(index)
	}
}

/// Info of a compiler-synthesized bindable resource returned by
/// [`SyntheticResourceMetadata::resource_info`]
/// (`slang::SyntheticResourceInfo` in slang.h, minus the leading `structSize`
/// ABI field). Borrows strings owned by the metadata object. See slang.h for
/// the `-1`/`0` sentinel conventions of the location fields.
pub struct SyntheticResourceInfo<'a> {
	/// Stable, opaque, non-zero synthetic resource identifier within the
	/// compiled program.
	pub id: u32,
	/// The Slang binding kind represented by this synthetic resource.
	pub binding_type: BindingType,
	/// Number of logical resources in the synthetic binding.
	pub array_size: u32,
	/// Whether the resource is global/root-scoped or attached to a specific
	/// entry point.
	pub scope: SyntheticResourceScope,
	/// Intended access pattern for the resource.
	pub access: SyntheticResourceAccess,
	/// Entry point index when `scope` is entry-point scoped, else -1.
	pub entry_point_index: i32,
	/// Descriptor space (D3D12 `space`, Vulkan descriptor set), or -1 when not
	/// reported for this target.
	pub space: i32,
	/// Descriptor binding index, or -1 when unavailable for this target.
	pub binding: i32,
	/// CPU/CUDA-style marshaling location in bytes, or -1 when unavailable.
	pub uniform_offset: i32,
	/// Byte stride between adjacent logical elements when CPU/CUDA-style
	/// marshaling is reported.
	pub uniform_stride: i32,
	/// Optional stable debug name for the synthetic resource.
	pub debug_name: Option<&'a str>,
}

/// Cooperative matrix and vector metadata (`ICooperativeTypesMetadata` in
/// slang.h). Cast from a [`Metadata`] object with [`Metadata::cast_as`].
#[repr(transparent)]
#[derive(Clone)]
pub struct CooperativeTypesMetadata(IUnknown);

unsafe impl Interface for CooperativeTypesMetadata {
	type Vtable = sys::ICooperativeTypesMetadataVtable;
	const IID: UUID = uuid(0x64c4_d536_d949_49c3_9fde_3f0f_9c6f_0131);
}

impl CooperativeTypesMetadata {
	/// Number of cooperative matrix types used by the compiled target.
	pub fn cooperative_matrix_type_count(&self) -> u64 {
		vcall!(self, getCooperativeMatrixTypeCount())
	}

	/// Number of cooperative matrix combinations used by the compiled target.
	pub fn cooperative_matrix_combination_count(&self) -> u64 {
		vcall!(self, getCooperativeMatrixCombinationCount())
	}

	/// Number of cooperative vector type-usage records of the compiled target.
	pub fn cooperative_vector_type_count(&self) -> u64 {
		vcall!(self, getCooperativeVectorTypeCount())
	}

	/// Number of cooperative vector combinations used by the compiled target.
	pub fn cooperative_vector_combination_count(&self) -> u64 {
		vcall!(self, getCooperativeVectorCombinationCount())
	}

	/// Gets the cooperative matrix type at `index`
	/// (`ICooperativeTypesMetadata::getCooperativeMatrixTypeByIndex`). Valid
	/// indices are `0..self.cooperative_matrix_type_count()`; an out-of-range
	/// index returns `Err`.
	pub fn cooperative_matrix_type_by_index(&self, index: u64) -> Result<CooperativeMatrixType> {
		// SAFETY: `slang_CooperativeMatrixType` is a C struct of scalars; an
		// all-zero value is a valid instance.
		let mut ty: sys::slang_CooperativeMatrixType = unsafe { std::mem::zeroed() };
		let result = vcall!(self, getCooperativeMatrixTypeByIndex(index, &mut ty));
		// `getCooperativeMatrixTypeByIndex` takes no diagnostics out-pointer;
		// the result code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(ty)
	}

	/// Gets the cooperative matrix combination at `index`
	/// (`ICooperativeTypesMetadata::getCooperativeMatrixCombinationByIndex`).
	/// Valid indices are `0..self.cooperative_matrix_combination_count()`; an
	/// out-of-range index returns `Err`.
	pub fn cooperative_matrix_combination_by_index(
		&self,
		index: u64,
	) -> Result<CooperativeMatrixCombination> {
		// SAFETY: `slang_CooperativeMatrixCombination` is a C struct of
		// scalars; an all-zero value is a valid instance.
		let mut combination: sys::slang_CooperativeMatrixCombination =
			unsafe { std::mem::zeroed() };
		let result = vcall!(
			self,
			getCooperativeMatrixCombinationByIndex(index, &mut combination)
		);
		// `getCooperativeMatrixCombinationByIndex` takes no diagnostics
		// out-pointer; the result code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(combination)
	}

	/// Gets the cooperative vector type-usage record at `index`
	/// (`ICooperativeTypesMetadata::getCooperativeVectorTypeByIndex`). Valid
	/// indices are `0..self.cooperative_vector_type_count()`; an out-of-range
	/// index returns `Err`.
	pub fn cooperative_vector_type_by_index(
		&self,
		index: u64,
	) -> Result<CooperativeVectorTypeUsageInfo> {
		// SAFETY: `slang_CooperativeVectorTypeUsageInfo` is a C struct of
		// scalars; an all-zero value is a valid instance.
		let mut ty: sys::slang_CooperativeVectorTypeUsageInfo = unsafe { std::mem::zeroed() };
		let result = vcall!(self, getCooperativeVectorTypeByIndex(index, &mut ty));
		// `getCooperativeVectorTypeByIndex` takes no diagnostics out-pointer;
		// the result code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(ty)
	}

	/// Gets the cooperative vector combination at `index`
	/// (`ICooperativeTypesMetadata::getCooperativeVectorCombinationByIndex`).
	/// Valid indices are `0..self.cooperative_vector_combination_count()`; an
	/// out-of-range index returns `Err`.
	pub fn cooperative_vector_combination_by_index(
		&self,
		index: u64,
	) -> Result<CooperativeVectorCombination> {
		// SAFETY: `slang_CooperativeVectorCombination` is a C struct of
		// scalars; an all-zero value is a valid instance.
		let mut combination: sys::slang_CooperativeVectorCombination =
			unsafe { std::mem::zeroed() };
		let result = vcall!(
			self,
			getCooperativeVectorCombinationByIndex(index, &mut combination)
		);
		// `getCooperativeVectorCombinationByIndex` takes no diagnostics
		// out-pointer; the result code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(combination)
	}
}

/// An owned reference to a Slang component type (`IComponentType` in
/// slang.h): a composable unit of shader code — a [`Module`], an
/// [`EntryPoint`], a [`TypeConformance`], or a composite/specialized/linked
/// combination thereof.
#[repr(transparent)]
#[derive(Clone)]
pub struct ComponentType(IUnknown);

unsafe impl Interface for ComponentType {
	type Vtable = sys::IComponentTypeVtable;
	const IID: UUID = uuid(0x5bc4_2be8_5c50_4929_9e5e_d15e_7c24_015f);
}

impl ComponentType {
	/// Gets the session this component type belongs to.
	pub fn get_session(&self) -> Session {
		let session = vcall!(self, getSession());
		let session = Session(IUnknown(std::ptr::NonNull::new(session as *mut _).unwrap()));
		// SAFETY: `session` is non-null, so the vtable call is valid.
		// `getSession` returns a borrowed reference — the session owns the
		// component type. Adding a reference turns it into an owned pointer
		// matching the `IUnknown` RAII drop semantics.
		unsafe { (session.as_unknown().vtable().ISlangUnknown_addRef)(session.as_raw()) };
		session
	}

	/// Gets the layout of this program for the chosen `target`
	/// (`IComponentType::getLayout`). The layout establishes offsets/bindings
	/// for all global and entry-point shader parameters. The returned
	/// reflection is owned by the component type.
	///
	/// Returns `Err` when layout fails (e.g. the component type is not fully
	/// specialized or linked); the error carries the diagnostics blob when
	/// Slang produced one.
	pub fn layout(&self, target: i64) -> Result<&reflection::Shader> {
		let mut diagnostics = null_mut();
		let ptr = vcall!(self, getLayout(target, &mut diagnostics));

		if ptr.is_null() {
			// A null layout is the only error signal; fall back to a generic
			// failure code when Slang did not fill the diagnostics blob.
			Err(error_from_diagnostics(diagnostics))
		} else {
			// SAFETY: `ptr` is non-null and points to a reflection object owned
			// by the component type, which outlives `&self`.
			Ok(unsafe { &*(ptr as *const _) })
		}
	}

	/// Gets the number of (unspecialized) specialization parameters of this
	/// component type.
	pub fn specialization_param_count(&self) -> i64 {
		vcall!(self, getSpecializationParamCount())
	}

	/// Specializes the component by binding its specialization parameters to
	/// concrete arguments. `specialization_args.len()` must match
	/// `specialization_param_count()`.
	pub fn specialize(&self, specialization_args: &[SpecializationArg]) -> Result<ComponentType> {
		let raw_args = specialization_args_as_sys(specialization_args);
		let mut specialized_component_type = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				specialize(
					raw_args.as_ptr(),
					raw_args.len() as _,
					&mut specialized_component_type,
					&mut diagnostics
				)
			),
			diagnostics,
		)?;

		Ok(ComponentType(IUnknown(
			std::ptr::NonNull::new(specialized_component_type as *mut _).unwrap(),
		)))
	}

	/// Computes a hash for the entry point at `entry_point_index` for the
	/// chosen `target_index`, usable as a key for shader caching.
	pub fn entry_point_hash(&self, entry_point_index: i64, target_index: i64) -> Blob {
		let mut hash = null_mut();
		vcall!(
			self,
			getEntryPointHash(entry_point_index, target_index, &mut hash)
		);
		Blob(IUnknown(std::ptr::NonNull::new(hash as *mut _).unwrap()))
	}

	/// Returns a new component type that represents a renamed entry point.
	/// This component type must be a single entry point, or a composite or
	/// specialized component type that contains one entry point component.
	pub fn rename_entry_point(&self, new_name: &str) -> Result<ComponentType> {
		let new_name = CString::new(new_name).unwrap();
		let mut entry_point = null_mut();

		let result = vcall!(self, renameEntryPoint(new_name.as_ptr(), &mut entry_point));
		if !succeeded(result) {
			// `renameEntryPoint` takes no diagnostics out-pointer; the result
			// code is the only error signal.
			return Err(Error::Code(result));
		}

		Ok(ComponentType(IUnknown(
			std::ptr::NonNull::new(entry_point as *mut _).unwrap(),
		)))
	}

	/// Links this component type, specifying additional compiler options used
	/// when generating code from the linked program.
	pub fn link_with_options(&self, options: &CompilerOptions) -> Result<ComponentType> {
		let mut linked_component_type = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				linkWithOptions(
					&mut linked_component_type,
					options.options.len() as _,
					options.options.as_ptr() as _,
					&mut diagnostics
				)
			),
			diagnostics,
		)?;

		Ok(ComponentType(IUnknown(
			std::ptr::NonNull::new(linked_component_type as *mut _).unwrap(),
		)))
	}

	/// Links this component type against all of its unsatisfied dependencies
	/// (`IComponentType::link`), e.g. the modules a module `import`s.
	///
	/// Returns `Err` when linking fails; the error carries the diagnostics
	/// blob when Slang produced one.
	pub fn link(&self) -> Result<ComponentType> {
		let mut linked_component_type = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(self, link(&mut linked_component_type, &mut diagnostics)),
			diagnostics,
		)?;

		Ok(ComponentType(IUnknown(
			std::ptr::NonNull::new(linked_component_type as *mut _).unwrap(),
		)))
	}

	/// Gets the compiled code for the chosen `target`
	/// (`IComponentType::getTargetCode`). Requires a fully specialized and
	/// fully linked component type.
	///
	/// Returns `Err` when code generation fails; the error carries the
	/// diagnostics blob when Slang produced one.
	pub fn target_code(&self, target: i64) -> Result<Blob> {
		let mut code = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(self, getTargetCode(target, &mut code, &mut diagnostics)),
			diagnostics,
		)?;

		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(code as *mut _).unwrap(),
		)))
	}

	/// Gets the compiled code for the entry point at `index` for the chosen
	/// `target` (`IComponentType::getEntryPointCode`). Requires a fully
	/// specialized and fully linked component type.
	///
	/// Returns `Err` when code generation fails; the error carries the
	/// diagnostics blob when Slang produced one.
	pub fn entry_point_code(&self, index: i64, target: i64) -> Result<Blob> {
		let mut code = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				getEntryPointCode(index, target, &mut code, &mut diagnostics)
			),
			diagnostics,
		)?;

		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(code as *mut _).unwrap(),
		)))
	}

	/// Gets the compilation result for the entry point at `index` for the
	/// chosen `target` as an in-memory [`MutableFileSystem`]
	/// (`IComponentType::getResultAsFileSystem`). The compiled code plus any
	/// associated artifacts (diagnostics, source maps, ...) are exposed as
	/// files in the returned file system instead of being written to disk.
	/// Has the same requirements as [`ComponentType::entry_point_code`].
	///
	/// Returns `Err` when code generation fails; `getResultAsFileSystem`
	/// takes no diagnostics out-pointer, so the error carries only the result
	/// code (any diagnostics are available as a file inside the file system).
	pub fn get_result_as_file_system(&self, index: i64, target: i64) -> Result<MutableFileSystem> {
		let mut file_system = null_mut();

		let result = vcall!(self, getResultAsFileSystem(index, target, &mut file_system));
		if !succeeded(result) {
			return Err(Error::Code(result));
		}

		Ok(MutableFileSystem(IUnknown(
			std::ptr::NonNull::new(file_system as *mut _).unwrap(),
		)))
	}

	/// Gets metadata for the chosen `target_index`
	/// (`IComponentType::getTargetMetadata`). Has the same requirements as
	/// [`ComponentType::entry_point_code`].
	///
	/// Returns `Err` when the metadata cannot be produced; the error carries
	/// the diagnostics blob when Slang produced one.
	pub fn target_metadata(&self, target_index: i64) -> Result<Metadata> {
		let mut metadata = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				getTargetMetadata(target_index, &mut metadata, &mut diagnostics)
			),
			diagnostics,
		)?;

		Ok(Metadata(IUnknown(
			std::ptr::NonNull::new(metadata as *mut _).unwrap(),
		)))
	}

	/// Gets metadata for the entry point at `entry_point_index` for the chosen
	/// `target_index` (`IComponentType::getEntryPointMetadata`). Has the same
	/// requirements as [`ComponentType::entry_point_code`].
	///
	/// Returns `Err` when the metadata cannot be produced; the error carries
	/// the diagnostics blob when Slang produced one.
	pub fn entry_point_metadata(
		&self,
		entry_point_index: i64,
		target_index: i64,
	) -> Result<Metadata> {
		let mut metadata = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				getEntryPointMetadata(
					entry_point_index,
					target_index,
					&mut metadata,
					&mut diagnostics
				)
			),
			diagnostics,
		)?;

		Ok(Metadata(IUnknown(
			std::ptr::NonNull::new(metadata as *mut _).unwrap(),
		)))
	}

	/// Compiles the entry point at `entry_point_index` for the chosen
	/// `target_index` into host-machine code and returns it as a
	/// [`SharedLibrary`] whose exported symbols can be called directly from
	/// the application (`IComponentType::getEntryPointHostCallable`). Requires
	/// a compilation target with a host-callable format (e.g.
	/// [`CompileTarget::ShaderHostCallable`]) and a downstream CPU compiler —
	/// see the "Host callable" section of Slang's `docs/cpu-target.md` for the
	/// ABI of the exported functions. The indices are plain `int` in slang.h
	/// (not `SlangInt`), hence `i32` here.
	///
	/// Returns `Err` when compilation fails; the error carries the diagnostics
	/// blob when Slang produced one.
	pub fn entry_point_host_callable(
		&self,
		entry_point_index: i32,
		target_index: i32,
	) -> Result<SharedLibrary> {
		let mut shared_library = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				getEntryPointHostCallable(
					entry_point_index,
					target_index,
					&mut shared_library,
					&mut diagnostics
				)
			),
			diagnostics,
		)?;

		Ok(SharedLibrary(IUnknown(
			std::ptr::NonNull::new(shared_library as *mut _).unwrap(),
		)))
	}

	/// Queries this component type for the [`ComponentType2`] extension
	/// interface (separate debug data). Returns `None` when the object does
	/// not implement `IComponentType2`.
	pub fn as_component_type2(&self) -> Option<ComponentType2> {
		self.as_unknown().query_interface()
	}
}

/// An owned reference to a Slang entry point (`IEntryPoint` in slang.h): a
/// shader entry point function, obtained from a [`Module`]. Converts into
/// [`ComponentType`] via `From` for composition and code generation.
#[repr(transparent)]
#[derive(Clone)]
pub struct EntryPoint(IUnknown);

unsafe impl Interface for EntryPoint {
	type Vtable = sys::IEntryPointVtable;
	const IID: UUID = uuid(0x8f24_1361_f5bd_4ca0_a3ac_02f7_fa24_02b8);
}

impl From<EntryPoint> for ComponentType {
	fn from(value: EntryPoint) -> Self {
		// SAFETY: `IEntryPoint` inherits `IComponentType` in slang.h, so the
		// interface pointer is a valid `IComponentType` pointer.
		unsafe { upcast(value) }
	}
}

impl EntryPoint {
	/// Gets the reflection of the entry point function
	/// (`IEntryPoint::getFunctionReflection`). The returned reflection is
	/// owned by the entry point.
	pub fn function_reflection(&self) -> &reflection::Function {
		let ptr = vcall!(self, getFunctionReflection());
		// SAFETY: `ptr` is non-null for a valid entry point and points to a
		// reflection object owned by the entry point, which outlives `&self`.
		unsafe { &*(ptr as *const _) }
	}
}

/// A component type representing a type's conformance to an interface
/// (`ITypeConformance` in slang.h). Created by
/// `Session::create_type_conformance_component_type`; include it in
/// `Session::create_composite_component_type` (via `ComponentType::from`) to
/// control which interface implementations end up in the compiled code, and
/// optionally to override the dispatch ID of an implementation type.
#[repr(transparent)]
#[derive(Clone)]
pub struct TypeConformance(IUnknown);

unsafe impl Interface for TypeConformance {
	type Vtable = sys::ITypeConformanceVtable;
	const IID: UUID = uuid(0x73eb_3147_e544_41b5_b8f0_a244_df21_940b);
}

impl From<TypeConformance> for ComponentType {
	fn from(value: TypeConformance) -> Self {
		// SAFETY: `ITypeConformance` inherits `IComponentType` in slang.h, so
		// the interface pointer is a valid `IComponentType` pointer.
		unsafe { upcast(value) }
	}
}

/// An owned reference to a Slang module (`IModule` in slang.h): the
/// granularity of shader code compilation and loading, typically a single
/// `.slang`/`.hlsl` file and everything it `#include`s. Loaded with
/// [`Session::load_module`]; converts into [`ComponentType`] via `From`.
#[repr(transparent)]
#[derive(Clone)]
pub struct Module(IUnknown);

unsafe impl Interface for Module {
	type Vtable = sys::IModuleVtable;
	const IID: UUID = uuid(0x0c72_0e64_8722_4d31_8990_638a_98b1_c279);
}

impl From<Module> for ComponentType {
	fn from(value: Module) -> Self {
		// SAFETY: `IModule` inherits `IComponentType` in slang.h, so the
		// interface pointer is a valid `IComponentType` pointer.
		unsafe { upcast(value) }
	}
}

impl Module {
	/// Finds an entry point by name (`IModule::findEntryPointByName`).
	/// Returns `None` when no entry point with that name exists. Note this
	/// only finds functions explicitly designated as entry points, e.g. with
	/// a `[shader("...")]` attribute; use [`Module::find_and_check_entry_point`]
	/// otherwise.
	pub fn find_entry_point_by_name(&self, name: &str) -> Option<EntryPoint> {
		let name = CString::new(name).unwrap();
		let mut entry_point = null_mut();
		vcall!(self, findEntryPointByName(name.as_ptr(), &mut entry_point));
		Some(EntryPoint(IUnknown(std::ptr::NonNull::new(
			entry_point as *mut _,
		)?)))
	}

	/// Gets the number of entry points defined in the module
	/// (`IModule::getDefinedEntryPointCount`).
	pub fn entry_point_count(&self) -> u32 {
		vcall!(self, getDefinedEntryPointCount()) as _
	}

	/// Gets the entry point defined in the module at `index`
	/// (`IModule::getDefinedEntryPoint`). Returns `None` when `index` is out
	/// of range.
	pub fn entry_point_by_index(&self, index: u32) -> Option<EntryPoint> {
		let mut entry_point = null_mut();
		vcall!(self, getDefinedEntryPoint(index as _, &mut entry_point));
		Some(EntryPoint(IUnknown(std::ptr::NonNull::new(
			entry_point as *mut _,
		)?)))
	}

	/// Iterates over the entry points defined in the module.
	pub fn entry_points(&self) -> impl ExactSizeIterator<Item = EntryPoint> {
		(0..self.entry_point_count()).map(|i| self.entry_point_by_index(i).unwrap())
	}

	/// Gets the name of the module (`IModule::getName`).
	pub fn name(&self) -> Option<&str> {
		let name = vcall!(self, getName());
		// SAFETY: the string returned by `getName` is owned by the module and
		// outlives `&self`.
		unsafe { str_from_slang(name) }
	}

	/// Gets the path of the module (`IModule::getFilePath`), e.g. the source
	/// file it was loaded from.
	pub fn file_path(&self) -> Option<&str> {
		let path = vcall!(self, getFilePath());
		// SAFETY: the string returned by `getFilePath` is owned by the module
		// and outlives `&self`.
		unsafe { str_from_slang(path) }
	}

	/// Gets the unique identity of the module (`IModule::getUniqueIdentity`),
	/// usable to distinguish two modules with the same name.
	pub fn unique_identity(&self) -> Option<&str> {
		let identity = vcall!(self, getUniqueIdentity());
		// SAFETY: the string returned by `getUniqueIdentity` is owned by the
		// module and outlives `&self`.
		unsafe { str_from_slang(identity) }
	}

	/// Gets the number of files this module depends on, including both the
	/// explicit source files and any files transitively referenced (e.g. via
	/// `#include`) (`IModule::getDependencyFileCount`).
	pub fn dependency_file_count(&self) -> i32 {
		vcall!(self, getDependencyFileCount())
	}

	/// Gets the path of the dependency file at `index`
	/// (`IModule::getDependencyFilePath`).
	pub fn dependency_file_path(&self, index: i32) -> Option<&str> {
		let path = vcall!(self, getDependencyFilePath(index));
		// SAFETY: the string returned by `getDependencyFilePath` is owned by
		// the module and outlives `&self`.
		unsafe { str_from_slang(path) }
	}

	/// Iterates over the paths of the files this module depends on.
	pub fn dependency_file_paths(&self) -> impl ExactSizeIterator<Item = Option<&str>> {
		(0..self.dependency_file_count()).map(|i| self.dependency_file_path(i))
	}

	/// Gets the reflection of the module's declaration
	/// (`IModule::getModuleReflection`). The returned reflection is owned by
	/// the module.
	pub fn module_reflection(&self) -> &reflection::Decl {
		let ptr = vcall!(self, getModuleReflection());
		// SAFETY: `ptr` is non-null for a valid module and points to a
		// reflection declaration owned by the module, which outlives `&self`.
		unsafe { &*(ptr as *const _) }
	}

	/// Serializes the checked module into a blob, suitable for
	/// [`Session::load_module_from_ir_blob`].
	pub fn serialize(&self) -> Result<Blob> {
		let mut blob = null_mut();
		let result = vcall!(self, serialize(&mut blob));
		// `serialize` takes no diagnostics out-pointer; the result code is the
		// only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(blob as *mut _).unwrap(),
		)))
	}

	/// Writes the serialized representation of this module to a file.
	pub fn write_to_file(&self, file_name: &str) -> Result<()> {
		let file_name = CString::new(file_name).unwrap();
		let result = vcall!(self, writeToFile(file_name.as_ptr()));
		// `writeToFile` takes no diagnostics out-pointer; the result code is
		// the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Finds and validates an entry point by name, even if the function is not
	/// marked with the `[shader("...")]` attribute.
	pub fn find_and_check_entry_point(&self, name: &str, stage: Stage) -> Result<EntryPoint> {
		let name = CString::new(name).unwrap();
		let mut entry_point = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				findAndCheckEntryPoint(name.as_ptr(), stage, &mut entry_point, &mut diagnostics)
			),
			diagnostics,
		)?;

		Ok(EntryPoint(IUnknown(
			std::ptr::NonNull::new(entry_point as *mut _).unwrap(),
		)))
	}

	/// Disassembles the module into human-readable IR text.
	pub fn disassemble(&self) -> Result<String> {
		let mut blob = null_mut();
		let result = vcall!(self, disassemble(&mut blob));
		// `disassemble` takes no diagnostics out-pointer; the result code is
		// the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		let blob = Blob(IUnknown(std::ptr::NonNull::new(blob as *mut _).unwrap()));
		Ok(String::from_utf8_lossy(blob.as_slice()).into_owned())
	}

	/// Queries this module for the experimental precompile service
	/// ([`ModulePrecompileService`]). Returns `None` when the module object
	/// does not implement the service interface.
	pub fn precompile_service(&self) -> Option<ModulePrecompileService> {
		self.as_unknown().query_interface()
	}
}

/// Experimental interface for target precompilation of a Slang module
/// (`IModulePrecompileService_Experimental` in slang.h). Obtained from
/// [`Module::precompile_service`].
///
/// NOTE: this API is experimental in Slang.
#[repr(transparent)]
#[derive(Clone)]
pub struct ModulePrecompileService(IUnknown);

unsafe impl Interface for ModulePrecompileService {
	type Vtable = sys::IModulePrecompileServiceExperimentalVtable;
	const IID: UUID = uuid(0x8e12_e8e3_5fcd_433e_afcb_13a0_88bc_5ee5);
}

impl ModulePrecompileService {
	/// Precompiles the module for `target` and embeds the resulting target
	/// library in the module
	/// (`IModulePrecompileService_Experimental::precompileForTarget`).
	///
	/// This mutates the module by adding precompiled target IR and temporary
	/// export metadata; per slang.h it is not thread-safe — callers must
	/// externally synchronize access to the module and must not call this
	/// concurrently with other operations on the same module or session.
	///
	/// Returns `Err` when precompilation fails; the error carries the
	/// diagnostics blob when Slang produced one.
	pub fn precompile_for_target(&self, target: CompileTarget) -> Result<()> {
		let mut diagnostics = null_mut();
		result_from_blob(
			vcall!(self, precompileForTarget(target, &mut diagnostics)),
			diagnostics,
		)
	}

	/// Gets the precompiled target code embedded by a previous
	/// [`ModulePrecompileService::precompile_for_target`] call
	/// (`IModulePrecompileService_Experimental::getPrecompiledTargetCode`).
	///
	/// Returns `Err` when the module has not been precompiled for `target`;
	/// the error carries the diagnostics blob when Slang produced one.
	pub fn precompiled_target_code(&self, target: CompileTarget) -> Result<Blob> {
		let mut code = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				getPrecompiledTargetCode(target, &mut code, &mut diagnostics)
			),
			diagnostics,
		)?;

		Ok(Blob(IUnknown(
			std::ptr::NonNull::new(code as *mut _).unwrap(),
		)))
	}

	/// Gets the number of modules this module depends on
	/// (`IModulePrecompileService_Experimental::getModuleDependencyCount`).
	pub fn module_dependency_count(&self) -> i64 {
		vcall!(self, getModuleDependencyCount())
	}

	/// Gets the module dependency at `index`
	/// (`IModulePrecompileService_Experimental::getModuleDependency`).
	///
	/// Returns `Err` when `index` is out of range. Note Slang signals this
	/// inconsistently per implementation: with `SLANG_E_INVALID_ARG`, or with
	/// `SLANG_OK` and a null module pointer (the plain-`Module`
	/// implementation) — the wrapper maps both to `Err`.
	pub fn module_dependency(&self, index: i64) -> Result<Module> {
		let mut module = null_mut();
		let mut diagnostics = null_mut();

		result_from_blob(
			vcall!(
				self,
				getModuleDependency(index, &mut module, &mut diagnostics)
			),
			diagnostics,
		)?;

		let Some(module) = std::ptr::NonNull::new(module as *mut _) else {
			return Err(Error::Code(sys::SLANG_E_INVALID_ARG));
		};
		Ok(Module(IUnknown(module)))
	}
}

/// An owned reference to a Slang bytecode runner (`IByteCodeRunner` in
/// slang.h): an experimental interpreter that executes Slang bytecode modules.
/// Created with [`ByteCodeRunner::new`].
///
/// This wrapper covers the module/function selection surface. The raw-pointer
/// and callback-driven methods of `IByteCodeRunner` (`execute`,
/// `getCurrentWorkingSet`, `getReturnValue`, `registerExtCall`,
/// `setPrintCallback`, `setExtInstHandlerUserData`) are deliberately not
/// wrapped; the sys vtable still carries their slots.
///
/// Loaded-state guard: in Slang v2026.14.1 the interpreter object leaves its
/// module view uninitialized until `loadModule` succeeds, and
/// `findFunctionByName`/`getFunctionInfo`/`selectFunctionByIndex` read it
/// unconditionally — calling them on a fresh runner dereferences indeterminate
/// memory inside Slang (observed as a SIGBUS on macOS aarch64). The wrapper
/// therefore tracks whether a module was loaded successfully and short-circuits
/// the module-dependent queries itself until then.
///
/// NOTE: this API is experimental in Slang.
#[derive(Clone)]
pub struct ByteCodeRunner {
	inner: ByteCodeRunnerInner,
	// Shared across clones so every handle to the same runner agrees on
	// whether a module was loaded.
	module_loaded: Arc<AtomicBool>,
}

/// The raw COM wrapper behind [`ByteCodeRunner`], kept `repr(transparent)` so
/// the [`Interface`] safety contract holds.
#[repr(transparent)]
#[derive(Clone)]
struct ByteCodeRunnerInner(IUnknown);

unsafe impl Interface for ByteCodeRunnerInner {
	type Vtable = sys::IByteCodeRunnerVtable;
	const IID: UUID = uuid(0xafda_b195_361f_42cb_9513_9006_261d_d8cd);
}

impl ByteCodeRunner {
	/// Creates a bytecode runner with the default description
	/// (`slang_createByteCodeRunner`).
	pub fn new() -> Result<ByteCodeRunner> {
		let desc = sys::slang_ByteCodeRunnerDesc {
			structSize: std::mem::size_of::<sys::slang_ByteCodeRunnerDesc>(),
		};
		let mut runner = null_mut();
		// SAFETY: `desc` and `runner` are valid in/out pointers; on success the
		// out-pointer receives a new reference owned by the caller.
		let result = unsafe { sys::slang_createByteCodeRunner(&desc, &mut runner) };
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(ByteCodeRunner {
			inner: ByteCodeRunnerInner(IUnknown(std::ptr::NonNull::new(runner as *mut _).unwrap())),
			module_loaded: Arc::new(AtomicBool::new(false)),
		})
	}

	/// Loads a bytecode module blob into the execution context
	/// (`IByteCodeRunner::loadModule`).
	pub fn load_module(&self, module: &Blob) -> Result<()> {
		let result = vcall!(self.inner, loadModule(module.as_raw()));
		// `loadModule` takes no diagnostics out-pointer; the result code is
		// the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		self.module_loaded.store(true, Ordering::Release);
		Ok(())
	}

	/// Selects the function at `index` for execution
	/// (`IByteCodeRunner::selectFunctionByIndex`). Returns `Err` with
	/// `SLANG_FAIL` when no module has been loaded yet.
	pub fn select_function_by_index(&self, index: u32) -> Result<()> {
		// See the loaded-state guard note on `ByteCodeRunner`.
		if !self.module_loaded.load(Ordering::Acquire) {
			return Err(Error::Code(sys::SLANG_FAIL));
		}
		let result = vcall!(self.inner, selectFunctionByIndex(index));
		// `selectFunctionByIndex` takes no diagnostics out-pointer; the result
		// code is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(())
	}

	/// Finds the index of the function with `name`
	/// (`IByteCodeRunner::findFunctionByName`), or -1 when the loaded module
	/// has no such function (or no module is loaded).
	pub fn find_function_by_name(&self, name: &str) -> i32 {
		// See the loaded-state guard note on `ByteCodeRunner`.
		if !self.module_loaded.load(Ordering::Acquire) {
			return -1;
		}
		let name = CString::new(name).unwrap();
		vcall!(self.inner, findFunctionByName(name.as_ptr()))
	}

	/// Gets the info of the function at `index`
	/// (`IByteCodeRunner::getFunctionInfo`). Returns `Err` with `SLANG_FAIL`
	/// when no module has been loaded yet.
	pub fn function_info(&self, index: u32) -> Result<ByteCodeFuncInfo> {
		// See the loaded-state guard note on `ByteCodeRunner`.
		if !self.module_loaded.load(Ordering::Acquire) {
			return Err(Error::Code(sys::SLANG_FAIL));
		}
		// SAFETY: `slang_ByteCodeFuncInfo` is a C struct of scalars; an
		// all-zero value is a valid instance.
		let mut info: sys::slang_ByteCodeFuncInfo = unsafe { std::mem::zeroed() };
		let result = vcall!(self.inner, getFunctionInfo(index, &mut info));
		// `getFunctionInfo` takes no diagnostics out-pointer; the result code
		// is the only error signal.
		if !succeeded(result) {
			return Err(Error::Code(result));
		}
		Ok(info)
	}

	/// Gets the runner's error string, if any
	/// (`IByteCodeRunner::getErrorString`).
	///
	/// This one is safe to call without a loaded module: the Slang
	/// implementation only reads its (properly constructed) error string
	/// builder, and a failed [`ByteCodeRunner::load_module`] reports its
	/// parse errors here.
	pub fn error_string(&self) -> Option<Blob> {
		let mut blob = null_mut();
		vcall!(self.inner, getErrorString(&mut blob));
		Some(Blob(IUnknown(std::ptr::NonNull::new(blob as *mut _)?)))
	}
}

/// Disassembles a Slang bytecode module blob into human-readable text
/// (`slang_disassembleByteCode` in slang.h).
///
/// NOTE: this API is experimental in Slang.
pub fn disassemble_byte_code(module: &Blob) -> Result<Blob> {
	let mut disassembly = null_mut();
	// SAFETY: both pointers are valid; on success the out-pointer receives a
	// new reference owned by the caller.
	let result = unsafe { sys::slang_disassembleByteCode(module.as_raw(), &mut disassembly) };
	if !succeeded(result) {
		return Err(Error::Code(result));
	}
	Ok(Blob(IUnknown(
		std::ptr::NonNull::new(disassembly as *mut _).unwrap(),
	)))
}

/// Description of a code generation target (`slang::TargetDesc` in slang.h),
/// built with [`TargetDesc::default`] plus builder methods and passed to
/// [`SessionDesc::targets`].
#[repr(transparent)]
pub struct TargetDesc<'a> {
	inner: sys::slang_TargetDesc,
	_phantom: PhantomData<&'a ()>,
}

impl std::ops::Deref for TargetDesc<'_> {
	type Target = sys::slang_TargetDesc;

	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl Default for TargetDesc<'_> {
	fn default() -> Self {
		Self {
			inner: sys::slang_TargetDesc {
				structureSize: std::mem::size_of::<sys::slang_TargetDesc>(),
				// SAFETY: `slang_TargetDesc` is a C struct of scalars and
				// pointers; an all-zero value is a valid instance.
				..unsafe { std::mem::zeroed() }
			},
			_phantom: PhantomData,
		}
	}
}

impl<'a> TargetDesc<'a> {
	/// Sets the target format to generate code for (e.g. SPIR-V, DXIL).
	pub fn format(mut self, format: CompileTarget) -> Self {
		self.inner.format = format;
		self
	}

	/// Sets the compilation profile supported by the target (e.g. "Shader
	/// Model 5.1"), as looked up with [`GlobalSession::find_profile`].
	pub fn profile(mut self, profile: ProfileID) -> Self {
		self.inner.profile = profile.0;
		self
	}

	/// Sets the code generation flags of the target (currently unused by
	/// Slang).
	pub fn flags(mut self, flags: TargetFlags) -> Self {
		self.inner.flags = flags;
		self
	}

	/// Sets the default mode to use for floating-point operations on the
	/// target.
	pub fn floating_point_mode(mut self, mode: FloatingPointMode) -> Self {
		self.inner.floatingPointMode = mode;
		self
	}

	/// Sets the line directive mode for output source code.
	pub fn line_directive_mode(mut self, mode: LineDirectiveMode) -> Self {
		self.inner.lineDirectiveMode = mode;
		self
	}

	/// Sets whether to force `scalar` layout for glsl shader storage buffers.
	pub fn force_glsl_scalar_buffer_layout(mut self, force: bool) -> Self {
		self.inner.forceGLSLScalarBufferLayout = force;
		self
	}

	/// Sets additional compiler options for the target
	/// (`compilerOptionEntries` in slang.h).
	pub fn options(mut self, options: &'a CompilerOptions) -> Self {
		self.inner.compilerOptionEntries = options.options.as_ptr() as _;
		self.inner.compilerOptionEntryCount = options.options.len() as _;
		self
	}
}

/// A preprocessor macro definition for [`SessionDesc::preprocessor_macros`].
///
/// Owns the name/value strings the raw desc points into.
#[repr(C)]
pub struct PreprocessorMacroDesc {
	// Must stay the first field: `SessionDesc::preprocessor_macros` casts a
	// slice of this wrapper to a slice of the sys struct, which is valid
	// because `repr(C)` places `inner` at offset 0.
	inner: sys::slang_PreprocessorMacroDesc,
	// Keep the strings alive for `inner` to point into. Moving the `CString`s
	// into the struct after taking their pointers is fine: the heap buffers
	// the pointers refer to do not move.
	_name: CString,
	_value: CString,
}

impl PreprocessorMacroDesc {
	/// Creates a preprocessor macro definition `name=value`.
	pub fn new(name: &str, value: &str) -> Self {
		let name = CString::new(name).unwrap();
		let value = CString::new(value).unwrap();
		Self {
			inner: sys::slang_PreprocessorMacroDesc {
				name: name.as_ptr(),
				value: value.as_ptr(),
			},
			_name: name,
			_value: value,
		}
	}
}

/// Description of a Slang session (`slang::SessionDesc` in slang.h), built
/// with [`SessionDesc::default`] plus builder methods and passed to
/// [`GlobalSession::create_session`].
#[repr(transparent)]
pub struct SessionDesc<'a> {
	inner: sys::slang_SessionDesc,
	_phantom: PhantomData<&'a ()>,
}

impl std::ops::Deref for SessionDesc<'_> {
	type Target = sys::slang_SessionDesc;

	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl Default for SessionDesc<'_> {
	fn default() -> Self {
		Self {
			inner: sys::slang_SessionDesc {
				structureSize: std::mem::size_of::<sys::slang_SessionDesc>(),
				// SAFETY: `slang_SessionDesc` is a C struct of scalars and
				// pointers; an all-zero value is a valid instance.
				..unsafe { std::mem::zeroed() }
			},
			_phantom: PhantomData,
		}
	}
}

impl<'a> SessionDesc<'a> {
	/// Sets the code generation targets to include in the session.
	pub fn targets(mut self, targets: &'a [TargetDesc]) -> Self {
		self.inner.targets = targets.as_ptr() as _;
		self.inner.targetCount = targets.len() as _;
		self
	}

	/// Sets the session flags (Slang currently only defines
	/// `kSessionFlags_None`, i.e. `0`).
	pub fn flags(mut self, flags: SessionFlags) -> Self {
		self.inner.flags = flags;
		self
	}

	/// Sets the default layout to assume for variables with matrix types.
	pub fn default_matrix_layout_mode(mut self, mode: MatrixLayoutMode) -> Self {
		self.inner.defaultMatrixLayoutMode = mode;
		self
	}

	/// Sets the paths used when searching for `#include`d or `import`ed
	/// files, as NUL-terminated C string pointers.
	pub fn search_paths(mut self, paths: &'a [*const i8]) -> Self {
		self.inner.searchPaths = paths.as_ptr();
		self.inner.searchPathCount = paths.len() as _;
		self
	}

	/// Sets global preprocessor definitions used for all code that gets
	/// `import`ed in the session.
	pub fn preprocessor_macros(mut self, macros: &'a [PreprocessorMacroDesc]) -> Self {
		self.inner.preprocessorMacros = macros.as_ptr() as _;
		self.inner.preprocessorMacroCount = macros.len() as _;
		self
	}

	/// Sets the file system the session loads source files through, replacing
	/// the default OS file system (see [`FileSystemObject`]).
	///
	/// Slang `addRef`s the file system during
	/// [`create_session`](GlobalSession::create_session) — `Linkage::setFileSystem`
	/// assigns it into a `ComPtr<ISlangFileSystem>` (slang-session.cpp:2075,
	/// slang-session.h:280) — so the session holds its own reference and
	/// `file_system` may be dropped as soon as `create_session` returns.
	pub fn file_system(mut self, file_system: &'a FileSystemObject) -> Self {
		// SAFETY: the object behind a `FileSystemObject` exposes the
		// `ISlangFileSystem` interface at its pointer value, per the
		// `Interface` safety contract, and outlives the resulting desc.
		self.inner.fileSystem = unsafe { file_system.as_raw() };
		self
	}

	/// Sets whether to enable support for legacy effect annotations.
	pub fn enable_effect_annotations(mut self, enable: bool) -> Self {
		self.inner.enableEffectAnnotations = enable;
		self
	}

	/// Sets whether to allow GLSL syntax in the loaded sources.
	pub fn allow_glsl_syntax(mut self, allow: bool) -> Self {
		self.inner.allowGLSLSyntax = allow;
		self
	}

	/// Sets additional compiler options for the session
	/// (`compilerOptionEntries` in slang.h).
	pub fn options(mut self, options: &'a CompilerOptions) -> Self {
		self.inner.compilerOptionEntries = options.options.as_ptr() as _;
		self.inner.compilerOptionEntryCount = options.options.len() as _;
		self
	}

	/// Sets whether to skip SPIR-V validation.
	pub fn skip_spirv_validation(mut self, skip: bool) -> Self {
		self.inner.skipSPIRVValidation = skip;
		self
	}
}

macro_rules! option {
	($name:ident, $func:ident($p_name:ident: $p_type:ident)) => {
		/// Appends the corresponding [`CompilerOptionName`] option with a
		/// single value.
		#[inline(always)]
		pub fn $func(self, $p_name: $p_type) -> Self {
			self.push_ints(CompilerOptionName::$name, $p_name as _, 0)
		}
	};

	($name:ident, $func:ident($p_name:ident: &str)) => {
		/// Appends the corresponding [`CompilerOptionName`] option with a
		/// string value.
		#[inline(always)]
		pub fn $func(self, $p_name: &str) -> Self {
			self.push_str1(CompilerOptionName::$name, $p_name)
		}
	};

	($name:ident, $func:ident($p_name1:ident: &str, $p_name2:ident: &str)) => {
		/// Appends the corresponding [`CompilerOptionName`] option with two
		/// string values.
		#[inline(always)]
		pub fn $func(self, $p_name1: &str, $p_name2: &str) -> Self {
			self.push_str2(CompilerOptionName::$name, $p_name1, $p_name2)
		}
	};
}

/// A list of compiler options (`slang::CompilerOptionEntry` in slang.h) for a
/// session or target, built with [`CompilerOptions::default`] plus builder
/// methods and passed to [`SessionDesc::options`] / [`TargetDesc::options`].
///
/// Owns the strings the raw option entries point into, so the entries stay
/// valid for as long as this value is borrowed.
#[derive(Default)]
pub struct CompilerOptions {
	strings: Vec<CString>,
	options: Vec<sys::slang_CompilerOptionEntry>,
}

impl CompilerOptions {
	fn push_ints(mut self, name: CompilerOptionName, i0: i32, i1: i32) -> Self {
		self.options.push(sys::slang_CompilerOptionEntry {
			name,
			value: sys::slang_CompilerOptionValue {
				kind: sys::slang_CompilerOptionValueKind::Int,
				intValue0: i0,
				intValue1: i1,
				stringValue0: null(),
				stringValue1: null(),
			},
		});

		self
	}

	fn push_strings(mut self, name: CompilerOptionName, s0: *const i8, s1: *const i8) -> Self {
		self.options.push(sys::slang_CompilerOptionEntry {
			name,
			value: sys::slang_CompilerOptionValue {
				kind: sys::slang_CompilerOptionValueKind::String,
				intValue0: 0,
				intValue1: 0,
				stringValue0: s0,
				stringValue1: s1,
			},
		});

		self
	}

	fn push_str1(mut self, name: CompilerOptionName, s0: &str) -> Self {
		let s0 = CString::new(s0).unwrap();
		let s0_ptr = s0.as_ptr();
		self.strings.push(s0);

		self.push_strings(name, s0_ptr, null())
	}

	fn push_str2(mut self, name: CompilerOptionName, s0: &str, s1: &str) -> Self {
		let s0 = CString::new(s0).unwrap();
		let s0_ptr = s0.as_ptr();
		self.strings.push(s0);

		let s1 = CString::new(s1).unwrap();
		let s1_ptr = s1.as_ptr();
		self.strings.push(s1);

		self.push_strings(name, s0_ptr, s1_ptr)
	}
}

impl CompilerOptions {
	/// Escape hatch: appends a single-integer entry for any
	/// [`CompilerOptionName`], including the many options that have no
	/// dedicated builder method.
	pub fn set_int(self, name: CompilerOptionName, value: i32) -> Self {
		self.push_ints(name, value, 0)
	}

	/// Escape hatch: appends a two-integer entry for any
	/// [`CompilerOptionName`] that takes two integer values.
	pub fn set_ints(self, name: CompilerOptionName, value0: i32, value1: i32) -> Self {
		self.push_ints(name, value0, value1)
	}

	/// Escape hatch: appends a single-string entry for any
	/// [`CompilerOptionName`], including the many options that have no
	/// dedicated builder method.
	pub fn set_string(self, name: CompilerOptionName, value: &str) -> Self {
		self.push_str1(name, value)
	}

	/// Escape hatch: appends a two-string entry for any
	/// [`CompilerOptionName`] that takes two string values.
	pub fn set_strings(self, name: CompilerOptionName, value0: &str, value1: &str) -> Self {
		self.push_str2(name, value0, value1)
	}
}

impl CompilerOptions {
	option!(MacroDefine, macro_define(key: &str, value: &str));
	option!(Include, include(path: &str));
	option!(Language, language(language: SourceLanguage));
	option!(MatrixLayoutColumn, matrix_layout_column(enable: bool));
	option!(MatrixLayoutRow, matrix_layout_row(enable: bool));

	/// Sets the compilation profile (`CompilerOptionName::Profile`).
	#[inline(always)]
	pub fn profile(self, profile: ProfileID) -> Self {
		self.push_ints(CompilerOptionName::Profile, profile.0 as _, 0)
	}

	option!(Stage, stage(stage: Stage));
	option!(Target, target(target: CompileTarget));
	option!(WarningsAsErrors, warnings_as_errors(warning_codes: &str));
	option!(DisableWarnings, disable_warnings(warning_codes: &str));
	option!(EnableWarning, enable_warning(warning_code: &str));
	option!(DisableWarning, disable_warning(warning_code: &str));
	option!(ReportDownstreamTime, report_downstream_time(enable: bool));
	option!(ReportPerfBenchmark, report_perf_benchmark(enable: bool));
	option!(SkipSPIRVValidation, skip_spirv_validation(enable: bool));

	// Target
	/// Adds a capability requirement (`CompilerOptionName::Capability`), as
	/// looked up with [`GlobalSession::find_capability`].
	#[inline(always)]
	pub fn capability(self, capability: CapabilityID) -> Self {
		self.push_ints(CompilerOptionName::Capability, capability.0 as _, 0)
	}

	option!(DefaultImageFormatUnknown, default_image_format_unknown(enable: bool));
	option!(DisableDynamicDispatch, disable_dynamic_dispatch(enable: bool));
	option!(DisableSpecialization, disable_specialization(enable: bool));
	option!(FloatingPointMode, floating_point_mode(mode: FloatingPointMode));
	option!(DebugInformation, debug_information(level: DebugInfoLevel));
	option!(LineDirectiveMode, line_directive_mode(mode: LineDirectiveMode));
	option!(Optimization, optimization(level: OptimizationLevel));
	option!(Obfuscate, obfuscate(enable: bool));
	option!(VulkanUseEntryPointName, vulkan_use_entry_point_name(enable: bool));
	option!(GLSLForceScalarLayout, glsl_force_scalar_layout(enable: bool));
	option!(EmitSpirvDirectly, emit_spirv_directly(enable: bool));

	// Debugging
	option!(NoCodeGen, no_code_gen(enable: bool));

	// Experimental
	option!(NoMangle, no_mangle(enable: bool));
	option!(ValidateUniformity, validate_uniformity(enable: bool));
}
