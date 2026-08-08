//! Safe, idiomatic wrappers over Slang's COM interfaces, covering the
//! "source in, bytecode out" compilation main line for the SPIRV target.
//!
//! Reflection, additional compile targets, and file-based module loading are
//! deliberately out of scope for this first stage — see the crate root docs.

use crate::sys::com::{vtable_fn, ComPtr, Unknown};
use crate::{Error, Result};
use shader_slang_sys as ffi;
use shader_slang_sys::slang::{
    IComponentType, IEntryPoint, IGlobalSession, IModule, ISession, SessionDesc, TargetDesc,
};
use shader_slang_sys::{ISlangBlob, ISlangUnknown, SlangResult};
use std::ffi::CString;
use std::os::raw::c_char;

unsafe impl Unknown for IGlobalSession {}
unsafe impl Unknown for ISession {}
unsafe impl Unknown for IModule {}
unsafe impl Unknown for IEntryPoint {}
unsafe impl Unknown for IComponentType {}
unsafe impl Unknown for ISlangBlob {}

fn as_unknown<T>(ptr: *mut T) -> *mut ISlangUnknown {
    ptr as *mut ISlangUnknown
}

fn cstring(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::InvalidArgument("string contains an interior NUL byte"))
}

/// Reads a blob's contents as a `Vec<u8>` and releases the blob.
unsafe fn take_blob_bytes(blob: *mut ISlangBlob) -> Option<Vec<u8>> {
    let ptr = std::ptr::NonNull::new(blob)?;
    let get_ptr: unsafe extern "C" fn(*mut ISlangBlob) -> *const std::ffi::c_void =
        vtable_fn(ptr.as_ptr(), 3);
    let get_size: unsafe extern "C" fn(*mut ISlangBlob) -> usize = vtable_fn(ptr.as_ptr(), 4);
    let data = get_ptr(ptr.as_ptr());
    let size = get_size(ptr.as_ptr());
    let bytes = if data.is_null() || size == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(data as *const u8, size).to_vec()
    };
    let release: unsafe extern "C" fn(*mut ISlangUnknown) -> u32 = vtable_fn(as_unknown(ptr.as_ptr()), 2);
    release(as_unknown(ptr.as_ptr()));
    Some(bytes)
}

/// Reads a blob's contents as a `String` (assumed UTF-8, which is what
/// Slang's diagnostic output uses) and releases the blob.
unsafe fn take_blob_string(blob: *mut ISlangBlob) -> Option<String> {
    take_blob_bytes(blob).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// The top-level entry point into the Slang compiler. Expensive to create
/// (it loads Slang's standard library), so applications should create one
/// and reuse it across [`Session`]s.
pub struct GlobalSession {
    inner: ComPtr<IGlobalSession>,
}

impl GlobalSession {
    /// Creates a new global session with default options.
    pub fn new() -> Result<Self> {
        let mut out: *mut IGlobalSession = std::ptr::null_mut();
        let result: SlangResult =
            unsafe { ffi::slang_createGlobalSession2(std::ptr::null(), &mut out) };
        if result < 0 {
            return Err(Error::ApiError(result));
        }
        let inner = unsafe { ComPtr::from_raw_owned(out) }.ok_or(Error::NullPointer)?;
        Ok(GlobalSession { inner })
    }

    /// Creates a new [`Session`] configured to compile to SPIRV.
    ///
    /// `spirv_profile` selects the SPIR-V version profile (e.g.
    /// `"spirv_1_5"`); pass `None` for Slang's default.
    pub fn create_session(&self, spirv_profile: Option<&str>) -> Result<Session> {
        let profile = match spirv_profile {
            Some(name) => self.find_profile(name)?,
            None => 0, // SLANG_PROFILE_UNKNOWN — Slang picks a default.
        };

        let target = TargetDesc {
            format: ffi::SlangCompileTarget_SLANG_SPIRV as _,
            profile,
            ..Default::default()
        };

        let desc = SessionDesc {
            targets: &target,
            targetCount: 1,
            ..Default::default()
        };

        let mut out: *mut ISession = std::ptr::null_mut();
        let create_session: unsafe extern "C" fn(
            *mut IGlobalSession,
            SessionDesc,
            *mut *mut ISession,
        ) -> SlangResult = unsafe { vtable_fn(self.inner.as_ptr(), 3) };
        let result = unsafe { create_session(self.inner.as_ptr(), desc, &mut out) };
        if result < 0 {
            return Err(Error::ApiError(result));
        }
        let inner = unsafe { ComPtr::from_raw_owned(out) }.ok_or(Error::NullPointer)?;
        Ok(Session {
            inner,
            _global: self.inner.clone(),
        })
    }

    fn find_profile(&self, name: &str) -> Result<u32> {
        let name = cstring(name)?;
        let find_profile: unsafe extern "C" fn(*mut IGlobalSession, *const c_char) -> u32 =
            unsafe { vtable_fn(self.inner.as_ptr(), 4) };
        Ok(unsafe { find_profile(self.inner.as_ptr(), name.as_ptr()) })
    }
}

/// A compilation session, scoped to a [`GlobalSession`] and a fixed set of
/// compile targets. Holds a reference to its [`GlobalSession`] so it stays
/// alive for at least as long as any session created from it.
pub struct Session {
    inner: ComPtr<ISession>,
    _global: ComPtr<IGlobalSession>,
}

impl Session {
    /// Loads a module from an in-memory Slang/HLSL source string.
    ///
    /// `module_name` is the logical module name (used for `import`
    /// resolution); `path` is a display-only path used in diagnostics.
    pub fn load_module_from_source(&self, module_name: &str, path: &str, source: &str) -> Result<Module> {
        let module_name = cstring(module_name)?;
        let path = cstring(path)?;
        let source = cstring(source)?;

        let mut diagnostics: *mut ISlangBlob = std::ptr::null_mut();
        let load: unsafe extern "C" fn(
            *mut ISession,
            *const c_char,
            *const c_char,
            *const c_char,
            *mut *mut ISlangBlob,
        ) -> *mut IModule = unsafe { vtable_fn(self.inner.as_ptr(), 20) };

        let module_ptr = unsafe {
            load(
                self.inner.as_ptr(),
                module_name.as_ptr(),
                path.as_ptr(),
                source.as_ptr(),
                &mut diagnostics,
            )
        };

        let diagnostics_text = unsafe { take_blob_string(diagnostics) };

        match unsafe { ComPtr::from_raw_owned(module_ptr) } {
            Some(inner) => Ok(Module {
                inner,
                _session: self.inner.clone(),
            }),
            None => Err(Error::Compilation(
                diagnostics_text.unwrap_or_else(|| "module failed to load (no diagnostics available)".into()),
            )),
        }
    }
}

/// A loaded, checked Slang/HLSL module.
pub struct Module {
    inner: ComPtr<IModule>,
    _session: ComPtr<ISession>,
}

impl Module {
    /// Looks up an entry point (a function marked `[shader("...")]`, or
    /// otherwise recognized as a stage entry point) by name.
    pub fn find_entry_point_by_name(&self, name: &str) -> Result<EntryPoint> {
        let name = cstring(name)?;
        let mut out: *mut IEntryPoint = std::ptr::null_mut();
        let find: unsafe extern "C" fn(
            *mut IModule,
            *const c_char,
            *mut *mut IEntryPoint,
        ) -> SlangResult = unsafe { vtable_fn(self.inner.as_ptr(), 17) };
        let result = unsafe { find(self.inner.as_ptr(), name.as_ptr(), &mut out) };
        if result < 0 {
            return Err(Error::ApiError(result));
        }
        let inner = unsafe { ComPtr::from_raw_owned(out) }.ok_or(Error::NullPointer)?;
        Ok(EntryPoint { inner })
    }
}

/// A single compilable entry point found within a [`Module`].
pub struct EntryPoint {
    inner: ComPtr<IEntryPoint>,
}

impl EntryPoint {
    /// Links this entry point (together with the module(s) it depends on)
    /// into a fully-resolved [`Program`] ready for code generation.
    pub fn link(&self) -> Result<Program> {
        let mut out: *mut IComponentType = std::ptr::null_mut();
        let mut diagnostics: *mut ISlangBlob = std::ptr::null_mut();
        // IEntryPoint inherits IComponentType's vtable (slots 3..=16), so
        // `link` (own-index 7 on IComponentType) sits at slot 3 + 7 = 10.
        let link: unsafe extern "C" fn(
            *mut IEntryPoint,
            *mut *mut IComponentType,
            *mut *mut ISlangBlob,
        ) -> SlangResult = unsafe { vtable_fn(self.inner.as_ptr(), 10) };
        let result = unsafe { link(self.inner.as_ptr(), &mut out, &mut diagnostics) };
        let diagnostics_text = unsafe { take_blob_string(diagnostics) };
        if result < 0 {
            return Err(Error::Compilation(
                diagnostics_text.unwrap_or_else(|| format!("link failed (SlangResult {result})")),
            ));
        }
        let inner = unsafe { ComPtr::from_raw_owned(out) }.ok_or(Error::NullPointer)?;
        Ok(Program { inner })
    }
}

/// A fully linked component type, ready to produce target code.
pub struct Program {
    inner: ComPtr<IComponentType>,
}

impl Program {
    /// Retrieves the compiled code for target index `target_index` (index
    /// into the [`Session`]'s target list; use `0` for the single-target
    /// sessions created by [`GlobalSession::create_session`]).
    pub fn get_target_code(&self, target_index: i64) -> Result<Vec<u8>> {
        let mut code: *mut ISlangBlob = std::ptr::null_mut();
        let mut diagnostics: *mut ISlangBlob = std::ptr::null_mut();
        // getTargetCode is own-index 11 on IComponentType => slot 3 + 11 = 14.
        let get_target_code: unsafe extern "C" fn(
            *mut IComponentType,
            i64,
            *mut *mut ISlangBlob,
            *mut *mut ISlangBlob,
        ) -> SlangResult = unsafe { vtable_fn(self.inner.as_ptr(), 14) };
        let result = unsafe {
            get_target_code(self.inner.as_ptr(), target_index, &mut code, &mut diagnostics)
        };
        let diagnostics_text = unsafe { take_blob_string(diagnostics) };
        if result < 0 {
            return Err(Error::Compilation(
                diagnostics_text.unwrap_or_else(|| format!("compilation failed (SlangResult {result})")),
            ));
        }
        unsafe { take_blob_bytes(code) }.ok_or(Error::NullPointer)
    }
}
