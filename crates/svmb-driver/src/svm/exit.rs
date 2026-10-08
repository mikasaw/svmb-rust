#![allow(dead_code)] // 布局库:访问器/常量按需使用
//! #VMEXIT 退出码与直方图槽位(APM Vol.2 §15.5,对齐 C 版 vmcb.h vmexit)

/// 异常类 exit 基址(0x40 + 异常号)
pub const VMEXIT_EXCP_BASE: u64 = 0x40;
pub const VMEXIT_INTR: u64 = 0x60;
pub const VMEXIT_NMI: u64 = 0x61;
pub const VMEXIT_SMI: u64 = 0x62;
pub const VMEXIT_INIT: u64 = 0x63;
pub const VMEXIT_VINTR: u64 = 0x64;
pub const VMEXIT_CPUID: u64 = 0x72;
pub const VMEXIT_HLT: u64 = 0x78;
pub const VMEXIT_INVLPGA: u64 = 0x7A;
pub const VMEXIT_IOIO: u64 = 0x7B;
pub const VMEXIT_MSR: u64 = 0x7C;
pub const VMEXIT_SHUTDOWN: u64 = 0x7F;
pub const VMEXIT_VMRUN: u64 = 0x80;
pub const VMEXIT_VMMCALL: u64 = 0x81;
pub const VMEXIT_VMLOAD: u64 = 0x82;
pub const VMEXIT_VMSAVE: u64 = 0x83;
pub const VMEXIT_STGI: u64 = 0x84;
pub const VMEXIT_CLGI: u64 = 0x85;
pub const VMEXIT_RDTSCP: u64 = 0x87;
pub const VMEXIT_WBINVD: u64 = 0x89;
pub const VMEXIT_MONITOR: u64 = 0x8A;
pub const VMEXIT_MWAIT: u64 = 0x8B;
pub const VMEXIT_XSETBV: u64 = 0x8D;
pub const VMEXIT_EFER_WRITE: u64 = 0x8F;
/// 嵌套页失效(#NPF)—— NPT hook 与受保护区域的核心 exit
pub const VMEXIT_NPF: u64 = 0x400;

// VMRUN 失败码(APM:VMRUN 经 RAX 返回)—— 与 vmcb::vmrun_fail 同值
pub const VMRUN_INVALID: u64 = u64::MAX;
pub const VMRUN_BUSY: u64 = u64::MAX - 1;
pub const VMRUN_ILLEGAL: u64 = u64::MAX - 2;

// ---- r45 exit 直方图槽位(svmb_protocol.h SVMB_XP_* 同源)----
pub const SVMB_XP_DENSE: usize = 0x100;
pub const SVMB_XP_NPF: usize = 0x100;
pub const SVMB_XP_OTHER: usize = 0x101;
pub const SVMB_XP_SLOTS: usize = 0x102;
