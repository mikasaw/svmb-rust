//! svmbctl-rs —— svmb-rust 驱动的用户态控制台(骨架)
//!
//! 用法:
//!   svmbctl-rs.exe [info|selftest]     # 默认 info

use std::mem::size_of;
use std::process::ExitCode;

use svmb_shared::{
    SvmbInfo, SvmbHvControl, SvmbSelftestResult, SVMB_IOCTL_GET_INFO, SVMB_IOCTL_HV_CONTROL,
    SVMB_IOCTL_SELFTEST, SVMB_PROTO_MAGIC, SVMB_USER_PATH,
};
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows_sys::Win32::System::IO::DeviceIoControl;

/// 打开控制设备(与 C 版 svmbctl 相同:读写 + 共享读写)
fn open_device() -> Result<HANDLE, String> {
    let mut wide: Vec<u16> = SVMB_USER_PATH.encode_utf16().collect();
    wide.push(0);
    // SAFETY: 纯 Win32 调用,参数均有效
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            core::ptr::null(),
            OPEN_EXISTING,
            0,
            core::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!(
            "CreateFileW({SVMB_USER_PATH}) 失败,GetLastError={}。驱动已加载?(sc start svmbrust)",
            std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
        ));
    }
    Ok(handle)
}

/// 发一个 METHOD_BUFFERED IOCTL,返回输出缓冲字节数
fn ioctl(handle: HANDLE, code: u32, out: &mut [u8]) -> Result<u32, String> {
    let mut returned: u32 = 0;
    // SAFETY: 句柄有效,缓冲生存期覆盖调用
    let ok = unsafe {
        DeviceIoControl(
            handle,
            code,
            core::ptr::null(),
            0,
            out.as_mut_ptr().cast(),
            out.len() as u32,
            &mut returned,
            core::ptr::null_mut(),
        )
    };
    if ok == 0 {
        let e = std::io::Error::last_os_error();
        return Err(format!(
            "DeviceIoControl({code:#010x}) 失败: {} (0x{:x})",
            e,
            e.raw_os_error().unwrap_or(0),
        ));
    }
    Ok(returned)
}

/// 发一个 METHOD_BUFFERED IOCTL(带输入缓冲),返回输出缓冲字节数
fn ioctl_in(handle: HANDLE, code: u32, input: &[u8], out: &mut [u8]) -> Result<u32, String> {
    let mut returned: u32 = 0;
    // SAFETY: 句柄有效,缓冲生存期覆盖调用
    let ok = unsafe {
        DeviceIoControl(
            handle,
            code,
            input.as_ptr().cast(),
            input.len() as u32,
            out.as_mut_ptr().cast(),
            out.len() as u32,
            &mut returned,
            core::ptr::null_mut(),
        )
    };
    if ok == 0 {
        let e = std::io::Error::last_os_error();
        return Err(format!(
            "DeviceIoControl({code:#010x}) 失败: {} (0x{:x})",
            e,
            e.raw_os_error().unwrap_or(0),
        ));
    }
    Ok(returned)
}

fn cmd_hv(action: u32, what: &str) -> Result<(), String> {
    let handle = open_device()?;
    let ctl = SvmbHvControl {
        action,
        repeat_count: 0,
    };
    // SAFETY: 结构体为纯 u32 字段,按字节切片发送
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(ctl).cast::<u8>(),
            size_of::<SvmbHvControl>(),
        )
    };
    let mut out = [0u8; 8];
    let r = ioctl_in(handle, SVMB_IOCTL_HV_CONTROL, bytes, &mut out)?;
    // SAFETY: HANDLE 由 CreateFileW 返回
    unsafe { CloseHandle(handle) };
    if r as usize != size_of::<SvmbHvControl>() {
        return Err(format!("协议不匹配: ret={r}"));
    }
    println!("{what} 完成");
    Ok(())
}

fn cmd_info() -> Result<(), String> {
    let handle = open_device()?;
    let mut buf = vec![0u8; size_of::<SvmbInfo>()];
    let ret = ioctl(handle, SVMB_IOCTL_GET_INFO, &mut buf)?;
    // SAFETY: 驱动按协议写入完整的 SvmbInfo
    let info: SvmbInfo = unsafe { core::ptr::read(buf.as_ptr().cast()) };
    // SAFETY: HANDLE 由 CreateFileW 返回
    unsafe { CloseHandle(handle) };

    if ret as usize != size_of::<SvmbInfo>() || info.magic != SVMB_PROTO_MAGIC {
        return Err(format!(
            "协议不匹配: ret={ret} magic={:#010x}(期望 {SVMB_PROTO_MAGIC:#010x})",
            info.magic
        ));
    }
    println!("magic          : {:#010x} ('SVMB')", info.magic);
    println!("proto_version  : {}", info.proto_version);
    println!("driver_version : {}", info.driver_version);
    println!("hv_state       : {}", info.hv_state);
    println!("cores_virt     : {}", info.cores_virtualized);
    println!("core_count     : {}", info.core_count);
    println!("pa_width_bits  : {}", info.pa_width_bits);
    println!("npt_views      : {}", info.npt_view_count);
    println!("exit_counter   : {}", info.exit_counter);
    println!("modules        : {}", info.module_count);
    Ok(())
}

fn cmd_info_raw_counter() -> Option<u64> {
    let handle = open_device().ok()?;
    let mut buf = vec![0u8; size_of::<SvmbInfo>()];
    let ret = ioctl(handle, SVMB_IOCTL_GET_INFO, &mut buf).ok()?;
    // SAFETY: 驱动按协议写入完整的 SvmbInfo
    let info: SvmbInfo = unsafe { core::ptr::read(buf.as_ptr().cast()) };
    // SAFETY: HANDLE 由 CreateFileW 返回
    unsafe { CloseHandle(handle) };
    let _ = ret;
    Some(info.exit_counter)
}

fn cmd_selftest() -> Result<(), String> {
    let handle = open_device()?;
    let mut buf = vec![0u8; size_of::<SvmbSelftestResult>()];
    let ret = ioctl(handle, SVMB_IOCTL_SELFTEST, &mut buf)?;
    let r: SvmbSelftestResult = unsafe { core::ptr::read(buf.as_ptr().cast()) };
    // SAFETY: HANDLE 由 CreateFileW 返回
    unsafe { CloseHandle(handle) };

    if ret as usize != size_of::<SvmbSelftestResult>() || r.magic != SVMB_PROTO_MAGIC {
        return Err(format!("协议不匹配: ret={ret} magic={:#010x}", r.magic));
    }
    println!("selftest: {}/{} checks, failed={}", r.total - r.failed, r.total, r.failed);
    if r.failed > 0 {
        let name_end = r.last_fail.iter().position(|&b| b == 0).unwrap_or(r.last_fail.len());
        println!("first fail: {}", String::from_utf8_lossy(&r.last_fail[..name_end]));
    }
    Ok(())
}


fn cmd_cpuid_probe() -> Result<(), String> {
    // r115: guest userspace view of CPUID 0x80000001 (SVM bit) / 0x8000000A
    unsafe {
        let i1 = cmd_info_raw_counter().unwrap_or(u64::MAX);
        let mut a = core::arch::x86_64::__cpuid_count(0x8000_0001, 0);
        for _ in 0..4 { a = core::arch::x86_64::__cpuid_count(0x8000_0001, 0); }
        let i2 = cmd_info_raw_counter().unwrap_or(u64::MAX);
        println!("exit_counter before={} after={} delta={}", i1, i2, i2.wrapping_sub(i1));
        println!("leaf 0x80000001: ecx={:#010x} svm_bit(ecx[2])={}", a.ecx, (a.ecx >> 2) & 1);
        let b = core::arch::x86_64::__cpuid_count(0x8000_000A, 0);
        println!("leaf 0x8000000A: edx={:#010x} npt_bit(edx[0])={}", b.edx, b.edx & 1);
        let c = core::arch::x86_64::__cpuid_count(0, 0);
        println!("leaf 0: max={:#x} vendor ebx={:#010x}", c.eax, c.ebx);
    }
    Ok(())
}

fn main() -> ExitCode {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "info".into());
    let r = match cmd.as_str() {
        "info" => cmd_info(),
        "selftest" => cmd_selftest(),
        "cpuid" => cmd_cpuid_probe(),
        "start" => cmd_hv(0, "hypervisor start"),
        "stop" => cmd_hv(1, "hypervisor stop"),
        other => Err(format!(
            "未知命令: {other}(可用: info | selftest | start | stop)"
        )),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("svmbctl-rs: {e}");
            ExitCode::FAILURE
        }
    }
}
