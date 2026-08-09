//! Reverse COM binding: a Rust implementation of Slang's `ISlangFileSystem`
//! interface that the Slang C++ side calls back into.
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
use crate::{IUnknown, Interface, UUID, str_from_slang, uuid};

/// IID of `ISlangCastable` (slang.h), needed by the `queryInterface`/`castAs`
/// thunks; the crate has no public wrapper for that interface.
const ISLANG_CASTABLE_IID: UUID = uuid(0x87ed_e0e1_4852_44b0_8bf2_cb31_874d_e239);

fn uuid_eq(a: &UUID, b: &UUID) -> bool {
	(a.data1, a.data2, a.data3, a.data4) == (b.data1, b.data2, b.data3, b.data4)
}

/// The error returned by [`FileSystem::load_file`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSystemError {
	/// The requested file does not exist. Maps to `SLANG_E_NOT_FOUND`, which
	/// lets Slang move on and try the next candidate path.
	NotFound,
	/// Any other failure. Maps to `SLANG_FAIL`.
	Other,
}

impl std::fmt::Display for FileSystemError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			FileSystemError::NotFound => write!(f, "file not found"),
			FileSystemError::Other => write!(f, "failed to load file"),
		}
	}
}

impl std::error::Error for FileSystemError {}

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

/// An owned COM object implementing Slang's `ISlangFileSystem`, backed by a
/// Rust [`FileSystem`] implementation.
///
/// Cloning performs a COM `addRef`, dropping a `release`; the backing Rust
/// object is reclaimed when the last reference is released. Because Slang
/// `addRef`s the file system when a session is created (see
/// [`SessionDesc::file_system`](crate::SessionDesc::file_system)), this
/// wrapper may be dropped as soon as `create_session` returns.
#[repr(transparent)]
#[derive(Clone)]
pub struct FileSystemObject(IUnknown);

unsafe impl Interface for FileSystemObject {
	type Vtable = sys::ISlangFileSystemVtable;
	// `ISlangFileSystem` IID from slang.h: 003A09FC-3A4D-4BA0-AD60-1FD863A915AB.
	const IID: UUID = uuid(0x003a_09fc_3a4d_4ba0_ad60_1fd8_63a9_15ab);
}

impl FileSystemObject {
	/// Wraps a [`FileSystem`] implementation in a COM object that can be
	/// passed to Slang.
	pub fn new(file_system: impl FileSystem + 'static) -> Self {
		let object = Box::new(FileSystemCom {
			vtable: &VTABLE,
			ref_count: AtomicU32::new(1),
			inner: Box::new(file_system),
		});
		let ptr = Box::into_raw(object);
		// SAFETY: `Box::into_raw` never returns null. The object exposes the
		// `ISlangFileSystem` vtable at its first field, satisfying the
		// `Interface` safety contract.
		Self(IUnknown(unsafe {
			std::ptr::NonNull::new_unchecked(ptr as *mut c_void)
		}))
	}
}

/// The heap-allocated COM object. `repr(C)` with the vtable pointer first,
/// matching the C++ object layout Slang expects.
#[repr(C)]
struct FileSystemCom {
	vtable: *const sys::ISlangFileSystemVtable,
	ref_count: AtomicU32,
	inner: Box<dyn FileSystem>,
}

static VTABLE: sys::ISlangFileSystemVtable = sys::ISlangFileSystemVtable {
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
	if uuid_eq(guid, &IUnknown::IID)
		|| uuid_eq(guid, &ISLANG_CASTABLE_IID)
		|| uuid_eq(guid, &FileSystemObject::IID)
	{
		// SAFETY: `out_object` is valid per the check above. This object
		// implements all three interfaces at the same pointer value.
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
		// `FileSystemObject::new` and is reclaimed exactly once here.
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
	if uuid_eq(guid, &IUnknown::IID)
		|| uuid_eq(guid, &ISLANG_CASTABLE_IID)
		|| uuid_eq(guid, &FileSystemObject::IID)
	{
		// `castAs` returns a *borrowed*, non-ref-counted pointer (slang.h).
		this
	} else {
		null_mut()
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
	if this.is_null() {
		return sys::SLANG_E_INVALID_ARG;
	}
	// SAFETY: `str_from_slang` accepts a null pointer; a non-null `path` from
	// Slang is a NUL-terminated string valid for the duration of the call.
	let Some(path) = (unsafe { str_from_slang(path) }) else {
		return sys::SLANG_E_INVALID_ARG;
	};
	// SAFETY: `this` points to a live `FileSystemCom` kept alive by the COM
	// reference count.
	let object = unsafe { &*(this as *const FileSystemCom) };
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
		Ok(Err(FileSystemError::NotFound)) => sys::SLANG_E_NOT_FOUND,
		Ok(Err(FileSystemError::Other)) | Err(_) => sys::SLANG_FAIL,
	}
}
