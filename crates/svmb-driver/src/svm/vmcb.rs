#![allow(dead_code)] // 布局库:访问器/常量按需使用
//! VMCB(虚拟机控制块)——镜像 C 版 hw/vmcb.h 的字节布局(已由其 static_assert 验证)。
//!
//! 保持裸字节存储 + 偏移常量 + 类型化访问器:避免 packed 结构体字段引用的
//! 对齐地雷,布局与 APM Vol.2 Table 15-10/15-11 一一对应。

/// VMCB 总大小(页)
pub const VMCB_SIZE: usize = 4096;
/// 保存区起始偏移
pub const VMCB_SAVE_AREA_OFFSET: usize = 0x400;

// ---- 控制区偏移(APM Table 15-10,与 C 版 vmcb.h 对齐)----
pub mod ctrl_off {
    pub const INTERCEPT_CR_READ: usize = 0x00;
    pub const INTERCEPT_CR_WRITE: usize = 0x02;
    pub const INTERCEPT_DR_READ: usize = 0x04;
    pub const INTERCEPT_DR_WRITE: usize = 0x06;
    pub const INTERCEPT_EXCEPTION: usize = 0x08;
    pub const INTERCEPT_OPCODE1: usize = 0x0C;
    pub const INTERCEPT_OPCODE2: usize = 0x10;
    pub const PAUSE_FILTER_THRESHOLD: usize = 0x3C;
    pub const PAUSE_FILTER_COUNT: usize = 0x3E;
    pub const IOPM_BASE_PA: usize = 0x40;
    pub const MSRPM_BASE_PA: usize = 0x48;
    pub const TSC_OFFSET: usize = 0x50;
    pub const GUEST_ASID: usize = 0x58;
    pub const TLB_CONTROL: usize = 0x5C;
    pub const VINTR: usize = 0x60; // byte 0 = V_TPR
    pub const EXIT_CODE: usize = 0x70;
    pub const EXIT_INFO1: usize = 0x78;
    pub const EXIT_INFO2: usize = 0x80;
    pub const EXIT_INT_INFO: usize = 0x88;
    pub const NP_ENABLE: usize = 0x90;
    pub const EVENT_INJ: usize = 0xA8;
    pub const NCR3: usize = 0xB0;
    pub const CLEAN_BITS: usize = 0xC0;
    pub const NRIP: usize = 0xC8;
}

// ---- 保存区偏移(APM Table 15-11,与 C 版 vmcb.h 对齐)----
pub mod save_off {
    pub const ES: usize = 0x400;
    pub const CS: usize = 0x410;
    pub const SS: usize = 0x420;
    pub const DS: usize = 0x430;
    pub const FS: usize = 0x440;
    pub const GS: usize = 0x450;
    pub const GDTR: usize = 0x460;
    pub const LDTR: usize = 0x470;
    pub const IDTR: usize = 0x480;
    pub const TR: usize = 0x490;
    pub const CPL: usize = 0x4CA;
    pub const EFER: usize = 0x4D0;
    pub const CR4: usize = 0x548;
    pub const CR3: usize = 0x550;
    pub const CR0: usize = 0x558;
    pub const DR7: usize = 0x560;
    pub const DR6: usize = 0x568;
    pub const RFLAGS: usize = 0x570;
    pub const RIP: usize = 0x578;
    pub const RSP: usize = 0x5D8;
    pub const RAX: usize = 0x5F8;
    pub const STAR: usize = 0x600;
    pub const LSTAR: usize = 0x608;
    pub const CSTAR: usize = 0x610;
    pub const SFMASK: usize = 0x618;
    pub const KERNEL_GS_BASE: usize = 0x620;
    pub const SYSENTER_CS: usize = 0x628;
    pub const SYSENTER_ESP: usize = 0x630;
    pub const SYSENTER_EIP: usize = 0x638;
    pub const CR2: usize = 0x640;
    pub const GPAT: usize = 0x668;
    pub const DBG_CTL: usize = 0x670;
}

/// 段寄存器在 VMCB 保存区里的 16 字节布局:Selector(2) Attribute(2) Limit(4) Base(8)
pub const SEGMENT_SIZE: usize = 16;

// ---- 拦截位(C 版 vmcb.h ic1/ic2)----
pub mod ic1 {
    pub const CPUID: u32 = 1 << 18;
    pub const MSR: u32 = 1 << 28; // RDMSR + WRMSR
}

pub mod ic2 {
    /// 防嵌套:guest 的 VMRUN 必须被拦截(参考实现注释:"vmrun拦截必须打开")
    pub const VMRUN: u32 = 1;
}

// ---- VMRUN 失败码(APM:VMRUN 以 RAX 返回)----
pub mod vmrun_fail {
    pub const INVALID: u64 = u64::MAX; // -1
    pub const BUSY: u64 = u64::MAX - 1; // -2
    pub const ILLEGAL: u64 = u64::MAX - 2; // -3
}

/// VMCB 裸页
#[repr(C, align(4096))]
#[derive(Clone)]
pub struct Vmcb {
    pub bytes: [u8; VMCB_SIZE],
}

impl Vmcb {
    pub fn zeroed(&mut self) {
        // SAFETY: 字节切片本身有效,write_bytes 仅触及 [0, VMCB_SIZE)
        unsafe {
            core::ptr::write_bytes(self.bytes.as_mut_ptr(), 0, VMCB_SIZE);
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.bytes.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.bytes.as_mut_ptr()
    }

    // ---- 类型化访问器 ----

    /// # Safety
    /// off+size 不超过 VMCB_SIZE
    pub unsafe fn read<const N: usize>(&self, off: usize) -> [u8; N] {
        unsafe { core::ptr::read_unaligned(self.bytes.as_ptr().add(off) as *const [u8; N]) }
    }

    /// # Safety
    /// off+size 不超过 VMCB_SIZE
    pub unsafe fn write<const N: usize>(&mut self, off: usize, v: [u8; N]) {
        unsafe { core::ptr::write_unaligned(self.bytes.as_mut_ptr().add(off) as *mut [u8; N], v) }
    }

    /// # Safety
    /// off+8 不超过 VMCB_SIZE
    pub unsafe fn get_u64(&self, off: usize) -> u64 {
        unsafe { u64::from_le_bytes(self.read::<8>(off)) }
    }

    /// # Safety
    /// off+8 不超过 VMCB_SIZE
    pub unsafe fn set_u64(&mut self, off: usize, v: u64) {
        unsafe { self.write::<8>(off, v.to_le_bytes()) }
    }

    /// # Safety
    /// off+4 不超过 VMCB_SIZE
    pub unsafe fn get_u32(&self, off: usize) -> u32 {
        unsafe { u32::from_le_bytes(self.read::<4>(off)) }
    }

    /// # Safety
    /// off+4 不超过 VMCB_SIZE
    pub unsafe fn set_u32(&mut self, off: usize, v: u32) {
        unsafe { self.write::<4>(off, v.to_le_bytes()) }
    }

    /// # Safety
    /// off+2 不超过 VMCB_SIZE
    pub unsafe fn get_u16(&self, off: usize) -> u16 {
        unsafe { u16::from_le_bytes(self.read::<2>(off)) }
    }

    /// # Safety
    /// off+2 不超过 VMCB_SIZE
    pub unsafe fn set_u16(&mut self, off: usize, v: u16) {
        unsafe { self.write::<2>(off, v.to_le_bytes()) }
    }

    /// # Safety
    /// off+1 不超过 VMCB_SIZE
    pub unsafe fn get_u8(&self, off: usize) -> u8 {
        // 切片索引本身安全(越界 panic 而非 UB);保留 unsafe 以对齐 API 语义
        self.bytes[off]
    }

    /// # Safety
    /// off+1 不超过 VMCB_SIZE
    pub unsafe fn set_u8(&mut self, off: usize, v: u8) {
        self.bytes[off] = v;
    }

    /// 清 CleanBits(APM:r38 定律 —— 运行期改控制区字段必须 CleanBits=0,
    /// 否则该 vhv 会丢弃修改)
    pub fn invalidate_clean_bits(&mut self) {
        // SAFETY: CLEAN_BITS 是合法的 u32 字段偏移
        unsafe {
            self.set_u32(ctrl_off::CLEAN_BITS, 0);
        }
    }
}

const _: () = assert!(VMCB_SIZE == 4096);
const _: () = assert!(VMCB_SAVE_AREA_OFFSET == 0x400);
