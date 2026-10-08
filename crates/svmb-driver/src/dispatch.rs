//! IRP 分发例程:CREATE/CLOSE + DEVICE_CONTROL(GET_INFO / HV_CONTROL / SELFTEST)

use core::mem::size_of;

use svmb_shared::{SvmbInfo, SvmbSelftestResult, SVMB_IOCTL_GET_INFO, SVMB_IOCTL_HV_CONTROL, SVMB_IOCTL_SELFTEST};
use crate::println;
use wdk_sys::{PDEVICE_OBJECT, PIRP, STATUS_BUFFER_TOO_SMALL, STATUS_INVALID_DEVICE_REQUEST, STATUS_SUCCESS};

use crate::{complete_irp, svm::hypervisor};

/// IRP_MJ_CREATE / IRP_MJ_CLOSE:直接成功
///
/// # Safety
/// irp 指向 WDM 传入的有效 IRP
pub unsafe extern "C" fn create_close(_device: PDEVICE_OBJECT, irp: PIRP) -> wdk_sys::NTSTATUS {
    unsafe { complete_irp(irp, STATUS_SUCCESS, 0) }
}

/// IRP_MJ_DEVICE_CONTROL:按 svmb 协议分发
///
/// # Safety
/// irp 指向 WDM 传入的有效 IRP;METHOD_BUFFERED 下输入输出共用 SystemBuffer
pub unsafe extern "C" fn device_control(_device: PDEVICE_OBJECT, irp: PIRP) -> wdk_sys::NTSTATUS {
    // 联合体字段路径(__bindgen_anon_1/__bindgen_anon_2)与 wdk-sys 生成的绑定逐字对齐
    unsafe {
        let stack = (*irp).Tail.Overlay.__bindgen_anon_2.__bindgen_anon_1.CurrentStackLocation;
        let ioctl = (*stack).Parameters.DeviceIoControl.IoControlCode;
        let out_len = (*stack).Parameters.DeviceIoControl.OutputBufferLength as usize;
        let in_len = (*stack).Parameters.DeviceIoControl.InputBufferLength as usize;
        let sys_buf = (*irp).AssociatedIrp.SystemBuffer as *mut u8;

        match ioctl as u32 {
            SVMB_IOCTL_GET_INFO => {
                if out_len < size_of::<SvmbInfo>() {
                    return complete_irp(irp, STATUS_BUFFER_TOO_SMALL, 0);
                }
                let info = sys_buf as *mut SvmbInfo;
                *info = SvmbInfo {
                    magic: svmb_shared::SVMB_PROTO_MAGIC,
                    proto_version: svmb_shared::SVMB_PROTO_VERSION,
                    driver_version: 2,
                    hv_state: hypervisor::info::hv_state(),
                    cores_virtualized: hypervisor::info::cores_virtualized(),
                    core_count: hypervisor::info::core_count(),
                    exit_counter: hypervisor::info::exit_counter(),
                    pa_width_bits: 0, // TODO: CPUID.80000008.EAX[7:0]
                    ..SvmbInfo::default()
                };
                complete_irp(irp, STATUS_SUCCESS, size_of::<SvmbInfo>())
            }
            SVMB_IOCTL_HV_CONTROL => {
                // 输入输出共用 SystemBuffer:先按 SVMB_HV_CONTROL 解析
                if in_len < size_of::<svmb_shared::SvmbHvControl>()
                    || out_len < size_of::<svmb_shared::SvmbHvControl>()
                {
                    return complete_irp(irp, STATUS_BUFFER_TOO_SMALL, 0);
                }
                let ctl = sys_buf as *mut svmb_shared::SvmbHvControl;
                let action = (*ctl).action;
                let status = match action {
                    0 => hypervisor::start(), // start
                    1 => hypervisor::stop(),  // stop
                    2 => wdk_sys::STATUS_NOT_IMPLEMENTED, // stress loop:未移植
                    _ => STATUS_INVALID_DEVICE_REQUEST,
                };
                let status = if status == 0 { STATUS_SUCCESS } else { status };
                complete_irp(irp, status, size_of::<svmb_shared::SvmbHvControl>())
            }
            SVMB_IOCTL_SELFTEST => {
                if out_len < size_of::<SvmbSelftestResult>() {
                    return complete_irp(irp, STATUS_BUFFER_TOO_SMALL, 0);
                }
                let r = sys_buf as *mut SvmbSelftestResult;
                *r = SvmbSelftestResult {
                    magic: svmb_shared::SVMB_PROTO_MAGIC,
                    total: 0,
                    failed: 0,
                    reserved: 0,
                    last_fail: [0; 96],
                };
                complete_irp(irp, STATUS_SUCCESS, size_of::<SvmbSelftestResult>())
            }
            code => {
                println!("[svmb-rust] unhandled ioctl {code:#010x}");
                complete_irp(irp, STATUS_INVALID_DEVICE_REQUEST, 0)
            }
        }
    }
}
