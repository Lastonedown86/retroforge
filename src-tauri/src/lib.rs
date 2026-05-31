mod device;
mod error;

use std::thread;
use std::time::{Duration, Instant};

use rusb::UsbContext;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::device::usb::{is_fel_device, FelTransport, UsbProbe};
use crate::device::{blobs, fel, memboot as memboot_ops};
use crate::device::{DeviceStatus, Monitor};
use crate::error::RfError;

/// Event name carrying live status changes to the frontend.
const STATUS_EVENT: &str = "device-status-changed";

const MEMBOOT_PROGRESS_EVENT: &str = "memboot-progress";

#[derive(Clone, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
enum MembootProgress {
    FetchingPayload,
    Connecting,
    InitDram,
    LoadingImage,
    LoadingUboot,
    Executing,
    WaitingForExit,
    Success,
    Failed { message: String },
}

fn emit_progress(app: &AppHandle, p: MembootProgress) {
    let _ = app.emit(MEMBOOT_PROGRESS_EVENT, p);
}

/// True if a FEL device is currently enumerated.
fn fel_present() -> bool {
    let Ok(ctx) = rusb::Context::new() else {
        return false;
    };
    let Ok(devices) = ctx.devices() else {
        return false;
    };
    devices.iter().any(|d| {
        d.device_descriptor()
            .map(|desc| is_fel_device(desc.vendor_id(), desc.product_id()))
            .unwrap_or(false)
    })
}

/// Open the currently-present FEL device as a transport.
fn open_fel() -> Result<FelTransport, RfError> {
    let ctx = rusb::Context::new().map_err(|e| RfError::FelProtocolError(e.to_string()))?;
    let devices = ctx
        .devices()
        .map_err(|e| RfError::FelProtocolError(e.to_string()))?;
    for device in devices.iter() {
        if let Ok(desc) = device.device_descriptor() {
            if is_fel_device(desc.vendor_id(), desc.product_id()) {
                return FelTransport::open(&device);
            }
        }
    }
    Err(RfError::DeviceGone)
}

fn run_memboot(app: &AppHandle, cache_dir: std::path::PathBuf) -> Result<(), RfError> {
    emit_progress(app, MembootProgress::FetchingPayload);
    blobs::ensure_blobs(&cache_dir)?;
    let fes1 = blobs::fes1();
    let uboot = blobs::uboot(&cache_dir)?;
    let boot_img = blobs::boot_img(&cache_dir)?;

    emit_progress(app, MembootProgress::Connecting);
    let mut transport = open_fel()?;

    emit_progress(app, MembootProgress::InitDram);
    memboot_ops::init_dram(&mut transport, fes1)?;
    emit_progress(app, MembootProgress::LoadingImage);
    {
        let padded = boot_img.len().div_ceil(fel::SECTOR_SIZE) * fel::SECTOR_SIZE;
        if padded as u32 > fel::TRANSFER_MAX_SIZE {
            return Err(RfError::ExecFailed("boot image too large".into()));
        }
        let mut kernel = boot_img.clone();
        kernel.resize(padded, 0);
        memboot_ops::write_memory(&mut transport, fel::TRANSFER_BASE, &kernel)?;
    }
    emit_progress(app, MembootProgress::LoadingUboot);
    emit_progress(app, MembootProgress::Executing);
    let cmd = format!("boota {:x}", fel::TRANSFER_BASE);
    memboot_ops::run_uboot_cmd(&mut transport, &uboot, &cmd)?;

    // Success = the FEL device leaves the bus (it is now running the image).
    drop(transport);
    emit_progress(app, MembootProgress::WaitingForExit);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if !fel_present() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(RfError::MembootTimeout)
}

#[tauri::command]
fn memboot(app: AppHandle) {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    std::thread::spawn(move || {
        let result = run_memboot(&app, cache_dir);
        match result {
            Ok(()) => emit_progress(&app, MembootProgress::Success),
            Err(e) => emit_progress(
                &app,
                MembootProgress::Failed {
                    message: e.to_string(),
                },
            ),
        }
    });
}

/// One-shot status read for initial render. Builds a throwaway Monitor so a
/// single probe maps through the same `ProbeOutcome -> DeviceStatus` path.
#[tauri::command]
fn get_device_status() -> DeviceStatus {
    let mut m = Monitor::new(UsbProbe::new());
    m.tick().unwrap_or(DeviceStatus::Disconnected)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_device_status, memboot])
        .setup(|app| {
            let handle = app.handle().clone();
            thread::spawn(move || {
                let mut monitor = Monitor::new(UsbProbe::new());
                loop {
                    if let Some(status) = monitor.tick() {
                        let _ = handle.emit(STATUS_EVENT, status);
                    }
                    thread::sleep(Duration::from_millis(1000));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
