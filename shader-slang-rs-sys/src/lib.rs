//! FFI bindings for the Slang shader language compiler

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]
// The bindgen-generated bindings included below trigger these lints in their
// constified-enum impls; they are not actionable in handwritten code.
#![allow(clippy::useless_transmute, clippy::missing_safety_doc, clippy::ptr_offset_with_cast)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::ffi::{c_char, c_int, c_void};

// Based on Slang version 2026.14.1

/// `SLANG_FAIL` from slang.h. bindgen cannot evaluate the `SLANG_MAKE_ERROR`
/// macro expression, so the constant is spelled out here (HRESULT `E_FAIL`).
pub const SLANG_FAIL: SlangResult = 0x80004005u32 as i32;

/// `SLANG_E_NO_INTERFACE` from slang.h
/// (`SLANG_MAKE_WIN_GENERAL_ERROR(0x4002)`, HRESULT `E_NOINTERFACE`); not
/// emitted by bindgen for the same reason as `SLANG_FAIL`.
pub const SLANG_E_NO_INTERFACE: SlangResult = 0x80004002u32 as i32;

/// `SLANG_E_INVALID_ARG` from slang.h
/// (`SLANG_MAKE_ERROR(SLANG_FACILITY_WIN_API, 0x57)`).
pub const SLANG_E_INVALID_ARG: SlangResult = 0x80070057u32 as i32;

/// `SLANG_E_NOT_FOUND` from slang.h (`SLANG_MAKE_CORE_ERROR(5)`).
pub const SLANG_E_NOT_FOUND: SlangResult = 0x82000005u32 as i32;

/// `SLANG_E_NOT_IMPLEMENTED` from slang.h
/// (`SLANG_MAKE_WIN_GENERAL_ERROR(0x4001)`, HRESULT `E_NOTIMPL`); not
/// emitted by bindgen for the same reason as `SLANG_FAIL`.
pub const SLANG_E_NOT_IMPLEMENTED: SlangResult = 0x80004001u32 as i32;

#[repr(C)]
pub struct ICastableVtable {
	pub _base: ISlangUnknown__bindgen_vtable,

	pub castAs: unsafe extern "C" fn(*mut c_void, guid: *const SlangUUID) -> *mut c_void,
}

#[repr(C)]
pub struct IBlobVtable {
	pub _base: ISlangUnknown__bindgen_vtable,

	pub getBufferPointer: unsafe extern "C" fn(*mut c_void) -> *const c_void,
	pub getBufferSize: unsafe extern "C" fn(*mut c_void) -> usize,
}

#[repr(C)]
pub struct ISlangFileSystemVtable {
	pub _base: ICastableVtable,

	pub loadFile: unsafe extern "C" fn(*mut c_void, path: *const c_char, outBlob: *mut *mut ISlangBlob) -> SlangResult,
}

#[repr(C)]
pub struct ISlangFileSystemExtVtable {
	pub _base: ISlangFileSystemVtable,

	pub getFileUniqueIdentity: unsafe extern "C" fn(*mut c_void, path: *const c_char, outUniqueIdentity: *mut *mut ISlangBlob) -> SlangResult,
	pub calcCombinedPath: unsafe extern "C" fn(*mut c_void, fromPathType: SlangPathType, fromPath: *const c_char, path: *const c_char, pathOut: *mut *mut ISlangBlob) -> SlangResult,
	pub getPathType: unsafe extern "C" fn(*mut c_void, path: *const c_char, pathTypeOut: *mut SlangPathType) -> SlangResult,
	pub getPath: unsafe extern "C" fn(*mut c_void, kind: PathKind, path: *const c_char, outPath: *mut *mut ISlangBlob) -> SlangResult,
	pub clearCache: unsafe extern "C" fn(*mut c_void),
	pub enumeratePathContents: unsafe extern "C" fn(*mut c_void, path: *const c_char, callback: FileSystemContentsCallBack, userData: *mut c_void) -> SlangResult,
	pub getOSPathKind: unsafe extern "C" fn(*mut c_void) -> OSPathKind,
}

#[repr(C)]
pub struct ISlangMutableFileSystemVtable {
	pub _base: ISlangFileSystemExtVtable,

	pub saveFile: unsafe extern "C" fn(*mut c_void, path: *const c_char, data: *const c_void, size: usize) -> SlangResult,
	pub saveFileBlob: unsafe extern "C" fn(*mut c_void, path: *const c_char, dataBlob: *mut ISlangBlob) -> SlangResult,
	pub remove: unsafe extern "C" fn(*mut c_void, path: *const c_char) -> SlangResult,
	pub createDirectory: unsafe extern "C" fn(*mut c_void, path: *const c_char) -> SlangResult,
}

// Note: `ISlangSharedLibrary` also exposes the `findFuncByName` convenience
// wrapper in slang.h, but that one is `SLANG_FORCE_INLINE` (not virtual), so
// the vtable holds `findSymbolAddressByName` only.
#[repr(C)]
pub struct ISlangSharedLibraryVtable {
	pub _base: ICastableVtable,

	pub findSymbolAddressByName: unsafe extern "C" fn(*mut c_void, name: *const c_char) -> *mut c_void,
}

#[repr(C)]
pub struct ISlangSharedLibraryLoaderVtable {
	pub _base: ISlangUnknown__bindgen_vtable,

	pub loadSharedLibrary: unsafe extern "C" fn(*mut c_void, path: *const c_char, sharedLibraryOut: *mut *mut ISlangSharedLibrary) -> SlangResult,
}

#[repr(C)]
pub struct IGlobalSessionVtable {
	pub _base: ISlangUnknown__bindgen_vtable,

	pub createSession: unsafe extern "C" fn(*mut c_void, desc: *const slang_SessionDesc, outSession: *mut *mut slang_ISession) -> SlangResult,
	pub findProfile: unsafe extern "C" fn(*mut c_void, name: *const c_char) -> SlangProfileID,
	pub setDownstreamCompilerPath: unsafe extern "C" fn(*mut c_void, passThrough: SlangPassThrough, path: *const c_char),
	#[deprecated( note = "Use setLanguagePrelude instead")]
	pub setDownstreamCompilerPrelude: unsafe extern "C" fn(*mut c_void, passThrough: SlangPassThrough, preludeText: *const c_char),
	#[deprecated( note = "Use getLanguagePrelude instead")]
	pub getDownstreamCompilerPrelude: unsafe extern "C" fn(*mut c_void, passThrough: SlangPassThrough, outPrelude: *mut *mut ISlangBlob),
	pub getBuildTagString: unsafe extern "C" fn(*mut c_void) -> *const c_char,
	pub setDefaultDownstreamCompiler: unsafe extern "C" fn(*mut c_void, sourceLanguage: SlangSourceLanguage, defaultCompiler: SlangPassThrough) -> SlangResult,
	pub getDefaultDownstreamCompiler: unsafe extern "C" fn(*mut c_void, sourceLanguage: SlangSourceLanguage) -> SlangPassThrough,
	pub setLanguagePrelude: unsafe extern "C" fn(*mut c_void, sourceLanguage: SlangSourceLanguage, preludeText: *const c_char),
	pub getLanguagePrelude: unsafe extern "C" fn(*mut c_void, sourceLanguage: SlangSourceLanguage, outPrelude: *mut *mut ISlangBlob),
	pub createCompileRequest: unsafe extern "C" fn(*mut c_void, *mut *mut slang_ICompileRequest) -> SlangResult,
	pub addBuiltins: unsafe extern "C" fn(*mut c_void, sourcePath: *const c_char, sourceString: *const c_char),
	pub setSharedLibraryLoader: unsafe extern "C" fn(*mut c_void, loader: *mut ISlangSharedLibraryLoader),
	pub getSharedLibraryLoader: unsafe extern "C" fn(*mut c_void) -> *mut ISlangSharedLibraryLoader,
	pub checkCompileTargetSupport: unsafe extern "C" fn(*mut c_void, target: SlangCompileTarget) -> SlangResult,
	pub checkPassThroughSupport: unsafe extern "C" fn(*mut c_void, passThrough: SlangPassThrough) -> SlangResult,
	pub compileCoreModule: unsafe extern "C" fn(*mut c_void, flags: slang_CompileCoreModuleFlags) -> SlangResult,
	pub loadCoreModule: unsafe extern "C" fn(*mut c_void, coreModule: *const c_void, coreModuleSizeInBytes: usize) -> SlangResult,
	pub saveCoreModule: unsafe extern "C" fn(*mut c_void, archiveType: SlangArchiveType, outBlob: *mut *mut ISlangBlob) -> SlangResult,
	pub findCapability: unsafe extern "C" fn(*mut c_void, name: *const c_char) -> SlangCapabilityID,
	pub setDownstreamCompilerForTransition: unsafe extern "C" fn(*mut c_void, source: SlangCompileTarget, target: SlangCompileTarget, compiler: SlangPassThrough),
	pub getDownstreamCompilerForTransition: unsafe extern "C" fn(*mut c_void, source: SlangCompileTarget, target: SlangCompileTarget) -> SlangPassThrough,
	pub getCompilerElapsedTime: unsafe extern "C" fn(*mut c_void, outTotalTime: *mut f64, outDownstreamTime: *mut f64),
	pub setSPIRVCoreGrammar: unsafe extern "C" fn(*mut c_void, jsonPath: *const c_char) -> SlangResult,
	pub parseCommandLineArguments: unsafe extern "C" fn(*mut c_void, argc: c_int, argv: *const *const c_char, outSessionDesc: *mut slang_SessionDesc, outAuxAllocation: *mut *mut ISlangUnknown) -> SlangResult,
	pub getSessionDescDigest: unsafe extern "C" fn(*mut c_void, sessionDesc: *const slang_SessionDesc, outBlob: *mut *mut ISlangBlob) -> SlangResult,
	pub compileBuiltinModule: unsafe extern "C" fn(*mut c_void, module: slang_BuiltinModuleName, flags: slang_CompileCoreModuleFlags) -> SlangResult,
	pub loadBuiltinModule: unsafe extern "C" fn(*mut c_void, module: slang_BuiltinModuleName, moduleData: *const c_void, sizeInBytes: usize) -> SlangResult,
	pub saveBuiltinModule: unsafe extern "C" fn(*mut c_void, module: slang_BuiltinModuleName, archiveType: SlangArchiveType, outBlob: *mut *mut ISlangBlob) -> SlangResult,
	pub getDownstreamCompilerVersion: unsafe extern "C" fn(*mut c_void, passThrough: SlangPassThrough, outMajor: *mut c_int, outMinor: *mut c_int) -> SlangResult,
}

#[repr(C)]
pub struct ISessionVtable {
	pub _base: ISlangUnknown__bindgen_vtable,

	pub getGlobalSession: unsafe extern "C" fn(*mut c_void) -> *mut slang_IGlobalSession,
	pub loadModule: unsafe extern "C" fn(*mut c_void, moduleName: *const c_char, outDiagnostics: *mut *mut ISlangBlob) -> *mut slang_IModule,
	pub loadModuleFromSource: unsafe extern "C" fn(*mut c_void, moduleName: *const c_char, path: *const c_char, source: *mut ISlangBlob, outDiagnostics: *mut *mut ISlangBlob) -> *mut slang_IModule,
	pub createCompositeComponentType: unsafe extern "C" fn(*mut c_void, componentTypes: *const *const slang_IComponentType, componentTypeCount: SlangInt, outCompositeComponentType: *mut *mut slang_IComponentType, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub specializeType: unsafe extern "C" fn(*mut c_void, type_: *mut slang_TypeReflection, specializationArgs: *const slang_SpecializationArg, specializationArgCount: SlangInt, outDiagnostics: *mut *mut ISlangBlob) -> *mut slang_TypeReflection,
	pub getTypeLayout: unsafe extern "C" fn(*mut c_void, type_: *mut slang_TypeReflection, targetIndex: SlangInt, rules: slang_LayoutRules, outDiagnostics: *mut *mut ISlangBlob) -> *mut slang_TypeLayoutReflection,
	pub getContainerType: unsafe extern "C" fn(*mut c_void, elementType: *mut slang_TypeReflection, containerType: slang_ContainerType, outDiagnostics: *mut *mut ISlangBlob) -> *mut slang_TypeReflection,
	pub getDynamicType: unsafe extern "C" fn(*mut c_void) -> *mut slang_TypeReflection,
	pub getTypeRTTIMangledName: unsafe extern "C" fn(*mut c_void, type_: *mut slang_TypeReflection, outNameBlob: *mut *mut ISlangBlob) -> SlangResult,
	pub getTypeConformanceWitnessMangledName: unsafe extern "C" fn(*mut c_void, type_: *mut slang_TypeReflection, interfaceType: *mut slang_TypeReflection, outNameBlob: *mut *mut ISlangBlob) -> SlangResult,
	pub getTypeConformanceWitnessSequentialID: unsafe extern "C" fn(*mut c_void, type_: *mut slang_TypeReflection, interfaceType: *mut slang_TypeReflection, outId: *mut u32) -> SlangResult,
	pub createCompileRequest: unsafe extern "C" fn(*mut c_void, outCompileRequest: *mut *mut slang_ICompileRequest) -> SlangResult,
	pub createTypeConformanceComponentType: unsafe extern "C" fn(*mut c_void, type_: *mut slang_TypeReflection, interfaceType: *mut slang_TypeReflection, outConformance: *mut *mut slang_ITypeConformance, conformanceIdOverride: SlangInt, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub loadModuleFromIRBlob: unsafe extern "C" fn(*mut c_void, moduleName: *const c_char, path: *const c_char, source: *mut ISlangBlob, outDiagnostics: *mut *mut ISlangBlob) -> *mut slang_IModule,
	pub getLoadedModuleCount: unsafe extern "C" fn(*mut c_void) -> SlangInt,
	pub getLoadedModule: unsafe extern "C" fn(*mut c_void, index: SlangInt) -> *mut slang_IModule,
	pub isBinaryModuleUpToDate: unsafe extern "C" fn(*mut c_void, modulePath: *const c_char, binaryModuleBlob: *mut ISlangBlob) -> bool,
	pub loadModuleFromSourceString: unsafe extern "C" fn(*mut c_void, moduleName: *const c_char, path: *const c_char, string: *const c_char, outDiagnostics: *mut *mut ISlangBlob) -> *mut slang_IModule,
	pub getDynamicObjectRTTIBytes: unsafe extern "C" fn(*mut c_void, type_: *mut slang_TypeReflection, interfaceType: *mut slang_TypeReflection, outRTTIDataBuffer: *mut u32, bufferSizeInBytes: u32) -> SlangResult,
	pub loadModuleInfoFromIRBlob: unsafe extern "C" fn(*mut c_void, source: *mut ISlangBlob, outModuleVersion: *mut SlangInt, outModuleCompilerVersion: *mut *const c_char, outModuleName: *mut *const c_char) -> SlangResult,
	pub getDeclSourceLocation: unsafe extern "C" fn(*mut c_void, decl: *mut slang_DeclReflection, outLocation: *mut slang_SourceLocation) -> SlangResult,
}

#[repr(C)]
pub struct IMetadataVtable {
	pub _base: ICastableVtable,

	pub isParameterLocationUsed: unsafe extern "C" fn(*mut c_void, category: SlangParameterCategory, spaceIndex: SlangUInt, registerIndex: SlangUInt, outUsed: *mut bool) -> SlangResult,
	pub getDebugBuildIdentifier: unsafe extern "C" fn(*mut c_void) -> *const c_char,
}

#[repr(C)]
pub struct IComponentTypeVtable {
	pub _base: ISlangUnknown__bindgen_vtable,

	pub getSession: unsafe extern "C" fn(*mut c_void) -> *mut slang_ISession,
	pub getLayout: unsafe extern "C" fn(*mut c_void, targetIndex: SlangInt, outDiagnostics: *mut *mut ISlangBlob) -> *mut slang_ProgramLayout,
	pub getSpecializationParamCount: unsafe extern "C" fn(*mut c_void) -> SlangInt,
	pub getEntryPointCode: unsafe extern "C" fn(*mut c_void, entryPointIndex: SlangInt, targetIndex: SlangInt, outCode: *mut *mut ISlangBlob, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub getResultAsFileSystem: unsafe extern "C" fn(*mut c_void, entryPointIndex: SlangInt, targetIndex: SlangInt, outFileSystem: *mut *mut ISlangMutableFileSystem) -> SlangResult,
	pub getEntryPointHash: unsafe extern "C" fn(*mut c_void, entryPointIndex: SlangInt, targetIndex: SlangInt, outHash: *mut *mut ISlangBlob),
	pub specialize: unsafe extern "C" fn(*mut c_void, specializationArgs: *const slang_SpecializationArg, specializationArgCount: SlangInt, outSpecializedComponentType: *mut *mut slang_IComponentType, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub link: unsafe extern "C" fn(*mut c_void, outLinkedComponentType: *mut *mut slang_IComponentType, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub getEntryPointHostCallable: unsafe extern "C" fn(*mut c_void, entryPointIndex: c_int, targetIndex: c_int, outSharedLibrary: *mut *mut ISlangSharedLibrary, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub renameEntryPoint: unsafe extern "C" fn(*mut c_void, newName: *const c_char, outEntryPoint: *mut *mut slang_IComponentType) -> SlangResult,
	pub linkWithOptions: unsafe extern "C" fn(*mut c_void, outLinkedComponentType: *mut *mut slang_IComponentType, compilerOptionEntryCount: u32, compilerOptionEntries: *mut slang_CompilerOptionEntry, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub getTargetCode: unsafe extern "C" fn(*mut c_void, targetIndex: SlangInt, outCode: *mut *mut ISlangBlob, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub getTargetMetadata: unsafe extern "C" fn(*mut c_void, targetIndex: SlangInt, outMetadata: *mut *mut slang_IMetadata, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub getEntryPointMetadata: unsafe extern "C" fn(*mut c_void, entryPointIndex: SlangInt, targetIndex: SlangInt, outMetadata: *mut *mut slang_IMetadata, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
}

#[repr(C)]
pub struct IEntryPointVtable {
	pub _base: IComponentTypeVtable,

	pub getFunctionReflection: unsafe extern "C" fn(*mut c_void) -> *mut slang_FunctionReflection,
}

#[repr(C)]
pub struct ITypeConformanceVtable {
	pub _base: IComponentTypeVtable,
}

#[repr(C)]
pub struct IModuleVtable {
	pub _base: IComponentTypeVtable,

	pub findEntryPointByName: unsafe extern "C" fn(*mut c_void, name: *const c_char, outEntryPoint: *mut *mut slang_IEntryPoint) -> SlangResult,
	pub getDefinedEntryPointCount: unsafe extern "C" fn(*mut c_void) -> SlangInt32,
	pub getDefinedEntryPoint: unsafe extern "C" fn(*mut c_void, index: SlangInt32, outEntryPoint: *mut *mut slang_IEntryPoint) -> SlangResult,
	pub serialize: unsafe extern "C" fn(*mut c_void, outSerializedBlob: *mut *mut ISlangBlob) -> SlangResult,
	pub writeToFile: unsafe extern "C" fn(*mut c_void, fileName: *const c_char) -> SlangResult,
	pub getName: unsafe extern "C" fn(*mut c_void) -> *const c_char,
	pub getFilePath: unsafe extern "C" fn(*mut c_void) -> *const c_char,
	pub getUniqueIdentity: unsafe extern "C" fn(*mut c_void) -> *const c_char,
	pub findAndCheckEntryPoint: unsafe extern "C" fn(*mut c_void, name: *const c_char, stage: SlangStage, outEntryPoint: *mut *mut slang_IEntryPoint, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub getDependencyFileCount: unsafe extern "C" fn(*mut c_void) -> SlangInt32,
	pub getDependencyFilePath: unsafe extern "C" fn(*mut c_void, index: SlangInt32) -> *const c_char,
	pub getModuleReflection: unsafe extern "C" fn(*mut c_void) -> *mut slang_DeclReflection,
	pub disassemble: unsafe extern "C" fn(*mut c_void, outDisassembledBlob: *mut *mut ISlangBlob) -> SlangResult,
}

#[repr(C)]
pub struct ICompileResultVtable {
	pub _base: ICastableVtable,

	pub getItemCount: unsafe extern "C" fn(*mut c_void) -> u32,
	pub getItemData: unsafe extern "C" fn(*mut c_void, index: u32, outBlob: *mut *mut ISlangBlob) -> SlangResult,
	pub getMetadata: unsafe extern "C" fn(*mut c_void, outMetadata: *mut *mut slang_IMetadata) -> SlangResult,
}

// Note: `IComponentType2` inherits `ISlangUnknown` directly (not
// `IComponentType`) in slang.h.
#[repr(C)]
pub struct IComponentType2Vtable {
	pub _base: ISlangUnknown__bindgen_vtable,

	pub getTargetCompileResult: unsafe extern "C" fn(*mut c_void, targetIndex: SlangInt, outCompileResult: *mut *mut slang_ICompileResult, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub getEntryPointCompileResult: unsafe extern "C" fn(*mut c_void, entryPointIndex: SlangInt, targetIndex: SlangInt, outCompileResult: *mut *mut slang_ICompileResult, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
	pub getTargetHostCallable: unsafe extern "C" fn(*mut c_void, targetIndex: c_int, outSharedLibrary: *mut *mut ISlangSharedLibrary, outDiagnostics: *mut *mut ISlangBlob) -> SlangResult,
}

#[repr(C)]
pub struct IBindlessResourceMetadataVtable {
	pub _base: ICastableVtable,

	pub usesBindlessResourceHeap: unsafe extern "C" fn(*mut c_void) -> bool,
}

#[repr(C)]
pub struct ICoverageTracingMetadataVtable {
	pub _base: ICastableVtable,

	pub getCounterCount: unsafe extern "C" fn(*mut c_void) -> u32,
	pub getEntryInfo: unsafe extern "C" fn(*mut c_void, index: u32, outInfo: *mut slang_CoverageEntryInfo) -> SlangResult,
	pub getBufferInfo: unsafe extern "C" fn(*mut c_void, outInfo: *mut slang_CoverageBufferInfo) -> SlangResult,
	pub getEntryCount: unsafe extern "C" fn(*mut c_void) -> u32,
}

#[repr(C)]
pub struct ISyntheticResourceMetadataVtable {
	pub _base: ICastableVtable,

	pub getResourceCount: unsafe extern "C" fn(*mut c_void) -> u32,
	pub getResourceInfo: unsafe extern "C" fn(*mut c_void, index: u32, outInfo: *mut slang_SyntheticResourceInfo) -> SlangResult,
	pub findResourceIndexByID: unsafe extern "C" fn(*mut c_void, id: u32, outIndex: *mut u32) -> SlangResult,
}

#[repr(C)]
pub struct ICooperativeTypesMetadataVtable {
	pub _base: ICastableVtable,

	pub getCooperativeMatrixTypeCount: unsafe extern "C" fn(*mut c_void) -> SlangUInt,
	pub getCooperativeMatrixTypeByIndex: unsafe extern "C" fn(*mut c_void, index: SlangUInt, outType: *mut slang_CooperativeMatrixType) -> SlangResult,
	pub getCooperativeMatrixCombinationCount: unsafe extern "C" fn(*mut c_void) -> SlangUInt,
	pub getCooperativeMatrixCombinationByIndex: unsafe extern "C" fn(*mut c_void, index: SlangUInt, outCombination: *mut slang_CooperativeMatrixCombination) -> SlangResult,
	pub getCooperativeVectorTypeCount: unsafe extern "C" fn(*mut c_void) -> SlangUInt,
	pub getCooperativeVectorTypeByIndex: unsafe extern "C" fn(*mut c_void, index: SlangUInt, outType: *mut slang_CooperativeVectorTypeUsageInfo) -> SlangResult,
	pub getCooperativeVectorCombinationCount: unsafe extern "C" fn(*mut c_void) -> SlangUInt,
	pub getCooperativeVectorCombinationByIndex: unsafe extern "C" fn(*mut c_void, index: SlangUInt, outCombination: *mut slang_CooperativeVectorCombination) -> SlangResult,
}

// --- ABI validation ---
//
// The handwritten vtables above must match the interface layouts declared in
// slang.h (Slang v2026.14.1): same methods, in declaration order. bindgen only
// emits a vtable type for `ISlangUnknown` (slang.h's `SLANG_NO_THROW` /
// `SLANG_MCALL` macro-wrapped virtual methods are not visible to it as such),
// so a direct size comparison against bindgen-generated vtables is not
// possible. Instead, each vtable derives its base layout from bindgen types
// through the `_base` chain, and the method counts below are pinned two ways:
//
// 1. The compile-time asserts check each vtable is exactly base + N method
//    slots (all slots are function pointers, so `repr(C)` adds no padding).
// 2. The `vtable_method_counts_match_slang_h` test parses the slang.h the
//    bindings were generated from and verifies N against the header itself.

/// Number of methods declared directly on each interface in slang.h
/// (Slang v2026.14.1), excluding inherited base-interface methods.
pub(crate) const ISLANG_UNKNOWN_METHODS: usize = 3;
pub(crate) const ISLANG_CASTABLE_METHODS: usize = 1;
pub(crate) const ISLANG_BLOB_METHODS: usize = 2;
pub(crate) const ISLANG_FILE_SYSTEM_METHODS: usize = 1;
pub(crate) const ISLANG_FILE_SYSTEM_EXT_METHODS: usize = 7;
pub(crate) const ISLANG_MUTABLE_FILE_SYSTEM_METHODS: usize = 4;
pub(crate) const ISLANG_SHARED_LIBRARY_METHODS: usize = 1;
pub(crate) const ISLANG_SHARED_LIBRARY_LOADER_METHODS: usize = 1;
pub(crate) const IGLOBAL_SESSION_METHODS: usize = 30;
pub(crate) const ISESSION_METHODS: usize = 21;
pub(crate) const IMETADATA_METHODS: usize = 2;
pub(crate) const ICOMPONENT_TYPE_METHODS: usize = 14;
pub(crate) const IENTRY_POINT_METHODS: usize = 1;
pub(crate) const ITYPE_CONFORMANCE_METHODS: usize = 0;
pub(crate) const IMODULE_METHODS: usize = 13;
pub(crate) const ICOMPILE_RESULT_METHODS: usize = 3;
pub(crate) const ICOMPONENT_TYPE2_METHODS: usize = 3;
pub(crate) const IBINDLESS_RESOURCE_METADATA_METHODS: usize = 1;
pub(crate) const ICOVERAGE_TRACING_METADATA_METHODS: usize = 4;
pub(crate) const ISYNTHETIC_RESOURCE_METADATA_METHODS: usize = 3;
pub(crate) const ICOOPERATIVE_TYPES_METADATA_METHODS: usize = 8;

const _: () = assert!(
    std::mem::size_of::<ISlangUnknown__bindgen_vtable>()
        == ISLANG_UNKNOWN_METHODS * std::mem::size_of::<*const c_void>(),
    "ISlangUnknown vtable should be exactly 3 method slots (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ICastableVtable>()
        == std::mem::size_of::<ISlangUnknown__bindgen_vtable>()
            + ISLANG_CASTABLE_METHODS * std::mem::size_of::<*const c_void>(),
    "ICastableVtable does not match ISlangUnknown + 1 method (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<IBlobVtable>()
        == std::mem::size_of::<ISlangUnknown__bindgen_vtable>()
            + ISLANG_BLOB_METHODS * std::mem::size_of::<*const c_void>(),
    "IBlobVtable does not match ISlangUnknown + 2 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ISlangFileSystemVtable>()
        == std::mem::size_of::<ICastableVtable>()
            + ISLANG_FILE_SYSTEM_METHODS * std::mem::size_of::<*const c_void>(),
    "ISlangFileSystemVtable does not match ISlangCastable + 1 method (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ISlangFileSystemExtVtable>()
        == std::mem::size_of::<ISlangFileSystemVtable>()
            + ISLANG_FILE_SYSTEM_EXT_METHODS * std::mem::size_of::<*const c_void>(),
    "ISlangFileSystemExtVtable does not match ISlangFileSystem + 7 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ISlangMutableFileSystemVtable>()
        == std::mem::size_of::<ISlangFileSystemExtVtable>()
            + ISLANG_MUTABLE_FILE_SYSTEM_METHODS * std::mem::size_of::<*const c_void>(),
    "ISlangMutableFileSystemVtable does not match ISlangFileSystemExt + 4 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ISlangSharedLibraryVtable>()
        == std::mem::size_of::<ICastableVtable>()
            + ISLANG_SHARED_LIBRARY_METHODS * std::mem::size_of::<*const c_void>(),
    "ISlangSharedLibraryVtable does not match ISlangCastable + 1 method (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ISlangSharedLibraryLoaderVtable>()
        == std::mem::size_of::<ISlangUnknown__bindgen_vtable>()
            + ISLANG_SHARED_LIBRARY_LOADER_METHODS * std::mem::size_of::<*const c_void>(),
    "ISlangSharedLibraryLoaderVtable does not match ISlangUnknown + 1 method (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<IGlobalSessionVtable>()
        == std::mem::size_of::<ISlangUnknown__bindgen_vtable>()
            + IGLOBAL_SESSION_METHODS * std::mem::size_of::<*const c_void>(),
    "IGlobalSessionVtable does not match ISlangUnknown + 30 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ISessionVtable>()
        == std::mem::size_of::<ISlangUnknown__bindgen_vtable>()
            + ISESSION_METHODS * std::mem::size_of::<*const c_void>(),
    "ISessionVtable does not match ISlangUnknown + 21 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<IMetadataVtable>()
        == std::mem::size_of::<ICastableVtable>()
            + IMETADATA_METHODS * std::mem::size_of::<*const c_void>(),
    "IMetadataVtable does not match ISlangCastable + 2 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<IComponentTypeVtable>()
        == std::mem::size_of::<ISlangUnknown__bindgen_vtable>()
            + ICOMPONENT_TYPE_METHODS * std::mem::size_of::<*const c_void>(),
    "IComponentTypeVtable does not match ISlangUnknown + 14 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<IEntryPointVtable>()
        == std::mem::size_of::<IComponentTypeVtable>()
            + IENTRY_POINT_METHODS * std::mem::size_of::<*const c_void>(),
    "IEntryPointVtable does not match IComponentType + 1 method (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ITypeConformanceVtable>()
        == std::mem::size_of::<IComponentTypeVtable>()
            + ITYPE_CONFORMANCE_METHODS * std::mem::size_of::<*const c_void>(),
    "ITypeConformanceVtable does not match IComponentType + 0 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<IModuleVtable>()
        == std::mem::size_of::<IComponentTypeVtable>()
            + IMODULE_METHODS * std::mem::size_of::<*const c_void>(),
    "IModuleVtable does not match IComponentType + 13 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ICompileResultVtable>()
        == std::mem::size_of::<ICastableVtable>()
            + ICOMPILE_RESULT_METHODS * std::mem::size_of::<*const c_void>(),
    "ICompileResultVtable does not match ISlangCastable + 3 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<IComponentType2Vtable>()
        == std::mem::size_of::<ISlangUnknown__bindgen_vtable>()
            + ICOMPONENT_TYPE2_METHODS * std::mem::size_of::<*const c_void>(),
    "IComponentType2Vtable does not match ISlangUnknown + 3 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<IBindlessResourceMetadataVtable>()
        == std::mem::size_of::<ICastableVtable>()
            + IBINDLESS_RESOURCE_METADATA_METHODS * std::mem::size_of::<*const c_void>(),
    "IBindlessResourceMetadataVtable does not match ISlangCastable + 1 method (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ICoverageTracingMetadataVtable>()
        == std::mem::size_of::<ICastableVtable>()
            + ICOVERAGE_TRACING_METADATA_METHODS * std::mem::size_of::<*const c_void>(),
    "ICoverageTracingMetadataVtable does not match ISlangCastable + 4 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ISyntheticResourceMetadataVtable>()
        == std::mem::size_of::<ICastableVtable>()
            + ISYNTHETIC_RESOURCE_METADATA_METHODS * std::mem::size_of::<*const c_void>(),
    "ISyntheticResourceMetadataVtable does not match ISlangCastable + 3 methods (slang.h)"
);
const _: () = assert!(
    std::mem::size_of::<ICooperativeTypesMetadataVtable>()
        == std::mem::size_of::<ICastableVtable>()
            + ICOOPERATIVE_TYPES_METADATA_METHODS * std::mem::size_of::<*const c_void>(),
    "ICooperativeTypesMetadataVtable does not match ISlangCastable + 8 methods (slang.h)"
);

#[cfg(test)]
mod tests {
    use super::*;

    /// Strips C/C++ comments so the brace matching and method counting below
    /// are not confused by comment contents.
    fn strip_comments(source: &str) -> String {
        let bytes = source.as_bytes();
        let mut out = String::with_capacity(source.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            } else if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            } else {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
        out
    }

    /// Finds the body of the `struct`/`class` named `name` (skipping forward
    /// declarations) and counts its pure-virtual (`= 0;`) methods. This
    /// catches methods declared without the `SLANG_MCALL` macro, e.g.
    /// `IMetadata::isParameterLocationUsed` in slang.h.
    fn count_pure_virtual_methods(source: &str, name: &str) -> Option<usize> {
        let bytes = source.as_bytes();

        // Collect every `struct NAME` / `class NAME` occurrence where the name
        // is followed by whitespace (this excludes forward declarations,
        // which end in `;` right after the name).
        let mut candidates: Vec<usize> = ["struct ", "class "]
            .into_iter()
            .flat_map(|keyword| {
                let pattern = format!("{keyword}{name}");
                source
                    .match_indices(&pattern)
                    .map(|(pos, _)| pos + pattern.len())
                    .collect::<Vec<_>>()
            })
            .filter(|&pos| pos < bytes.len() && bytes[pos].is_ascii_whitespace())
            .collect();
        candidates.sort_unstable();

        // Locate the declaration, skipping forward declarations (`struct X;`).
        let decl_end = candidates.into_iter().find(|&pos| {
            matches!(
                bytes[pos..].iter().find(|b| !b.is_ascii_whitespace()).copied(),
                Some(b'{') | Some(b':')
            )
        })?;

        // Brace-match the declaration body.
        let body_start = source[decl_end..].find('{')? + decl_end;
        let mut depth = 0;
        let mut body_end = body_start;
        loop {
            match bytes[body_end] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            body_end += 1;
        }

        // Count `= 0;` pure-virtual markers within the body. The `=` must not
        // be part of another operator (`!=`, `<=`, ...); default arguments
        // such as `= 0)` do not end in `;` and are ignored.
        let body = &bytes[body_start..body_end];
        let mut count = 0;
        for (i, b) in body.iter().enumerate() {
            if *b != b'=' {
                continue;
            }
            let prev = body[..i].iter().rev().find(|b| !b.is_ascii_whitespace());
            if prev.is_some_and(|b| b"!<>=+-*/&|^%".contains(b)) {
                continue;
            }
            let mut j = i + 1;
            while j < body.len() && body[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < body.len() && body[j] == b'0' {
                j += 1;
                while j < body.len() && body[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < body.len() && body[j] == b';' {
                    count += 1;
                }
            }
        }
        Some(count)
    }

    /// Cross-checks the handwritten vtable method counts against the slang.h
    /// the bindings were generated from (path exported by build.rs).
    #[test]
    fn vtable_method_counts_match_slang_h() {
        let include_dir = env!("SHADER_SLANG_RS_SYS_SLANG_INCLUDE_DIR");
        let source = std::fs::read_to_string(format!("{include_dir}/slang.h"))
            .expect("failed to read slang.h");
        let source = strip_comments(&source);

        for (interface, expected) in [
            ("ISlangUnknown", ISLANG_UNKNOWN_METHODS),
            ("ISlangCastable", ISLANG_CASTABLE_METHODS),
            ("ISlangBlob", ISLANG_BLOB_METHODS),
            ("ISlangFileSystem", ISLANG_FILE_SYSTEM_METHODS),
            ("ISlangFileSystemExt", ISLANG_FILE_SYSTEM_EXT_METHODS),
            (
                "ISlangMutableFileSystem",
                ISLANG_MUTABLE_FILE_SYSTEM_METHODS,
            ),
            ("ISlangSharedLibrary", ISLANG_SHARED_LIBRARY_METHODS),
            (
                "ISlangSharedLibraryLoader",
                ISLANG_SHARED_LIBRARY_LOADER_METHODS,
            ),
            ("IGlobalSession", IGLOBAL_SESSION_METHODS),
            ("ISession", ISESSION_METHODS),
            ("IMetadata", IMETADATA_METHODS),
            ("IComponentType", ICOMPONENT_TYPE_METHODS),
            ("IEntryPoint", IENTRY_POINT_METHODS),
            ("ITypeConformance", ITYPE_CONFORMANCE_METHODS),
            ("IModule", IMODULE_METHODS),
            ("ICompileResult", ICOMPILE_RESULT_METHODS),
            ("IComponentType2", ICOMPONENT_TYPE2_METHODS),
            ("IBindlessResourceMetadata", IBINDLESS_RESOURCE_METADATA_METHODS),
            ("ICoverageTracingMetadata", ICOVERAGE_TRACING_METADATA_METHODS),
            ("ISyntheticResourceMetadata", ISYNTHETIC_RESOURCE_METADATA_METHODS),
            ("ICooperativeTypesMetadata", ICOOPERATIVE_TYPES_METADATA_METHODS),
        ] {
            assert_eq!(
                count_pure_virtual_methods(&source, interface),
                Some(expected),
                "{interface}: slang.h method count does not match the handwritten vtable",
            );
        }
    }
}
