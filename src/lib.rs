//! # shader-slang
//!
//! Safe, idiomatic Rust bindings for the [Slang](https://github.com/shader-slang/slang)
//! shading language compiler.
//!
//! ## Overview
//!
//! This crate provides Rust bindings to the Slang compiler, allowing you to:
//! - Compile HLSL/Slang shaders to SPIRV (with more targets planned)
//! - Manage shader compilation sessions
//!
//! ## Architecture
//!
//! This crate is organized into several layers:
//!
//! - **`shader-slang-sys`** - Low-level FFI bindings generated from Slang C headers
//! - **`sys`** - System-level wrappers around FFI (COM interface handling, etc.)
//! - **High-level API** - Safe, idiomatic Rust wrappers
//!
//! ## Usage
//!
//! ```rust,no_run
//! use shader_slang::GlobalSession;
//!
//! let session = GlobalSession::new()?;
//! // ... compile shaders
//! # Ok::<(), shader_slang::Error>(())
//! ```

#![allow(non_camel_case_types, non_snake_case)]

pub mod api;
pub mod sys;

pub use api::{EntryPoint, GlobalSession, Module, Program, Session};

/// Raw FFI bindings, re-exported from the `shader-slang-sys` crate.
pub use shader_slang_sys as ffi;

// Re-export low-level types from ffi for convenience
pub use shader_slang_sys::root::*;

// Version information
pub const SLANG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Result type alias for Slang operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for Slang operations
#[derive(Debug, Clone)]
pub enum Error {
    /// Slang API returned an error code
    ApiError(i32),
    /// Invalid argument passed to function
    InvalidArgument(&'static str),
    /// Null pointer encountered
    NullPointer,
    /// Operation not supported
    NotSupported,
    /// String conversion error
    StringConversion,
    /// Compilation failed; contains the diagnostics text Slang produced
    /// (syntax errors, semantic errors, etc.), if any was available.
    Compilation(String),
    /// Custom error message
    Custom(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ApiError(code) => write!(f, "Slang API error: {}", code),
            Error::InvalidArgument(msg) => write!(f, "Invalid argument: {}", msg),
            Error::NullPointer => write!(f, "Null pointer encountered"),
            Error::NotSupported => write!(f, "Operation not supported"),
            Error::StringConversion => write!(f, "String conversion error"),
            Error::Compilation(diagnostics) => write!(f, "Slang compilation failed:\n{}", diagnostics),
            Error::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for Error {}

/// Convert a SlangResult to our Error type
pub use sys::result_from_slang;