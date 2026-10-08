//! 构建脚本:按官方 windows-drivers-rs 范式,把 Cargo.toml 里的
//! `[package.metadata.wdk.driver-model]` 解析成链接 WDK 内核运行时
//! 所需的全部链接器参数(/SUBSYSTEM:NATIVE、WDK 库搜索路径等)。
//! WDK 版本自动探测本机 Windows Kits(本机为 10.0.26100.0 / 10.0.28000.0)。

fn main() -> Result<(), wdk_build::ConfigError> {
    wdk_build::configure_wdk_binary_build()
}
