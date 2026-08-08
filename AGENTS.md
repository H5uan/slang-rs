# AGENTS.md

本文件为 AI 代理和贡献者提供本仓库的关键信息。

## 项目概述

slang-rs 是 [Slang 着色器语言编译器](https://github.com/shader-slang/slang)的 Rust 绑定,架构参照 [FloatyMonkey/slang-rs](https://github.com/FloatyMonkey/slang-rs)(MIT OR Apache-2.0)。当前绑定的 Slang 版本为 **v2026.14.1**。

## 仓库结构

- `slang-rs-sys/` — 底层 FFI crate。bindgen 仅从 `slang.h` 生成数据类型/枚举/free function;COM 接口的 vtable 在 `slang-rs-sys/src/lib.rs` 中**手写**(`#[repr(C)]` 结构体,`_base` 字段表达继承)。
- `src/` — 高层安全 API crate(slang-rs):`IUnknown` 引用计数 RAII、`GlobalSession`/`Session`/`Module`/`ComponentType`/`EntryPoint`/`Blob`/`Metadata`、builder 风格 desc、`reflection/` 反射模块(零开销 borrowed 引用包装)。
- `src/tests.rs` — 端到端集成测试(真编译,非 mock)。
- `shaders/test.slang` — 测试用 shader。
- `slang/` — Slang 源码 git submodule,pin 在 v2026.14.1 tag。**不要修改其内容**(包括其中的 `.claude/`、`CLAUDE.md`,那是上游仓库的文件)。
- `target/reference/`、`target/slang-bin/` — 本地参考克隆和预编译库缓存,已 gitignore,**不要提交**。

## 构建与测试

```bash
cargo test --workspace          # 默认:自动下载 v2026.14.1 预编译库(缓存到 target/slang-bin)
cargo test --features source-build   # 改用 cmake 构建 slang/ 子模块(首次需数十分钟)
```

- 环境变量 `SLANG_DIR` / `SLANG_LIB_DIR` / `SLANG_INCLUDE_DIR` 可覆盖库和头文件位置。
- 运行时动态链接 `slang.dll`;build.rs 会把 DLL 拷到可执行文件目录,无需手动设 PATH。
- 平台:目前只保证 Windows/MSVC。
- 工具链:edition 2024,需要 Rust 1.85+。

## 约定

- 代码风格:tab 缩进(见 `rustfmt.toml`,`hard_tabs = true`)。
- **升级 Slang 版本时**,手写 vtable 必须与新版本 `slang.h` 逐方法核对(顺序、签名;Slang 只在接口末尾追加方法)。同时更新 `slang-rs-sys/build.rs` 中的 `SLANG_VERSION`、slang/ 子模块 tag 和 `slang-rs-sys/src/lib.rs` 顶部的版本注释。
- 高层 API 表面与参考库 FloatyMonkey/slang-rs 对齐;新增 Slang 能力的封装应保持其风格(`vcall!`/`rcall!` 宏、`repr(transparent)` 包装)。
- 变更后运行 `cargo test --workspace` 验证。
