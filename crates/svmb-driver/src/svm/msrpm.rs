#![allow(dead_code)] // 布局库:访问器/常量按需使用
//! MSRPM(I/O 与 MSR 权限位图)初始化。
//! 布局(APM Vol.2 §15.5):每 MSR 2 位(bit0=读拦截,bit1=写拦截),三段:
//!   0x00000000-0x00001FFF -> 字节偏移 0x0000
//!   0xC0000000-0xC0001FFF -> 字节偏移 0x0800
//!   0xC0010000-0xC0011FFF -> 字节偏移 0x1000
//! IOPM:4 页(8KB 读位图 + 8KB 写位图,C 版 Iopm::SIZE 同源),全零 = 全放行。

use crate::phys::{alloc_contiguous, free_contiguous};

pub const MSRPM_SIZE: usize = 2 * 0x1000;
pub const IOPM_SIZE: usize = 4 * 0x1000;

pub const TAG_SVMB_MSRPM: u32 = u32::from_ne_bytes(*b"SVMB");

/// MSR → (字节偏移, 位偏移);不在三段内返回 None
pub fn msr_to_bit_offset(msr: u32) -> Option<(usize, u8)> {
    let base: usize = if msr <= 0x00001FFF {
        0x0000
    } else if (0xC0000000..=0xC0001FFF).contains(&msr) {
        0x0800
    } else if (0xC0010000..=0xC0011FFF).contains(&msr) {
        0x1000
    } else {
        return None;
    };
    let msr_idx = (msr & 0x1FFF) as usize;
    Some((base + msr_idx / 4, ((msr_idx % 4) * 2) as u8))
}

/// MSRPM 缓冲(骨架:单缓冲;C 版的三缓冲轮换等模块化时再移植)
pub struct Msrpm {
    va: *mut u8,
    pa: u64,
}

impl Msrpm {
    /// 分配并清零(全零 = 不拦截任何 MSR),随后把 EFER/VM_CR 置为读写拦截
    pub fn new() -> Option<Self> {
        let va = alloc_contiguous(MSRPM_SIZE) as *mut u8;
        if va.is_null() {
            return None;
        }
        // SAFETY: va 来自成功的连续分配
        unsafe {
            core::ptr::write_bytes(va, 0, MSRPM_SIZE);
        }
        let mut m = Msrpm { va, pa: 0 };
        // EFER / VM_CR 读写全部拦截(hide-SVM 基线;HSAVE 保护默认关闭,与 C 一致)
        let efer = crate::svm::msr::MSR_EFER;
        let vmcr = crate::svm::msr::MSR_VM_CR;
        m.set(efer, true, true, true);
        m.set(vmcr, true, true, true);
        // SAFETY: va 有效
        m.pa = unsafe { crate::phys::virtual_to_physical(m.va as *const _) };
        Some(m)
    }

    /// # Safety
    /// self 必须由 new() 构造且尚未 free
    pub unsafe fn destroy(&mut self) {
        unsafe {
            if !self.va.is_null() {
                free_contiguous(self.va as *mut core::ffi::c_void, MSRPM_SIZE);
                self.va = core::ptr::null_mut();
                self.pa = 0;
            }
        }
    }

    pub fn physical(&self) -> u64 {
        self.pa
    }

    fn set(&mut self, msr: u32, read: bool, write: bool, on: bool) {
        let Some((byte_off, bit_off)) = msr_to_bit_offset(msr) else {
            return;
        };
        // SAFETY: byte_off < MSRPM_SIZE(由 msr_to_bit_offset 保证)
        unsafe {
            let p = self.va.add(byte_off);
            let mut mask: u8 = 0;
            if read {
                mask |= 1 << bit_off;
            }
            if write {
                mask |= 1 << (bit_off + 1);
            }
            let cur = core::ptr::read_volatile(p);
            let new = if on { cur | mask } else { cur & !mask };
            core::ptr::write_volatile(p, new);
        }
    }
}

/// IOPM(4 页,全零 = 所有 IO 放行;C 版同源)
pub struct Iopm {
    va: *mut u8,
    pa: u64,
}

impl Iopm {
    pub fn new() -> Option<Self> {
        let va = alloc_contiguous(IOPM_SIZE) as *mut u8;
        if va.is_null() {
            return None;
        }
        // SAFETY: va 来自成功的连续分配
        unsafe {
            core::ptr::write_bytes(va, 0, IOPM_SIZE);
            let pa = crate::phys::virtual_to_physical(va as *const _);
            Some(Iopm { va, pa })
        }
    }

    /// # Safety
    /// self 必须由 new() 构造且尚未 free
    pub unsafe fn destroy(&mut self) {
        unsafe {
            if !self.va.is_null() {
                free_contiguous(self.va as *mut core::ffi::c_void, IOPM_SIZE);
                self.va = core::ptr::null_mut();
                self.pa = 0;
            }
        }
    }

    pub fn physical(&self) -> u64 {
        self.pa
    }
}
