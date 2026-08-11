//! Reverse COM bindings: Rust implementations of Slang's file system
//! interfaces (`ISlangFileSystem`, `ISlangFileSystemExt`,
//! `ISlangMutableFileSystem`) that the Slang C++ side calls back into.
//!
//! Unlike every other wrapper in this crate (C++ objects called from Rust),
//! the object defined here is allocated on the Rust side and its vtable
//! consists of Rust `extern "C"` thunks. The object layout follows the COM
//! convention: the vtable pointer is the first field.

use std::ffi::{c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::sys;
use crate::{
	Blob, IUnknown, Interface, MutableFileSystem, OSPathKind, PathKind, PathType, UUID,
	str_from_slang, uuid,
};

/// IID of `ISlangCastable` (slang.h), needed by the `queryInterface`/`castAs`
/// thunks; the crate has no public wrapper for that interface.
const ISLANG_CASTABLE_IID: UUID = uuid(0x87ed_e0e1_4852_44b0_8bf2_cb31_874d_e239);

/// IID of `ISlangFileSystemExt` (slang.h):
/// 5FB632D2-979D-4481-9FEE-663C3F1449E1. The crate's forward wrapper
/// [`MutableFileSystem`] covers the Ext methods, so no separate public
/// wrapper carries this IID; the reverse COM thunks keep it here.
const ISLANG_FILE_SYSTEM_EXT_IID: UUID = uuid(0x5fb6_32d2_979d_4481_9fee_663c_3f14_49e1);

fn uuid_eq(a: &UUID, b: &UUID) -> bool {
	(a.data1, a.data2, a.data3, a.data4) == (b.data1, b.data2, b.data3, b.data4)
}

/// The error returned by the [`FileSystem`] family of callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSystemError {
	/// The requested file does not exist. Maps to `SLANG_E_NOT_FOUND`, which
	/// lets Slang move on and try the next candidate path.
	NotFound,
	/// The operation is not implemented by this file system. Maps to
	/// `SLANG_E_NOT_IMPLEMENTED`, which lets Slang fall back to a default
	/// behavior where it has one.
	NotImplemented,
	/// Any other failure. Maps to `SLANG_FAIL`.
	Other,
}

impl std::fmt::Display for FileSystemError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			FileSystemError::NotFound => write!(f, "file not found"),
			FileSystemError::NotImplemented => write!(f, "operation not implemented"),
			FileSystemError::Other => write!(f, "failed to load file"),
		}
	}
}

impl std::error::Error for FileSystemError {}

/// Maps a [`FileSystemError`] to the Slang result code reported to C++.
fn error_to_slang(error: FileSystemError) -> sys::SlangResult {
	match error {
		FileSystemError::NotFound => sys::SLANG_E_NOT_FOUND,
		FileSystemError::NotImplemented => sys::SLANG_E_NOT_IMPLEMENTED,
		FileSystemError::Other => sys::SLANG_FAIL,
	}
}

/// A (real or virtual) file system the compiler loads source files through.
///
/// Implementations are exposed to Slang as a COM `ISlangFileSystem` via
/// [`FileSystemObject`]. Slang may invoke the callback from internal compiler
/// threads, hence the `Send + Sync` bound; implementations must be prepared
/// for calls from any thread and must synchronize any shared state themselves.
///
/// A panic inside [`FileSystem::load_file`] never unwinds across the FFI
/// boundary into C++: it is caught at the callback thunk and reported to Slang
/// as `SLANG_FAIL` (the panic message still reaches the default panic hook).
pub trait FileSystem: Send + Sync {
	/// Loads the file at `path` and returns its exact bytes.
	///
	/// `path` is a UTF-8 path as handed to Slang (search paths, `import`
	/// resolution, etc.). Return [`FileSystemError::NotFound`] when the file
	/// does not exist and [`FileSystemError::Other`] for any other failure.
	fn load_file(&self, path: &str) -> Result<Vec<u8>, FileSystemError>;
}

/// Extension of [`FileSystem`] giving the implementation control over path
/// management (Slang's `ISlangFileSystemExt`): how paths are combined and
/// canonicalized, and how it is determined whether two paths name the same
/// file.
///
/// Expose an implementation to Slang with [`FileSystemObject::new_ext`]. An
/// Ext-level object answers `queryInterface(ISlangFileSystemExt)`, so Slang
/// uses the implementation's path management directly instead of wrapping a
/// plain [`FileSystem`] in its emulating `CacheFileSystem`
/// (`Linkage::setFileSystem` in slang-session.cpp).
///
/// The same threading and panic rules as [`FileSystem`] apply: callbacks may
/// run on internal compiler threads, and a panic is caught at the callback
/// thunk and reported as `SLANG_FAIL` (for `clear_cache`, which returns
/// nothing, a contained panic is swallowed; `os_path_kind` then reports
/// [`OSPathKind::None`]).
pub trait FileSystemExt: FileSystem {
	/// Returns a string that uniquely identifies the object at `path`
	/// (`getFileUniqueIdentity`). Two paths may only report the same identity
	/// when their contents are identical — breaking that constraint can
	/// produce incorrect compilation. Slang uses the identity for source
	/// caching and `#pragma once`. A canonical path is a good identity when
	/// the file system has one.
	fn file_unique_identity(&self, path: &str) -> Result<String, FileSystemError>;

	/// Combines `from_path` with `path` into a single path
	/// (`calcCombinedPath`). `from_path_type` tells the implementation
	/// whether to interpret `from_path` as a file (combine relative to its
	/// directory) or as a directory.
	fn calc_combined_path(
		&self,
		from_path_type: PathType,
		from_path: &str,
		path: &str,
	) -> Result<String, FileSystemError>;

	/// Returns whether `path` names a file or a directory (`getPathType`).
	/// Return [`FileSystemError::NotFound`] when the path does not exist, so
	/// Slang moves on to the next candidate path.
	fn path_type(&self, path: &str) -> Result<PathType, FileSystemError>;

	/// Returns `path` converted to the requested `kind` (`getPath`), e.g.
	/// simplified or canonicalized. The default implementation reports
	/// [`FileSystemError::NotImplemented`], which slang.h explicitly allows.
	fn get_path(&self, _kind: PathKind, _path: &str) -> Result<String, FileSystemError> {
		Err(FileSystemError::NotImplemented)
	}

	/// Clears any cached path information (`clearCache`). Does nothing by
	/// default.
	fn clear_cache(&self) {}

	/// Enumerates the entries of the directory at `path`
	/// (`enumeratePathContents`), invoking `callback` with the type and bare
	/// name of each entry. Normal Slang operation does not require
	/// enumeration, so the default implementation reports
	/// [`FileSystemError::NotImplemented`].
	fn enumerate_path_contents(
		&self,
		_path: &str,
		_callback: &mut dyn FnMut(PathType, &str),
	) -> Result<(), FileSystemError> {
		Err(FileSystemError::NotImplemented)
	}

	/// Returns how paths used with this file system map to the operating
	/// system's file system (`getOSPathKind`). Defaults to
	/// [`OSPathKind::None`]: paths do not map to the OS file system.
	fn os_path_kind(&self) -> OSPathKind {
		OSPathKind::None
	}
}

/// Extension of [`FileSystemExt`] with write operations (Slang's
/// `ISlangMutableFileSystem`). Expose an implementation to Slang with
/// [`FileSystemObject::new_writable`].
///
/// The same threading and panic rules as [`FileSystem`] apply; see
/// [`FileSystemExt`].
pub trait WritableFileSystem: FileSystemExt {
	/// Writes `data` to `path` (`saveFile`), replacing any existing file.
	fn save_file(&self, path: &str, data: &[u8]) -> Result<(), FileSystemError>;

	/// Writes the contents of `data` to `path` (`saveFileBlob`). The default
	/// implementation forwards to [`WritableFileSystem::save_file`].
	fn save_file_blob(&self, path: &str, data: &Blob) -> Result<(), FileSystemError> {
		self.save_file(path, data.as_slice())
	}

	/// Removes the file or empty directory at `path` (`remove`).
	fn remove(&self, path: &str) -> Result<(), FileSystemError>;

	/// Creates the directory at `path` (`createDirectory`). The parent path
	/// must exist.
	fn create_directory(&self, path: &str) -> Result<(), FileSystemError>;
}

/// An owned COM object implementing Slang's `ISlangFileSystem` (or one of its
/// extensions), backed by a Rust [`FileSystem`] implementation.
///
/// Cloning performs a COM `addRef`, dropping a `release`; the backing Rust
/// object is reclaimed when the last reference is released. Because Slang
/// `addRef`s the file system when a session is created (see
/// [`SessionDesc::file_system`](crate::SessionDesc::file_system)), this
/// wrapper may be dropped as soon as `create_session` returns.
///
/// The constructor picks the interface level the object exposes to Slang:
/// [`FileSystemObject::new`] exposes `ISlangFileSystem` only,
/// [`FileSystemObject::new_ext`] exposes `ISlangFileSystemExt`, and
/// [`FileSystemObject::new_writable`] exposes `ISlangMutableFileSystem`.
#[repr(transparent)]
#[derive(Clone)]
pub struct FileSystemObject(IUnknown);

unsafe impl Interface for FileSystemObject {
	type Vtable = sys::ISlangFileSystemVtable;
	// `ISlangFileSystem` IID from slang.h: 003A09FC-3A4D-4BA0-AD60-1FD863A915AB.
	const IID: UUID = uuid(0x003a_09fc_3a4d_4ba0_ad60_1fd8_63a9_15ab);
}

// SAFETY: the backing Rust implementation is `Send + Sync` (a `FileSystem`
// supertrait bound) and the reference count is atomic, so the wrapper may
// move between threads with exclusive ownership. `Sync` is deliberately
// not implemented, matching the other COM wrappers (a session using this
// object must be externally synchronized per slang.h anyway).
unsafe impl Send for FileSystemObject {}

impl std::fmt::Debug for FileSystemObject {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "FileSystemObject({:p})", self.as_unknown())
	}
}

impl FileSystemObject {
	/// Wraps a [`FileSystem`] implementation in a COM object that can be
	/// passed to Slang. The object answers `queryInterface` for
	/// `ISlangFileSystem` only; since it does not expose
	/// `ISlangFileSystemExt`, Slang wraps it in its own `CacheFileSystem`,
	/// which emulates path management on top of [`FileSystem::load_file`].
	pub fn new(file_system: impl FileSystem + 'static) -> Self {
		Self::from_inner(
			Box::new(WritableFileSystemAdapter(FileSystemExtAdapter(file_system))),
			&BASE_VTABLE as *const sys::ISlangFileSystemVtable as *const c_void,
			&[Self::IID],
		)
	}

	/// Wraps a [`FileSystemExt`] implementation in a COM object that can be
	/// passed to Slang. The object answers `queryInterface` for
	/// `ISlangFileSystemExt`, so Slang uses the implementation's path
	/// management directly instead of wrapping it in a `CacheFileSystem`.
	pub fn new_ext(file_system: impl FileSystemExt + 'static) -> Self {
		Self::from_inner(
			Box::new(WritableFileSystemAdapter(file_system)),
			&EXT_VTABLE as *const sys::ISlangFileSystemExtVtable as *const c_void,
			&[Self::IID, ISLANG_FILE_SYSTEM_EXT_IID],
		)
	}

	/// Wraps a [`WritableFileSystem`] implementation in a COM object that can
	/// be passed to Slang. The object answers `queryInterface` for
	/// `ISlangMutableFileSystem` (and thereby both base interfaces).
	pub fn new_writable(file_system: impl WritableFileSystem + 'static) -> Self {
		Self::from_inner(
			Box::new(file_system),
			&WRITABLE_VTABLE as *const sys::ISlangMutableFileSystemVtable as *const c_void,
			&[
				Self::IID,
				ISLANG_FILE_SYSTEM_EXT_IID,
				MutableFileSystem::IID,
			],
		)
	}

	fn from_inner(
		inner: Box<dyn WritableFileSystem>,
		vtable: *const c_void,
		iids: &'static [UUID],
	) -> Self {
		let object = Box::new(FileSystemCom {
			vtable,
			ref_count: AtomicU32::new(1),
			iids,
			inner,
		});
		let ptr = Box::into_raw(object);
		// SAFETY: `Box::into_raw` never returns null. The object exposes its
		// vtable pointer at its first field, satisfying the `Interface`
		// safety contract for the `ISlangFileSystem` prefix of that vtable.
		Self(IUnknown(unsafe {
			std::ptr::NonNull::new_unchecked(ptr as *mut c_void)
		}))
	}
}

/// Promotes a [`FileSystem`] to [`FileSystemExt`], reporting "not
/// implemented" for every Ext method. Exists to unify the COM object layout
/// on `Box<dyn WritableFileSystem>`; the Ext methods are unreachable for a
/// base-level object because its vtable does not expose them.
struct FileSystemExtAdapter<F>(F);

impl<F: FileSystem> FileSystem for FileSystemExtAdapter<F> {
	fn load_file(&self, path: &str) -> Result<Vec<u8>, FileSystemError> {
		self.0.load_file(path)
	}
}

impl<F: FileSystem> FileSystemExt for FileSystemExtAdapter<F> {
	fn file_unique_identity(&self, _path: &str) -> Result<String, FileSystemError> {
		Err(FileSystemError::NotImplemented)
	}

	fn calc_combined_path(
		&self,
		_from_path_type: PathType,
		_from_path: &str,
		_path: &str,
	) -> Result<String, FileSystemError> {
		Err(FileSystemError::NotImplemented)
	}

	fn path_type(&self, _path: &str) -> Result<PathType, FileSystemError> {
		Err(FileSystemError::NotImplemented)
	}
	// `get_path` / `clear_cache` / `enumerate_path_contents` /
	// `os_path_kind` keep the trait defaults, which are also "not
	// implemented" (or no-op) and equally unreachable.
}

/// Promotes a [`FileSystemExt`] to [`WritableFileSystem`], reporting "not
/// implemented" for every write operation while delegating everything else.
/// Exists to unify the COM object layout on `Box<dyn WritableFileSystem>`;
/// the write methods are unreachable unless the object was created with
/// [`FileSystemObject::new_writable`].
struct WritableFileSystemAdapter<F>(F);

impl<F: FileSystemExt> FileSystem for WritableFileSystemAdapter<F> {
	fn load_file(&self, path: &str) -> Result<Vec<u8>, FileSystemError> {
		self.0.load_file(path)
	}
}

impl<F: FileSystemExt> FileSystemExt for WritableFileSystemAdapter<F> {
	fn file_unique_identity(&self, path: &str) -> Result<String, FileSystemError> {
		self.0.file_unique_identity(path)
	}

	fn calc_combined_path(
		&self,
		from_path_type: PathType,
		from_path: &str,
		path: &str,
	) -> Result<String, FileSystemError> {
		self.0.calc_combined_path(from_path_type, from_path, path)
	}

	fn path_type(&self, path: &str) -> Result<PathType, FileSystemError> {
		self.0.path_type(path)
	}

	fn get_path(&self, kind: PathKind, path: &str) -> Result<String, FileSystemError> {
		self.0.get_path(kind, path)
	}

	fn clear_cache(&self) {
		self.0.clear_cache();
	}

	fn enumerate_path_contents(
		&self,
		path: &str,
		callback: &mut dyn FnMut(PathType, &str),
	) -> Result<(), FileSystemError> {
		self.0.enumerate_path_contents(path, callback)
	}

	fn os_path_kind(&self) -> OSPathKind {
		self.0.os_path_kind()
	}
}

impl<F: FileSystemExt> WritableFileSystem for WritableFileSystemAdapter<F> {
	fn save_file(&self, _path: &str, _data: &[u8]) -> Result<(), FileSystemError> {
		Err(FileSystemError::NotImplemented)
	}

	fn remove(&self, _path: &str) -> Result<(), FileSystemError> {
		Err(FileSystemError::NotImplemented)
	}

	fn create_directory(&self, _path: &str) -> Result<(), FileSystemError> {
		Err(FileSystemError::NotImplemented)
	}
	// `save_file_blob` keeps the trait default, which forwards to the
	// (unreachable) `save_file` above.
}

/// The heap-allocated COM object. `repr(C)` with the vtable pointer first,
/// matching the C++ object layout Slang expects.
#[repr(C)]
struct FileSystemCom {
	/// Points at one of the vtable statics below, depending on the interface
	/// level the object was created with.
	vtable: *const c_void,
	ref_count: AtomicU32,
	/// The IIDs (beyond `ISlangUnknown`/`ISlangCastable`) this object answers
	/// `queryInterface`/`castAs` for.
	iids: &'static [UUID],
	inner: Box<dyn WritableFileSystem>,
}

static BASE_VTABLE: sys::ISlangFileSystemVtable = sys::ISlangFileSystemVtable {
	_base: sys::ICastableVtable {
		_base: sys::ISlangUnknown__bindgen_vtable {
			ISlangUnknown_queryInterface: query_interface,
			ISlangUnknown_addRef: add_ref,
			ISlangUnknown_release: release,
		},
		castAs: cast_as,
	},
	loadFile: load_file,
};

static EXT_VTABLE: sys::ISlangFileSystemExtVtable = sys::ISlangFileSystemExtVtable {
	_base: sys::ISlangFileSystemVtable {
		_base: sys::ICastableVtable {
			_base: sys::ISlangUnknown__bindgen_vtable {
				ISlangUnknown_queryInterface: query_interface,
				ISlangUnknown_addRef: add_ref,
				ISlangUnknown_release: release,
			},
			castAs: cast_as,
		},
		loadFile: load_file,
	},
	getFileUniqueIdentity: get_file_unique_identity,
	calcCombinedPath: calc_combined_path,
	getPathType: get_path_type,
	getPath: get_path,
	clearCache: clear_cache,
	enumeratePathContents: enumerate_path_contents,
	getOSPathKind: get_os_path_kind,
};

static WRITABLE_VTABLE: sys::ISlangMutableFileSystemVtable = sys::ISlangMutableFileSystemVtable {
	_base: sys::ISlangFileSystemExtVtable {
		_base: sys::ISlangFileSystemVtable {
			_base: sys::ICastableVtable {
				_base: sys::ISlangUnknown__bindgen_vtable {
					ISlangUnknown_queryInterface: query_interface,
					ISlangUnknown_addRef: add_ref,
					ISlangUnknown_release: release,
				},
				castAs: cast_as,
			},
			loadFile: load_file,
		},
		getFileUniqueIdentity: get_file_unique_identity,
		calcCombinedPath: calc_combined_path,
		getPathType: get_path_type,
		getPath: get_path,
		clearCache: clear_cache,
		enumeratePathContents: enumerate_path_contents,
		getOSPathKind: get_os_path_kind,
	},
	saveFile: save_file,
	saveFileBlob: save_file_blob,
	remove,
	createDirectory: create_directory,
};

unsafe extern "C" fn query_interface(
	this: *mut sys::ISlangUnknown,
	guid: *const sys::SlangUUID,
	out_object: *mut *mut c_void,
) -> sys::SlangResult {
	if this.is_null() || guid.is_null() || out_object.is_null() {
		return sys::SLANG_E_INVALID_ARG;
	}
	// SAFETY: `out_object` is valid per the check above.
	unsafe { *out_object = null_mut() };
	// SAFETY: `guid` is valid per the check above.
	let guid = unsafe { &*guid };
	// SAFETY: `this` is non-null and points to a live `FileSystemCom` kept
	// alive by the COM reference count.
	let object = unsafe { &*(this as *const FileSystemCom) };
	if uuid_eq(guid, &IUnknown::IID)
		|| uuid_eq(guid, &ISLANG_CASTABLE_IID)
		|| object.iids.iter().any(|iid| uuid_eq(guid, iid))
	{
		// SAFETY: `out_object` is valid per the check above. This object
		// implements all of its interfaces at the same pointer value.
		unsafe { *out_object = this as *mut c_void };
		// The query hands out a new reference.
		// SAFETY: `this` is non-null and points to a live `FileSystemCom`.
		unsafe { add_ref(this) };
		0
	} else {
		sys::SLANG_E_NO_INTERFACE
	}
}

unsafe extern "C" fn add_ref(this: *mut sys::ISlangUnknown) -> u32 {
	// SAFETY: per COM rules, `addRef` is only called on a live object.
	let object = unsafe { &*(this as *const FileSystemCom) };
	object.ref_count.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "C" fn release(this: *mut sys::ISlangUnknown) -> u32 {
	// SAFETY: per COM rules, `release` is only called on a live object that
	// the caller holds a reference to.
	let object = unsafe { &*(this as *const FileSystemCom) };
	let previous = object.ref_count.fetch_sub(1, Ordering::Release);
	if previous == 1 {
		// Pair with the Release store above so no use of the object can be
		// observed after the deallocation.
		std::sync::atomic::fence(Ordering::Acquire);
		// SAFETY: the last reference is gone, so no thread can still access
		// the object; `this` was created by `Box::into_raw` in
		// `FileSystemObject::from_inner` and is reclaimed exactly once here.
		drop(unsafe { Box::from_raw(this as *mut FileSystemCom) });
		0
	} else {
		previous - 1
	}
}

unsafe extern "C" fn cast_as(this: *mut c_void, guid: *const sys::SlangUUID) -> *mut c_void {
	if this.is_null() || guid.is_null() {
		return null_mut();
	}
	// SAFETY: `guid` is valid per the check above.
	let guid = unsafe { &*guid };
	// SAFETY: `this` is non-null and points to a live `FileSystemCom` kept
	// alive by the COM reference count.
	let object = unsafe { &*(this as *const FileSystemCom) };
	if uuid_eq(guid, &IUnknown::IID)
		|| uuid_eq(guid, &ISLANG_CASTABLE_IID)
		|| object.iids.iter().any(|iid| uuid_eq(guid, iid))
	{
		// `castAs` returns a *borrowed*, non-ref-counted pointer (slang.h).
		this
	} else {
		null_mut()
	}
}

/// Extracts the COM object from a thunk's `this` pointer, or `None` when the
/// pointer is null.
///
/// SAFETY: a non-null `this` must point to a live `FileSystemCom` kept alive
/// by the COM reference count.
unsafe fn object<'a>(this: *mut c_void) -> Option<&'a FileSystemCom> {
	// SAFETY: upheld by the caller.
	unsafe { (this as *const FileSystemCom).as_ref() }
}

/// Extracts a path argument of a thunk, or `None` when it is null or not
/// valid UTF-8.
///
/// SAFETY: a non-null `path` must be a NUL-terminated string valid for the
/// duration of the call.
unsafe fn path_arg<'a>(path: *const c_char) -> Option<&'a str> {
	// SAFETY: upheld by the caller.
	unsafe { str_from_slang(path) }
}

/// Maps the outcome of a string-returning callback (a panic-contained
/// `Result<Result<String, FileSystemError>>`) to a NUL-terminated blob
/// written to `out`, plus the Slang result code.
fn string_result_to_slang(
	result: std::thread::Result<Result<String, FileSystemError>>,
	out: *mut *mut sys::ISlangBlob,
) -> sys::SlangResult {
	match result {
		Ok(Ok(string)) => {
			// Path/identity blobs hold the string zero terminated (slang.h).
			// The extra NUL byte also means the blob is never empty, avoiding
			// the null-blob edge case of `slang_createBlob`.
			let mut bytes = string.into_bytes();
			bytes.push(0);
			// SAFETY: `bytes` is readable for `bytes.len()` bytes and
			// `slang_createBlob` copies the bytes into the new blob.
			let blob =
				unsafe { sys::slang_createBlob(bytes.as_ptr() as *const c_void, bytes.len()) };
			if blob.is_null() {
				sys::SLANG_FAIL
			} else {
				// SAFETY: `out` is valid; the caller checked it.
				unsafe { *out = blob };
				0
			}
		}
		Ok(Err(error)) => error_to_slang(error),
		// A contained panic surfaces as a generic failure.
		Err(_) => sys::SLANG_FAIL,
	}
}

/// Maps the outcome of a unit-returning callback to a Slang result code.
fn unit_result_to_slang(
	result: std::thread::Result<Result<(), FileSystemError>>,
) -> sys::SlangResult {
	match result {
		Ok(Ok(())) => 0,
		Ok(Err(error)) => error_to_slang(error),
		// A contained panic surfaces as a generic failure.
		Err(_) => sys::SLANG_FAIL,
	}
}

unsafe extern "C" fn load_file(
	this: *mut c_void,
	path: *const c_char,
	out_blob: *mut *mut sys::ISlangBlob,
) -> sys::SlangResult {
	if out_blob.is_null() {
		return sys::SLANG_E_INVALID_ARG;
	}
	// Every exit path below leaves the out pointer either null or holding a
	// valid blob; start from null so failures never leak a stale value.
	// SAFETY: `out_blob` is valid per the check above.
	unsafe { *out_blob = null_mut() };
	// SAFETY: `this`/`path` are checked by the helpers (null-safe per their
	// contracts); a live object is kept alive by the COM reference count.
	let (Some(object), Some(path)) = (unsafe { object(this) }, unsafe { path_arg(path) }) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	// A panic must never unwind across the FFI boundary into C++.
	// `AssertUnwindSafe` is sound here: the closure only borrows `object`
	// immutably, and a panicking `load_file` implementation cannot leave the
	// `FileSystemCom` itself in an inconsistent state (it owns no mutable
	// state shared with the caller).
	let result = catch_unwind(AssertUnwindSafe(|| object.inner.load_file(path)));
	match result {
		Ok(Ok(data)) => {
			// SAFETY: `data` is readable for `data.len()` bytes and
			// `slang_createBlob` copies the bytes into the new blob. Note the
			// C++ side returns null for empty input, which is reported as a
			// generic failure — an empty file cannot be represented.
			let blob = unsafe { sys::slang_createBlob(data.as_ptr() as *const c_void, data.len()) };
			if blob.is_null() {
				sys::SLANG_FAIL
			} else {
				// SAFETY: `out_blob` is valid per the check above.
				unsafe { *out_blob = blob };
				0
			}
		}
		Ok(Err(error)) => error_to_slang(error),
		Err(_) => sys::SLANG_FAIL,
	}
}

unsafe extern "C" fn get_file_unique_identity(
	this: *mut c_void,
	path: *const c_char,
	out_unique_identity: *mut *mut sys::ISlangBlob,
) -> sys::SlangResult {
	if out_unique_identity.is_null() {
		return sys::SLANG_E_INVALID_ARG;
	}
	// SAFETY: `out_unique_identity` is valid per the check above; starting
	// from null means failures never leak a stale value.
	unsafe { *out_unique_identity = null_mut() };
	// SAFETY: see `load_file`.
	let (Some(object), Some(path)) = (unsafe { object(this) }, unsafe { path_arg(path) }) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	let result = catch_unwind(AssertUnwindSafe(|| object.inner.file_unique_identity(path)));
	string_result_to_slang(result, out_unique_identity)
}

unsafe extern "C" fn calc_combined_path(
	this: *mut c_void,
	from_path_type: sys::SlangPathType,
	from_path: *const c_char,
	path: *const c_char,
	path_out: *mut *mut sys::ISlangBlob,
) -> sys::SlangResult {
	if path_out.is_null() {
		return sys::SLANG_E_INVALID_ARG;
	}
	// SAFETY: `path_out` is valid per the check above; starting from null
	// means failures never leak a stale value.
	unsafe { *path_out = null_mut() };
	// SAFETY: see `load_file`.
	let (Some(object), Some(from_path), Some(path)) = (
		unsafe { object(this) },
		unsafe { path_arg(from_path) },
		unsafe { path_arg(path) },
	) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	let result = catch_unwind(AssertUnwindSafe(|| {
		object
			.inner
			.calc_combined_path(from_path_type, from_path, path)
	}));
	string_result_to_slang(result, path_out)
}

unsafe extern "C" fn get_path_type(
	this: *mut c_void,
	path: *const c_char,
	path_type_out: *mut sys::SlangPathType,
) -> sys::SlangResult {
	if path_type_out.is_null() {
		return sys::SLANG_E_INVALID_ARG;
	}
	// SAFETY: see `load_file`.
	let (Some(object), Some(path)) = (unsafe { object(this) }, unsafe { path_arg(path) }) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	match catch_unwind(AssertUnwindSafe(|| object.inner.path_type(path))) {
		Ok(Ok(path_type)) => {
			// SAFETY: `path_type_out` is valid per the check above.
			unsafe { *path_type_out = path_type };
			0
		}
		Ok(Err(error)) => error_to_slang(error),
		Err(_) => sys::SLANG_FAIL,
	}
}

unsafe extern "C" fn get_path(
	this: *mut c_void,
	kind: sys::PathKind,
	path: *const c_char,
	out_path: *mut *mut sys::ISlangBlob,
) -> sys::SlangResult {
	if out_path.is_null() {
		return sys::SLANG_E_INVALID_ARG;
	}
	// SAFETY: `out_path` is valid per the check above; starting from null
	// means failures never leak a stale value.
	unsafe { *out_path = null_mut() };
	// SAFETY: see `load_file`.
	let (Some(object), Some(path)) = (unsafe { object(this) }, unsafe { path_arg(path) }) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	let result = catch_unwind(AssertUnwindSafe(|| object.inner.get_path(kind, path)));
	string_result_to_slang(result, out_path)
}

unsafe extern "C" fn clear_cache(this: *mut c_void) {
	// SAFETY: see `load_file`.
	if let Some(object) = unsafe { object(this) } {
		// `clearCache` returns nothing, so a contained panic is swallowed.
		let _ = catch_unwind(AssertUnwindSafe(|| object.inner.clear_cache()));
	}
}

unsafe extern "C" fn enumerate_path_contents(
	this: *mut c_void,
	path: *const c_char,
	callback: sys::FileSystemContentsCallBack,
	user_data: *mut c_void,
) -> sys::SlangResult {
	let Some(callback) = callback else {
		return sys::SLANG_E_INVALID_ARG;
	};
	// SAFETY: see `load_file`.
	let (Some(object), Some(path)) = (unsafe { object(this) }, unsafe { path_arg(path) }) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	let mut forward = |path_type: PathType, name: &str| {
		// Names containing an interior NUL cannot be passed through the C
		// callback; skip them rather than failing the whole enumeration.
		let Ok(name) = std::ffi::CString::new(name) else {
			return;
		};
		// SAFETY: `callback` is valid per the check above; `user_data` is
		// passed through exactly as Slang provided it; `name` outlives the
		// call.
		unsafe { callback(path_type, name.as_ptr(), user_data) };
	};
	let result = catch_unwind(AssertUnwindSafe(|| {
		object.inner.enumerate_path_contents(path, &mut forward)
	}));
	unit_result_to_slang(result)
}

unsafe extern "C" fn get_os_path_kind(this: *mut c_void) -> sys::OSPathKind {
	// SAFETY: see `load_file`.
	let Some(object) = (unsafe { object(this) }) else {
		return sys::OSPathKind::None;
	};
	// `getOSPathKind` returns a plain value, so a contained panic falls back
	// to `None` (paths do not map to the OS file system).
	catch_unwind(AssertUnwindSafe(|| object.inner.os_path_kind())).unwrap_or(sys::OSPathKind::None)
}

unsafe extern "C" fn save_file(
	this: *mut c_void,
	path: *const c_char,
	data: *const c_void,
	size: usize,
) -> sys::SlangResult {
	// SAFETY: see `load_file`.
	let (Some(object), Some(path)) = (unsafe { object(this) }, unsafe { path_arg(path) }) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	let data: &[u8] = if size == 0 {
		&[]
	} else {
		if data.is_null() {
			return sys::SLANG_E_INVALID_ARG;
		}
		// SAFETY: `data` is non-null per the check above; Slang guarantees
		// `size` readable bytes for the duration of the call.
		unsafe { std::slice::from_raw_parts(data as *const u8, size) }
	};
	let result = catch_unwind(AssertUnwindSafe(|| object.inner.save_file(path, data)));
	unit_result_to_slang(result)
}

unsafe extern "C" fn save_file_blob(
	this: *mut c_void,
	path: *const c_char,
	data_blob: *mut sys::ISlangBlob,
) -> sys::SlangResult {
	// SAFETY: see `load_file`.
	let (Some(object), Some(path)) = (unsafe { object(this) }, unsafe { path_arg(path) }) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	let Some(blob) = std::ptr::NonNull::new(data_blob as *mut c_void) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	// `saveFileBlob` only borrows the blob for the duration of the call;
	// `ManuallyDrop` keeps the wrapper from releasing it.
	let blob = std::mem::ManuallyDrop::new(Blob(IUnknown(blob)));
	let result = catch_unwind(AssertUnwindSafe(|| {
		object.inner.save_file_blob(path, &blob)
	}));
	unit_result_to_slang(result)
}

unsafe extern "C" fn remove(this: *mut c_void, path: *const c_char) -> sys::SlangResult {
	// SAFETY: see `load_file`.
	let (Some(object), Some(path)) = (unsafe { object(this) }, unsafe { path_arg(path) }) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	let result = catch_unwind(AssertUnwindSafe(|| object.inner.remove(path)));
	unit_result_to_slang(result)
}

unsafe extern "C" fn create_directory(this: *mut c_void, path: *const c_char) -> sys::SlangResult {
	// SAFETY: see `load_file`.
	let (Some(object), Some(path)) = (unsafe { object(this) }, unsafe { path_arg(path) }) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	let result = catch_unwind(AssertUnwindSafe(|| object.inner.create_directory(path)));
	unit_result_to_slang(result)
}
