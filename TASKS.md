# svmb-rust 持续开发任务清单

> 自动循环模式:每项任务 = 开发 → 构建验证 → **子代理验收**(diff review)→ 提交 → 下一项。
> 循环纪律:VM 相关测试遵守 AGENTS.md(windbg-mcp/reset 落证/哈希对账/sc start=fire)。
> 状态标记:[ ] 待办 [~] 进行中 [x] 完成 [!] 受阻(注明原因)

## 主线:首-exit 冻结终局定位(r151 后唯一剩余差分=LLVM vs MSVC 二进制本身)

- [x] **T4 r152a:C-obj 混链终局实验 —— 已定谳:存活!**(R152_CLOOP SURVIVED T+308s,hv_state=2/6 核/exit_counter=13956=C 基线级;凶手=Rust 版 asm 循环本体;R2/R3 确认轮进行中;待 T4b 指令级病灶定位)
  - 内容:把 C 仓库的 `svm_entry.asm` 编成 COFF obj(MASM/ml64,offsets.inc 就位),
    以 staticlib 方式链进 svmb-driver(build.rs 加 `cargo:rustc-link-search` + 打包 .lib),
    Rust 侧 extern 到 C 符号 `_svmb_vmm_loop`,enter_core 换调之(替代 svmb_vmm_loop_c)。
  - 判据:构建过+部署 fire;**活 ⇒ 凶手=Rust 代码生成,再逐函数换回定位;死 ⇒ 凶手在链接形态/更底层**。
  - 验收:子代理 review 链接方式与符号契约;VM 轮结果记 rounds.txt。
  - 注意:C 侧 VcpuContext 布局与 Rust 不同(C: GuestVmcb 指针+Regs 内嵌偏移不同)——
    混链 obj 期望的 vcpu 布局必须与 Rust VcpuContext 对齐(必要时在 Rust 侧构造 C 兼容视图
    或改用 C 的 vcpu.h 布局断言逐项核对;这是本任务的最大风险点,验收必查)。

- [ ] **T5 r152b:C 侧干净时代布局扰动重测**(r119 重做+哈希纪律)
  - 内容:稳定 C 基线 + 256B dead 函数垫(重启 r119 实验,这次全程哈希对账+RUNNING 闸门)。
  - 判据:垫后仍稳(r123 后装置可信)⇒ C 稳定性非布局敏感 ⇒ 布局理论排除;
    垫后崩 ⇒ vhv 对 L1 驱动映像布局敏感(重大,回到 r118-r119 语境但用干净数据)。

- [ ] **T6 治愈后验收流程**(依赖 T4/T5 结论)
  - crumbs 重启(ENABLE_CRUMBS=true)+ 3 轮 15min soak;判据对齐 C 基线(14835 exits/8min 级)。

## 装置与代码卫生(低风险,先行建立循环节奏)

- [x] **T1 round.sh 适配 AutoStart 流程**(commit 2428e8a)
  - 内容:sc start 即 fire(AutoStart 已上);去掉 sleep 150 与 vm_ctl_run start;
    保留 reset→落证→alive→clean→push→哈希→create→start(=fire)→观测;
    观测首点提前到 T+10s(sc start 后 box 可能秒死)。
  - 判据:脚本跑通一轮,rounds.txt 记录格式不变。
  - 验收:子代理 review 脚本逻辑(闸门完整性、无 ctl start 残留)。

- [x] **T2 kd_forensic.py 的 .data RVA 动态化**(commit 85b461e)
  - 内容:dump_seq 用 PE 节表动态解析 svmb_rust.sys 的 .data RVA(替代硬编码;
    已两次踩 0x1E000→0x17000 漂移坑)。
  - 判据:python 语法过+对当前 .sys 解析出 0x17000。
  - 验收:子代理 review 解析正确性(节表结构/回退路径)。

- [x] **T3 死代码清理与开关整理**(commit 6689116)
  - 内容:r151 后确认未用的:旧 svmb_vmm_loop(Rust 侧 extern 与 asm 若仍保留)、
    svmb_run_guest(r148 后未用)、ENABLE_ONE_SHOT 分支、ENABLE_HANDLER_ADVANCE_ONLY 分支、
    CPUID_ONLY/MSR_ONLY 门(恢复完整拦截后)。**保守策略:标注 #[allow(dead_code)] +
    注释保留价值,而非物理删除**(二分装置是资产;svmb_restore_context 仍在 teardown 用,勿动)。
  - 判据:构建过+警告减少+行为零变化(const 全部维持现值)。
  - 验收:子代理 review 确认无活路径被误删。

- [ ] **T7 cmd_info_raw_counter helper bug**(低优先)
  - 记忆载:"cmd_info_raw_counter helper 有 bug 未修,用 shell 层差分"。定位修复,补差分测试。

## 环境例程(每轮 VM 任务前置)

- VM reset 后必查 vmware.log 落 VMAutomation_Reset 行(曾静默失败 80min 卡死)。
- 每会话开始:tasklist 查 kd.exe 残留;rounds.txt 追加不覆盖。
- guest 快照态若缺测试目录:先 vm_guest_mkdir.bat。

---
*更新本文件的状态标记即视为任务台账;每完成一项在 git log 留对应 commit。*
