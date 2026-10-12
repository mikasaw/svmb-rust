# svmb-rust

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%20x64-blue.svg)](#)
[![Arch](https://img.shields.io/badge/Arch-AMD--V%20%28SVM%29-red.svg)](#)
[![Language](https://img.shields.io/badge/Language-Rust-orange.svg)](#)

A Rust port of a Windows kernel-mode AMD-V (SVM) hypervisor driver, built on the official [microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs) (wdk-rs) WDM pattern.

**Status: core virtualization engine stable, feature modules in progress.**

The VMRUN loop, register synchronization, exit handling pipeline, and MSR stealth emulation have been validated against the C reference baseline (6 cores, 26,953 exits / 15 min, 3×15 min soak all green). The higher-level feature modules from the C reference (NPT hooks, CR3 monitor, syscall audit, debugger, module manager) are pending port. See [Feature Status](#feature-status) for details.

[中文文档](README_zh-CN.md)

---

## Table of Contents

- [Overview](#overview)
- [How It Works](#how-it-works)
- [Project Layout](#project-layout)
- [Prerequisites](#prerequisites)
- [Build](#build)
- [Deploy to a Test VM](#deploy-to-a-test-vm)
- [Quick Test](#quick-test)
- [VMRUN Loop Architecture](#vmrun-loop-architecture)
- [Debug Tooling](#debug-tooling)
- [Troubleshooting](#troubleshooting)
- [Acknowledgments](#acknowledgments)
- [License](#license)

---

## Overview

svmb-rust is a thin hypervisor: the kernel is the OS itself (no separate guest), running in AMD-V SVM mode with NPT disabled. The original C svmb is a working reference implementation; this project ports its core virtualization engine to Rust.

### Feature Status

**Working and enabled:**

- 6-core virtualization on Windows 10 x64 (build 18362+)
- AutoStart: service start enters virtualization at `DriverEntry` (registry-configurable)
- CPUID interception with transparent host passthrough
- MSR interception with stealth emulation (EFER hides SVME, VM_CR forces SVMDIS, HSAVE_PA fingerprint hidden)
- Hypercall system via CPUID leaf: PROBE / VERSION / EXIT_VMM (with CPL gating)
- Anti-nested-VMRUN: guest VMRUN → inject #BP
- Shutdown forensics: guest triple-fault → deliberate `KeBugCheckEx` with register state
- Graceful stop: LEAVING marker → IPI nudge → bounded wait
- Per-core crumb ring (128-slot beacon) + exit histogram instrumentation
- KDNET forensic tooling (5 modes: linkcheck / forensic / arm / watch / patrol)

**Code exists but disabled (compile-time or config):**

- CPUID hiding (SVM bit + NPT bit) — `ENABLE_HIDE_BITS=false`
- NPT (nested page table) — 1TB identity view built but `NP_ENABLE=0` (matches C baseline on VMware vhv)

**Not yet ported from C reference:**

- NPT hooks (VA hooks, execute-deny, hook slide, shared pages)
- CR3 monitor (process watch, per-process NPT views, READVM/WRITEVM)
- Syscall audit (execute-sense auditing of MmCopyVirtualMemory / NtCreateFile / SSDT)
- Debugger (software breakpoints, #DB/#UD events, DR hiding)
- Module manager (5 loadable modules with attach/detach/query)
- Stealth scrub (leaf-1 bit31, VMware leaf cleanup)
- Self-test suite (C version has 22 offline tests)
- 21 additional IOCTLs (only 3 of ~24 are implemented)

## How It Works

```
┌─────────────────────────────────────────────────────┐
│                    Windows Kernel                     │
│  ┌─────────────┐  ┌──────────────────────────────┐  │
│  │ svmb_rust   │  │   Guest (the OS itself)      │  │
│  │   .sys      │  │   runs inside VMRUN          │  │
│  │             │  │                              │  │
│  │ VMM loop    │──│─ #VMEXIT ──→ exit handler   │  │
│  │ (asm)       │  │   ↑            (Rust)        │  │
│  │             │──│─ VMRUN ────←──────────────── │  │
│  └─────────────┘  └──────────────────────────────┘  │
│       │                        │                     │
│  VMCB (guest)            VMCB (host snapshot)       │
│  HSAVE_PA                VcpuContext blob            │
└─────────────────────────────────────────────────────┘
```

Each CPU core runs a tight loop: `VMRUN` → guest executes → `#VMEXIT` → save guest state → handle exit (CPUID/MSR emulation) → restore guest state → `VMRUN` again. All interrupts flow to the guest via GIF management during VMRUN.

## Project Layout

```
svmb-rust/
├── Cargo.toml                  # workspace (shared + ctl; driver standalone)
├── rust-toolchain.toml         # stable MSVC
├── LICENSE                     # MIT
├── crates/
│   ├── svmb-shared/            # R3↔R0 protocol (no_std)
│   ├── svmb-driver/            # WDM driver (cdylib → svmb_rust.sys)
│   │   └── src/svm/
│   │       ├── asm.rs          # VMRUN loop + register plumbing
│   │       ├── exit_handler.rs # CPUID/MSR emulation + exit dispatch
│   │       ├── hypervisor.rs   # start/stop + enter_core + VMCB config
│   │       ├── vmcb.rs         # VMCB field offsets + accessors
│   │       ├── percpu.rs       # per-core VcpuContext blob
│   │       └── msrpm.rs        # MSR permission bitmap
│   └── svmbctl-rs/             # user-mode console (info | selftest | cpuid)
├── build/
│   ├── build_rust.bat          # build + rename + test-sign
│   ├── round.sh                # automated test round (reset→deploy→observe)
│   ├── vm_push_rust.bat        # push driver to guest VM
│   ├── vm_svc_create.bat       # sc create kernel service
│   ├── vm_svc_start.bat        # sc start (= fire via AutoStart)
│   └── kd_forensic.py          # KDNET forensic tool (multi-mode)
└── TASKS.md                    # task ledger
```

## Prerequisites

| Component | Version | Notes |
|-----------|---------|-------|
| Rust | stable MSVC | `rust-toolchain.toml` pinned |
| LLVM | **17.x** | 23 breaks wdk-sys bindgen layout asserts |
| WDK | 10.0.28000+ | must have km kernel headers |
| Visual Studio | MSVC linker | any recent version |
| Test cert | any | for `signtool` test-signing |
| VMware Workstation | any | for the test VM (or any hypervisor with nested AMD-V) |

The driver package requires static CRT: `crates/svmb-driver/.cargo/config.toml` already sets `target-feature=+crt-static`.

## Build

```bat
build\build_rust.bat              & :: Debug (default)
build\build_rust.bat Release
```

Artifacts: `x64\<config>\svmb_rust.sys` + `x64\<config>\svmbctl-rs.exe`.

## Deploy to a Test VM

### 1. Configure credentials

Copy the template and fill in your VM credentials:

```bat
copy build\vmenv.local.template.bat build\vmenv.local.bat
:: Edit vmenv.local.bat: VM_VMX, VM_USER, VM_PASS, GUEST_TEST_DIR, KDNET_KEY
```

### 2. Push and start

```bat
build\vm_push_rust.bat
build\vm_svc_create.bat
build\vm_svc_start.bat    :: AutoStart: enters virtualization at DriverEntry
```

### 3. Verify

```bat
svmbctl-rs.exe info
:: Expected: hv_state=2, cores_virt=6, exit_counter > 0
```

## Quick Test

One fully automated round (reset → deploy → start → observe 5 min):

```bash
cd build
./round.sh my_test_label
```

15-minute soak test:

```bash
./round.sh my_soak_label 30
```

## VMRUN Loop Architecture

The main loop is hand-written assembly in `asm.rs` (`svmb_vmm_loop_c`). Per cycle:

1. `vmload` guest segment/MSR state
2. `vmrun` — enter guest mode
3. Check exit code (failure / sentinel)
4. `vmsave` guest state → guest VMCB
5. `vmload` host state
6. Save guest GPRs/XMMs to mirror
7. Sync VMCB save-area 4 fields (RIP/RSP/RAX/RFLAGS) → mirror
8. Call Rust exit handler (CPUID/MSR emulation)
9. Write mirror → VMCB 4 fields + sentinel
10. Restore guest GPRs/XMMs from mirror
11. `vmsave` host state (capture handler drift)
12. Jump to step 1

**Critical register discipline**: VMRUN does not reload `rbx/rcx/rdx/r8-r15` from the VMCB — they pass through physically. Only `rax` is safe to clobber between RESTORE and VMRUN. Violating this leaks host residue into the guest (see the project's debugging history for details).

## Debug Tooling

`build/kd_forensic.py` is a multi-mode KDNET forensic tool:

```bash
python kd_forensic.py --linkcheck    # verify debug link
python kd_forensic.py --forensic     # post-crash dump
python kd_forensic.py --watch        # passive bugcheck watch
python kd_forensic.py --patrol       # periodic freeze sampling
```

Requires the `SVMB_KDNET` environment variable and the `mcp` Python package.

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `sc start` returns error 193 | Guest .sys file corrupted | Re-push with `vm_push_rust.bat` + verify hash |
| `sc start` returns error 577 | Test signing not enabled | `bcdedit /set testsigning on` + reboot |
| VM freezes after `sc start` | Check VMRUN loop register discipline | See architecture section above |
| No KDNET output | Key not set / wrong port | Check `bcdedit /dbgsettings` + env var |

## Acknowledgments

- [microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs) — WDM driver build pattern
- [tandasat/SimpleSvm](https://github.com/tandasat/SimpleSvm) — SVM semantics blueprint
- [barevisor](https://github.com/barevisor/barevisor) — return-style VMRUN architecture reference
- [memN0ps/matrix-rs](https://github.com/memN0ps/matrix-rs) — Rust AMD-V reference
- AMD Architecture Programmer's Manual Vol. 2 §15 — architecture specification

## License

[MIT](LICENSE)
