# AGENTS.md

本文件为 AI 代理和贡献者提供本仓库的关键信息。

## 项目概述

shader-slang-rs 是 [Slang 着色器语言编译器](https://github.com/shader-slang/slang)的 Rust 绑定,以 [FloatyMonkey/slang-rs](https://github.com/FloatyMonkey/slang-rs)(MIT OR Apache-2.0)为起点,能力面已扩展对齐 Slang **v2026.14.1**。workspace 包含两个 crate:高层安全 API `shader-slang-rs` 和底层 FFI `shader-slang-rs-sys`。

## 仓库结构

- `shader-slang-rs-sys/` — 底层 FFI crate。bindgen 仅从 `slang.h` 生成数据类型/枚举/free function;COM 接口的 vtable 在 `shader-slang-rs-sys/src/lib.rs` 中**手写**(`#[repr(C)]` 结构体,`_base` 字段表达继承)。
- `src/` — 高层安全 API crate(shader-slang-rs):`IUnknown` 引用计数 RAII、`GlobalSession`/`Session`/`Module`/`ComponentType`/`EntryPoint`/`Blob`/`Metadata`/`MutableFileSystem`、builder 风格 desc、`reflection/` 反射模块(零开销 borrowed 引用包装)、`file_system.rs` 反向 COM(Rust 实现 `ISlangFileSystem`/`ISlangFileSystemExt`/`ISlangMutableFileSystem` 供 C++ 回调:`FileSystem`/`FileSystemExt`/`WritableFileSystem` trait + `FileSystemObject` 包装,手写 vtable thunk + `catch_unwind` panic 隔离)。
- `src/tests.rs` — 端到端集成测试(真编译,非 mock)。
- `examples/` — 可运行示例:`compile`(最小编译流程)、`reflect`(反射遍历)、`virtual_file_system`(内存文件系统)、`host_callable`(编译为宿主机器码并直接调用)。`cargo run --example <name>`。
- `shaders/test.slang` — 测试用 shader。
- `.github/workflows/ci.yml` — CI:windows/ubuntu/macos(aarch64)三平台跑 fmt/clippy/test;不拉取 slang/ 子模块(走预编译包)。
- `slang/` — Slang 源码 git submodule,pin 在 v2026.14.1 tag。**不要修改其内容**(包括其中的 `.claude/`、`CLAUDE.md`,那是上游仓库的文件)。
- `target/reference/`、`target/slang-bin/` — 本地参考克隆和预编译库缓存(按 `target/slang-bin/v<版本>/` 分版本存放),已 gitignore,**不要提交**。

## 构建与测试

```bash
cargo test --workspace          # 默认:自动下载 v2026.14.1 预编译库(缓存到 target/slang-bin/v2026.14.1)
cargo test --features source-build   # 改用 cmake 构建 slang/ 子模块(首次需数十分钟)
```

- 环境变量 `SLANG_DIR` / `SLANG_LIB_DIR` / `SLANG_INCLUDE_DIR` 可覆盖库和头文件位置。
- 运行时动态链接 Slang 共享库;build.rs 会把运行时库(Windows 的 DLL、Linux 的 `libslang*.so*`、macOS 的 `libslang*.dylib*`,含版本化 soname 别名)拷到 Cargo profile 目录及其 `deps/`(`cargo test`/`cargo run` 的 loader 搜索路径只含这两个目录,rustc-link-search 不在运行时搜索路径里)。脱离 Cargo 运行的二进制需自行配置 rpath 或 loader path。
- 平台:Windows/MSVC 本机实测;Linux x86_64/aarch64 与 macOS x86_64/aarch64 走官方预编译包(代码按 release artifact 事实编写,**未经本机实测**)。
- 工具链:edition 2024,需要 Rust 1.85+。

## 约定

- 代码风格:tab 缩进(见 `rustfmt.toml`,`hard_tabs = true`)。
- **升级 Slang 版本时**,手写 vtable 必须与新版本 `slang.h` 逐方法核对(顺序、签名;Slang 只在接口末尾追加方法)。版本号的单一来源是 `shader-slang-rs-sys/build.rs` 中的 `SLANG_VERSION`,升级清单:
  1. `shader-slang-rs-sys/build.rs` 的 `SLANG_VERSION`(canonical);
  2. slang/ 子模块 checkout 到新 tag(`git submodule update` 后核对 `git -C slang describe --tags`);
  3. `shader-slang-rs-sys/src/lib.rs` 顶部的 `// Based on Slang version ...` 注释(有 guard 测试校对);
  4. 逐方法核对手写 vtable 与各接口方法数常量(`*_METHODS`),`vtable_method_counts_match_slang_h` 与 vtable 大小 assert 会兜底;
  5. 重新生成签名快照:`SLANG_UPDATE_VTABLE_SNAPSHOT=1 cargo test -p shader-slang-rs-sys`,然后 review `shader-slang-rs-sys/tests/vtable_signatures.snap` 的 diff(快照头的版本号也有 guard 测试校对);
  6. 散文提及:`README.md`(标题、Installation、仓库结构表、Acknowledgments)、`shader-slang-rs-sys/README.md` 标题行和 `src/lib.rs` 顶部 doc comment 中的 `v2026.14.1`(grep 全仓库 `2026.14.1` 兜底;行为注释如 `src/lib.rs` 中的 loaded-state guard 若仍成立可保留)。
- 高层 API 以 FloatyMonkey/slang-rs 为起点,能力面对齐 Slang v2026.14.1;新增 Slang 能力的封装应保持其风格(`vcall!`/`rcall!` 宏、`repr(transparent)` 包装)。
- 变更后运行 `cargo test --workspace` 验证。
