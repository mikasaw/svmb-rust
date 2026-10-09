//! 构建脚本:按官方 windows-drivers-rs 范式,把 Cargo.toml 里的
//! `[package.metadata.wdk.driver-model]` 解析成链接 WDK 内核运行时
//! 所需的全部链接器参数(/SUBSYSTEM:NATIVE、WDK 库搜索路径等)。
//! WDK 版本自动探测本机 Windows Kits(本机为 10.0.26100.0 / 10.0.28000.0)。

fn main() -> Result<(), wdk_build::ConfigError> {
    wdk_build::configure_wdk_binary_build()?;
    // r152 混链实验:链入 C 仓库 ml64 编译的 svm_entry.obj(_svmb_vmm_loop,
    // 布局与 VcpuContext/GuestRegs 逐项同源 —— offsets.inc)。重编命令:
    // MSYS_NO_PATHCONV=1 ml64 /c /W2 /Fo svm_entry.obj svm_entry.asm
    // (源:svmb/driver/src/hw/asm/,产物拷贝为本文件旁的 ext_svm_entry.obj)
    let obj = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("ext_svm_entry.obj");
    println!("cargo:rustc-link-arg={}", obj.display());
    println!("cargo:rerun-if-changed={}", obj.display());
    Ok(())
}
