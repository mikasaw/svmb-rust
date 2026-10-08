//! SVM(AMD-V)核心 —— C 版 svmb 与 AMD APM Vol.2 §15 的移植
//!
//! 起停链路(C 版 core/hypervisor.cpp + hw/asm/svm_entry.asm 对应):
//! `hypervisor::start` → per-core `enter_core`(两阶段寄存器保存 → VMCB 配置 →
//! EFER.SVME/VM_HSAVE_PA → VMRUN 自虚拟化)→ `#VMEXIT` 落入
//! `exit_handler`(CPUID/MSR 直通、VMRUN 防嵌套、CPUID 叶超级调用)→
//! `hypervisor::stop` 经 Leaving 状态 + IPI 触发各核就地退出虚拟化。
//!
//! 参考实现:C 版 svmb(主蓝本)、tandasat/SimpleSvm、barevisor、matrix-rs。

pub mod asm;
pub mod exit;
pub mod exit_handler;
pub mod hypervisor;
pub mod msr;
pub mod msrpm;
pub mod npt;
pub mod percpu;
pub mod regs;
pub mod segments;
pub mod vmcb;
