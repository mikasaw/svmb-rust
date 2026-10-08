//! SVM VMRUN 循环与寄存器管道 —— C 版 hw/asm/svm_entry.asm 的 global_asm! 移植。
//!
//! 参考 hypervisor 技术:GIF 在 guest 执行期间保持使能(VMRUN 置位,#VMEXIT
//! 清零 —— exit 处理器在无中断环境运行);host 状态 = guest 快照(薄 hypervisor);
//! 退出虚拟化由 exit 处理器经 GuestRegs.Extra1/Extra2 请求(asm 契约)。
//!
//! 与 C 版的差异:
//! - MASM 的 SEH 序言(.pushframe)不可用 —— WinDbg 无法穿过 VMM 栈回溯,
//!   只影响可调试性不影响功能;
//! - r113 CrumbPost 信标省略(骨架不需要,连带免掉了 r113 的
//!   "loop-back 全 volatile 组入栈"成本)。
//!
//! 偏移常量(与 regs.rs/percpu.rs/vmcb.rs 的 const 断言一一对应):
//!   VCPU_GUEST_VMCB=0  VCPU_INFO=0x3000  VCPU_REGS=0x4000
//!   VMCB_EXIT_CODE=0x70  V_TPR=0x60  Save.Cr3=0x550  Save.Rip[7]=0x57F
//!   GREG_RAX=0x170 RCX=0x160 RDX=0x158 RFLAGS=0x178 RIP=0x180 RSP=0x188
//!   GREG_EXTRA1=0x190 EXTRA2=0x198

use core::arch::global_asm;

macro_rules! backup_regs {
    ($b:expr) => {
        concat!(
            "movaps xmmword ptr [", $b, " + 0x000], xmm0\n",
            "movaps xmmword ptr [", $b, " + 0x010], xmm1\n",
            "movaps xmmword ptr [", $b, " + 0x020], xmm2\n",
            "movaps xmmword ptr [", $b, " + 0x030], xmm3\n",
            "movaps xmmword ptr [", $b, " + 0x040], xmm4\n",
            "movaps xmmword ptr [", $b, " + 0x050], xmm5\n",
            "movaps xmmword ptr [", $b, " + 0x060], xmm6\n",
            "movaps xmmword ptr [", $b, " + 0x070], xmm7\n",
            "movaps xmmword ptr [", $b, " + 0x080], xmm8\n",
            "movaps xmmword ptr [", $b, " + 0x090], xmm9\n",
            "movaps xmmword ptr [", $b, " + 0x0A0], xmm10\n",
            "movaps xmmword ptr [", $b, " + 0x0B0], xmm11\n",
            "movaps xmmword ptr [", $b, " + 0x0C0], xmm12\n",
            "movaps xmmword ptr [", $b, " + 0x0D0], xmm13\n",
            "movaps xmmword ptr [", $b, " + 0x0E0], xmm14\n",
            "movaps xmmword ptr [", $b, " + 0x0F0], xmm15\n",
            "mov [", $b, " + 0x100], r15\n",
            "mov [", $b, " + 0x108], r14\n",
            "mov [", $b, " + 0x110], r13\n",
            "mov [", $b, " + 0x118], r12\n",
            "mov [", $b, " + 0x120], r11\n",
            "mov [", $b, " + 0x128], r10\n",
            "mov [", $b, " + 0x130], r9\n",
            "mov [", $b, " + 0x138], r8\n",
            "mov [", $b, " + 0x140], rbp\n",
            "mov [", $b, " + 0x148], rsi\n",
            "mov [", $b, " + 0x150], rdi\n",
            "mov [", $b, " + 0x158], rdx\n",
            "mov [", $b, " + 0x160], rcx\n",
            "mov [", $b, " + 0x168], rbx\n",
        )
    };
}

// rax 故意不恢复:它要保持指向 GuestRegs 基址,调用方随后读 Extra1/Extra2
macro_rules! restore_regs {
    ($b:expr) => {
        concat!(
            "movaps xmm0, xmmword ptr [", $b, " + 0x000]\n",
            "movaps xmm1, xmmword ptr [", $b, " + 0x010]\n",
            "movaps xmm2, xmmword ptr [", $b, " + 0x020]\n",
            "movaps xmm3, xmmword ptr [", $b, " + 0x030]\n",
            "movaps xmm4, xmmword ptr [", $b, " + 0x040]\n",
            "movaps xmm5, xmmword ptr [", $b, " + 0x050]\n",
            "movaps xmm6, xmmword ptr [", $b, " + 0x060]\n",
            "movaps xmm7, xmmword ptr [", $b, " + 0x070]\n",
            "movaps xmm8, xmmword ptr [", $b, " + 0x080]\n",
            "movaps xmm9, xmmword ptr [", $b, " + 0x090]\n",
            "movaps xmm10, xmmword ptr [", $b, " + 0x0A0]\n",
            "movaps xmm11, xmmword ptr [", $b, " + 0x0B0]\n",
            "movaps xmm12, xmmword ptr [", $b, " + 0x0C0]\n",
            "movaps xmm13, xmmword ptr [", $b, " + 0x0D0]\n",
            "movaps xmm14, xmmword ptr [", $b, " + 0x0E0]\n",
            "movaps xmm15, xmmword ptr [", $b, " + 0x0F0]\n",
            "mov r15, [", $b, " + 0x100]\n",
            "mov r14, [", $b, " + 0x108]\n",
            "mov r13, [", $b, " + 0x110]\n",
            "mov r12, [", $b, " + 0x118]\n",
            "mov r11, [", $b, " + 0x120]\n",
            "mov r10, [", $b, " + 0x128]\n",
            "mov r9,  [", $b, " + 0x130]\n",
            "mov r8,  [", $b, " + 0x138]\n",
            "mov rbp, [", $b, " + 0x140]\n",
            "mov rsi, [", $b, " + 0x148]\n",
            "mov rdi, [", $b, " + 0x150]\n",
            "mov rdx, [", $b, " + 0x158]\n",
            "mov rcx, [", $b, " + 0x160]\n",
            "mov rbx, [", $b, " + 0x168]\n",
        )
    };
}

global_asm!(concat!(
    "
// ---- 两阶段寄存器保存/恢复(svm_entry.asm _svmb_save_or_load_regs 原样移植)----
// Phase 1(普通调用,返回 rax=0):把全部寄存器/rflags/恢复点存入 [rcx] 的 GuestRegs。
// Phase 2(VMRUN 以 rax=&GuestRegs 进入 if_load_regs):全部恢复并跳回
// phase-1 保存的返回地址 —— 线程作为 guest 透明继续。
.globl svmb_save_or_load_regs
svmb_save_or_load_regs:
    ", backup_regs!("rcx"), "

    pushfq
    mov rax, [rsp]
    mov [rcx + 0x178], rax          // GREG_RFLAGS
    popfq

    lea rax, [rip + svmb_if_load_regs]
    mov [rcx + 0x180], rax          // GREG_RIP = if_load_regs(asm 契约)

    mov rax, [rsp]                 // phase-1 返回地址 = 恢复点
    mov [rcx + 0x190], rax          // GREG_EXTRA1

    mov rax, rsp
    add rax, 8                     // 调用者 rsp(弹出返回地址)
    mov [rcx + 0x188], rax          // GREG_RSP

    mov rax, 0                     // phase-1 标记:跳过恢复
svmb_if_load_regs:
    test rax, rax
    jz svmb_phase1_return

    ", restore_regs!("rax"), "
    // rcx = &GuestRegs(phase-1 参数)
    push qword ptr [rax + 0x178]
    popfq
    mov rsp, [rax + 0x188]
    mov rax, [rax + 0x170]
    jmp qword ptr [rcx + 0x190]

svmb_phase1_return:
    mov rax, [rcx + 0x170]
    ret

// ---- 主 VMRUN 循环(_svmb_vmm_loop 原样移植,去 SEH 序言与 crumb 信标)----
// void svmb_vmm_loop(vcpu=rcx, guestVmcbPa=rdx, hostVmcbPa=r8, vmmStackTop=r9)
// 成功时永不返回(核心停在 VMRUN 循环);仅 VMRUN 本身失败(-1/-2/-3)才返回。
.globl svmb_vmm_loop
svmb_vmm_loop:
    mov rax, rsp
    mov rsp, r9                    // 切到 VMM 栈

    sub rsp, 0x28                   // WinDbg 走栈用 scratch
    sub rsp, 0x8                    // 16 字节对齐垫
    push rax                       // [rsp+0x28] oldRsp 副本
    push rax                       // [rsp+0x20] oldRsp
    push r9                        // [rsp+0x18] stackTop
    push r8                        // [rsp+0x10] hostVmcbPa
    push rdx                       // [rsp+0x08] guestVmcbPa
    push rcx                       // [rsp+0x00] vcpu

svmb_enter_guest:
    // r113 bank-everything(C 版 svm_entry.asm 原样移植):loop-back 路径上
    // RESTORE_REGISTERS 已把 guest 的 GPR/XMM 放进物理寄存器,此处调用 C
    // 函数前必须 bank 全 volatile 组(r113 定律:d1.dmp 曾因 guest 拿 host
    // 残留 GPR 执行而 SYSTEM_SERVICE_EXCEPTION)。vcpu 在 bank 之上([rsp+0xA0])。
    sub rsp, 0xA0
    mov [rsp + 0x00], rax
    mov [rsp + 0x08], rcx
    mov [rsp + 0x10], rdx
    mov [rsp + 0x18], r8
    mov [rsp + 0x20], r9
    mov [rsp + 0x28], r10
    mov [rsp + 0x30], r11
    movaps [rsp + 0x40], xmm0
    movaps [rsp + 0x50], xmm1
    movaps [rsp + 0x60], xmm2
    movaps [rsp + 0x70], xmm3
    movaps [rsp + 0x80], xmm4
    movaps [rsp + 0x90], xmm5
    mov rax, [rsp + 0xA0]          // vcpu(6 push 的第一项在 bank 之上)
    mov rdx, [rax + 0]             // vcpu->GuestVmcb(指针)
    mov rdx, [rdx + 0xC0]          // Ctrl.CleanBits(C 版 0x184 信标数据)
    mov rcx, 0x184                 // CRUMB_VMRUN_PRE
    sub rsp, 0x30
    call svmb_crumb_post_d
    add rsp, 0x30
    movaps xmm5, [rsp + 0x90]
    movaps xmm4, [rsp + 0x80]
    movaps xmm3, [rsp + 0x70]
    movaps xmm2, [rsp + 0x60]
    movaps xmm1, [rsp + 0x50]
    movaps xmm0, [rsp + 0x40]
    mov r11, [rsp + 0x30]
    mov r10, [rsp + 0x28]
    mov r9, [rsp + 0x20]
    mov r8, [rsp + 0x18]
    mov rdx, [rsp + 0x10]
    mov rcx, [rsp + 0x08]
    mov rax, [rsp + 0x00]
    add rsp, 0xA0

    mov rax, [rsp + 0x8]
    vmload rax                     // 载入 guest 段/MSR 状态

    mov rax, [rsp + 0x8]
    vmrun rax                      // 进入 guest(置 GIF)

    // VMRUN 失败?(无效 -1 / 忙 -2 / 非法 -3)
    mov rax, [rsp]
    mov rax, [rax + 0]             // vcpu->GuestVmcb(指针,+0)
    mov rax, [rax + 0x70]           // Ctrl.ExitCode
    cmp rax, -1
    je svmb_vmrun_failed
    cmp rax, -2
    je svmb_vmrun_failed
    cmp rax, -3
    je svmb_vmrun_failed

    mov rax, [rsp + 0x8]
    vmsave rax                     // guest 状态存回 guest VMCB

    mov rax, [rsp + 0x10]
    vmload rax                     // 载入 host 状态(host = guest 快照)

    // 捕获 guest GPRs 到 vcpu->Regs(rax/rsp/rip/rflags 由 Rust shim 从保存区补齐)
    mov rax, [rsp]
    add rax, 0x4000
    ", backup_regs!("rax"), "

    // svmb_fill_machine_frame(machineFrame = rsp+0x38, vcpu)
    mov rcx, rsp
    add rcx, 0x38
    mov rdx, [rsp]
    sub rsp, 0x30
    call svmb_fill_machine_frame
    add rsp, 0x30

    // svmb_vm_exit_entry(vcpu, regs)
    mov rcx, [rsp]
    mov rdx, rcx
    add rdx, 0x4000
    sub rsp, 0x30
    call svmb_vm_exit_entry
    add rsp, 0x30

    mov rax, [rsp]
    add rax, 0x4000
    ", restore_regs!("rax"), "

    // devirt 请求?Extra1 = &Regs(非零 = 请求)
    mov rax, [rax + 0x190]
    test rax, rax
    jnz svmb_exit_virtualization

    // self-exit 检查:State(VCPU_INFO+0)== Leaving(3) 表示 Stop() 要求本核
    // 退出 VMM。就地执行:关 SVME 并原地恢复被打断的线程(r28 方案)。
    mov rax, [rsp]
    mov eax, [rax + 0x3000]         // Info.State(dword,零扩展)
    cmp eax, 3
    jne svmb_no_self_exit

    mov rax, [rsp]                 // vcpu(rax 已死:State)
    mov rcx, [rax + 0]             // guest VMCB VA(指针)
    // r28b 门:仅当被打断的上下文是内核态(Save.Rip 最高字节 0xFF = 内核
    // canonical VA)才就地恢复;用户态上下文需要 VMRUN 的完整状态重载。
    mov al, [rcx + 0x57F]           // Save.Rip byte 7
    cmp al, 0x0FF
    jne svmb_no_self_exit
    mov rdx, [rcx + 0x550]          // 被打断线程的 CR3(Save.Cr3)
    mov [rsp + 0x30], rdx           // 存入帧 scratch
    mov rax, [rsp + 8]             // guestVmcbPa
    vmload rax                     // 必须在 SVME=1 时执行(APM:SVME=0 时 VMLOAD #UD)
    stgi                           // 必须在 SVME=1 时执行(STGI 同上,r28c 教训)
    mov ecx, 0x0C0000080            // MSR_EFER
    rdmsr
    btr eax, 12                    // 清 SVME:本核退出虚拟化
    wrmsr
    mov rdx, [rsp + 0x30]           // rdx 已死(EFER 高位)
    mov cr3, rdx                   // 线程 CR3(内核 VA 全局,后续解绕安全)
    mov rax, [rsp]                 // vcpu
    mov dword ptr [rax + 0x3000], 0 // State = Off(Stop 的轮询解除阻塞)
    mov rcx, [rax + 0]             // VMCB VA 再取(rcx 刚被 EFER 索引占用)
    movzx edx, byte ptr [rcx + 0x60] // V_TPR = exit 时的 guest CR8
    mov cr8, rdx                   // 不恢复会让 IPI_LEVEL 上下文在 cr8=0 恢复
    add rax, 0x4000                 // Regs 基址(rax 重用为指针)
    push qword ptr [rax + 0x178]
    popfq                          // 线程 rflags(IF 回 1 —— 但 GIF 仍关:不投递)
    mov rcx, [rax + 0x160]
    mov rdx, [rax + 0x158]
    mov rsp, [rax + 0x188]          // 线程内核栈
    push qword ptr [rax + 0x180]    // 瞬时写到 rsp 之下 —— 安全,Win64 无红区
    mov rax, [rax + 0x170]          // 上下文 rax
    ret                            // 原地恢复被打断的线程

svmb_no_self_exit:
    mov rax, [rsp + 0x10]
    vmsave rax                     // 捕获处理器对 host MSR/段的修改
    jmp svmb_enter_guest

svmb_vmrun_failed:
    mov rax, [rsp + 0x20]           // 原始 rsp
    mov rsp, rax
    ret

svmb_exit_virtualization:
    // rax = GuestRegs 基址;切回被打断上下文的栈,从 Extra2 原地恢复
    stgi                           // r28:以 HOST 恢复 —— GIF 被 #VMEXIT 清零
    mov rsp, [rax + 0x188]
    push qword ptr [rax + 0x198]    // 恢复 rip(Extra2)
    push qword ptr [rax + 0x170]    // 上下文 rax
    push qword ptr [rax + 0x178]    // 上下文 rflags
    popfq
    pop rax
    ret

// ---- r124 返回式 VMRUN(barevisor run_guest.S 架构移植)----
// 单圈:载入 guest GPR/XMM → vmload/vmrun/vmsave guest → 存回 guest GPR/XMM
// → 恢复 host → vmload host VMCB(WinDbg 调试友好)→ ret。
// exit 处理由 Rust 侧在普通驱动栈上做(每圈返回),无 VMM 栈/无两阶段重入。
// GuestRegs 偏移:xmm0-5 @0x000-0x050,r15..rbx @0x100-0x168
//   (r15=0x100 r14=0x108 r13=0x110 r12=0x118 r11=0x120 r10=0x128 r9=0x130
//    r8=0x138 rbp=0x140 rsi=0x148 rdi=0x150 rdx=0x158 rcx=0x160 rbx=0x168)
// extern C fn(regs=rcx, vmcb_pa=rdx, host_vmcb_pa=r8)
.globl svmb_run_guest
svmb_run_guest:
    mov rax, r8
    vmsave rax             // r148:每圈首刷新 host VMCB(handler 段/MSR 漂移)= C 的 vmsave(host)
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    sub rsp, 0x60
    movaps xmmword ptr [rsp], xmm0
    movaps xmmword ptr [rsp + 0x10], xmm1
    movaps xmmword ptr [rsp + 0x20], xmm2
    movaps xmmword ptr [rsp + 0x30], xmm3
    movaps xmmword ptr [rsp + 0x40], xmm4
    movaps xmmword ptr [rsp + 0x50], xmm5

    mov r15, rcx          // r15 = &GuestRegs
    mov rax, rdx          // rax = vmcb_pa
    push rcx              // [rsp] = &GuestRegs(#1)

    movaps xmm0, xmmword ptr [r15 + 0x000]
    movaps xmm1, xmmword ptr [r15 + 0x010]
    movaps xmm2, xmmword ptr [r15 + 0x020]
    movaps xmm3, xmmword ptr [r15 + 0x030]
    movaps xmm4, xmmword ptr [r15 + 0x040]
    movaps xmm5, xmmword ptr [r15 + 0x050]
    mov rbx, [r15 + 0x168]
    mov rcx, [r15 + 0x160]
    mov rdx, [r15 + 0x158]
    mov rdi, [r15 + 0x150]
    mov rsi, [r15 + 0x148]
    mov rbp, [r15 + 0x140]
    mov r8,  [r15 + 0x138]
    mov r9,  [r15 + 0x130]
    mov r10, [r15 + 0x128]
    mov r11, [r15 + 0x120]
    mov r12, [r15 + 0x118]
    mov r13, [r15 + 0x110]
    mov r14, [r15 + 0x108]
    mov r15, [r15 + 0x100]

    // r148:精确复刻 C 版(svm_entry.asm)的每圈三步状态舞 ——
    //   vmload(guest) → vmrun → vmsave(guest) → vmload(host) → handler
    //   (GIF=0,interrupt-free)→ 下一圈首 vmsave(host)
    // r138/r140 删 vmload/vmsave 的〔冗余〕判断错误:硬件只自动管一部分,
    // 手工舞步是 C 稳定运行的必要条件;r147 的 stgi 更破坏了〔HSAVE
    // 窗口内 host 态不得变异〕不变量(VMRUN 自己给 guest 重开 GIF)。
    vmload rax
    vmrun rax
    vmsave rax

    xchg r15, [rsp]       // r15 = &GuestRegs;[rsp] = guest r15
    mov [r15 + 0x168], rbx
    mov [r15 + 0x160], rcx
    mov [r15 + 0x158], rdx
    mov [r15 + 0x148], rsi
    mov [r15 + 0x150], rdi
    mov [r15 + 0x140], rbp
    mov [r15 + 0x138], r8
    mov [r15 + 0x130], r9
    mov [r15 + 0x128], r10
    mov [r15 + 0x120], r11
    mov [r15 + 0x118], r12
    mov [r15 + 0x110], r13
    mov [r15 + 0x108], r14
    mov rax, [rsp]
    mov [r15 + 0x100], rax
    movaps xmmword ptr [r15 + 0x000], xmm0
    movaps xmmword ptr [r15 + 0x010], xmm1
    movaps xmmword ptr [r15 + 0x020], xmm2
    movaps xmmword ptr [r15 + 0x030], xmm3
    movaps xmmword ptr [r15 + 0x040], xmm4
    movaps xmmword ptr [r15 + 0x050], xmm5
    pop rax               // 丢弃 #1

    movaps xmm5, xmmword ptr [rsp + 0x50]
    movaps xmm4, xmmword ptr [rsp + 0x40]
    movaps xmm3, xmmword ptr [rsp + 0x30]
    movaps xmm2, xmmword ptr [rsp + 0x20]
    movaps xmm1, xmmword ptr [rsp + 0x10]
    movaps xmm0, xmmword ptr [rsp]
    add rsp, 0x60
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax

    // r148:撤销 r147 的 stgi —— C 版模型:handler 必须在 GIF=0 下跑
    // (svm_entry.asm 注释:interrupt-free),VMRUN 自己给 guest 重开
    // GIF;stgi 在圈间开中断会让 host 态在 HSAVE 保存/恢复窗口内变异。
    mov rax, r8            // host 段/MSR —— handler 上下文(C 版同位)
    vmload rax
    ret

// ---- r124 capture/restore:线程上下文进出虚拟化的干净版 ----
// capture(rcx=&GuestRegs):在 enter_core 内调用;保存 GPR/XMM/rflags 与
// **真实返回地址/rsp**(供退出虚拟化时原地恢复线程)。rax 存 0(volatile)。
.globl svmb_capture_regs
svmb_capture_regs:
    mov rax, [rsp]              // capture 的返回地址 = 线程恢复点
    mov [rcx + 0x180], rax      // RIP
    lea rax, [rsp + 8]
    mov [rcx + 0x188], rax      // RSP(返回后)
    pushfq
    pop rax
    mov [rcx + 0x178], rax      // RFLAGS
    mov qword ptr [rcx + 0x170], 0  // RAX(volatile,占位)
    movaps xmmword ptr [rcx + 0x000], xmm0
    movaps xmmword ptr [rcx + 0x010], xmm1
    movaps xmmword ptr [rcx + 0x020], xmm2
    movaps xmmword ptr [rcx + 0x030], xmm3
    movaps xmmword ptr [rcx + 0x040], xmm4
    movaps xmmword ptr [rcx + 0x050], xmm5
    movaps xmmword ptr [rcx + 0x060], xmm6
    movaps xmmword ptr [rcx + 0x070], xmm7
    movaps xmmword ptr [rcx + 0x080], xmm8
    movaps xmmword ptr [rcx + 0x090], xmm9
    movaps xmmword ptr [rcx + 0x0A0], xmm10
    movaps xmmword ptr [rcx + 0x0B0], xmm11
    movaps xmmword ptr [rcx + 0x0C0], xmm12
    movaps xmmword ptr [rcx + 0x0D0], xmm13
    movaps xmmword ptr [rcx + 0x0E0], xmm14
    movaps xmmword ptr [rcx + 0x0F0], xmm15
    mov [rcx + 0x100], r15
    mov [rcx + 0x108], r14
    mov [rcx + 0x110], r13
    mov [rcx + 0x118], r12
    mov [rcx + 0x120], r11
    mov [rcx + 0x128], r10
    mov [rcx + 0x130], r9
    mov [rcx + 0x138], r8
    mov [rcx + 0x140], rbp
    mov [rcx + 0x148], rsi
    mov [rcx + 0x150], rdi
    mov [rcx + 0x158], rdx
    mov [rcx + 0x160], rcx
    mov [rcx + 0x168], rbx
    mov rax, 0                  // 调用者拿到的返回值(capture 成功)
    ret

// restore(rcx=&GuestRegs):退出虚拟化;置 GIF(循环里 GIF=0)+回线程栈,
// 从保存的 RIP 原地继续 —— 不返回。
// r134 根因修复:压栈顺序必须是 rip 先(深)、rflags 后(栈顶)——
// popfq 弹栈顶、ret 弹次顶。旧序 popfq 把 rip 值装进 RFLAGS(随机
// IF/IOPL/VIP)、ret 跳去 rflags 值(如 0x202)⇒ 每次退出虚拟化必毁
// (r133 定位:capture+init+正常返回=存活,加 restore 舞蹈=冻结)。
.globl svmb_restore_context
svmb_restore_context:
    stgi                        // 循环外 GIF=0,恢复的线程必须开中断
    mov rax, [rcx + 0x170]      // RAX(在换栈前取,rcx 此后不变)
    mov rsp, [rcx + 0x188]
    push qword ptr [rcx + 0x180]  // rip(深处,ret 弹它)
    push qword ptr [rcx + 0x178]  // rflags(栈顶,popfq 弹它)
    popfq
    ret                         // 回到保存的 rip(返回到 capture 返回点)

// ---- 描述符表 / 段寄存器 / 控制寄存器 / MSR 助手 ----

.globl svmb_sgdt
svmb_sgdt:
    sub rsp, 0x10
    sgdt [rsp]
    mov ax, [rsp]
    mov word ptr [rdx], ax         // limit
    mov rax, [rsp + 2]
    mov qword ptr [rcx], rax       // base
    add rsp, 0x10
    ret

.globl svmb_sidt
svmb_sidt:
    sub rsp, 0x10
    sidt [rsp]
    mov ax, [rsp]
    mov word ptr [rdx], ax
    mov rax, [rsp + 2]
    mov qword ptr [rcx], rax
    add rsp, 0x10
    ret

.globl svmb_sldt
svmb_sldt:
    sldt ax
    mov word ptr [rcx], ax
    ret

.globl svmb_str
svmb_str:
    str ax
    mov word ptr [rcx], ax
    ret

.globl svmb_read_cs
svmb_read_cs:
    mov ax, cs
    ret
.globl svmb_read_ds
svmb_read_ds:
    mov ax, ds
    ret
.globl svmb_read_es
svmb_read_es:
    mov ax, es
    ret
.globl svmb_read_fs
svmb_read_fs:
    mov ax, fs
    ret
.globl svmb_read_gs
svmb_read_gs:
    mov ax, gs
    ret
.globl svmb_read_ss
svmb_read_ss:
    mov ax, ss
    ret

.globl svmb_read_cr0
svmb_read_cr0:
    mov rax, cr0
    ret
.globl svmb_read_cr2
svmb_read_cr2:
    mov rax, cr2
    ret
.globl svmb_read_cr3
svmb_read_cr3:
    mov rax, cr3
    ret
.globl svmb_read_cr4
svmb_read_cr4:
    mov rax, cr4
    ret

// u64 svmb_rdmsr(u32 msr /*rcx*/)
.globl svmb_rdmsr
svmb_rdmsr:
    rdmsr                          // EDX:EAX
    shl rdx, 32
    or rax, rdx
    ret

// void svmb_wrmsr(u32 msr /*rcx*/, u64 value /*rdx*/)
.globl svmb_wrmsr
svmb_wrmsr:
    mov eax, edx                   // 低 32 位
    shr rdx, 32                    // 高 32 位
    wrmsr
    ret

// ---- r151:C 型主循环(svm_entry.asm 逐条结构移植) ----
// svmb_vmm_loop_c(vcpu=rcx, guest_pa=rdx, host_pa=r8, stack_top=r9) -> i32
// 成功时永不返回;eax: 0 = leaving 请求(走 Rust teardown),非 0 = VMRUN
// 失败码(-1/-2/-3 或哨兵 1)。VcpuContext:guest_vmcb 指针@0,regs 内嵌
// @+0x4000(xmm0-15@0x000-0x0F0,GPR@0x100-0x168);VMCB ctrl EXIT_CODE@0x70;
// save:RFLAGS@0x570 RIP@0x578 RSP@0x5D8 RAX@0x5F8。
.globl svmb_vmm_loop_c
svmb_vmm_loop_c:
    mov rax, rsp
    mov rsp, r9                  // 切 VMM 栈
    sub rsp, 0x30                // 机器帧 scratch + 对齐
    push rax                     // [rsp+0x20] oldRsp
    push r9
    push r8
    push rdx
    push rcx                     // [rsp] vcpu

c_enter_guest:
    mov rax, [rsp]               // vcpu
    mov r10, [rax]               // guest_vmcb VA(指针@0)
    lea r11, [rax + 0x4000]      // &regs(内嵌)
    mov rcx, [r11 + 0x180]
    mov [r10 + 0x578], rcx       // RIP
    mov rcx, [r11 + 0x188]
    mov [r10 + 0x5D8], rcx       // RSP
    mov rcx, [r11 + 0x170]
    mov [r10 + 0x5F8], rcx       // RAX
    mov rcx, [r11 + 0x178]
    mov [r10 + 0x570], rcx       // RFLAGS
    mov qword ptr [r10 + 0x70], 1    // r125 哨兵

    mov rax, [rsp + 0x8]
    vmload rax                   // guest 段/MSR
    mov rax, [rsp + 0x8]
    vmrun rax

    mov rax, [rsp]
    mov rax, [rax]               // guest_vmcb
    mov rax, [rax + 0x70]        // ExitCode
    cmp rax, 1
    je c_vmrun_failed
    cmp rax, -1
    je c_vmrun_failed
    cmp rax, -2
    je c_vmrun_failed
    cmp rax, -3
    je c_vmrun_failed

    mov rax, [rsp + 0x8]
    vmsave rax                   // guest 状态 -> guest VMCB
    mov rax, [rsp + 0x10]
    vmload rax                   // host 状态

    mov rax, [rsp]
    add rax, 0x4000
    movaps xmmword ptr [rax + 0x000], xmm0
    movaps xmmword ptr [rax + 0x010], xmm1
    movaps xmmword ptr [rax + 0x020], xmm2
    movaps xmmword ptr [rax + 0x030], xmm3
    movaps xmmword ptr [rax + 0x040], xmm4
    movaps xmmword ptr [rax + 0x050], xmm5
    movaps xmmword ptr [rax + 0x060], xmm6
    movaps xmmword ptr [rax + 0x070], xmm7
    movaps xmmword ptr [rax + 0x080], xmm8
    movaps xmmword ptr [rax + 0x090], xmm9
    movaps xmmword ptr [rax + 0x0A0], xmm10
    movaps xmmword ptr [rax + 0x0B0], xmm11
    movaps xmmword ptr [rax + 0x0C0], xmm12
    movaps xmmword ptr [rax + 0x0D0], xmm13
    movaps xmmword ptr [rax + 0x0E0], xmm14
    movaps xmmword ptr [rax + 0x0F0], xmm15
    mov [rax + 0x100], r15
    mov [rax + 0x108], r14
    mov [rax + 0x110], r13
    mov [rax + 0x118], r12
    mov [rax + 0x120], r11
    mov [rax + 0x128], r10
    mov [rax + 0x130], r9
    mov [rax + 0x138], r8
    mov [rax + 0x140], rbp
    mov [rax + 0x148], rsi
    mov [rax + 0x150], rdi
    mov [rax + 0x158], rdx
    mov [rax + 0x160], rcx
    mov [rax + 0x168], rbx

    sub rsp, 0x30                // Win64 shadow space
    mov rcx, [rsp + 0x30]        // vcpu
    lea rdx, [rcx + 0x4000]      // regs
    call svmb_handle_exit_c
    add rsp, 0x30
    test eax, eax
    jnz c_leave_loop

    mov rax, [rsp]
    add rax, 0x4000
    mov r15, [rax + 0x100]
    mov r14, [rax + 0x108]
    mov r13, [rax + 0x110]
    mov r12, [rax + 0x118]
    mov r11, [rax + 0x120]
    mov r10, [rax + 0x128]
    mov r9,  [rax + 0x130]
    mov r8,  [rax + 0x138]
    mov rbp, [rax + 0x140]
    mov rsi, [rax + 0x148]
    mov rdi, [rax + 0x150]
    mov rdx, [rax + 0x158]
    mov rcx, [rax + 0x160]
    mov rbx, [rax + 0x168]
    movaps xmm0, xmmword ptr [rax + 0x000]
    movaps xmm1, xmmword ptr [rax + 0x010]
    movaps xmm2, xmmword ptr [rax + 0x020]
    movaps xmm3, xmmword ptr [rax + 0x030]
    movaps xmm4, xmmword ptr [rax + 0x040]
    movaps xmm5, xmmword ptr [rax + 0x050]
    movaps xmm6, xmmword ptr [rax + 0x060]
    movaps xmm7, xmmword ptr [rax + 0x070]
    movaps xmm8, xmmword ptr [rax + 0x080]
    movaps xmm9, xmmword ptr [rax + 0x090]
    movaps xmm10, xmmword ptr [rax + 0x0A0]
    movaps xmm11, xmmword ptr [rax + 0x0B0]
    movaps xmm12, xmmword ptr [rax + 0x0C0]
    movaps xmm13, xmmword ptr [rax + 0x0D0]
    movaps xmm14, xmmword ptr [rax + 0x0E0]
    movaps xmm15, xmmword ptr [rax + 0x0F0]

    mov rax, [rsp + 0x10]
    vmsave rax                   // 捕获 handler 的 host 漂移
    jmp c_enter_guest

c_vmrun_failed:
    mov r10, rax
    mov rsp, [rsp + 0x20]        // 回原栈
    mov rax, r10
    ret

c_leave_loop:
    xor eax, eax
    mov rsp, [rsp + 0x20]
    ret
    ",
));

unsafe extern "C" {
    /// 主 VMRUN 循环:成功时不返回;仅 VMRUN 失败(-1/-2/-3)时返回
    pub unsafe fn svmb_vmm_loop(
        vcpu: *mut crate::svm::percpu::VcpuContext,
        guest_vmcb_pa: u64,
        host_vmcb_pa: u64,
        vmm_stack_top: *mut u8,
    );

    /// r151:C 型主循环(结构移植);成功不返回;eax=0 离开请求/非 0=VMRUN 失败码
    pub unsafe fn svmb_vmm_loop_c(
        vcpu: *mut crate::svm::percpu::VcpuContext,
        guest_vmcb_pa: u64,
        host_vmcb_pa: u64,
        vmm_stack_top: *mut u8,
    ) -> i32;

    /// r124 返回式单圈 VMRUN:每圈返回,exit 处理由 Rust 侧普通栈做
    pub unsafe fn svmb_run_guest(
        regs: *mut crate::svm::regs::GuestRegs,
        guest_vmcb_pa: u64,
        host_vmcb_pa: u64,
    );

    /// r124:捕获当前线程上下文(真实返回点/rsp)进 GuestRegs
    pub unsafe fn svmb_capture_regs(regs: *mut crate::svm::regs::GuestRegs);

    /// r124:退出虚拟化;从 GuestRegs 原地恢复线程(不返回)
    pub unsafe fn svmb_restore_context(regs: *mut crate::svm::regs::GuestRegs);

    /// 两阶段寄存器保存/恢复(phase-2 由 VMRUN 从 if_load_regs 进入)
    pub unsafe fn svmb_save_or_load_regs(regs: *mut crate::svm::regs::GuestRegs);

    pub unsafe fn svmb_sgdt(base: *mut u64, limit: *mut u16);
    pub unsafe fn svmb_sidt(base: *mut u64, limit: *mut u16);
    pub unsafe fn svmb_sldt(selector: *mut u16);
    pub unsafe fn svmb_str(selector: *mut u16);

    pub unsafe fn svmb_read_cs() -> u16;
    pub unsafe fn svmb_read_ds() -> u16;
    pub unsafe fn svmb_read_es() -> u16;
    pub unsafe fn svmb_read_fs() -> u16;
    pub unsafe fn svmb_read_gs() -> u16;
    pub unsafe fn svmb_read_ss() -> u16;

    pub unsafe fn svmb_read_cr0() -> u64;
    pub unsafe fn svmb_read_cr2() -> u64;
    pub unsafe fn svmb_read_cr3() -> u64;
    pub unsafe fn svmb_read_cr4() -> u64;

    pub unsafe fn svmb_rdmsr(msr: u32) -> u64;
    pub unsafe fn svmb_wrmsr(msr: u32, value: u64);
}
