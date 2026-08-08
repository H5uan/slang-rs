//! Generic COM-style smart pointer and virtual-call helpers for Slang's
//! `ISlangUnknown`-derived interfaces.
//!
//! `bindgen` emits Slang's C++ interfaces as opaque structs with a single
//! `vtable_` pointer (it does not generate per-method call wrappers for
//! virtual functions), so every method call here is a manual indexed read
//! through that vtable. The slot index for each method is fixed by its
//! declaration order in `slang.h` (queryInterface=0, addRef=1, release=2,
//! then each interface's own methods in declaration order) — see
//! `docs/vtable-layout.md` for the derivation. Slang's ABI stability policy
//! (documented in its own CLAUDE.md: append-only, never reorder) is what
//! makes hardcoding these offsets safe across patch versions.

use shader_slang_sys::ISlangUnknown;
use std::marker::PhantomData;
use std::ptr::NonNull;

/// Anything that starts with an `ISlangUnknown` base (guaranteed by C++
/// single inheritance + `#[repr(C)]` field order in the generated bindings).
///
/// # Safety
/// Implementors must guarantee that a `*mut Self` is safely castable to
/// `*mut ISlangUnknown` (i.e. `Self` starts with `ISlangUnknown` as its
/// first field, directly or transitively).
pub unsafe trait Unknown {}

unsafe impl Unknown for ISlangUnknown {}

/// Calls the `n`th virtual method slot with no extra arguments, of the shape
/// `SLANG_MCALL(ISlangUnknown*) -> R`. `addRef`/`release` are slots 1 and 2.
unsafe fn vcall0<R>(ptr: *mut ISlangUnknown, slot: usize) -> R {
    let vtable = *(ptr as *const *const ());
    let func_ptr = *(vtable as *const usize).add(slot) as *const ();
    let func: unsafe extern "C" fn(*mut ISlangUnknown) -> R = std::mem::transmute(func_ptr);
    func(ptr)
}

/// A reference-counted smart pointer over a Slang COM interface `T`.
/// `Clone` calls `addRef`; `Drop` calls `release`.
pub struct ComPtr<T: Unknown> {
    ptr: NonNull<T>,
    _marker: PhantomData<T>,
}

impl<T: Unknown> ComPtr<T> {
    /// Wraps a raw pointer that already owns one reference (e.g. returned
    /// directly from a Slang factory function like `createSession`).
    /// Returns `None` for a null pointer.
    ///
    /// # Safety
    /// `ptr`, if non-null, must point to a live object implementing the
    /// Slang `ISlangUnknown` ABI, with the caller's reference being
    /// transferred to the returned `ComPtr` (no extra `addRef` is taken).
    pub unsafe fn from_raw_owned(ptr: *mut T) -> Option<Self> {
        NonNull::new(ptr).map(|ptr| ComPtr {
            ptr,
            _marker: PhantomData,
        })
    }

    /// Wraps a borrowed raw pointer, taking a new reference via `addRef`.
    ///
    /// # Safety
    /// `ptr`, if non-null, must point to a live object implementing the
    /// Slang `ISlangUnknown` ABI.
    pub unsafe fn from_raw_borrowed(ptr: *mut T) -> Option<Self> {
        let ptr = NonNull::new(ptr)?;
        vcall0::<u32>(ptr.as_ptr() as *mut ISlangUnknown, 1);
        Some(ComPtr {
            ptr,
            _marker: PhantomData,
        })
    }

    pub fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T: Unknown> Clone for ComPtr<T> {
    fn clone(&self) -> Self {
        unsafe {
            vcall0::<u32>(self.ptr.as_ptr() as *mut ISlangUnknown, 1);
        }
        ComPtr {
            ptr: self.ptr,
            _marker: PhantomData,
        }
    }
}

impl<T: Unknown> Drop for ComPtr<T> {
    fn drop(&mut self) {
        unsafe {
            vcall0::<u32>(self.ptr.as_ptr() as *mut ISlangUnknown, 2);
        }
    }
}

// SAFETY: Slang's COM objects use a plain (non-atomic) refcount and document
// that a session and everything created from it must be externally
// synchronized when shared across threads; ComPtr does not add any
// synchronization of its own, so it inherits that same-thread-only contract
// and is intentionally not Send/Sync.

/// Reads the function pointer at vtable slot `slot` for the object `ptr`
/// points to, and transmutes it to `F`. Callers supply the exact `extern
/// "C"` signature (including the leading `*mut T` "this" parameter) for the
/// method at that slot.
///
/// # Safety
/// `slot` must name a virtual method on `T`'s interface whose signature
/// exactly matches `F`, per the vtable layout documented on [`vcall0`].
pub(crate) unsafe fn vtable_fn<T, F: Copy>(ptr: *mut T, slot: usize) -> F {
    let vtable = *(ptr as *const *const ());
    let func_ptr = *(vtable as *const usize).add(slot) as *const ();
    std::mem::transmute_copy(&func_ptr)
}

