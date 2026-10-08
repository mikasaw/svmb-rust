#![allow(dead_code)] // 布局库:访问器/常量按需使用
//! per-core 虚拟 CPU 上下文 —— 布局锁死(C 版 vcpu.h + offsets.inc):
//! blob 内 Info @0x3000 / Regs @0x4000 / VmmStack @0x5000(总 0xB000);
//! GuestVmcb/HostVmcb/HSave 是指向独立物理连续页的指针(blob 前 0x18 字节)。

use core::sync::atomic::AtomicI32;

use super::regs::GuestRegs;
use super::vmcb::Vmcb;
use crate::svm::exit::SVMB_XP_SLOTS;

/// vCPU 生命周期状态(C 版 VcpuState)
pub mod vcpu_state {
    pub const OFF: i32 = 0;
    pub const ENTERING: i32 = 1;
    pub const GUEST: i32 = 2;
    pub const LEAVING: i32 = 3;
}

pub const VMM_STACK_SIZE: usize = 0x6000;
/// 每核 exit 追踪预算(r13 教训:预算大了会灌爆 KDNET)
pub const EXIT_TRACE_BUDGET: i32 = 8;

/// vCPU 运行信息(位于 blob 的 0x3000 槽,asm 只按偏移写第一个 LONG State)
#[repr(C)]
pub struct VcpuInfo {
    /// vcpu_state::* —— asm 以普通 dword store 写 0(r28 就地退出);
    /// Rust 侧经 AtomicI32 访问(与 C 版 Interlocked 同一机器语义)
    pub state: AtomicI32,
    pub exit_trace_left: i32,
    pub pending_tlb_kick: i32,
    pub pending_view_switch: i32,
    pub pending_view_pml4: u64,
    pub cpu_index: u32,
    /// r124:restore 后跳过重入的标志(原 _pad0 借用)
    pub resumed: u32,
    pub raw_alloc: *mut u8,         // 未对齐的池基址(stop 后按纪律泄漏)
    pub hv: *mut core::ffi::c_void, // 预留
    pub exit_count: u64,
    pub active_ncr3: u64,
    pub published_ncr3: u64,
    pub exit_hist: [u64; SVMB_XP_SLOTS],
    pub guest_vmcb_pa: u64,
    pub host_vmcb_pa: u64,
    pub vmm_stack_top: u64,
}

const _: () = assert!(core::mem::offset_of!(VcpuInfo, state) == 0);
const _: () = assert!(core::mem::size_of::<VcpuInfo>() <= 0x1000);

/// per-core 上下文 blob(总 0xB000,页对齐分配)
#[repr(C)]
pub struct VcpuContext {
    pub guest_vmcb: *mut Vmcb, // +0x0000(独立连续页)
    pub host_vmcb: *mut Vmcb,  // +0x0008
    pub hsave: *mut u8,        // +0x0010(VM_HSAVE_PA 目标)
    pub _pad1: [u8; 0x3000 - 0x18],
    pub info: VcpuInfo,        // +0x3000
    pub _pad2: [u8; 0x1000 - core::mem::size_of::<VcpuInfo>()],
    pub regs: GuestRegs,       // +0x4000
    pub _pad3: [u8; 0x1000 - core::mem::size_of::<GuestRegs>()],
    pub vmm_stack: [u8; VMM_STACK_SIZE], // +0x5000
}

const _: () = assert!(core::mem::offset_of!(VcpuContext, guest_vmcb) == 0x0000);
const _: () = assert!(core::mem::offset_of!(VcpuContext, host_vmcb) == 0x0008);
const _: () = assert!(core::mem::offset_of!(VcpuContext, hsave) == 0x0010);
const _: () = assert!(core::mem::offset_of!(VcpuContext, info) == 0x3000);
const _: () = assert!(core::mem::offset_of!(VcpuContext, regs) == 0x4000);
const _: () = assert!(core::mem::offset_of!(VcpuContext, vmm_stack) == 0x5000);
const _: () = assert!(core::mem::size_of::<VcpuContext>() == 0xB000);

/// blob 池分配尺寸(页对齐预留 0x1000 余量)
pub const BLOB_ALLOC_SIZE: usize = core::mem::size_of::<VcpuContext>() + 0x1000;

impl VcpuContext {
    /// 从池分配(未对齐)基址构造页对齐 blob 并清零
    ///
    /// # Safety
    /// raw 必须来自 ExAllocatePoolWithTag(BLOB_ALLOC_SIZE) 且未被使用
    pub unsafe fn init(raw: *mut u8, cpu_index: u32) -> *mut VcpuContext {
        unsafe {
            let aligned = ((raw as u64) + 0xFFF) & !0xFFF;
            core::ptr::write_bytes(raw, 0, BLOB_ALLOC_SIZE);
            let v = aligned as *mut VcpuContext;
            (*v).info.raw_alloc = raw;
            (*v).info.cpu_index = cpu_index;
            (*v).info.exit_trace_left = EXIT_TRACE_BUDGET;
            (*v).info.resumed = 0;
            (*v).info.state = AtomicI32::new(vcpu_state::OFF);
            v
        }
    }
}
