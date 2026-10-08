//! NPT(嵌套页表)默认视图:把整个 [0, 1TB) 物理窗口恒等映射为 2MB RWX 大页。
//!
//! 对齐 C 版 NptManager::FillIdentityView(IDENTITY_FILL_END_PA = 1TB):VMware 设备
//! 窗口(0xFD00000000 一带)、LAPIC/IO-APIC/HPET 洞、PCI ECAM 都必须可走,否则
//! 设备 ISR 的访存会打进 NPF 懒映射路径(IRQL 违例/B2 形状冻结类)。表成本
//! 1 PML4 + 2 PDPT + 1024 PD ≈ 4.1MB,与 C 版一致。
//!
//! 两条硬件/性能纪律(2026-10-01 实证):
//! - **单块连续分配**:1027 次逐页 MmAllocateContiguousMemory 每次都要扫 PFN
//!   数据库,start 卡在 MiFindContiguousPages 上分钟级(实测);改为一次
//!   0x403000 连续分配后按 PA 线性切片,毫秒级。
//! - **NptView 本体走堆**:8.2KB 的结构体若按值进出栈,debug 构建会把
//!   start() 的帧撑到 chkstk 探底蓝屏 0x50(内核栈只有 24KB)。
#![allow(dead_code)]

use alloc::boxed::Box;

use crate::phys::{alloc_contiguous, alloc_pool, free_contiguous, free_pool, virtual_to_physical};

/// 恒等映射窗口上限:1TB(C 版 r116 B5 修复的语义)
pub const IDENTITY_FILL_END_PA: u64 = 0x100_0000_0000; // 1 TB = 2^40
const LARGE_PAGE: u64 = 1 << 21; // 2MB
/// PDE 标志:P|RW|U/S|PS|A|D = 0xE7(与 C 版 npt.cpp L261 逐位一致;
/// NX=0 → RWX)。A|D 位不可省:C 版所有表项都置 A|D,缺位的模板与
/// KTIMER 链表损坏(0xA)相关 —— 2026-10-06 修复轮。
const PDE_LARGE_RWX: u64 = 0xE7;
/// PML4E/PDPTE 标志:P|RW|U/S|A|D = 0x67(C 版 npt.cpp L170/L189 同源)
const PTE_RWX: u64 = 0x67;

const NPDPT_PAGES: usize = (IDENTITY_FILL_END_PA / (512 * 512 * LARGE_PAGE)) as usize; // 2
const NPD_PAGES: usize = (IDENTITY_FILL_END_PA / LARGE_PAGE / 512) as usize; // 1024
/// PML4(1) + PDPT(2) + PD(1024) 页数
const NPAGE_TOTAL: usize = 1 + NPDPT_PAGES + NPD_PAGES; // 1027
/// 页区总字节数
const REGION_SIZE: usize = NPAGE_TOTAL * 0x1000;

pub struct NptView {
    /// 页区基址(pml4 = base+0, pdpts = base+0x1000.., pds = base+0x3000..)
    region: *mut u8,
    pml4_pa: u64,
}

impl NptView {
    /// 构建 1TB 恒等 RWX 视图 —— 单块连续分配 + PA 线性切片
    pub fn build() -> Option<Box<NptView>> {
        // SAFETY: 整区连续分配;struct 本体走池(避免栈上 8.2KB)
        unsafe {
            let region = alloc_contiguous(REGION_SIZE) as *mut u8;
            if region.is_null() {
                return None;
            }
            core::ptr::write_bytes(region, 0, REGION_SIZE);
            let region_pa = virtual_to_physical(region as *const _);
            if region_pa & 0xFFF != 0 {
                free_contiguous(region as *mut _, REGION_SIZE);
                return None;
            }

            let raw = alloc_pool(core::mem::size_of::<NptView>());
            if raw.is_null() {
                free_contiguous(region as *mut _, REGION_SIZE);
                return None;
            }
            core::ptr::write_bytes(raw, 0, core::mem::size_of::<NptView>());
            let mut view = Box::from_raw(raw as *mut NptView);
            (*view).region = region;
            (*view).pml4_pa = region_pa;

            let pml4 = region;
            let pdpt0 = region.add(0x1000);
            let pdpt1 = region.add(0x2000);
            let pds_base = region.add(0x3000);

            // PML4E:pml4[0] → pdpt0(0-512GB),pml4[1] → pdpt1(512GB-1TB)。
            // 页 PA = region_pa + 页区偏移(物理连续)
            core::ptr::write_unaligned(pml4 as *mut u64, (region_pa + 0x1000) | PTE_RWX);
            core::ptr::write_unaligned(pml4.add(8) as *mut u64, (region_pa + 0x2000) | PTE_RWX);
            // 全部 1024 个 PD 接到所属 PDPT(0..512 → pdpt0,512..1024 → pdpt1);
            // VMware 设备窗口在 0xFD00000000(≈1016GB),必须覆盖满 1TB
            for i in 0..NPD_PAGES {
                let pd_pa = region_pa + 0x3000 + (i as u64) * 0x1000;
                let pdpt = if i < 512 { pdpt0 } else { pdpt1 };
                let pdpte_ptr = pdpt.add((i % 512) * 8);
                core::ptr::write_unaligned(pdpte_ptr as *mut u64, pd_pa | PTE_RWX);
            }
            for i in 0..NPD_PAGES {
                let pd = pds_base.add(i * 0x1000);
                for k in 0..512usize {
                    let chunk_pa = ((i * 512 + k) as u64) * LARGE_PAGE;
                    let pde_ptr = pd.add(k * 8);
                    core::ptr::write_unaligned(pde_ptr as *mut u64, chunk_pa | PDE_LARGE_RWX);
                }
            }
            Some(view)
        }
    }

    pub fn pml4_pa(&self) -> u64 {
        self.pml4_pa
    }
}

impl Drop for NptView {
    fn drop(&mut self) {
        // 构建失败路径释放;r112 纪律下成功路径由调用方 leak,本 Drop 仅兜底。
        // Box 本体经 GlobalAlloc 归还池;页区整块归还连续分配器。
        // SAFETY: region 只能来自 build() 的 alloc_contiguous(REGION_SIZE)
        unsafe {
            if !self.region.is_null() {
                free_contiguous(self.region as *mut _, REGION_SIZE);
                self.region = core::ptr::null_mut();
            }
        }
    }
}

const _: () = {
    assert!(NPAGE_TOTAL == 1 + NPDPT_PAGES + NPD_PAGES);
    assert!(REGION_SIZE == NPAGE_TOTAL * 0x1000);
};
