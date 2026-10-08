# r151: append C-style vmm loop into asm.rs global_asm block + extern decl
import io, sys

ASM = r'''
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
'''

p = r"C:\Users\www\AiCode\svmb-rust\crates\svmb-driver\src\svm\asm.rs"
s = io.open(p, encoding="utf-8").read()

anchor = ("    mov eax, edx                   // 低 32 位\n"
          "    shr rdx, 32                    // 高 32 位\n"
          "    wrmsr\n"
          "    ret\n"
          '    ",\n'
          "))")
tail = ("    mov eax, edx                   // 低 32 位\n"
        "    shr rdx, 32                    // 高 32 位\n"
        "    wrmsr\n"
        "    ret\n")
new_tail = tail + ASM + '    ",\n))'
assert anchor in s, "asm tail anchor missing"
s = s.replace(anchor, new_tail, 1)

old_decl = ("    /// r124 返回式单圈 VMRUN:每圈返回,exit 处理由 Rust 侧普通栈做\n"
            "    pub unsafe fn svmb_run_guest(")
new_decl = ("    /// r151:C 型主循环(结构移植);成功不返回;eax=0 离开请求/非 0=VMRUN 失败码\n"
            "    pub unsafe fn svmb_vmm_loop_c(\n"
            "        vcpu: *mut crate::svm::percpu::VcpuContext,\n"
            "        guest_vmcb_pa: u64,\n"
            "        host_vmcb_pa: u64,\n"
            "        vmm_stack_top: *mut u8,\n"
            "    ) -> i32;\n\n"
            "    /// r124 返回式单圈 VMRUN:每圈返回,exit 处理由 Rust 侧普通栈做\n"
            "    pub unsafe fn svmb_run_guest(")
assert old_decl in s, "decl anchor missing"
s = s.replace(old_decl, new_decl, 1)

io.open(p, "w", encoding="utf-8").write(s)
print("asm.rs patched ok")
