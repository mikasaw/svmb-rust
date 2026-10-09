# svmb-rust

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%20x64-blue.svg)](#)
[![Arch](https://img.shields.io/badge/Arch-AMD--V%20%28SVM%29-red.svg)](#)
[![Language](https://img.shields.io/badge/Language-Rust-orange.svg)](#)

Windows 内核态 AMD-V (SVM) hypervisor 驱动的 Rust 移植,基于 [microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs)(wdk-rs)官方 WDM 范式搭建。

**当前状态:稳定。** 6 核全虚拟化,exit 吞吐与 C 基线同级(26,953 exits / 15 min vs C 的 14,835 / 8 min),3×15 min soak 全绿。

[English](README.md)

---

## 目录

- [项目简介](#项目简介)
- [工作原理](#工作原理)
- [项目结构](#项目结构)
- [环境要求](#环境要求)
- [构建](#构建)
- [部署到测试 VM](#部署到测试-vm)
- [快速测试](#快速测试)
- [VMRUN 循环架构](#vmrun-循环架构)
- [调试工具](#调试工具)
- [故障排查](#故障排查)
- [致谢](#致谢)
- [许可](#许可)

---

## 项目简介

svmb-rust 是一个 thin hypervisor:内核即操作系统本身(无独立 guest),以 AMD-V SVM 模式运行,NPT 关闭。原 C 版 svmb 是可运行的参照实现;本项目将其完整移植到 Rust。

主要特性:

- **6 核虚拟化**(Windows 10 x64,build 18362+)
- **AutoStart**:服务启动即在 `DriverEntry` 进入虚拟化(无需手动触发)
- **CPUID / MSR 拦截**,透明直通
- **VMRUN 循环**手写汇编,C 形状寄存器纪律
- **C-obj A/B 桥**:可链入 C 参照循环做对照(`ENABLE_CLOOP` 开关)

## 工作原理

```
┌─────────────────────────────────────────────────────┐
│                    Windows 内核                       │
│  ┌─────────────┐  ┌──────────────────────────────┐  │
│  │ svmb_rust   │  │   Guest(即 OS 自身)         │  │
│  │   .sys      │  │   在 VMRUN 内运行             │  │
│  │             │  │                              │  │
│  │ VMM 循环    │──│─ #VMEXIT ──→ exit 处理器     │  │
│  │ (asm)       │  │   ↑            (Rust)        │  │
│  │             │──│─ VMRUN ────←────────────────  │  │
│  └─────────────┘  └──────────────────────────────┘  │
│       │                        │                     │
│  VMCB (guest)            VMCB (host 快照)           │
│  HSAVE_PA                VcpuContext blob           │
└─────────────────────────────────────────────────────┘
```

每个 CPU 核跑一个紧凑循环:`VMRUN` → guest 执行 → `#VMEXIT` → 保存 guest 状态 → 处理 exit(CPUID/MSR 模拟)→ 恢复 guest 状态 → 再次 `VMRUN`。VMRUN 期间通过 GIF 管理让所有中断流向 guest。

## 项目结构

```
svmb-rust/
├── Cargo.toml                  # workspace(shared + ctl;driver 独立)
├── rust-toolchain.toml         # stable MSVC
├── LICENSE                     # MIT
├── crates/
│   ├── svmb-shared/            # R3↔R0 协议(no_std)
│   ├── svmb-driver/            # WDM 驱动(cdylib → svmb_rust.sys)
│   │   └── src/svm/
│   │       ├── asm.rs          # VMRUN 循环 + 寄存器管道
│   │       ├── exit_handler.rs # CPUID/MSR 模拟 + exit 分发
│   │       ├── hypervisor.rs   # 起停 + enter_core + VMCB 配置
│   │       ├── vmcb.rs         # VMCB 字段偏移 + 访问器
│   │       ├── percpu.rs       # 每核 VcpuContext blob
│   │       └── msrpm.rs        # MSR 权限位图
│   └── svmbctl-rs/             # 用户态控制台(info | selftest | cpuid)
├── build/
│   ├── build_rust.bat          # 构建 + 改名 + 测试签名
│   ├── round.sh                # 自动测试轮(reset→部署→观测)
│   ├── vm_push_rust.bat        # 推送驱动到 guest VM
│   ├── vm_svc_create.bat       # sc create 内核服务
│   ├── vm_svc_start.bat        # sc start(= AutoStart 触发)
│   └── kd_forensic.py          # KDNET 取证工具(多模式)
└── TASKS.md                    # 任务台账
```

## 环境要求

| 组件 | 版本 | 说明 |
|------|------|------|
| Rust | stable MSVC | `rust-toolchain.toml` 钉住 |
| LLVM | **17.x** | 23 会让 wdk-sys bindgen 布局断言炸裂 |
| WDK | 10.0.28000+ | 必须有 km 内核头文件 |
| Visual Studio | MSVC 链接器 | 任意近期版本 |
| 测试证书 | 任意 | 用于 `signtool` 测试签名 |
| VMware Workstation | 任意 | 测试 VM(或任何支持嵌套 AMD-V 的 hypervisor) |

驱动包要求静态 CRT:`crates/svmb-driver/.cargo/config.toml` 已配置 `target-feature=+crt-static`。

## 构建

```bat
build\build_rust.bat              :: Debug(默认)
build\build_rust.bat Release
```

产物:`x64\<config>\svmb_rust.sys` + `x64\<config>\svmbctl-rs.exe`。

## 部署到测试 VM

### 1. 配置凭据

复制模板并填入你的 VM 凭据:

```bat
copy build\vmenv.local.template.bat build\vmenv.local.bat
:: 编辑 vmenv.local.bat:VM_VMX、VM_USER、VM_PASS、GUEST_TEST_DIR、KDNET_KEY
```

### 2. 推送并启动

```bat
build\vm_push_rust.bat
build\vm_svc_create.bat
build\vm_svc_start.bat    :: AutoStart:DriverEntry 即进入虚拟化
```

### 3. 验证

```bat
svmbctl-rs.exe info
:: 预期输出:hv_state=2, cores_virt=6, exit_counter > 0
```

## 快速测试

单轮全自动(reset → 部署 → 启动 → 观测 5 分钟):

```bash
cd build
./round.sh my_test_label
```

15 分钟 soak 测试:

```bash
./round.sh my_soak_label 30
```

## VMRUN 循环架构

主循环是 `asm.rs`(`svmb_vmm_loop_c`)中的手写汇编。每圈:

1. `vmload` guest 段/MSR 状态
2. `vmrun` —— 进入 guest 模式
3. 检查 exit code(失败/哨兵)
4. `vmsave` guest 状态 → guest VMCB
5. `vmload` host 状态
6. 保存 guest GPR/XMM 到镜像
7. 同步 VMCB save 区 4 字段(RIP/RSP/RAX/RFLAGS)→ 镜像
8. 调用 Rust exit 处理器(CPUID/MSR 模拟)
9. 写镜像 → VMCB 4 字段 + 哨兵
10. 从镜像恢复 guest GPR/XMM
11. `vmsave` host 状态(捕获 handler 漂移)
12. 跳回步骤 1

**关键寄存器纪律**:VMRUN 不会从 VMCB 重载 `rbx/rcx/rdx/r8-r15`——它们物理直通。RESTORE 到 VMRUN 之间只有 `rax` 可以安全践踏。违反此纪律会把宿主残留泄漏进 guest(详见项目调试历史)。

## 调试工具

`build/kd_forensic.py` 是多模式 KDNET 取证工具:

```bash
python kd_forensic.py --linkcheck    # 验证调试链路
python kd_forensic.py --forensic     # 崩溃后 dump
python kd_forensic.py --watch        # 被动 bugcheck 监听
python kd_forensic.py --patrol       # 周期性冻结采样
```

需要 `SVMB_KDNET` 环境变量和 `mcp` Python 包。

## 故障排查

| 症状 | 原因 | 修复 |
|------|------|------|
| `sc start` 返回错误 193 | guest .sys 文件损坏 | 重新推送 `vm_push_rust.bat` + 哈希校验 |
| `sc start` 返回错误 577 | 未启用测试签名 | `bcdedit /set testsigning on` + 重启 |
| `sc start` 后 VM 冻结 | VMRUN 循环寄存器纪律问题 | 见上方架构章节 |
| 无 KDNET 输出 | 密钥未设/端口错误 | 检查 `bcdedit /dbgsettings` + 环境变量 |

## 致谢

- [microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs) —— WDM 驱动构建范式
- [tandasat/SimpleSvm](https://github.com/tandasat/SimpleSvm) —— SVM 语义蓝本
- [barevisor](https://github.com/barevisor/barevisor) —— 返回式 VMRUN 架构参照
- [memN0ps/matrix-rs](https://github.com/memN0ps/matrix-rs) —— Rust AMD-V 参考
- AMD Architecture Programmer's Manual Vol. 2 §15 —— 架构规范

## 许可

[MIT](LICENSE)
