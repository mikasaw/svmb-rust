#![allow(dead_code)] // 布局库:访问器/常量按需使用
//! 物理内存助手:连续页分配 / 池分配 / VA→PA(SVM 结构与 NPT 页表的硬件前提)

use wdk_sys::{
    ntddk::{MmAllocateContiguousMemory, MmFreeContiguousMemory, MmGetPhysicalAddress},
    PHYSICAL_ADDRESS, PVOID,
};

unsafe extern "C" {
    // ExAllocatePoolWithTag 不在 wdk-sys 绑定里(WDK 28000 弃用旧池 API 到
    // bindgen 排除),但 ntoskrnl 一直导出;链接期由 ntoskrnl.lib 解析。
    pub fn ExAllocatePoolWithTag(PoolType: i32, NumberOfBytes: usize, Tag: u32) -> *mut u8;
    pub fn ExFreePoolWithTag(P: PVOID, Tag: u32);
}

pub const NON_PAGED_POOL: i32 = 0; // POOL_TYPE::NonPagedPool
pub const POOL_TAG_SVMB: u32 = u32::from_ne_bytes(*b"SVMB");

/// 分配物理连续、4K 对齐的内核内存(非分页、可缓存)
pub fn alloc_contiguous(size: usize) -> *mut core::ffi::c_void {
    // HighestAcceptableAddress = 全物理空间(0xFFFF..FFFF)
    // SAFETY: 常量物理上限;失败返回 NULL 由调用方处理
    unsafe {
        let mut highest = core::mem::zeroed::<PHYSICAL_ADDRESS>();
        core::ptr::addr_of_mut!(highest.QuadPart).write_unaligned(-1i64);
        let p = MmAllocateContiguousMemory(size as u64, highest);
        if !p.is_null() {
            // C 版语义(AllocContiguous):分配即清零。host_vmcb 控制区与
            // HSAVE 页真硬件不读,但 vhv 嵌套层的 vmsave/vmload/HSAVE 仿真
            // 接触面更宽 —— 池残留垃圾进这些区域是 2026-10-07 轮的
            // 头号嫌疑(C 清零而 Rust 不清,启动路径唯一实锤物理差异)
            core::ptr::write_bytes(p as *mut u8, 0, size);
        }
        p
    }
}

/// # Safety
/// p 必须来自 alloc_contiguous 且 size 与之匹配
pub unsafe fn free_contiguous(p: *mut core::ffi::c_void, _size: usize) {
    unsafe { MmFreeContiguousMemory(p) }
}

/// 非分页池分配(失败返回 null)
pub fn alloc_pool(size: usize) -> *mut u8 {
    // SAFETY: 池分配无前置状态
    unsafe { ExAllocatePoolWithTag(NON_PAGED_POOL, size, POOL_TAG_SVMB) }
}

/// # Safety
/// p 必须来自 alloc_pool
#[allow(dead_code)] // 分配/释放对称保留(per-core 页永不 free = r112 泄漏纪律)
pub unsafe fn free_pool(p: *mut u8) {
    unsafe { ExFreePoolWithTag(p as PVOID, POOL_TAG_SVMB) }
}

/// # Safety
/// va 必须是非分页内核 VA
pub unsafe fn virtual_to_physical(va: *const core::ffi::c_void) -> u64 {
    // SAFETY: 调用方保证 va 有效
    unsafe {
        let pa = MmGetPhysicalAddress(va as PVOID);
        core::ptr::addr_of!(pa.QuadPart).read_unaligned() as u64
    }
}
