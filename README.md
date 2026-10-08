# svmb-rust

svmb(Windows AMD-V/SVM 虚拟化框架)的 Rust 移植 —— 骨架阶段。
按 [microsoft/windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs)(wdk-rs)官方 WDM 范式搭建。

## 结构

```
svmb-rust/
├── Cargo.toml              # workspace(shared + ctl;driver 独立,官方约束)
├── rust-toolchain.toml     # stable MSVC
├── crates/
│   ├── svmb-shared/        # R3<->R0 协议(镜像 C 版 svmb_protocol.h;no_std)
│   ├── svmb-driver/        # WDM 驱动(cdylib -> svmb_rust.sys)
│   │   └── src/svm/        # SVM 核心移植落点:asm(已可用)/msr/exit/vmcb(骨架)
│   └── svmbctl-rs/         # 用户态控制台(info | selftest)
└── build/build_rust.bat    # cargo 构建 + dll->sys 改名 + 测试签名
```

## 前提(本机已就绪)

- Rust stable MSVC(rust-toolchain.toml 钉住)+ **LLVM 17.0.6**
  - ⚠️ LLVM 必须 17.x:LLVM 23 会让 wdk-sys 的 bindgen 布局断言炸裂
    (结构体被当不透明类型,E0080 overflow;官方 CI 也固定 17.0.6)
- WDK:wdk-build 自动探测 → 本机用 **10.0.28000**(26100 只有 SDK、没有 km 内核头,
  不能用 `Version_Number` 钉回 26100;28000 虽是 Insider 但 x64 绑定生成正常)
- VS 18 Insiders MSVC(链接器)+ 测试证书 `svmb-test`(宿主 My 存储;
  VM 端 Root+TrustedPeople + testsigning)
- 驱动包还要求静态 CRT:`crates/svmb-driver/.cargo/config.toml` 已配
  `target-feature=+crt-static`(否则 ConfigError::StaticCrtNotEnabled)

## 构建

```bat
build\build_rust.bat            :: Debug(对齐 vm_push_driver.bat 的 x64\Debug 约定)
build\build_rust.bat Release
```

产物:`x64\<config>\svmb_rust.sys`、`x64\<config>\svmbctl-rs.exe`。

## 部署到测试 VM(复用 vm-driver-debug-loop 流程)

vmrun 推送后,在 guest 内:

```bat
sc create svmbrust type= kernel binPath= C:\Users\wwa\Desktop\driverTest\svmb-test\svmb_rust.sys
sc start svmbrust
svmbctl-rs.exe info
svmbctl-rs.exe selftest
sc stop svmbrust & sc delete svmbrust
```

设备名 `\Device\svmb_rust`(用户路径 `\\.\svmb_rust`),与 C 版 svmb 同名不冲突,可同机共存对照。

## 与官方范式的对应

| 官方(sample-wdm-driver) | 本项目 |
|---|---|
| `[package.metadata.wdk.driver-model] driver-type="WDM"` | 同 |
| build.rs `wdk_build::configure_wdk_binary_build()` | 同 |
| `#![cfg_attr(not(test), no_std)]` + `wdk_panic` | 同 |
| `wdk-alloc` 的 `WdkAllocator` | **不用** —— 它走 `ExAllocatePool2`(Win10 2004/19041 才导出),测试机 18362(1903)内核解析不到,`sc start` 报 127;改为自写 `PoolAllocator`(`ExAllocatePoolWithTag`,手写 extern 声明) |
| `#[unsafe(export_name = "DriverEntry")]` | 同 |
| 驱动包排除出 workspace(单 WDK 配置/profile 隔离) | 同(root Cargo.toml exclude) |
| cargo-make / cargo-wdk 打包(inf2cat 等) | 未采用 —— 走 sc create + 测试签名,与 C 版流程一致 |

## VM 验证结论(2026-10-01)

骨架已在 Windows 10 x64 测试机(18362)全流程验证:

- 部署:`build\vm_push_rust.bat` → SHA256 双向对账一致
- 加载:`vm_svc_create/start.bat` → `sc query svmbrust` = **STATE 4 RUNNING**
- IOCTL:guest 内 `svmbctl-rs.exe info`(magic 'SVMB'、core_count 实时来自内核)
  / `selftest`(0/0)—— R3→R0 分发、METHOD_BUFFERED、IRP 完成全通
- 卸载:`vm_svc_clean.bat` → STOPPED + DeleteService 成功,无冻结

脚本清单(vm_* 一律 bat 包装、exec 尾 `< NUL`、guest 端纯 ASCII):
`vm_push_rust / vm_hash_check / vm_svc_clean / vm_svc_create / vm_svc_start / vm_ctl_run / vm_pull_log`。

## 移植路线(下一步)

1. SVM 起停:`svm/msr.rs` 常量 + 每核 EFER.SVME / VM_HSAVE_PA / IOPM / MSRPM 分配(对齐 C 版 hv 起停)
2. VMCB 字段化 + `asm::svmb_asm_vmrun` 的 per-core 循环与 #VMEXIT 分发(exit.rs)
3. NPT 根表/视图切换(nCr3),随后按 C 版模块逐个挂:cr3 monitor → npt hook → sysaudit
4. shared crate 补齐尾部结构(NPT_HOOK_CTL / CR3_* / SYSAUD_* 等,见 svmb-shared/src/lib.rs 尾部 TODO)

参考:AMD APM Vol.2 §15;tandasat/SimpleSvm、barevisor;memN0ps/matrix-rs;C 版 svmb 源码。
