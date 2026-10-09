# svmb-rust

**A Rust port of a Windows AMD-V (SVM) hypervisor framework — stable and validated against the C reference baseline.**

**Windows AMD-V (SVM) 虚拟化框架的 Rust 移植 —— 已稳定,并通过与 C 参照基线的对照验收。**

[中文](#中文) | [English](#english)

---

## 中文

### 项目简介

svmb-rust 是一个 Windows 内核态 AMD-V (SVM) hypervisor 驱动的 Rust 移植,基于
[microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs)(wdk-rs)
官方 WDM 范式搭建。原 C 版 svmb 是一个可运行的 thin hypervisor(内核=OS 自身,
NPT-off 基线);本项目将其完整移植到 Rust。

**当前状态(2026-10-09):驱动稳定。** 6 核全虚拟化、exit 吞吐与 C 基线同级
(26953 exits / 15min vs C 的 14835/8min)、3×15min soak 全绿。支持 AutoStart
(服务启动即进入虚拟化,与 C 版 r30 语义对齐)。

### 结构

```
svmb-rust/
├── Cargo.toml              # workspace(shared + ctl;driver 独立,官方约束)
├── rust-toolchain.toml     # stable MSVC
├── crates/
│   ├── svmb-shared/        # R3<->R0 协议(no_std)
│   ├── svmb-driver/        # WDM 驱动(cdylib -> svmb_rust.sys)
│   │   ├── ext_svm_entry.obj  # C 参照的 ml64 编译产物(A/B 验证用)
│   │   └── src/svm/        # SVM 核心:asm(主循环)/exit_handler/vmcb/percpu/npt
│   └── svmbctl-rs/         # 用户态控制台(info | selftest | cpuid)
├── build/                  # 构建与 VM 测试脚本(见下)
└── TASKS.md                # 任务台账
```

### 构建

```bat
build\build_rust.bat            :: Debug
build\build_rust.bat Release
```

产物:`x64\<config>\svmb_rust.sys` + `svmbctl-rs.exe`。

**工具链要求**:
- Rust stable MSVC + **LLVM 17.x**(LLVM 23 会让 wdk-sys bindgen 布局断言炸裂)
- WDK 10.0.28000(或任何有 km 内核头的版本)
- VS MSVC 链接器 + 测试签名证书
- 驱动包要求静态 CRT(已配置)

### 部署到测试 VM

```bat
:: 1. 复制 build\vmenv.local.template.bat -> vmenv.local.bat,填入你的 VM 凭据
:: 2. 推送驱动与控制台
build\vm_push_rust.bat

:: 3. guest 内创建并启动服务(AutoStart:启动即虚拟化)
sc create svmbrust type= kernel binPath= <guest-path>\svmb_rust.sys
sc start svmbrust

:: 4. 查询状态
svmbctl-rs.exe info
```

### 快速测试(单轮全自动)

```bash
cd build
./round.sh <label>          # reset -> deploy -> sc start(=fire) -> observe 5min
./round.sh <label> 30       # 15min soak
```

凭据从 git-ignored 的 `vmenv.local.sh` 读取(模板见 `vmenv.local.sh.template`)。

### 与 C 参照的关系

本项目在 bring-up 过程中使用了"C-obj 混链"技术(把 C 版汇编循环的 .obj 链入
Rust 驱动做 A/B 对照)来物理隔离代码生成变量,最终定位并修复了 Rust 移植中的
6 个寄存器/状态同步 bug。`ext_svm_entry.obj` 是该实验的遗留资产,可通过
`ENABLE_CLOOP` 常量切换(默认 false = 纯 Rust 循环)。

### 调试装置(可选)

`build/` 下有一套 VM 调试循环脚本(reset 落证/哈希闸门/locale-proof 错误检测/
双连探活),以及 KDNET 取证工具(`kd_forensic.py`,支持 --linkcheck/--forensic/
--arm/--watch/--patrol 模式)。这些是开发期资产,发布无需使用。

### 致谢

- [microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs) — WDM 驱动构建范式
- [tandasat/SimpleSvm](https://github.com/tandasat/SimpleSvm) — SVM 语义蓝本
- [barevisor](https://github.com/barevisor/barevisor) — 返回式 VMRUN 架构参照
- [memN0ps/matrix-rs](https://github.com/memN0ps/matrix-rs) — Rust AMD-V 参考
- AMD APM Vol.2 §15 — 架构规范

### 许可

见 [LICENSE](LICENSE)。

---

## English

### Overview

svmb-rust is a Rust port of a Windows kernel-mode AMD-V (SVM) hypervisor
driver, built on the official
[microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs)
(wdk-rs) WDM pattern. The original C svmb is a working thin hypervisor
(kernel = the OS itself, NPT-off baseline); this project ports it fully
to Rust.

**Current status (2026-10-09): driver stable.** All 6 cores virtualized,
exit throughput at C-baseline class (26953 exits / 15min vs C's 14835/8min),
3×15min soak all green. AutoStart supported (service start enters
virtualization immediately, matching the C r30 semantics).

### Layout

```
svmb-rust/
├── Cargo.toml              # workspace (shared + ctl; driver standalone)
├── rust-toolchain.toml     # stable MSVC
├── crates/
│   ├── svmb-shared/        # R3<->R0 protocol (no_std)
│   ├── svmb-driver/        # WDM driver (cdylib -> svmb_rust.sys)
│   │   ├── ext_svm_entry.obj  # ml64-compiled C reference loop (A/B asset)
│   │   └── src/svm/        # SVM core: asm (main loop) / exit_handler / vmcb / percpu / npt
│   └── svmbctl-rs/         # user-mode console (info | selftest | cpuid)
├── build/                  # build & VM test scripts
└── TASKS.md                # task ledger
```

### Build

```bat
build\build_rust.bat            :: Debug
build\build_rust.bat Release
```

Artifacts: `x64\<config>\svmb_rust.sys` + `svmbctl-rs.exe`.

**Toolchain requirements**:
- Rust stable MSVC + **LLVM 17.x** (LLVM 23 breaks wdk-sys bindgen layout asserts)
- WDK 10.0.28000 (or any version with km kernel headers)
- VS MSVC linker + test-signing certificate
- Static CRT for the driver package (already configured)

### Deploy to a test VM

```bat
:: 1. Copy build\vmenv.local.template.bat -> vmenv.local.bat, fill in credentials
:: 2. Push driver + console
build\vm_push_rust.bat

:: 3. In the guest: create & start the service (AutoStart = instant virtualization)
sc create svmbrust type= kernel binPath= <guest-path>\svmb_rust.sys
sc start svmbrust

:: 4. Query status
svmbctl-rs.exe info
```

### Quick test (fully automated single round)

```bash
cd build
./round.sh <label>          # reset -> deploy -> sc start (=fire) -> observe 5min
./round.sh <label> 30       # 15min soak
```

Credentials are read from the git-ignored `vmenv.local.sh`
(see `vmenv.local.sh.template`).

### Relationship to the C reference

During bring-up, a "C-obj mixed link" technique was used (linking the C
assembly loop's .obj into the Rust driver for A/B comparison) to physically
isolate the code-generation variable, ultimately locating and fixing 6
register/state synchronization bugs in the Rust port. `ext_svm_entry.obj`
is the remaining asset from that experiment, toggleable via the
`ENABLE_CLOOP` constant (default false = pure Rust loop).

### Debug tooling (optional)

`build/` contains a VM debug-loop script suite (reset evidence gates /
hash verification / locale-proof error detection / dual-miss aliveness)
and KDNET forensic tools (`kd_forensic.py`, supporting --linkcheck /
--forensic / --arm / --watch / --patrol modes). These are development-time
assets; not needed for normal use.

### Acknowledgments

- [microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs) — WDM driver build pattern
- [tandasat/SimpleSvm](https://github.com/tandasat/SimpleSvm) — SVM semantics blueprint
- [barevisor](https://github.com/barevisor/barevisor) — return-style VMRUN architecture reference
- [memN0ps/matrix-rs](https://github.com/memN0ps/matrix-rs) — Rust AMD-V reference
- AMD APM Vol.2 §15 — architecture specification

### License

See [LICENSE](LICENSE).
