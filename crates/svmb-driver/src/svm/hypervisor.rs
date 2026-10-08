#![allow(dead_code)]
//! Hypervisor 起停状态机 —— C 版 core/hypervisor.cpp Start/Stop/EnterCore/
//! ConfigureVmcb/FillGuestState 的移植。
//!
//! 生命周期(与 C 版逐一对应,顺序不可调换 —— 每一步的次序都有实测依据):
//! 1. CheckCpu:AMD + SVM 位 + VM_CR.SVMDIS=0 + NPT/NRIPS
//! 2. MSRPM/IOPM/每核上下文/VMCB/HSAVE 分配(PASSIVE,先于任何 IPI)
//! 3. NPT 默认视图(1TB 恒等 RWX 大页;该 vhv 上不开 NPT 会触发 guest reset)
//! 4. RunOnEachCore(线程组亲和逐核):EnterCore —— 两阶段寄存器保存后
//!    VMRUN 自虚拟化;线程作为 guest 从 phase-2 重入点透明继续
//! 5. Stop:每核标 Leaving → IPI(CPUID 触发 exit)→ 各核在下一个自然
//!    exit 就地退出虚拟化(r28 方案)→ 轮询 → 泄漏 per-core 页(r112 纪律:
//!    陈旧 PTE 引用的连续页释放回池会 BSOD)

use alloc::boxed::Box;
use core::arch::x86_64::__cpuid_count;
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

use wdk_sys::{
    ntddk::{
        KeDelayExecutionThread, KeGetProcessorNumberFromIndex, KeIpiGenericCall,
        KeQueryActiveProcessorCountEx, KeRevertToUserGroupAffinityThread,
        KeSetSystemGroupAffinityThread,
    },
    GROUP_AFFINITY, LARGE_INTEGER, PROCESSOR_NUMBER,
};

use super::asm::{
    svmb_read_cr0, svmb_read_cr2, svmb_read_cr3, svmb_read_cr4, svmb_read_cs, svmb_read_ds,
    svmb_read_es, svmb_read_fs, svmb_read_gs, svmb_read_ss, svmb_rdmsr,
    svmb_sidt, svmb_sgdt, svmb_sldt, svmb_str, svmb_vmm_loop_c, svmb_wrmsr,
    svmb_capture_regs, svmb_restore_context};
// r151 后退役的 asm 路径(保留本体供二分回退):
// - svmb_vmm_loop: 前返回式架构的旧循环(r124 前)
// - svmb_run_guest: r124 返回式单圈(r148 舞步修正版;现由 svmb_vmm_loop_c 取代)
// - svmb_save_or_load_regs: 两阶段重入 hack(r124 淘汰)
#[allow(unused_imports)]
use super::asm::{svmb_vmm_loop, svmb_run_guest, svmb_save_or_load_regs};
use super::msrpm::{Iopm, Msrpm};
use super::msr;
use super::npt::NptView;
use super::percpu::{vcpu_state, VcpuContext, BLOB_ALLOC_SIZE};
use super::segments;
use super::vmcb::{ctrl_off, ic1, ic2, save_off, VMCB_SAVE_AREA_OFFSET};
use crate::phys::{alloc_contiguous, alloc_pool};

// ---- NTSTATUS(骨架自持的最小集合)----
pub mod st {
    pub const SUCCESS: i32 = 0;
    pub const UNSUCCESSFUL: i32 = 0xC000_0001u32 as i32;
    pub const INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    pub const INSUFFICIENT_RESOURCES: i32 = 0xC000_009Au32 as i32;
    pub const ALREADY_REGISTERED: i32 = 0xC000_0719u32 as i32;
    pub const HV_FEATURE_UNAVAILABLE: i32 = 0xC035_1000u32 as i32;
    pub const HV_OPERATION_FAILED: i32 = 0xC035_1001u32 as i32;
}

// hypervisor 总状态(off/starting/running)
const HV_OFF: u32 = 0;
const HV_STARTING: u32 = 1;
const HV_RUNNING: u32 = 2;

static HV_STATE: AtomicU32 = AtomicU32::new(HV_OFF);
static CORE_COUNT: AtomicU32 = AtomicU32::new(0);
static VCPUS: [AtomicPtr<VcpuContext>; MAX_CORES] =
    [const { AtomicPtr::new(core::ptr::null_mut()) }; MAX_CORES];
static MSRPM: AtomicPtr<Msrpm> = AtomicPtr::new(core::ptr::null_mut());
static IOPM: AtomicPtr<Iopm> = AtomicPtr::new(core::ptr::null_mut());
static NPT: AtomicPtr<NptView> = AtomicPtr::new(core::ptr::null_mut());

const MAX_CORES: usize = 64;

/// VMCB 保存区里的段寄存器写入(selector/attr/limit/base = 2+2+4+8)
///
/// # Safety
/// off 必须是合法的段槽偏移
unsafe fn write_seg(vmcb: *mut super::vmcb::Vmcb, off: usize, selector: u16, attribute: u16, limit: u32, base: u64) {
    unsafe {
        (*vmcb).set_u16(off, selector);
        (*vmcb).set_u16(off + 2, attribute);
        (*vmcb).set_u32(off + 4, limit);
        (*vmcb).set_u64(off + 8, base);
    }
}

/// 每核 VMRUN 前的 SVM 健全门 + EnterCore(C 版 EnterCore 语义)
///
/// # Safety
/// 仅在 run_on_each_core 的目标核上下文调用(PASSIVE)
unsafe fn enter_core(cpu_idx: u32) -> i32 {
    unsafe {
        let vcpu = VCPUS[cpu_idx as usize].load(Ordering::Acquire);
        if vcpu.is_null() {
            return st::INVALID_PARAMETER;
        }

        // 单核限制实验:超过限额的核直接跳过(walk 仍完整走完)
        if cpu_idx as usize >= ENTER_CORE_LIMIT {
            return st::SUCCESS;
        }

        // r132 二分:纯 walk 编排对照 —— enter 什么都不做(capture/init/
        // teardown/restore 全跳过)。存活 ⇒ 冻结凶手=walk 跨核编排;死 ⇒
        // 凶手在 capture/restore/init(r131 已排除 VMRUN 与 #UD)
        if !ENABLE_ENTER_BODY {
            return st::SUCCESS;
        }

        // per-core SVM 门:CheckCpu 在 IOCTL 线程跑,VMRUN 在本核打 ——
        // 在即将 vmrun 的核上复测,失败就干净退出
        let ext = __cpuid_count(0x8000_0001, 0);
        let vmcr = svmb_rdmsr(msr::MSR_VM_CR);
        if (ext.ecx >> msr::SVM_CPUID_BIT) & 1 == 0 || (vmcr >> msr::VM_CR_SVMDIS) & 1 != 0 {
            crate::println!("[svmb-rust] per-core svm gate FAILED cpu={cpu_idx} - skipping vmrun");
            return st::HV_FEATURE_UNAVAILABLE;
        }

        // r124 返回式架构(barevisor 路线):capture 当前线程上下文(真实
        // 返回点/rsp),guest 从 capture 返回点继续;每圈 VMRUN 返回后在本
        // 线程普通栈上处理 exit,再重入。无 VMM 栈、无两阶段重入 hack。
        let regs = core::ptr::addr_of_mut!((*vcpu).regs);
        unsafe { svmb_capture_regs(regs) };

        // 两种落到这里的路径:①guest 第一圈(state 已 ENTERING,CAS 失败
        // 早退,线程回系统);②退出虚拟化后 restore 回来(resumed=1,直接
        // 正常返回);③真正的首次 enter(CAS 成功→init→主循环)。
        if (*vcpu).info.resumed != 0 {
            // 路径②:本线程刚退出虚拟化(restore_context 已 STGI 并跳回)。
            // r130 根因修复:EFER.SVME 只能在这里清 —— SVME=0 时 STGI 是
            // #UD(r129 minidump 实锤:0x3B/0xC000001D @ svmb_rust+0xba11
            // = restore_context 首指令 stgi)。此前 teardown 在 restore 前
            // 清 SVME ⇒ 每次退出虚拟化必 #UD:干净上下文=bugcheck,虚拟
            // 化上下文=IPI 打进 VMRUN 中的核 ⇒ triple fault/冻结(随机)。
            svmb_wrmsr(
                msr::MSR_EFER,
                svmb_rdmsr(msr::MSR_EFER) & !(1 << msr::EFER_SVME),
            );
            return st::SUCCESS;
        }
        // 每核只 enter 一次:CAS Off→Entering(原子,对齐 C 版语义)
        if (*vcpu)
            .info
            .state
            .compare_exchange(
                vcpu_state::OFF,
                vcpu_state::ENTERING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return st::SUCCESS; // 路径①:guest 第一圈或重入
        }

        configure_vmcb(vcpu);

        // 2026-09-07 实测:必须先开 host SVME 再写 VM_HSAVE_PA —— native 上
        // SVME=0 时 wrmsr HSAVE 会 #GP,VMware L1 吞掉该 #GP 不转发
        let efer = svmb_rdmsr(msr::MSR_EFER);
        svmb_wrmsr(msr::MSR_EFER, efer | (1 << msr::EFER_SVME));
        let hsave_pa = crate::phys::virtual_to_physical((*vcpu).hsave as *const _);
        svmb_wrmsr(msr::MSR_VM_HSAVE_PA, hsave_pa);

        fill_guest_state(vcpu);
        // VMRUN 有效性前提:guest EFER.SVME 必须为 1
        let g_efer = (*(*vcpu).guest_vmcb).get_u64(save_off::EFER);
        (*(*vcpu).guest_vmcb).set_u64(save_off::EFER, g_efer | (1 << msr::EFER_SVME));

        // host 状态 = guest 快照(薄 hypervisor:共享 CR3/IDT/GDT)
        // SAFETY: 两个 VMCB 都是 4KB 页;拷贝保存区 [0x400,0x1000)
        core::ptr::copy_nonoverlapping(
            (*(*vcpu).guest_vmcb).as_ptr().add(VMCB_SAVE_AREA_OFFSET),
            (*(*vcpu).host_vmcb).as_mut_ptr().add(VMCB_SAVE_AREA_OFFSET),
            4096 - VMCB_SAVE_AREA_OFFSET,
        );

        let guest_pa = crate::phys::virtual_to_physical((*(*vcpu).guest_vmcb).as_ptr() as *const _);
        let host_pa = crate::phys::virtual_to_physical((*(*vcpu).host_vmcb).as_ptr() as *const _);
        let stack_top =
            core::ptr::addr_of_mut!((*vcpu).vmm_stack).byte_add(super::percpu::VMM_STACK_SIZE) as *mut u8;
        (*vcpu).info.guest_vmcb_pa = guest_pa;
        (*vcpu).info.host_vmcb_pa = host_pa;
        (*vcpu).info.state.store(vcpu_state::GUEST, Ordering::Release);

        crate::println!(
            "[svmb-rust] vmrun cpu={cpu_idx} vmcb={guest_pa:#x} efer={:#x} ncr3={:#x}",
            svmb_rdmsr(msr::MSR_EFER),
            (*(*vcpu).guest_vmcb).get_u64(ctrl_off::NCR3)
        );

        // ---- r124 主循环:每圈 svmb_run_guest 返回后在普通栈处理 exit ----
        // r129 二分开关:ENABLE_VMRUN=false 走完 init(capture/CAS/configure/
        // SVME/HSAVE/fill/host 快照)后直接 teardown,不进入 VMRUN/exit 循环。
        // 存活 ⇒ 杀手在循环;死 ⇒ 杀手在 init 路径(r128b 已排除打印/kd)
        // r151:C 型主循环(结构移植)——单次调用,成功不返回;
        // 返回值:0 = leaving 请求(走下方 teardown/restore),
        // 非 0 = VMRUN 失败码(哨兵路径,记 0x190 信标)
        if ENABLE_VMRUN {
            if ENABLE_CLOOP {
                // C 循环仅在 VMRUN 失败时返回;返回即失败(哨兵记 0x190)
                unsafe { super::asm::c_vmm_loop(vcpu, guest_pa, host_pa, stack_top) };
                unsafe { crate::svm::exit_handler::svmb_crumb_post_d(0x190, 1) };
            } else {
                let reason =
                    unsafe { svmb_vmm_loop_c(vcpu, guest_pa, host_pa, stack_top) };
                if reason != 0 {
                    unsafe {
                        crate::svm::exit_handler::svmb_crumb_post_d(0x190, reason as u64)
                    };
                }
            }
        }

        // r133 二分:capture+CAS+init 全走,但**正常 Rust 返回**(不做
        // teardown store + restore_context 舞蹈)。存活 ⇒ 凶手=restore/
        // capture 对;死 ⇒ 凶手=init 本体。SVME 会残留置位(无害,仅门控
        // SVM 指令合法性)
        if !ENABLE_RESTORE_DANCE {
            return st::SUCCESS;
        }

        // 循环退出:状态回 Off、标记 resumed,原地恢复被打断的线程。
        // r130:SVME 保持置位 —— restore_context 首指令 stgi 在 SVME=0 时
        // 是 #UD;清除动作移到 resumed 分支(restore 跳回后的正常上下文)
        let st_now = (*vcpu).info.state.load(Ordering::Acquire);
        (*vcpu).info.state.store(vcpu_state::OFF, Ordering::Release);
        (*vcpu).info.resumed = 1;
        unsafe { crate::svm::exit_handler::svmb_crumb_post_d(0x191, st_now as u64) };
        // SAFETY: regs 由本函数开头 capture;restore 不返回
        unsafe { svmb_restore_context(regs) };
        // 不可达(restore 不返回);满足类型系统
        st::SUCCESS
    }
}

/// bisect 开关(2026-10-06 对照轮结论:op1=0 也复位 → exit 处理器无罪,
/// 损坏在结构层)。恢复完整拦截:CPUID|MSR 基线让 stop() 的 Leaving+IPI
/// 自退出机制可用(bypass 下无 exit,stop 必然失败)。
const INTERCEPT_BYPASS: bool = false; // r141 判决:零拦截存活=地基无罪;r142 拦截重开

/// r129 二分:false = init-only(capture→CAS→configure→SVME→HSAVE→fill→
/// host 快照后直接 teardown/restore,零 VMRUN)。r128b 已证明打印与 kd
/// 无关;本开关把"init 路径"与"VMRUN/exit 循环"切开。
/// r129 判决:init-only 也死(0x3B #UD @ restore_context 的 stgi)⇒ 循环
/// 无罪;r130 修复 SVME 清除时序后恢复 true 全功能验证。
const ENABLE_VMRUN: bool = true; // r135 判决:init-only+双修复=存活
/// r152 终局实验:链入 C 仓库 ml64 编译的 svm_entry.obj(_svmb_vmm_loop),
/// 物理隔离 LLVM vs MSVC 代码生成变量。true=C 循环;false=Rust 版。
const ENABLE_CLOOP: bool = true;
const ENABLE_ONE_SHOT: bool = false; // r147:stgi 修复后全功能
const ENABLE_ENTER_BODY: bool = true; // r132 判决:walk 无罪(存活)
const ENABLE_RESTORE_DANCE: bool = true; // r133 判决:restore 舞蹈是冻结凶手(r134 修压栈序)

/// 单核限制实验(2026-10-06 晚):KTIMER 链表损坏集中在 core-1 进入窗口。
/// 只虚拟化 [0, LIMIT) 号核:若单核长稳 → 确诊多核进入窗口竞争,再逐核
/// 加回定位到具体核;walk 仍遍历全部核(仅跳过 VMRUN)以验证 walk 完成。
/// 2026-10-06 深夜更新:单核也崩 → 多核理论否;真凶指向 NPT(C 活体
/// VMCB 显示 NP=0/NCr3=0/TLB=0 —— C 版在此 vhv 上默认无 NPT 裸跑)。
/// 本轮完全复刻 C 活体形态:关 NPT + 全核放开。
const ENTER_CORE_LIMIT: usize = 64; // r139 判决:单核也冻

/// VMCB 配置(基线 = C 版活体 VMCB 逐字段复刻,2026-10-06 kd dump:
/// OP1=0x10040000, OP2=1, ASID=1, NP=0, NCr3=0, TLB_CTL=0, IOPM=0,
/// MSRPM=实地址;无异常/CR/DR 拦截)
///
/// # Safety
/// vcpu 已初始化
unsafe fn configure_vmcb(vcpu: *mut VcpuContext) {
    unsafe {
        let vmcb = &mut *(*vcpu).guest_vmcb;
        vmcb.zeroed();

        // r144b/c 二分:单拦截开关 —— 哪个拦截开启才死,哪个就是风暴源
        const CPUID_ONLY: bool = false; // r144b 判决:CPUID-only 也死(=首 exit 即死)
        const MSR_ONLY: bool = false; // r144c:只拦 MSR
        if INTERCEPT_BYPASS {
            vmcb.set_u32(ctrl_off::INTERCEPT_OPCODE1, 0);
        } else if CPUID_ONLY {
            vmcb.set_u32(ctrl_off::INTERCEPT_OPCODE1, ic1::CPUID);
        } else if MSR_ONLY {
            vmcb.set_u32(ctrl_off::INTERCEPT_OPCODE1, ic1::MSR);
        } else {
            vmcb.set_u32(ctrl_off::INTERCEPT_OPCODE1, ic1::CPUID | ic1::MSR);
        }
        vmcb.set_u32(ctrl_off::INTERCEPT_OPCODE2, ic2::VMRUN);
        // 异常/CR/DR 拦截:全零(r13 教训:强制 #DE/#BP/#UD 会让该 vhv
        // 在 "Invalid VMCB" 上 panic)

        vmcb.set_u32(ctrl_off::GUEST_ASID, 1);
        // C 活体:TLB_CTL=0(NPT 关闭形态;flush-all 定律属 NPT-on 语境)
        vmcb.set_u8(ctrl_off::TLB_CONTROL, 0);

        // NPT 本轮关闭:C 版在此 vhv 上的稳定形态就是 NP=0/NCr3=0
        // (NptEnable 注册表默认 0)。此前"必须开 NPT"的注释源自 C 版
        // round-10 过时史料;我的全部崩溃轮次都开着自建 NPT —— 本轮
        // 关闭以隔离。NPT 视图仍构建(为后续 NPT-on 课题保留),仅不发布。
        vmcb.set_u64(ctrl_off::NP_ENABLE, 0);
        vmcb.set_u64(ctrl_off::NCR3, 0);

        let msrpm = MSRPM.load(Ordering::Acquire);
        vmcb.set_u64(ctrl_off::MSRPM_BASE_PA, (*msrpm).physical());
        // C 版对齐:IOPM base 保持 0(C 的 ApplyToVcpu 只写 MsrpmBasePa)。
        // IOPM 结构仍分配(位图全 0=全放行),仅不向 VMCB 发布 —— A/B 轮
        // (2026-10-06)排查 KTIMER 链表损坏是否源于该字段的形状差异
        vmcb.set_u64(ctrl_off::IOPM_BASE_PA, 0);

        vmcb.invalidate_clean_bits();
    }
}

/// guest 状态填充(C 版 FillGuestState;r18 参考全对齐:数据段 limit=0、
/// DR6/DR7=0,TR/LDTR 用 0xFFFFFFFF)
///
/// # Safety
/// vcpu 已初始化;在 host SVME=1 后调用(其 MSR 读不在 MSRPM 拦截内)
unsafe fn fill_guest_state(vcpu: *mut VcpuContext) {
    unsafe {
        let vmcb = &mut *(*vcpu).guest_vmcb;
        let (mut gdt_base, mut gdt_limit) = (0u64, 0u16);
        let (mut idt_base, mut idt_limit) = (0u64, 0u16);
        let (mut tr_sel, mut ldtr_sel) = (0u16, 0u16);
        svmb_sgdt(core::ptr::addr_of_mut!(gdt_base), core::ptr::addr_of_mut!(gdt_limit));
        svmb_sidt(core::ptr::addr_of_mut!(idt_base), core::ptr::addr_of_mut!(idt_limit));
        svmb_str(core::ptr::addr_of_mut!(tr_sel));
        svmb_sldt(core::ptr::addr_of_mut!(ldtr_sel));

        // GDTR/IDTR:selector=0, attribute=0(零初始化),真实 base/limit
        write_seg(vmcb, save_off::GDTR, 0, 0, gdt_limit as u32, gdt_base);
        write_seg(vmcb, save_off::IDTR, 0, 0, idt_limit as u32, idt_base);

        // 数据段:attribute 从 GDT 取,limit 强制 0(r18:该 vhv 上非零
        // limit 与延迟硬复位 1:1 相关;long mode 硬件忽略 limit)
        let cs_sel = svmb_read_cs();
        let jobs: [(usize, u16); 8] = [
            (save_off::ES, svmb_read_es()),
            (save_off::CS, cs_sel),
            (save_off::SS, svmb_read_ss()),
            (save_off::DS, svmb_read_ds()),
            (save_off::FS, svmb_read_fs()),
            (save_off::GS, svmb_read_gs()),
            (save_off::LDTR, ldtr_sel),
            (save_off::TR, tr_sel),
        ];
        for (off, sel) in jobs {
            let attr = segments::segment_attribute(sel, gdt_base);
            let base = match off {
                save_off::FS => svmb_rdmsr(msr::MSR_FS_BASE),
                save_off::GS => svmb_rdmsr(msr::MSR_GS_BASE),
                _ => segments::segment_base(sel, gdt_base),
            };
            let limit = match off {
                save_off::LDTR | save_off::TR => segments::VMCB_LIMIT,
                _ => 0,
            };
            write_seg(vmcb, off, sel, attr, limit, base);
        }

        vmcb.set_u64(save_off::EFER, svmb_rdmsr(msr::MSR_EFER));
        vmcb.set_u64(save_off::CR4, svmb_read_cr4());
        vmcb.set_u64(save_off::CR3, svmb_read_cr3());
        vmcb.set_u64(save_off::CR0, svmb_read_cr0());
        vmcb.set_u64(save_off::CR2, svmb_read_cr2());
        // DR6/DR7 保持 0(r18 参考对齐;零初始化 VMCB 已保证)
        vmcb.set_u64(save_off::GPAT, svmb_rdmsr(msr::MSR_PAT));

        vmcb.set_u64(save_off::STAR, svmb_rdmsr(msr::MSR_STAR));
        vmcb.set_u64(save_off::LSTAR, svmb_rdmsr(msr::MSR_LSTAR));
        vmcb.set_u64(save_off::CSTAR, svmb_rdmsr(msr::MSR_CSTAR));
        vmcb.set_u64(save_off::SFMASK, svmb_rdmsr(msr::MSR_SFMASK));
        vmcb.set_u64(save_off::KERNEL_GS_BASE, svmb_rdmsr(msr::MSR_KERNEL_GS_BASE));
        vmcb.set_u64(save_off::SYSENTER_CS, svmb_rdmsr(msr::MSR_SYSENTER_CS));
        vmcb.set_u64(save_off::SYSENTER_ESP, svmb_rdmsr(msr::MSR_SYSENTER_ESP));
        vmcb.set_u64(save_off::SYSENTER_EIP, svmb_rdmsr(msr::MSR_SYSENTER_EIP));

        // asm 契约:guest 以 rax = &Regs 进入 if_load_regs(恢复分支的钥匙);
        // rip = phase-1 埋的重入点
        let regs_base = core::ptr::addr_of_mut!((*vcpu).regs);
        vmcb.set_u64(save_off::RAX, regs_base as u64);
        vmcb.set_u64(save_off::RFLAGS, (*vcpu).regs.rflags);
        vmcb.set_u64(save_off::RSP, (*vcpu).regs.rsp);
        vmcb.set_u64(save_off::RIP, (*vcpu).regs.rip);
        vmcb.set_u8(save_off::CPL, (cs_sel & 3) as u8);
    }
}

/// SVM 可用性探测(C 版 CheckCpu:每个拒绝分支都给出具体原因)
fn check_cpu() -> Result<(), i32> {
    let vendor = __cpuid_count(0, 0);
    let mut v = [0u8; 12];
    v[0..4].copy_from_slice(&vendor.ebx.to_le_bytes());
    v[4..8].copy_from_slice(&vendor.edx.to_le_bytes());
    v[8..12].copy_from_slice(&vendor.ecx.to_le_bytes());
    if &v != b"AuthenticAMD" {
        crate::println!("[svmb-rust] svm check: not AMD cpu, vendor={:?}", core::str::from_utf8(&v));
        return Err(st::HV_FEATURE_UNAVAILABLE);
    }
    let ext = __cpuid_count(0x8000_0001, 0);
    if (ext.ecx >> msr::SVM_CPUID_BIT) & 1 == 0 {
        crate::println!("[svmb-rust] svm check: SVM CPUID bit is 0 - L1 or BIOS disabled SVM");
        return Err(st::HV_FEATURE_UNAVAILABLE);
    }
    let vmcr = unsafe { svmb_rdmsr(msr::MSR_VM_CR) };
    if (vmcr >> msr::VM_CR_SVMDIS) & 1 != 0 {
        crate::println!("[svmb-rust] svm check: VM_CR.SVMDIS=1 - SVM disabled by firmware");
        return Err(st::HV_FEATURE_UNAVAILABLE);
    }
    let feat = __cpuid_count(0x8000_000A, 0);
    if (feat.edx >> msr::NPT_CPUID_BIT) & 1 == 0 {
        crate::println!("[svmb-rust] svm check: NPT not supported (CPUID.8000000A.EDX={:#x})", feat.edx);
        return Err(st::HV_FEATURE_UNAVAILABLE);
    }
    if (feat.edx >> msr::NRIPS_CPUID_BIT) & 1 == 0 {
        crate::println!("[svmb-rust] svm check: NRIPS not supported (CPUID.8000000A.EDX={:#x})", feat.edx);
        return Err(st::HV_FEATURE_UNAVAILABLE);
    }
    crate::println!("[svmb-rust] svm check: OK (AMD, SVM=1, SVMDIS=0, NPT+NRIPS)");
    Ok(())
}

/// 逐核执行(线程组亲和,PASSIVE;C 版 RunOnEachCore 同源)
fn run_on_each_core(mut f: impl FnMut(u32) -> i32) -> i32 {
    let count = unsafe { KeQueryActiveProcessorCountEx(0xFFFF) };
    for i in 0..count {
        let mut num = PROCESSOR_NUMBER::default();
        // SAFETY: 输出指针有效
        let stt = unsafe { KeGetProcessorNumberFromIndex(i, &mut num) };
        if stt != 0 {
            return stt;
        }
        let mut aff = GROUP_AFFINITY {
            Mask: 1u64 << num.Number,
            Group: num.Group,
            Reserved: [0; 3],
        };
        let mut old = GROUP_AFFINITY::default();
        // SAFETY: 两个亲和结构均为有效栈对象
        unsafe {
            KeSetSystemGroupAffinityThread(&mut aff, &mut old);
            let stt = f(i);
            KeRevertToUserGroupAffinityThread(&mut old);
            if stt != 0 {
                return stt;
            }
        }
    }
    st::SUCCESS
}

/// IPI 广播回调:在每核 IPI_LEVEL 执行 CPUID 强制一次 exit,让 VMM 循环
/// 看到 Leaving 并就地退出虚拟化(C 版 IpiNudgeCallback + r28b:CPUID
/// 在基线拦截里,回调跑内核态 —— 正是 devirt 门等待的上下文)
unsafe extern "C" fn ipi_nudge(_ctx: u64) -> u64 {
    let _ = __cpuid_count(0, 0);
    0
}

/// 启动虚拟化(全核)
pub fn start() -> i32 {
    if HV_STATE
        .compare_exchange(HV_OFF, HV_STARTING, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return st::ALREADY_REGISTERED;
    }

    if let Err(e) = check_cpu() {
        stop();
        return e;
    }

    let core_count = unsafe { KeQueryActiveProcessorCountEx(0xFFFF) };
    if core_count as usize > MAX_CORES {
        stop();
        return st::INVALID_PARAMETER;
    }
    CORE_COUNT.store(core_count, Ordering::Release);

    // ---- 全局结构分配(PASSIVE,先于任何 IPI)----
    let msrpm = match Msrpm::new() {
        Some(m) => Box::leak(Box::new(m)),
        None => {
            stop();
            return st::INSUFFICIENT_RESOURCES;
        }
    };
    MSRPM.store(msrpm as *mut _, Ordering::Release);

    let iopm = match Iopm::new() {
        Some(i) => Box::leak(Box::new(i)),
        None => {
            stop();
            return st::INSUFFICIENT_RESOURCES;
        }
    };
    IOPM.store(iopm as *mut _, Ordering::Release);

    let npt = match NptView::build() {
        Some(v) => Box::leak(v), // Box<NptView> → &'static mut(r112 纪律:泄漏)
        None => {
            stop();
            return st::INSUFFICIENT_RESOURCES;
        }
    };
    NPT.store(npt as *mut _, Ordering::Release);

    // ---- 每核上下文(blob + GuestVmcb/HostVmcb/HSave 连续页)----
    for i in 0..core_count as usize {
        let raw = alloc_pool(BLOB_ALLOC_SIZE);
        if raw.is_null() {
            stop();
            return st::INSUFFICIENT_RESOURCES;
        }
        // SAFETY: raw 来自成功的池分配
        let vcpu = unsafe { VcpuContext::init(raw, i as u32) };
        let g = alloc_contiguous(4096) as *mut super::vmcb::Vmcb;
        let h = alloc_contiguous(4096) as *mut super::vmcb::Vmcb;
        let hs = alloc_contiguous(4096) as *mut u8;
        if g.is_null() || h.is_null() || hs.is_null() {
            stop();
            return st::INSUFFICIENT_RESOURCES;
        }
        // SAFETY: 均为成功的连续分配,PA 必须 4K 对齐
        unsafe {
            if crate::phys::virtual_to_physical(g as *const _) & 0xFFF != 0
                || crate::phys::virtual_to_physical(h as *const _) & 0xFFF != 0
                || crate::phys::virtual_to_physical(hs as *const _) & 0xFFF != 0
            {
                crate::println!("[svmb-rust] non-page-aligned contiguous PA cpu={i}");
                stop();
                return st::HV_OPERATION_FAILED;
            }
            (*vcpu).guest_vmcb = g;
            (*vcpu).host_vmcb = h;
            (*vcpu).hsave = hs;
        }
        VCPUS[i].store(vcpu, Ordering::Release);
    }

    // ---- 全核进入虚拟化 ----
    crate::println!("[svmb-rust] running enter on {core_count} cores");
    let stt = run_on_each_core(|i| unsafe { enter_core(i) });
    if stt != st::SUCCESS {
        crate::println!("[svmb-rust] enter virtualization failed on some core: {stt:#x}");
        stop();
        return stt;
    }

    HV_STATE.store(HV_RUNNING, Ordering::Release);
    crate::println!("[svmb-rust] virtualization active on {core_count} cores");
    st::SUCCESS
}

/// 停止虚拟化(C 版 Stop:self-exit 方案,永不提前 break,硬门后泄漏)
pub fn stop() -> i32 {
    let count = CORE_COUNT.load(Ordering::Acquire);
    if HV_STATE.load(Ordering::Acquire) == HV_OFF && count == 0 {
        return st::SUCCESS;
    }

    // 每个仍虚拟化的核标 Leaving;该核自己的 VMM 循环在下一个自然 exit
    // 就地退出虚拟化。绝不提前 break —— 一核拒绝不能把其余核拴在即将
    // 泄漏的内存上
    for i in 0..count as usize {
        let v = VCPUS[i].load(Ordering::Acquire);
        if !v.is_null() {
            // SAFETY: v 有效
            unsafe {
                let _ = (*v).info.state.compare_exchange(
                    vcpu_state::GUEST,
                    vcpu_state::LEAVING,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
            }
        }
    }

    // IPI 强制每核过一次 exit(空闲核没有可拦截流量,靠这个唤醒)
    unsafe {
        KeIpiGenericCall(Some(ipi_nudge as unsafe extern "C" fn(u64) -> u64), 0);
    }

    // 有界等待(r28:Leaving 的核在自己的下一个自然 exit 排水;空闲核
    // 最坏情形靠 IPI 唤醒,15s 预算兜底)
    for _ in 0..300 {
        let mut all_off = true;
        for i in 0..count as usize {
            let v = VCPUS[i].load(Ordering::Acquire);
            if !v.is_null() && unsafe { (*v).info.state.load(Ordering::Acquire) } != vcpu_state::OFF
            {
                all_off = false;
                break;
            }
        }
        if all_off {
            break;
        }
        // SAFETY: 栈上 LARGE_INTEGER,50ms 相对等待
        unsafe {
            let mut delay = LARGE_INTEGER::default();
            core::ptr::addr_of_mut!(delay.QuadPart).write_unaligned(-500_000);
            let _ = KeDelayExecutionThread(0 /*KernelMode*/, 0, &mut delay);
        }
    }
    for i in 0..count as usize {
        let v = VCPUS[i].load(Ordering::Acquire);
        let st_now = if v.is_null() {
            -1
        } else {
            unsafe { (*v).info.state.load(Ordering::Acquire) }
        };
        crate::println!("[svmb-rust] stop: core {i} self-exit state={st_now}");
    }

    // 硬门:任何核还在 SVM 就绝不泄指针槽(保持可重试 Stop 的余地)
    for i in 0..count as usize {
        let v = VCPUS[i].load(Ordering::Acquire);
        if !v.is_null() && unsafe { (*v).info.state.load(Ordering::Acquire) } != vcpu_state::OFF {
            crate::println!("[svmb-rust] stop aborted: core {i} still in SVM");
            return st::HV_OPERATION_FAILED;
        }
    }

    HV_STATE.store(HV_OFF, Ordering::Release);
    // r112 纪律:per-core 页/连续页不 free(陈旧 PTE 引用 + 池复用 = BSOD),
    // 指针槽清零即可;重 Start 会重新分配一套
    for i in 0..count as usize {
        VCPUS[i].store(core::ptr::null_mut(), Ordering::Release);
    }
    MSRPM.store(core::ptr::null_mut(), Ordering::Release);
    IOPM.store(core::ptr::null_mut(), Ordering::Release);
    NPT.store(core::ptr::null_mut(), Ordering::Release);
    CORE_COUNT.store(0, Ordering::Release);

    crate::println!("[svmb-rust] hypervisor stopped (vCPU pages leaked for safe teardown)");
    st::SUCCESS
}

/// 供 IOCTL/GET_INFO 读取的快照
pub mod info {
    use super::*;

    /// hv_state(SVMB_HV_STATE 语义)
    pub fn hv_state() -> u32 {
        match HV_STATE.load(Ordering::Acquire) {
            HV_OFF => 0,      // Off
            HV_STARTING => 1, // Starting
            _ => 2,           // Running
        }
    }

    pub fn core_count() -> u32 {
        unsafe { KeQueryActiveProcessorCountEx(0xFFFF) }
    }

    /// 当前处于 SVM 之下的核数
    pub fn cores_virtualized() -> u32 {
        let n = CORE_COUNT.load(Ordering::Acquire);
        let mut live = 0;
        for i in 0..n as usize {
            let v = VCPUS[i].load(Ordering::Acquire);
            if !v.is_null() && unsafe { (*v).info.state.load(Ordering::Acquire) } == vcpu_state::GUEST
            {
                live += 1;
            }
        }
        live
    }

    /// vmexit 总数(各核之和,近似)
    pub fn exit_counter() -> u64 {
        let n = CORE_COUNT.load(Ordering::Acquire);
        let mut total = 0u64;
        for i in 0..n as usize {
            let v = VCPUS[i].load(Ordering::Acquire);
            if !v.is_null() {
                total = total.wrapping_add(unsafe { (*v).info.exit_count });
            }
        }
        total
    }
}
