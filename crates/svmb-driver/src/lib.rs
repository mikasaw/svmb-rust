//! svmb-driver — AMD-V (SVM) 虚拟化 WDM 驱动骨架
//!
//! 按 microsoft/windows-drivers-rs 官方范式搭建:
//! - `no_std` + `wdk-alloc`(内核池分配器)+ `wdk-panic`(panic 处理)
//! - build.rs 经 `wdk_build::configure_wdk_binary_build` 注入 WDK 链接配置
//! - 导出 `DriverEntry`,创建控制设备 `\Device\svmb_rust` + `\DosDevices\svmb_rust`
//!
//! SVM 核心逻辑按 C 版 svmb 逐模块移植,占位见 `svm` 模块。

#![cfg_attr(not(test), no_std)]

extern crate alloc;

#[cfg(not(test))]
extern crate wdk_panic;

mod dispatch;
mod phys;
mod svm;

use alloc::string::String;
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicPtr, Ordering};

use wdk_sys::{
    ntddk::{
        ExFreePoolWithTag, IoCreateDevice, IoCreateSymbolicLink, IoDeleteDevice,
        IoDeleteSymbolicLink, IofCompleteRequest, RtlInitUnicodeString, ZwClose, ZwOpenKey,
        ZwQueryValueKey,
    },
    DO_DEVICE_INITIALIZING, DRIVER_OBJECT, FILE_DEVICE_SECURE_OPEN, FILE_DEVICE_UNKNOWN,
    IRP_MJ_CLOSE, IRP_MJ_CREATE, IRP_MJ_DEVICE_CONTROL, KEY_QUERY_VALUE,
    KEY_VALUE_INFORMATION_CLASS, KEY_VALUE_PARTIAL_INFORMATION, NTSTATUS, OBJ_CASE_INSENSITIVE,
    OBJ_KERNEL_HANDLE, OBJECT_ATTRIBUTES, PDEVICE_OBJECT, PDRIVER_OBJECT, PCUNICODE_STRING,
    PIRP, STATUS_SUCCESS, UNICODE_STRING,
};

/// r128b 决定性实验:全驱动 DbgPrint 编译期关断。
/// 动机:Rust 版打印从未流到 kd(C 版能);"近 fire 死"(r123 后干净记录
/// 全部 ≤2.5min,多数 ≤20s)与 enter 期 DbgPrint(KDNET TX 需 ACK)阻塞
/// 模型吻合 —— 打印本身可能就是冻结/triple-fault 的触发器。
/// true = 恢复打印(对照组)。
pub(crate) const ENABLE_DBGPRINT: bool = false;

macro_rules! println {
    ($($arg:tt)*) => {
        if crate::ENABLE_DBGPRINT {
            wdk::println!($($arg)*);
        }
    };
}
pub(crate) use println;

/// 最小内核池分配器(NonPagedPool)。
/// 不用 wdk-alloc 的 WdkAllocator:它走 ExAllocatePool2(Windows 10 2004 /
/// 19041 才导出),测试机 18362(1903)内核解析不到该导入,sc start 报 127。
struct PoolAllocator;

const POOL_TAG_SVMB: u32 = u32::from_ne_bytes(*b"SVMB");

unsafe impl GlobalAlloc for PoolAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.align() > 16 {
            // x64 池块至多 16 字节对齐;超对齐需求骨架不支持(NULL = OOM)
            return core::ptr::null_mut();
        }
        // SAFETY: alloc_pool 本身是 unsafe fn(契约见其文档),此处满足其前提
        phys::alloc_pool(layout.size())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // SAFETY: ptr 只能来自本分配器的 alloc
        unsafe { ExFreePoolWithTag(ptr as wdk_sys::PVOID, POOL_TAG_SVMB) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: PoolAllocator = PoolAllocator;

/// 控制设备对象(unload 时删除);AtomicPtr 规避 static mut 引用(2024 edition)
static CONTROL_DEVICE: AtomicPtr<wdk_sys::DEVICE_OBJECT> = AtomicPtr::new(core::ptr::null_mut());

/// 把 &str 就地转成 NUL 结尾的 UTF-16(栈缓冲,RtlInitUnicodeString 只读一次)
fn widen_into<'a>(buf: &'a mut [u16], s: &str) -> &'a mut [u16] {
    let mut n = 0;
    for (i, u) in s.encode_utf16().enumerate() {
        buf[i] = u;
        n = i + 1;
    }
    buf[n] = 0;
    &mut buf[..n + 1]
}

/// WDM 入口 —— 骨架阶段:创建设备 + 符号链接,挂分发例程。
///
/// # Safety
/// 指针由 WDM 传入;导出名必须为 "DriverEntry"。
#[cfg_attr(not(test), unsafe(export_name = "DriverEntry"))]
pub unsafe extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    println!("[svmb-rust] DriverEntry");

    // 打印注册表参数键(官方 sample 同款),证明 DbgPrint/KDNET 通路正常
    if !registry_path.is_null() {
        let rp = unsafe { &*registry_path };
        if !rp.Buffer.is_null() {
            let s = String::from_utf16_lossy(unsafe {
                core::slice::from_raw_parts(rp.Buffer, rp.Length as usize / 2)
            });
            println!("[svmb-rust] registry: {s}");
        }
    }

    let mut dev_buf = [0u16; 32];
    let mut dos_buf = [0u16; 32];
    let dev_wide = widen_into(&mut dev_buf, svmb_shared::SVMB_DEVICE_NAME);
    let dos_wide = widen_into(&mut dos_buf, svmb_shared::SVMB_DOS_NAME);

    // SAFETY: 缓冲在本作用域内有效;输出指针由我们提供
    let (mut dev_name, mut dos_name) = unsafe { core::mem::zeroed::<(UNICODE_STRING, UNICODE_STRING)>() };
    unsafe {
        RtlInitUnicodeString(&mut dev_name, dev_wide.as_ptr());
        RtlInitUnicodeString(&mut dos_name, dos_wide.as_ptr());
    }

    let mut device: PDEVICE_OBJECT = core::ptr::null_mut();
    // SAFETY: driver 来自 WDM;device 为未初始化输出参数
    let status = unsafe {
        IoCreateDevice(
            driver as *mut DRIVER_OBJECT as PDRIVER_OBJECT,
            0,
            &mut dev_name,
            FILE_DEVICE_UNKNOWN,
            FILE_DEVICE_SECURE_OPEN,
            0,
            &mut device,
        )
    };
    if status != STATUS_SUCCESS {
        println!("[svmb-rust] IoCreateDevice failed: {status:#010x}");
        return status;
    }

    // SAFETY: device 刚由 IoCreateDevice 填充
    unsafe {
        (*device).Flags &= !DO_DEVICE_INITIALIZING;
    }
    CONTROL_DEVICE.store(device, Ordering::Release);

    // 回调签名与 PDRIVER_DISPATCH(Option<extern "C">)一致,经引用写入无需 unsafe
    driver.MajorFunction[IRP_MJ_CREATE as usize] = Some(dispatch::create_close);
    driver.MajorFunction[IRP_MJ_CLOSE as usize] = Some(dispatch::create_close);
    driver.MajorFunction[IRP_MJ_DEVICE_CONTROL as usize] = Some(dispatch::device_control);
    driver.DriverUnload = Some(driver_unload);

    // SAFETY: 两个字符串刚初始化且有效
    let link_status = unsafe { IoCreateSymbolicLink(&mut dos_name, &mut dev_name) };
    if link_status != STATUS_SUCCESS {
        println!("[svmb-rust] IoCreateSymbolicLink failed: {link_status:#010x}");
    }

    println!("[svmb-rust] device ready: {}", svmb_shared::SVMB_USER_PATH);

    // r149 AutoStart:移植 C 版 r30 语义(load == virtualize,默认开)——
    // DriverEntry 尾部直接进虚拟化,跑在 System 进程加载线程、服务 RUNNING
    // 上报之前;Start 失败驱动保持加载(取证可用)。Parameters\AutoStart=0
    // 可关闭(同 C)。
    if read_autostart_flag() != 0 {
        crate::println!("[svmb-rust] AutoStart: entering virtualization at DriverEntry");
        let st_start = svm::hypervisor::start();
        if st_start != STATUS_SUCCESS as i32 {
            crate::println!(
                "[svmb-rust] AutoStart start failed: {st_start:#x} (driver stays loaded)"
            );
        }
    }
    STATUS_SUCCESS
}

/// 读 HKLM\SYSTEM\CurrentControlSet\Services\svmbrust\Parameters\AutoStart
/// (DWORD)。缺省/读不到 = 1(默认开,与 C 版 r30 对齐)。
fn read_autostart_flag() -> u32 {
    const PARAMS: &[u16] = &widen_const(
        "\\Registry\\Machine\\SYSTEM\\CurrentControlSet\\Services\\svmbrust\\Parameters",
    );
    const AUTOSTART: &[u16] = &widen_const("AutoStart");
    const REG_DWORD: u32 = 4;

    // SAFETY: 本函数所有指针指向本函数栈/静态数据;Zw 句柄配对关闭
    unsafe {
        let (mut key_name, mut val_name) = core::mem::zeroed::<(UNICODE_STRING, UNICODE_STRING)>();
        RtlInitUnicodeString(&mut key_name, PARAMS.as_ptr());
        RtlInitUnicodeString(&mut val_name, AUTOSTART.as_ptr());
        let mut oa = OBJECT_ATTRIBUTES {
            Length: core::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: core::ptr::null_mut(),
            ObjectName: &mut key_name,
            Attributes: OBJ_CASE_INSENSITIVE | OBJ_KERNEL_HANDLE,
            SecurityDescriptor: core::ptr::null_mut(),
            SecurityQualityOfService: core::ptr::null_mut(),
        };
        let mut key: wdk_sys::HANDLE = core::ptr::null_mut();
        // STATUS_SUCCESS = 0;开键失败 = 用默认值
        if ZwOpenKey(&mut key, KEY_QUERY_VALUE, &mut oa) != 0 {
            return 1;
        }
        // KEY_VALUE_PARTIAL_INFORMATION(16B 头)+ 一个 DWORD 数据
        let mut buf = [0u8; core::mem::size_of::<KEY_VALUE_PARTIAL_INFORMATION>() + 4];
        let mut retlen: u32 = 0;
        // KEY_VALUE_INFORMATION_CLASS 是 i32 别名;PartialInformation = 2
        const KV_PARTIAL: KEY_VALUE_INFORMATION_CLASS = 2;
        let st = ZwQueryValueKey(
            key,
            &mut val_name,
            KV_PARTIAL,
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            buf.len() as u32,
            &mut retlen,
        );
        let _ = ZwClose(key);
        if st == 0 {
            let info = buf.as_ptr() as *const KEY_VALUE_PARTIAL_INFORMATION;
            // SAFETY: buf 尺寸按该结构+4 声明,查询成功时内核已回填
            let typ = core::ptr::addr_of!((*info).Type).read_unaligned();
            let dlen = core::ptr::addr_of!((*info).DataLength).read_unaligned();
            if typ == REG_DWORD && dlen >= 4 {
                let data = core::ptr::addr_of!((*info).Data) as *const u32;
                return data.read_unaligned();
            }
        }
        1
    }
}

/// const 上下文的 ASCII→UTF-16 widen(编译期生成 NUL 结尾宽字符串)
const fn widen_const(s: &str) -> [u16; 74] {
    let b = s.as_bytes();
    let mut out = [0u16; 74];
    let mut i = 0;
    while i < b.len() {
        out[i] = b[i] as u16;
        i += 1;
    }
    out
}

/// 卸载:停 hypervisor → 删符号链接 → 删设备
unsafe extern "C" fn driver_unload(_driver: PDRIVER_OBJECT) {
    println!("[svmb-rust] unload");
    let device = CONTROL_DEVICE.swap(core::ptr::null_mut(), Ordering::AcqRel);

    // 先停虚拟化(仍在 SVM 里的核会拒绝并保住资源)
    let st = svm::hypervisor::stop();
    if st != STATUS_SUCCESS {
        println!("[svmb-rust] hypervisor stop failed on unload: {st:#x}");
    }

    if device.is_null() {
        return;
    }

    let mut dos_buf = [0u16; 32];
    let dos_wide = widen_into(&mut dos_buf, svmb_shared::SVMB_DOS_NAME);
    // SAFETY: device 来自 IoCreateDevice 并保存在 CONTROL_DEVICE 中
    unsafe {
        let mut dos_name: UNICODE_STRING = core::mem::zeroed();
        RtlInitUnicodeString(&mut dos_name, dos_wide.as_ptr());
        let _ = IoDeleteSymbolicLink(&mut dos_name);
        IoDeleteDevice(device);
    }
    println!("[svmb-rust] unload done");
}

/// 完成已同步处理的 IRP(IO_NO_INCREMENT = 0)
///
/// # Safety
/// irp 必须是当前驱动收到的、尚未完成的 IRP
unsafe fn complete_irp(irp: PIRP, status: NTSTATUS, information: usize) -> NTSTATUS {
    unsafe {
        (*irp).IoStatus.__bindgen_anon_1.Status = status;
        (*irp).IoStatus.Information = information as u64;
        IofCompleteRequest(irp, 0);
    }
    status
}
