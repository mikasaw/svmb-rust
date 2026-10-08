#![allow(dead_code)]
//! GuestRegs —— 与 asm VMRUN 循环共享的寄存器镜像。
//! 布局锁死(C 版 core/regs.h + offsets.inc):xmm0-15 @0x000, r15..rbx @0x100..0x168,
//! rax/rflags/rip/rsp/extra1/extra2 @0x170..0x198,总大小 0x1A0。

/// 128 位暂存(xmm)
#[repr(C)]
#[derive(Clone, Copy)]
pub union M128 {
    pub lo_hi: [u64; 2],
    pub bytes: [u8; 16],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GuestRegs {
    pub xmm: [M128; 16], // 0x000 - 0x0FF
    pub r15: u64,        // 0x100
    pub r14: u64,        // 0x108
    pub r13: u64,        // 0x110
    pub r12: u64,        // 0x118
    pub r11: u64,        // 0x120
    pub r10: u64,        // 0x128
    pub r9: u64,         // 0x130
    pub r8: u64,         // 0x138
    pub rbp: u64,        // 0x140
    pub rsi: u64,        // 0x148
    pub rdi: u64,        // 0x150
    pub rdx: u64,        // 0x158
    pub rcx: u64,        // 0x160
    pub rbx: u64,        // 0x168
    pub rax: u64,        // 0x170
    pub rflags: u64,     // 0x178
    pub rip: u64,        // 0x180
    pub rsp: u64,        // 0x188
    /// devirt 请求标志 / phase-1 恢复点(asm 契约)
    pub extra1: u64, // 0x190
    /// devirt 恢复 rip(asm 契约)
    pub extra2: u64, // 0x198
}

const _: () = assert!(core::mem::offset_of!(GuestRegs, rax) == 0x170);
const _: () = assert!(core::mem::offset_of!(GuestRegs, rflags) == 0x178);
const _: () = assert!(core::mem::offset_of!(GuestRegs, rip) == 0x180);
const _: () = assert!(core::mem::offset_of!(GuestRegs, rsp) == 0x188);
const _: () = assert!(core::mem::offset_of!(GuestRegs, extra1) == 0x190);
const _: () = assert!(core::mem::offset_of!(GuestRegs, extra2) == 0x198);
const _: () = assert!(core::mem::size_of::<GuestRegs>() == 0x1A0);

/// asm 侧使用的 GuestRegs 字段偏移(与 offsets.inc 对齐)
pub mod greg_off {
    pub const RAX: usize = 0x170;
    pub const RCX: usize = 0x160;
    pub const RDX: usize = 0x158;
    pub const RFLAGS: usize = 0x178;
    pub const RIP: usize = 0x180;
    pub const RSP: usize = 0x188;
    pub const EXTRA1: usize = 0x190;
    pub const EXTRA2: usize = 0x198;
    pub const SIZE: usize = 0x1A0;
}

impl GuestRegs {
    pub const fn zeroed() -> Self {
        // SAFETY: 全零 GuestRegs 是合法位型
        unsafe { core::mem::zeroed() }
    }

    /// CR/DR exit 的 EXITINFO1[3:0] 编码 → 寄存器引用
    /// (0=AX 1=CX 2=DX 3=BX 4=SP 5=BP 6=SI 7=DI 8-15=r8-r15)
    /// # Safety
    /// 返回的引用须在 regs 存活期内使用
    pub unsafe fn gpr_mut(&mut self, idx: u32) -> &mut u64 {
        match idx & 0xF {
            0 => &mut self.rax,
            1 => &mut self.rcx,
            2 => &mut self.rdx,
            3 => &mut self.rbx,
            4 => &mut self.rsp,
            5 => &mut self.rbp,
            6 => &mut self.rsi,
            7 => &mut self.rdi,
            8 => &mut self.r8,
            9 => &mut self.r9,
            10 => &mut self.r10,
            11 => &mut self.r11,
            12 => &mut self.r12,
            13 => &mut self.r13,
            14 => &mut self.r14,
            _ => &mut self.r15,
        }
    }
}
