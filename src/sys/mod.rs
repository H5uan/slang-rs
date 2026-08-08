//! # System-level wrappers for Slang FFI
//!
//! This module provides safe wrappers around the low-level FFI bindings.
//! It handles:
//! - Null pointer checks
//! - Error code conversion
//! - String conversions

use crate::{Error, Result};
use std::ffi::CStr;

pub mod com;
pub use com::{ComPtr, Unknown};

/// SlangResult type alias from FFI
pub type SlangResult = shader_slang_sys::SlangResult;

/// Helper to convert a C string pointer to a Rust string.
///
/// # Safety
/// `ptr`, if non-null, must point to a valid NUL-terminated C string.
pub unsafe fn ptr_to_string(ptr: *const std::os::raw::c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string())
}

/// Check if a SlangResult indicates success
pub fn slang_succeeded(result: SlangResult) -> bool {
    result >= 0
}

/// Check if a SlangResult indicates failure
pub fn slang_failed(result: SlangResult) -> bool {
    result < 0
}

/// Convert a SlangResult to a Result<()>
pub fn result_from_slang(result: SlangResult) -> Result<()> {
    if slang_succeeded(result) {
        Ok(())
    } else {
        Err(Error::ApiError(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_checking() {
        assert!(slang_succeeded(0)); // SLANG_OK
        assert!(slang_succeeded(1));
        assert!(slang_failed(-1));
    }
}
