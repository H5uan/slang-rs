This crate links against the Slang shading language compiler
(https://github.com/shader-slang/slang), either by downloading its official
prebuilt binaries (the `prebuilt` feature) or by building it from the
`slang` git submodule (the `build-from-source` feature). Slang itself is
licensed separately from this crate:

    SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception

The full license text is available at:
- https://github.com/shader-slang/slang/blob/master/LICENSE
- https://github.com/shader-slang/slang/blob/master/LICENSES

Slang's own dependencies (bundled or vendored as needed by its build) carry
their own licenses; see the Slang repository's `LICENSES` directory and its
`external/` submodules for details.

This file documents the license of the third-party binary/source Slang
distributes and this crate consumes; it does not change or extend the
license of `shader-slang-sys` or `shader-slang` themselves (see `LICENSE-MIT`
and `LICENSE-APACHE` in the repository root).
