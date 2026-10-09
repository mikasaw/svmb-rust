//! #VMEXIT 处理器 —— C 版 SvmbVmExitEntry / HandleCpuidExit / HandleMsrExit /
//! HandleVmrunExit / HcExitVmm / SvmbFillMachineFrame 的移植。
//!
//! r13 纪律:exit 路径的日志一律走 per-core 预算(默认 8 条),否则 CPUID
//! 拦截 + KDNET DbgPrint 会把 guest 拖到爬行。

use core::arch::x86_64::__cpuid_count;

use super::asm::{svmb_rdmsr, svmb_wrmsr};
use super::exit;
use super::msr;
use super::percpu::{vcpu_state, VcpuContext};
use super::regs::GuestRegs;
use super::vmcb::{ctrl_off, ic1, ic2, save_off, vmrun_fail, Vmcb};

/// CPUID 超级调用叶(C 版 HYPERCALL_CPUID_LEAF)
pub const HYPERCALL_CPUID_LEAF: u32 = 0x4000_00FF;
pub const HYPERCALL_MAGIC: u32 = 0x5342_4D53; // 'SVMB'
/// 超级调用号
pub mod hc {
    pub const PROBE: u32 = 0x0000;
    pub const VERSION: u32 = 0x0001;
    pub const EXIT_VMM: u32 = 0x0002;
}

#[inline]
fn vmcb_of<'a>(vcpu: *mut VcpuContext) -> &'a mut Vmcb {
    // SAFETY: vcpu 来自 asm 循环,guest_vmcb 指向有效连续页
    unsafe { &mut *((*vcpu).guest_vmcb) }
}

/// 注入事件(APM EventInj:vector | type<<8 | ev<<11 | valid<<31)。
/// r38 定律:控制区写入必须清 CleanBits,否则该 vhv 丢弃修改。
///
/// # Safety
/// vcpu 必须有效
pub unsafe fn inject_event(vcpu: *mut VcpuContext, vector: u32, evt_type: u32, ec_valid: bool, error_code: u32) {
    unsafe {
        let vmcb = vmcb_of(vcpu);
        let mut data: u64 = (vector & 0xFF) as u64;
        data |= ((evt_type & 7) as u64) << 8;
        if ec_valid {
            data |= 1 << 11;
            data |= ((error_code as u64) & 0xFFFF_FFFF) << 32;
        }
        data |= 1 << 31; // Valid
        vmcb.set_u64(ctrl_off::EVENT_INJ, data);
        vmcb.invalidate_clean_bits();
    }
}

/// 是否 fault 类 exit(NRip 指向故障指令:重执行;否则指向下一条:前进)
fn is_fault_class_exit(code: u64) -> bool {
    (0x40..=0x5F).contains(&code) // EXCEPTION(0..31)
        || code == exit::VMEXIT_NPF
        || code == exit::VMEXIT_SHUTDOWN
        || code == exit::VMEXIT_INTR
        || code == exit::VMEXIT_NMI
        || code == exit::VMEXIT_SMI
        || code == exit::VMEXIT_INIT
        || code == exit::VMEXIT_VINTR
}

fn trace_exit(vcpu: *mut VcpuContext, code: u64, rip: u64, nrip: u64) {
    // r125:GIF=0 循环内禁 println(DbgPrint 锁风险)—— 信标替代
    // (tag=0x192,data=code;rip/nrip 可经 crumbs 前后条目推断)
    unsafe {
        if (*vcpu).info.exit_trace_left <= 0 {
            return;
        }
        (*vcpu).info.exit_trace_left -= 1;
        svmb_crumb_post_d(0x192, code);
        svmb_crumb_post_d(0x193, rip);
        svmb_crumb_post_d(0x194, nrip);
    }
}

/// CPUID 直通 + 隐藏 SVM/NPT 能力位(C 版默认 GuestCpuidHideEnabled=1)
///
/// # Safety
/// regs/vcpu 有效
unsafe fn handle_cpuid(vcpu: *mut VcpuContext, regs: *mut GuestRegs) -> bool {
    unsafe {
        let leaf = (*regs).rax as u32;
        let subleaf = (*regs).rcx as u32;

        if ENABLE_HYPERCALL {
            // 超级调用 CPUID 叶回退通道(VMMCALL 不在基线拦截里,这是唯一通道)
            if leaf == HYPERCALL_CPUID_LEAF {
                let nr = (*regs).rcx as u32; // 注意:与 subleaf 同源(C 版同款)
                let cpl = vmcb_of(vcpu).get_u8(save_off::CPL) & 3;
                // CPUID 是公开探测通道:probe/version 任何 CPL 可调,其余仅内核
                if cpl != 0 && nr != hc::PROBE && nr != hc::VERSION {
                    (*regs).rax = 0xC000_0022; // STATUS_ACCESS_DENIED
                    return true;
                }
                let res = hypercall_invoke(vcpu, nr, (*regs).rbx, (*regs).rdx, (*regs).rip);
                (*regs).rax = res;
                return true;
            }
            // 在位探测叶(教育 hypervisor 风格):leaf - 1
            if leaf == HYPERCALL_CPUID_LEAF - 1 {
                (*regs).rax = HYPERCALL_MAGIC as u64;
                return true;
            }
        }

        // 普通 CPUID:宿主直通(宿主视图即该 vhv 调整后的视图)
        let r = __cpuid_count(leaf, subleaf);
        (*regs).rax = r.eax as u64;
        (*regs).rbx = r.ebx as u64;
        (*regs).rcx = r.ecx as u64;
        (*regs).rdx = r.edx as u64;
        // 隐藏 SVM 位(0x80000001.ECX[2])与 NPT 位(0x8000000A.EDX[0])
        if ENABLE_HIDE_BITS {
            if ENABLE_HIDE_SVM && leaf == 0x8000_0001 {
                (*regs).rcx &= !(1 << 2);
            }
            if ENABLE_HIDE_NPT && leaf == 0x8000_000A {
                (*regs).rdx &= !(1 << 0);
            }
            if ENABLE_POISON_RBX {
                (*regs).rbx &= !(1 << 0);
            }
        }
        if ENABLE_TOY_WRITE {
            // r114f:代码形状与篡改同构,值不进 guest(asm 不恢复此目标)
            let cur = core::ptr::read_volatile(core::ptr::addr_of!(TOY_COUNTER));
            core::ptr::write_volatile(
                core::ptr::addr_of_mut!(TOY_COUNTER),
                cur.wrapping_add(1),
            );
        }
        true
    }
}

/// 超级调用分发(返回值入 rax)
///
/// # Safety
/// vcpu 有效
#[allow(dead_code)] // r114 MINIMAL_EXIT 消除分支后保留
unsafe fn hypercall_invoke(vcpu: *mut VcpuContext, nr: u32, _a1: u64, _a2: u64, nrip: u64) -> u64 {
    unsafe {
        match nr {
            hc::PROBE => HYPERCALL_MAGIC as u64,
            hc::VERSION => 1, // 骨架版本;后续对齐 C 版版本纪律
            hc::EXIT_VMM => {
                // r124 返回式:置 LEAVING,当圈退出主循环;线程经 restore
                // 从超级调用发起点的上下文继续(nrip 前进已由 advance 处理)
                (*vcpu).info.state.store(vcpu_state::LEAVING, core::sync::atomic::Ordering::Release);
                0
            }
            _ => 0xFFFF_FFFFC000_0001, // STATUS_UNSUCCESSFUL
        }
    }
}

#[inline]
#[allow(dead_code)] // 旧 asm 循环路径遗留;r151 结构循环改用 vcpu+0x4000 内嵌偏移
unsafe fn regs_of(vcpu: *mut VcpuContext) -> *mut GuestRegs {
    unsafe { core::ptr::addr_of_mut!((*vcpu).regs) }
}

/// MSR exit(仅 EFER/VM_CR 在 MSRPM 里)—— C 版 HandleMsrExit 语义
///
/// # Safety
/// regs/vcpu 有效
#[allow(dead_code)] // r114 MINIMAL_EXIT 消除分支后保留
unsafe fn handle_msr(_vcpu: *mut VcpuContext, regs: *mut GuestRegs, info1: u64) -> bool {
    unsafe {
        let msr_addr = (*regs).rcx as u32;
        let is_write = (info1 & 1) != 0;
        let value =
            |r: *mut GuestRegs| ((*r).rax & 0xFFFF_FFFF) | ((*r).rdx << 32);
        let put = |r: *mut GuestRegs, v: u64| {
            (*r).rax = v & 0xFFFF_FFFF;
            (*r).rdx = v >> 32;
        };

        if msr_addr == msr::MSR_VM_CR {
            if is_write {
                // 保持"guest 视角 SVM 被固件禁用"的伪装
                let v = value(regs) | (1 << msr::VM_CR_SVMDIS);
                svmb_wrmsr(msr_addr, v);
            } else {
                let v = svmb_rdmsr(msr_addr) | (1 << msr::VM_CR_SVMDIS);
                put(regs, v);
            }
            return true;
        }
        if msr_addr == msr::MSR_VM_HSAVE_PA {
            if is_write {
                // 丢弃 guest 写:宿主 HSAVE 区不许动
            } else {
                put(regs, 0); // 隐藏非零 HSAVE PA —— 经典 SVM 存在性指纹
            }
            return true;
        }
        if msr_addr == msr::MSR_EFER {
            if is_write {
                let v = value(regs) | (1 << msr::EFER_SVME); // 我们活着时 SVM 保持开启
                svmb_wrmsr(msr_addr, v);
            } else {
                let v = svmb_rdmsr(msr_addr) & !(1 << msr::EFER_SVME);
                put(regs, v);
            }
            return true;
        }
        // 其余被 MSRPM 拦截的 MSR:透明直通
        if is_write {
            svmb_wrmsr(msr_addr, value(regs));
        } else {
            put(regs, svmb_rdmsr(msr_addr));
        }
        true
    }
}

/// r114 函数级二分开关(MINIMAL_EXIT 三元化):
/// LEVEL-0(全 false)= r114 实测 T+390s/4248 exits 存活(历史崩窗 10-30s)
/// ⇒ 崩因在 exit 处理内容。逐级开回定位:
/// r114a: 只开普通 CPUID(执行+写回+隐藏位);r114b: 加超级调用;+MSR。
#[allow(dead_code)] // 二分资产:exit 拦截开关组之一(现由 hypervisor 侧 CPUID_ONLY/MSR_ONLY 接管)
const ENABLE_CPUID_EXEC: bool = true;
/// r115(2026-10-08):对齐 C 版稳定形态的机器行为 —— CrumbPost 信标群
/// (r107 freeze flight recorder)+ asm r113 bank-everything。C 版 Debug
/// 形态每 exit 真实调用这些;Rust 版此前整体省略,是"每轮恒在"的最大
/// 机器行为差异(崩点 3s~22min 宽分布下,单轮 A/B 均不可信,先对齐形态)。
const ENABLE_CRUMBS: bool = false; // r143a:反向二分首刀(r140 全 handler=冻结,r142 空=bugcheck)

/// r142 判决格:true = 处理器内容清零(只前进 rip)。r141 已证零拦截存活,
/// 凶手在拦截/exit 路径;本格分离裸循环 vs 处理器内容。
const ENABLE_HANDLER_ADVANCE_ONLY: bool = false; // r142 判决:空 handler=bugcheck 非冻结
const ENABLE_HIDE_BITS: bool = false;
const ENABLE_HIDE_SVM: bool = false; // 0x80000001.ECX[2]
const ENABLE_HIDE_NPT: bool = false; // 0x8000000A.EDX[0]
const ENABLE_POISON_RBX: bool = false;
const ENABLE_TOY_WRITE: bool = false;
static mut TOY_COUNTER: u64 = 0;
const ENABLE_HYPERCALL: bool = true;
const ENABLE_MSR: bool = true;

// ---- CrumbPostD 复刻(C 版 core/crumbs.cpp;ring 语义 1:1)----
const CRUMBS_MAGIC0: i32 = 0x4356_4253; // 'SBVC'
const CRUMBS_MAX: usize = 128;

#[repr(C)]
#[derive(Clone, Copy)]
struct Crumb {
    tag: i32,
    cpu: i32,
    tick: i64,
    seq: i64,
    data: i64,
}

#[repr(C)]
struct CrumbRing {
    magic0: i32,
    magic1: i32,
    w: core::sync::atomic::AtomicI32,
    _pad: i32,
    dropped: i32,
    _pad2: i32,
    e: [Crumb; CRUMBS_MAX],
}

// SAFETY: 全零 + magic 就位的 ring 可安全作 static(写入者只碰自己的槽字段)
static mut CRUMB_RING: CrumbRing = CrumbRing {
    magic0: CRUMBS_MAGIC0,
    magic1: 0x3130_3752, // 'r701'(C 版 MAGIC1)
    w: core::sync::atomic::AtomicI32::new(0),
    _pad: 0,
    dropped: 0,
    _pad2: 0,
    e: [Crumb { tag: 0, cpu: 0, tick: 0, seq: 0, data: 0 }; CRUMBS_MAX],
};

#[inline]
fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    // SAFETY: rdtsc 无副作用,任意上下文可执行
    unsafe { core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack)) };
    ((hi as u64) << 32) | lo as u64
}

/// C 版 CrumbPostD(tag, data)同源:原子取号→写槽→Seq 最后 commit
///
/// # Safety
/// 任意 IRQL;无锁
#[unsafe(export_name = "svmb_crumb_post_d")]
pub unsafe extern "C" fn svmb_crumb_post_d(tag: i32, data: u64) {
    unsafe {
        let r = &raw mut CRUMB_RING;
        if (*r).magic0 != CRUMBS_MAGIC0 {
            return;
        }
        let seq = (*r).w.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let c = &mut (*r).e[(seq as usize) & (CRUMBS_MAX - 1)];
        c.tag = tag;
        c.cpu = 0; // x64 ntoskrnl.lib 无 KeGetCurrentProcessorNumber 导出,信标字段简化
        c.tick = rdtsc() as i64; // C 版用 KeQueryInterruptTime;单调性等价
        c.data = data as i64;
        c.seq = seq as i64 + 1; // commit last(解码序键)
    }
}

#[inline]
unsafe fn crumb_post(tag: i32) {
    unsafe { svmb_crumb_post_d(tag, 0) };
}

/// exit 主入口(asm 循环调用;运行在 GIF 关闭环境,不得阻塞)
///
/// # Safety
/// 由 asm 循环在 VMEXIT 后调用;vcpu/regs 均有效;返回后 asm 恢复寄存器
#[unsafe(export_name = "svmb_vm_exit_entry")]
pub unsafe extern "C" fn svmb_vm_exit_entry(vcpu: *mut VcpuContext, regs: *mut GuestRegs) {
    unsafe {
        let vmcb = vmcb_of(vcpu);
        if ENABLE_CRUMBS {
            crumb_post(0x185); // CRUMB_EXIT_ENTRY(C 版 SvmbVmExitEntry 首行)
        }
        let code = vmcb.get_u64(ctrl_off::EXIT_CODE);
        let info1 = vmcb.get_u64(ctrl_off::EXIT_INFO1);
        let nrip = vmcb.get_u64(ctrl_off::NRIP);

        // 从 VMCB 保存区补齐 rax/rsp/rip/rflags(其余 GPRs 已由 asm 备份)
        (*regs).rip = vmcb.get_u64(save_off::RIP);
        (*regs).rsp = vmcb.get_u64(save_off::RSP);
        (*regs).rax = vmcb.get_u64(save_off::RAX);
        (*regs).rflags = vmcb.get_u64(save_off::RFLAGS);

        // asm 契约:每次 exit 都清 —— 否则陈旧 phase-1 恢复 rip 会在首个
        // exit 误触发 devirt(ret 到空 rip)
        (*regs).extra1 = 0;
        (*regs).extra2 = 0;

        let mut advance = !is_fault_class_exit(code);

        {
            match code {
                exit::VMEXIT_CPUID => {
                    advance = handle_cpuid(vcpu, regs);
                }
                exit::VMEXIT_MSR if ENABLE_MSR => {
                    advance = handle_msr(vcpu, regs, info1);
                }
                exit::VMEXIT_VMRUN => {
                    // 防嵌套:注入 #BP(软件中断语义)并前进 —— 不前进会永远
                    // 重执行 VMRUN
                    inject_event(vcpu, 3 /*#BP*/, 3 /*SOFTINT*/, false, 0);
                    advance = true;
                }
                exit::VMEXIT_NPF => {
                    trace_exit(vcpu, code, (*regs).rip, nrip);
                    advance = false;
                }
                exit::VMEXIT_SHUTDOWN => {
                    // r127:同 handle_exit —— shutdown exit 必须自首,禁止重入
                    if ENABLE_CRUMBS {
                        svmb_crumb_post_d(0x195, (*regs).rip);
                    }
                    wdk_sys::ntddk::KeBugCheckEx(
                        0xDEAD_DEAD,
                        0x7F,
                        (*regs).rip,
                        nrip,
                        info1,
                    );
                }
                other => {
                    trace_exit(vcpu, other, (*regs).rip, nrip);
                }
            }
        }

        // TF 修复(C 版 ApplyTfFixup 的前提 —— 骨架不拦截 #DB/不注入事件
        // 之外的路径,保持空)

        // rip 前进:注入后也必须前进,否则停在未执行的指令上死循环
        // (C 版 AdvanceGuestRip:写进 regs.rip,随后的统一写回落盘)
        if advance {
            (*regs).rip = nrip;
        }

        // 写回寄存器镜像
        vmcb.set_u64(save_off::RIP, (*regs).rip);
        vmcb.set_u64(save_off::RSP, (*regs).rsp);
        vmcb.set_u64(save_off::RAX, (*regs).rax);
        vmcb.set_u64(save_off::RFLAGS, (*regs).rflags);

        (*vcpu).info.exit_count = (*vcpu).info.exit_count.wrapping_add(1);

        // r45 exit 直方图:归属核独写自己的槽(单写者,无原子)
        let reason = code;
        let slot = if reason < 0x100 {
            reason as usize
        } else if reason == 0x400 {
            exit::SVMB_XP_NPF
        } else {            exit::SVMB_XP_OTHER
        };
        (*vcpu).info.exit_hist[slot] = (*vcpu).info.exit_hist[slot].wrapping_add(1);

        // round-21(C 版):exit 处理与 VMRUN 重入之间垫 pacing —— 参考实现在
        // 此间隔有一段 ~0.5µs 的比较逻辑,而"立即重入"与该 vhv 的延迟硬
        // 复位相关。r116(2026-10-08)C 侧反向二分实锤统一模型:C Debug(O0
        // 慢代码=天然大 pacing)稳 15min+,C Release/Debug+O2(快)30s 崩,
        // Rust(快)同崩 —— pacing 回 2µs(r116b 实证时长无关)
        // SAFETY: 任意 IRQL 可调用
        unsafe {
            wdk_sys::ntddk::KeStallExecutionProcessor(2);
        }
        if ENABLE_CRUMBS {
            crumb_post(0x183); // CRUMB_EXIT_EPILOGUE(即将返回 asm 循环)
        }
    }
}

/// r151:asm 主循环调用的 C ABI 包装(eax: 1 = 请求离开虚拟化)
#[unsafe(export_name = "svmb_handle_exit_c")]
pub unsafe extern "C" fn handle_exit_c(
    vcpu: *mut VcpuContext,
    regs: *mut GuestRegs,
) -> i32 {
    unsafe {
        if handle_exit(vcpu, regs) {
            1
        } else {
            0
        }
    }
}

// ---- r152 C-obj 混链桥:svm_entry.obj(ml64)的三个外部符号 ----
// 契约见 svmb/driver/src/hw/asm/svm_entry.asm;VcpuContext/GuestRegs 布局
// 与 offsets.inc 逐项同源(guest_vmcb@0/info@0x3000/regs@0x4000,
// GREG_RAX@0x170..EXTRA2@0x198),vcpu_state::LEAVING=3 同值。

/// C asm 每圈 vmrun-pre 信标(0x184,携带 Ctrl.CleanBits)
#[unsafe(export_name = "SvmbCrumbPostD")]
pub unsafe extern "C" fn svmb_crumb_postd_c(tag: i32, data: u64) {
    unsafe { svmb_crumb_post_d(tag, data) }
}

/// C asm 的 WinDbg 栈走查机器帧(与既有 svmb_fill_machine_frame 同体,
/// 仅为满足 C 符号名)
#[unsafe(export_name = "SvmbFillMachineFrame")]
pub unsafe extern "C" fn svmb_fill_machine_frame_c(frame: *mut u64, vcpu: *mut VcpuContext) {
    unsafe {
        let vmcb = vmcb_of(vcpu);
        *frame.add(0) = vmcb.get_u64(ctrl_off::NRIP);
        *frame.add(1) = vmcb.get_u16(save_off::CS) as u64;
        *frame.add(2) = vmcb.get_u64(save_off::RFLAGS) & 0xFFFF_FFFF;
        *frame.add(3) = vmcb.get_u64(save_off::RSP);
        *frame.add(4) = vmcb.get_u16(save_off::SS) as u64;
    }
}

/// C asm 的 exit shim:C 版 SvmbVmExitEntry 语义逐条对齐 ——
/// VMCB save 4 字段→镜像、清 Extra1/2、分发、镜像→VMCB、Extra1 devirt 契约
#[unsafe(export_name = "SvmbVmExitEntry")]
pub unsafe extern "C" fn svmb_vm_exit_entry_c(vcpu: *mut VcpuContext, regs: *mut GuestRegs) {
    unsafe {
        let vmcb = vmcb_of(vcpu);
        (*regs).rip = vmcb.get_u64(save_off::RIP);
        (*regs).rsp = vmcb.get_u64(save_off::RSP);
        (*regs).rax = vmcb.get_u64(save_off::RAX);
        (*regs).rflags = vmcb.get_u64(save_off::RFLAGS);
        (*regs).extra1 = 0;
        (*regs).extra2 = 0;

        // r152 验收修正:不在 shim 里置 Extra1/Extra2 —— C asm 的
        // State==Leaving 自退出机器(内核 rip 门/vmload guest/清 SVME/
        // V_TPR->CR8/State=Off)才是所有 leaving 流程(stop 与超级调用
        // EXIT_VMM 同一状态机)的正确出口;shim 劫持会掉 State=Off、
        // 0x0A 用户 rip 恢复窗口、陈旧 KERNEL_GS_BASE 三连坑。
        // LEAVING 态也必须照常服务 exit(C dispatcher 语义):分发内核
        // 无条件执行,leaving 只由 asm 侧消费。
        dispatch_exit(vcpu, regs);

        vmcb.set_u64(save_off::RIP, (*regs).rip);
        vmcb.set_u64(save_off::RSP, (*regs).rsp);
        vmcb.set_u64(save_off::RAX, (*regs).rax);
        vmcb.set_u64(save_off::RFLAGS, (*regs).rflags);
    }
}

/// r124 返回式架构的 exit 处理(普通 Rust 栈,主循环每圈调用)。
/// 返回 true = 请求退出虚拟化循环(LEAVING/超级调用 EXIT_VMM)。
///
/// # Safety
/// vcpu/regs 有效;GIF=0(vmrun 刚返回),不得阻塞/拿锁
pub unsafe fn handle_exit(vcpu: *mut VcpuContext, regs: *mut GuestRegs) -> bool {
    unsafe {
        // Stop() 请求本核退出(IPI 的 CPUID exit 会落到这里)。
        // r151 Rust 循环路径专用:C 混链桥(SvmbVmExitEntry)走 dispatch_exit
        // ——LEAVING 态也要服务 exit,leaving 由 asm 自退出机器消费。
        if (*vcpu).info.state.load(core::sync::atomic::Ordering::Acquire) == vcpu_state::LEAVING {
            return true;
        }
        dispatch_exit(vcpu, regs);
        (*vcpu).info.state.load(core::sync::atomic::Ordering::Acquire) == vcpu_state::LEAVING
    }
}

/// exit 分发内核(C dispatcher 语义):不管 LEAVING 态,无条件服务当前
/// exit(crumbs/match/rip 前进/计数)。C 混链桥每圈调用;leaving 信号由
/// asm 侧读 VcpuInfo.state 自行消费。
///
/// # Safety
/// vcpu/regs 有效;GIF=0(vmrun 刚返回),不得阻塞/拿锁
pub unsafe fn dispatch_exit(vcpu: *mut VcpuContext, regs: *mut GuestRegs) {
    unsafe {
        let vmcb = vmcb_of(vcpu);

        // r142 判决格:拦截开 + 处理器内容清零 —— 只读 nrip 前进,无
        // crumbs/无 KeStall/无 CPUID/MSR 处理/无计数。exit 流量真实。
        // 存活(或垃圾 CPUID 缓死)⇒ 凶手在处理器内容;~3s 冻结与 r140
        // 同形 ⇒ 裸 exit→re-VMRUN 循环本身有罪
        if ENABLE_HANDLER_ADVANCE_ONLY {
            let nrip = vmcb.get_u64(ctrl_off::NRIP);
            if nrip != 0 {
                (*regs).rip = nrip;
            }
            return;
        }

        if ENABLE_CRUMBS {
            crumb_post(0x185);
        }
        let code = vmcb.get_u64(ctrl_off::EXIT_CODE);
        let info1 = vmcb.get_u64(ctrl_off::EXIT_INFO1);
        let nrip = vmcb.get_u64(ctrl_off::NRIP);

        // rax/rsp/rip/rflags 已由主循环从 VMCB 补进镜像
        let mut advance = !is_fault_class_exit(code);
        let mut devirt = false;

        {
            match code {
                exit::VMEXIT_CPUID => {
                    advance = handle_cpuid(vcpu, regs);
                }
                exit::VMEXIT_MSR if ENABLE_MSR => {
                    advance = handle_msr(vcpu, regs, info1);
                }
                exit::VMEXIT_VMRUN => {
                    inject_event(vcpu, 3, 3, false, 0);
                    advance = true;
                }
                exit::VMEXIT_NPF => {
                    trace_exit(vcpu, code, (*regs).rip, nrip);
                    advance = false;
                }
                exit::VMEXIT_SHUTDOWN => {
                    // r127:guest triple fault 在 SVM 下以 shutdown exit 送达。
                    // 旧路径落入 other 分支后带着 shutdown 态 VMCB 重入 VMRUN
                    // = 未定义行为 → 物理 triple fault → VMware 复位、证据
                    // 灭失(r126b:fire→复位 ≤2s,KDNET 心跳仲裁)。改为临死
                    // 自首:最后一条 crumb + MANUALLY_INITIATED_CRASH 携带
                    // 现场(kd 实时打印;dump 开启时 ring/VMCB 全落盘)。
                    if ENABLE_CRUMBS {
                        svmb_crumb_post_d(0x195, (*regs).rip);
                    }
                    wdk_sys::ntddk::KeBugCheckEx(
                        0xDEAD_DEAD,
                        0x7F,
                        (*regs).rip,
                        nrip,
                        info1,
                    );
                }
                other => {
                    trace_exit(vcpu, other, (*regs).rip, nrip);
                    // 未知拦截类:透明前进;fault 类:重执行
                }
            }
        }
        let _ = &mut advance;

        if advance {
            (*regs).rip = nrip;
        }

        (*vcpu).info.exit_count = (*vcpu).info.exit_count.wrapping_add(1);
        let slot = if code < 0x100 {
            code as usize
        } else if code == 0x400 {
            exit::SVMB_XP_NPF
        } else {
            exit::SVMB_XP_OTHER
        };
        (*vcpu).info.exit_hist[slot] = (*vcpu).info.exit_hist[slot].wrapping_add(1);

        if ENABLE_CRUMBS {
            crumb_post(0x183);
        }
        let _ = devirt;
    }
}
///
/// # Safety
/// 由 asm 在 exit 后调用;frame 指向栈上 0x28 字节 scratch,vcpu 有效
#[unsafe(export_name = "svmb_fill_machine_frame")]
pub unsafe extern "C" fn svmb_fill_machine_frame(frame: *mut u64, vcpu: *mut VcpuContext) {
    unsafe {
        let vmcb = vmcb_of(vcpu);
        // MachineFrame { Rip, Cs, EFlags, OldRsp, Ss }
        *frame.add(0) = vmcb.get_u64(ctrl_off::NRIP);
        *frame.add(1) = vmcb.get_u16(save_off::CS) as u64;
        *frame.add(2) = vmcb.get_u64(save_off::RFLAGS) & 0xFFFF_FFFF;
        *frame.add(3) = vmcb.get_u64(save_off::RSP);
        *frame.add(4) = vmcb.get_u16(save_off::SS) as u64;
    }
}

// 引用防未用警告(基线未用到的常量仍属布局文档的一部分)
const _: () = {
    assert!(ic1::CPUID != 0);
    assert!(ic2::VMRUN != 0);
    assert!(vmrun_fail::INVALID == u64::MAX);
};
