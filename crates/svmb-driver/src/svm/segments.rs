#![allow(dead_code)] // 布局库:访问器/常量按需使用
//! GDT 描述符解析(SimpleSvm 技术):long mode 下 CS/DS/ES/SS 的 base+limit 被
//! CPU 忽略,但每个段的 attribute 都参与段检查,TR/LDTR 保有真实 base/limit
//! —— 所以从 GDT 取真值而不是伪造。

/// GDT 段描述符位域解析结果
pub struct SegInfo {
    pub attribute: u16,
    pub base: u64,
}

/// 读取 GDT 中 selector 对应的 8 字节描述符(selector 为空返回 0)
///
/// # Safety
/// gdt_base 必须是当前 GDT 的有效基址
unsafe fn descriptor(selector: u16, gdt_base: u64) -> u64 {
    unsafe {
        if selector & !7 == 0 {
            return 0;
        }
        let p = (gdt_base + ((selector & !7) as u64)) as *const u64;
        core::ptr::read_unaligned(p)
    }
}

/// VMCB 段寄存器的 attribute 字段:Type(4) S(1) DPL(2) P(1) AVL(1) L(1) D(1) G(1)
///
/// # Safety
/// gdt_base 必须是当前 GDT 的有效基址
pub unsafe fn segment_attribute(selector: u16, gdt_base: u64) -> u16 {
    unsafe {
        let d = descriptor(selector, gdt_base);
        if d == 0 {
            return 0;
        }
        let type_ = (d >> 40) & 0xF;
        let system = (d >> 44) & 1;
        let dpl = (d >> 45) & 3;
        let present = (d >> 47) & 1;
        let avl = (d >> 52) & 1;
        let long = (d >> 53) & 1;
        let default_bit = (d >> 54) & 1;
        let gran = (d >> 55) & 1;
        (type_ | (system << 4) | (dpl << 5) | (present << 7) | (avl << 8)
            | (long << 9) | (default_bit << 10) | (gran << 11)) as u16
    }
}

/// 段基址(系统描述符/TSS-LDT 是 16 字节,高 32 位基址在第二对 8 字节里)
///
/// # Safety
/// gdt_base 必须是当前 GDT 的有效基址
pub unsafe fn segment_base(selector: u16, gdt_base: u64) -> u64 {
    unsafe {
        let d = descriptor(selector, gdt_base);
        if d == 0 {
            return 0;
        }
        // x86 描述符基址位域(C 版 segments.cpp 同源;2026-10-06 VMCB diff
        // 实锤此前笔误:BaseLow 只取 8 位、BaseMiddle 未左移 → TR base 带
        // 垃圾低位 → IST/RSP0 错栈 → 中断写坏内核内存 = KTIMER 损坏根因)
        let mut base = (d >> 16) & 0xFFFF; // BaseLow:16 位
        base |= ((d >> 32) & 0xFF) << 16; // BaseMiddle:左移 16
        base |= ((d >> 56) & 0xFF) << 24; // BaseHigh:左移 24
        let system = (d >> 44) & 1;
        if system == 0 {
            // 16 字节系统描述符:基址位 32-63 在字节 8-11
            let p = (gdt_base + ((selector & !7) as u64) + 8) as *const u8;
            base |= (*p as u64) << 32;
            base |= (*p.add(1) as u64) << 40;
            base |= (*p.add(2) as u64) << 48;
            base |= (*p.add(3) as u64) << 56;
        }
        base
    }
}

/// VMCB 用 limit(C 版 GetSegmentLimitForVmcb):长模式下 limit 被 CPU 忽略,
/// 但 VMRUN 校验要求非零 —— 恒返回 0xFFFFFFFF。
pub const VMCB_LIMIT: u32 = 0xFFFF_FFFF;
