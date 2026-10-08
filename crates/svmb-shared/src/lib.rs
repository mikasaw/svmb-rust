//! svmb-rust R3 <-> R0 共享协议
//!
//! 镜像 C 版 `svmb/shared/svmb_protocol.h`(单一事实来源,两端 ABI 逐字段对齐)。
//! 内核驱动(no_std)与用户态 ctl 共同依赖;结构布局以 C 版为准,
//! 追加字段必须走 C 版既有的"只允许尾部追加"纪律(r82 ABI 约定)。

#![no_std]

use core::mem::size_of;

// ---------------------------------------------------------------------------
// 魔数 / 版本 / 设备名
// ---------------------------------------------------------------------------

pub const SVMB_PROTO_MAGIC: u32 = 0x5342_4D53; // 'SVMB'
pub const SVMB_PROTO_VERSION: u32 = 1;

/// `\Device\svmb_rust`(Rust 版独立命名,避免与 C 版 svmb.sys 同机冲突)
pub const SVMB_DEVICE_NAME: &str = "\\Device\\svmb_rust";
/// `\DosDevices\svmb_rust`
pub const SVMB_DOS_NAME: &str = "\\DosDevices\\svmb_rust";
/// 用户态打开路径 `\\.\svmb_rust`
pub const SVMB_USER_PATH: &str = "\\\\.\\svmb_rust";

// ---------------------------------------------------------------------------
// IOCTL codes —— 与 C 版 SVMB_CTL(fn) 完全一致
// CTL_CODE(type, fn, METHOD_BUFFERED, FILE_ANY_ACCESS)
//   = (0x8000 << 16) | (fn << 2)
// ---------------------------------------------------------------------------

pub const SVMB_CTL_TYPE: u32 = 0x8000;

/// C 宏 `SVMB_CTL(fn)` 的 Rust 等价(const fn,编译期求值)
pub const fn svmb_ctl(function: u32) -> u32 {
    (SVMB_CTL_TYPE << 16) | (function << 2)
}

pub const SVMB_IOCTL_GET_INFO: u32 = svmb_ctl(0x800);
pub const SVMB_IOCTL_HV_CONTROL: u32 = svmb_ctl(0x801);
pub const SVMB_IOCTL_LOG_READ: u32 = svmb_ctl(0x802);
pub const SVMB_IOCTL_MODULE_CTL: u32 = svmb_ctl(0x803);
pub const SVMB_IOCTL_NPT_HOOK: u32 = svmb_ctl(0x810);
pub const SVMB_IOCTL_NPT_PROBE: u32 = svmb_ctl(0x811);
pub const SVMB_IOCTL_CR3_CONFIG: u32 = svmb_ctl(0x820);
pub const SVMB_IOCTL_CR3_STATS: u32 = svmb_ctl(0x821);
pub const SVMB_IOCTL_CR3_POISON: u32 = svmb_ctl(0x822);
pub const SVMB_IOCTL_CR3_PROT: u32 = svmb_ctl(0x823);
pub const SVMB_IOCTL_CR3_PROBE: u32 = svmb_ctl(0x824);
pub const SVMB_IOCTL_CR3_READVM: u32 = svmb_ctl(0x825);
pub const SVMB_IOCTL_SUSP_LIST: u32 = svmb_ctl(0x826);
pub const SVMB_IOCTL_SUSP_RESUME: u32 = svmb_ctl(0x827);
pub const SVMB_IOCTL_RDTSC: u32 = svmb_ctl(0x828);
pub const SVMB_IOCTL_SYSCALL_AUDIT: u32 = svmb_ctl(0x829);
pub const SVMB_IOCTL_KDUMP: u32 = svmb_ctl(0x82A);
pub const SVMB_IOCTL_SYSAUD_BEHAVIOR: u32 = svmb_ctl(0x82B);
pub const SVMB_IOCTL_CRUMBS: u32 = svmb_ctl(0x82C);
pub const SVMB_IOCTL_CR3_WRITEVM: u32 = svmb_ctl(0x82D);
pub const SVMB_IOCTL_DBG_CONFIG: u32 = svmb_ctl(0x830);
pub const SVMB_IOCTL_DBG_EVENTS: u32 = svmb_ctl(0x831);
pub const SVMB_IOCTL_STRESS: u32 = svmb_ctl(0x840);
pub const SVMB_IOCTL_SELFTEST: u32 = svmb_ctl(0x850);
pub const SVMB_IOCTL_EXITPROF: u32 = svmb_ctl(0x851);

// ---------------------------------------------------------------------------
// 基础结构(骨架阶段已移植;后续模块按需补齐对应结构)
// ---------------------------------------------------------------------------

/// C: `SVMB_HV_STATE`
pub mod hv_state {
    pub const OFF: u32 = 0;
    pub const STARTING: u32 = 1;
    pub const RUNNING: u32 = 2;
    pub const STOPPING: u32 = 3;
}

/// C: `SVMB_INFO`(IOCTL_GET_INFO 输出)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SvmbInfo {
    pub magic: u32,             // SVMB_PROTO_MAGIC
    pub proto_version: u32,
    pub driver_version: u32,
    pub hv_state: u32,          // hv_state::*
    pub cores_virtualized: u32, // 当前处于 SVM 之下的核心数
    pub core_count: u32,        // 活动处理器数
    pub pa_width_bits: u32,     // 物理地址宽度(CPUID.80000008)
    pub npt_view_count: u32,
    pub exit_counter: u64,      // vmexit 总数(近似,各核之和)
    pub module_count: u32,
    pub reserved: u32,
}

/// C: `SVMB_HV_CONTROL`(IOCTL_HV_CONTROL)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SvmbHvControl {
    pub action: u32,      // 0 = start, 1 = stop, 2 = stress loop
    pub repeat_count: u32,
}

/// C: `SVMB_LOG_ENTRY`(IOCTL_LOG_READ 条目)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SvmbLogEntry {
    pub core: u32,
    pub level: u32, // 1=err 2=warn 3=info 4=verbose
    pub tsc: u64,
    pub length: u16,
    pub text: [u8; 224],
}

pub const SVMB_MAX_MODULES: usize = 16;

/// C: `SVMB_MODULE_INFO`
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SvmbModuleInfo {
    pub name: [u8; 16],
    pub attached: u32,     // 1 = 处理器已挂载
    pub exit_handled: u32, // 该模块处理的 exit 计数(近似)
}

/// C: `SVMB_MODULE_LIST`(MODULE_CTL Action=2 查询输出)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SvmbModuleList {
    pub count: u32,
    pub modules: [SvmbModuleInfo; SVMB_MAX_MODULES],
}

/// C: `SVMB_MODULE_CTL`(IOCTL_MODULE_CTL)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SvmbModuleCtl {
    pub action: u32,   // 0 = attach, 1 = detach, 2 = query
    pub name: [u8; 16],
}

/// C: `SVMB_EXITPROF` 直方图槽位
pub const SVMB_XP_DENSE: usize = 0x100;
pub const SVMB_XP_NPF: usize = 0x100; // NPF exit(0x400)归并槽
pub const SVMB_XP_OTHER: usize = 0x101;
pub const SVMB_XP_SLOTS: usize = 0x102;

/// C: `SVMB_EXITPROF`(IOCTL_EXITPROF 输出)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SvmbExitProf {
    pub version: u32,
    pub cpu_count: u32,
    pub total: u64,
    pub hist: [u64; SVMB_XP_SLOTS],
}

/// C: `SVMB_DBG_EVT`
pub mod dbg_evt {
    pub const BREAKPOINT: u32 = 1; // #BP
    pub const SINGLE_STEP: u32 = 2; // #DB with TF
    pub const DEBUG_REG: u32 = 3; // DR access
    pub const INVALID_OPCODE: u32 = 4; // #UD
    pub const PROCESS_WATCH: u32 = 5;
    pub const VIEW_ACTIVATE: u32 = 6;
    pub const SENTINEL_TRIP: u32 = 7;
    pub const REGION_TRIP: u32 = 8;
}

/// C: `SVMB_DBG_EVENT`(IOCTL_DBG_EVENTS 条目)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SvmbDbgEvent {
    pub evt_type: u32, // dbg_evt::*
    pub core: u32,
    pub rip: u64,
    pub cr3: u64,
    pub rsp: u64,
    pub extra: u64,
}

pub const SVMB_DBG_MAX_EVENTS: usize = 64;

/// C: `SVMB_DBG_EVENT_BUFFER`(IOCTL_DBG_EVENTS 输出)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SvmbDbgEventBuffer {
    pub count: u32,
    pub lost_count: u32,
    pub events: [SvmbDbgEvent; SVMB_DBG_MAX_EVENTS],
}

/// C: `SVMB_DBG_CONFIG`(IOCTL_DBG_CONFIG)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SvmbDbgConfig {
    pub enable_bp: u32,
    pub enable_db: u32,
    pub enable_ud: u32,
    pub hide_dr: u32,
    pub spoof_dr7_zero: u32,
    pub arm_single_step: u32,
    pub reserved: [u32; 2],
}

/// C: `SVMB_STRESS`(IOCTL_STRESS)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SvmbStress {
    pub kind: u32, // 0 = npt hook add/remove 循环, 1 = exit 风暴
    pub iterations: u32,
    pub status: u64,
}

/// C: `SVMB_SELFTEST_RESULT`(IOCTL_SELFTEST,离线自检)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SvmbSelftestResult {
    pub magic: u32, // SVMB_PROTO_MAGIC
    pub total: u32,
    pub failed: u32,
    pub reserved: u32,
    pub last_fail: [u8; 96], // 首个失败项名,全绿为空串
}

// ---------------------------------------------------------------------------
// TODO(移植): 以下 C 版结构在对应功能模块移植时补齐(见 svmb_protocol.h)
//   - SVMB_NPT_HOOK_CTL / SVMB_NPT_PROBE          (NPT hook 与探针)
//   - SVMB_CR3_CONFIG / STATS / POISON / PROT / PROBE / READVM / WRITEVM
//   - SVMB_SUSP_*                                   (r91 可逆挂起)
//   - SVMB_SYSAUD_* / SVMB_KDUMP / SVMB_CRUMBS     (r99+ 审计与取证)
// IOCTL 码已在上面全部定义,新增结构时直接对齐 C 版字段与尾部追加纪律。
// ---------------------------------------------------------------------------

/// 编译期断言:已移植结构的大小与 C 版一致时才有意义(布局回归时核对用)
pub const SIZEOF_INFO: usize = size_of::<SvmbInfo>();
