# Slang COM vtable layout (as of Slang v2026.14.1)

Slang's public C++ interfaces (`ISession`, `IModule`, `IComponentType`, etc.)
are plain COM-style vtables: single inheritance, `virtual` methods declared
in `include/slang.h`, called through `SLANG_MCALL`. `bindgen` (see
`shader-slang-sys/build.rs`) emits each interface as an opaque struct with
one `vtable_: *const ...__bindgen_vtable` field — it does not generate
per-method wrapper functions for virtual calls. `src/sys/com.rs` therefore
calls methods by reading a function pointer out of the vtable at a fixed
integer slot and transmuting it to the right `extern "C"` signature.

Slot 0 is always `queryInterface`, slot 1 `addRef`, slot 2 `release`
(inherited from `ISlangUnknown`). Slots 3+ are each interface's own methods,
numbered in the order they're declared in `slang.h`, plus the slot count of
every base interface they extend.

This is safe to hardcode because Slang's own `CLAUDE.md` documents its ABI
policy for public headers: virtual methods are **never reordered** and
**only appended**, never inserted in the middle — see "Modifying Public
Headers (include/)" > "Virtual tables (COM interfaces)" in
`shader-slang-sys/slang/CLAUDE.md`. If a future Slang release breaks this
policy, these offsets must be re-derived (re-run the extraction below)
before this crate can support it.

## Derivation

Extracted by parsing `slang.h` for each interface's own `virtual ... SLANG_MCALL name(...)`
declarations in file order:

```
IGlobalSession (base=ISlangUnknown, offset 3):
  3: createSession
  4: findProfile
  ... (createSession is the only method used by this crate so far)

ISession (base=ISlangUnknown, offset 3):
  3: getGlobalSession
  4: loadModule
  5: loadModuleFromSource
  ...
  20: loadModuleFromSourceString   (slot 3 + 17)

IComponentType (base=ISlangUnknown, offset 3):
  3: getSession
  4: getLayout
  5: getSpecializationParamCount
  6: getEntryPointCode
  7: getResultAsFileSystem
  8: getEntryPointHash
  9: specialize
  10: link
  ...
  14: getTargetCode   (slot 3 + 11)

IModule (base=IComponentType, own methods start at offset 3+14=17):
  17: findEntryPointByName

IEntryPoint (base=IComponentType, own methods start at offset 3+14=17):
  17: getFunctionReflection   (unused by this crate)
```

Only the slots actually called by `src/api.rs` are used; the table above
lists them for the methods this crate currently binds
(`createSession`, `loadModuleFromSourceString`, `findEntryPointByName`,
`link`, `getTargetCode`) plus enough surrounding context to sanity-check the
offset arithmetic. Re-derive from `slang.h` before adding a call to any
other method.
