#![allow(dead_code)]
//! SVM 相关 MSR 常量(APM Vol.2 §15.4,C 版 svm_defs.h 同源)
//!
//! 读写一律走 svm/asm.rs 的 svmb_rdmsr/svmb_wrmsr(privileged 指令没有
//! Rust intrinsic;只在正确的核心上下文/IRQL 调用)。

/// EFER(扩展特性使能)
pub const MSR_EFER: u32 = 0xC000_0080;
/// EFER.SVME —— SVM 总开关(VMRUN 前提)
pub const EFER_SVME: u32 = 12;

/// SYSCALL/SYSRET 目标 MSR(FillGuestState 快照)
pub const MSR_STAR: u32 = 0xC000_0081;
pub const MSR_LSTAR: u32 = 0xC000_0082;
pub const MSR_CSTAR: u32 = 0xC000_0083;
pub const MSR_SFMASK: u32 = 0xC000_0084;
pub const MSR_FS_BASE: u32 = 0xC000_0100;
pub const MSR_GS_BASE: u32 = 0xC000_0101;
pub const MSR_KERNEL_GS_BASE: u32 = 0xC000_0102;
pub const MSR_SYSENTER_CS: u32 = 0x174;
pub const MSR_SYSENTER_ESP: u32 = 0x175;
pub const MSR_SYSENTER_EIP: u32 = 0x176;
/// IA32_MSR_PAT(FillGuestState 快照)
pub const MSR_PAT: u32 = 0x277;

/// VM_CR(SVM 全局控制)
pub const MSR_VM_CR: u32 = 0xC001_0114;
/// VM_CR.SVMDIS 位(固件级禁用)
pub const VM_CR_SVMDIS: u32 = 4;

/// VMCB 物理基址(HOST 版,VMRUN 前必写)
pub const MSR_VM_HSAVE_PA: u32 = 0xC001_0117;

// ---- CPUID 叶(C 版 svm_defs.h 同源)----
pub const CPUID_EXT_FEATURES: u32 = 0x8000_0001; // ECX[2] = SVM
pub const CPUID_SVM_FEATURES: u32 = 0x8000_000A; // EDX[0]=NPT, EDX[3]=NRIPS
pub const SVM_CPUID_BIT: u32 = 2;
pub const NPT_CPUID_BIT: u32 = 0;
pub const NRIPS_CPUID_BIT: u32 = 3;
