mod device;
mod error;

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rusb::UsbContext;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Set while a privileged device operation (memboot, shell) holds the USB
/// interface, so the background poll thread does not contend for it.
static DEVICE_BUSY: AtomicBool = AtomicBool::new(false);

use crate::device::netshell;
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

/// Hakchi's "SD" U-Boot transform: swap the two 4-byte words in the trailing
/// 8 bytes. Selects booting the RAM-staged image instead of NAND.
fn to_sd_uboot(uboot: &[u8]) -> Vec<u8> {
    let mut v = uboot.to_vec();
    let n = v.len();
    if n >= 8 {
        let last8: Vec<u8> = v[n - 8..].to_vec();
        v[n - 8..n - 4].copy_from_slice(&last8[4..8]);
        v[n - 4..n].copy_from_slice(&last8[0..4]);
    }
    v
}

fn run_memboot(app: &AppHandle, cache_dir: std::path::PathBuf) -> Result<(), RfError> {
    emit_progress(app, MembootProgress::FetchingPayload);
    blobs::ensure_blobs(&cache_dir)?;
    let boot_img = blobs::boot_img(&cache_dir)?;
    stage_boot_image(app, &cache_dir, &boot_img)
}

/// Shared memboot staging: init DRAM, write the given boot image, boota it, and
/// wait for the FEL device to leave the bus. Used by both the stock memboot and
/// the custom boot-screen memboot.
fn stage_boot_image(
    app: &AppHandle,
    cache_dir: &std::path::Path,
    boot_img: &[u8],
) -> Result<(), RfError> {
    let fes1 = blobs::fes1();
    // Hakchi memboots with the "SD" U-Boot variant: the last two 4-byte words
    // are swapped (a boot-source flag). The raw/"normal" U-Boot boots from NAND;
    // the SD variant boota's the image we staged in RAM.
    let uboot = to_sd_uboot(&blobs::uboot(cache_dir)?);

    emit_progress(app, MembootProgress::Connecting);
    let mut transport = open_fel()?;

    emit_progress(app, MembootProgress::InitDram);
    memboot_ops::init_dram(&mut transport, fes1)?;
    // The DRAM controller needs time to come up after fes1 runs before we can
    // write to DRAM addresses; the reference (Hakchi InitDram) sleeps 2s here.
    // Without it the first DRAM bulk write hits uninitialized memory and stalls.
    thread::sleep(Duration::from_millis(2000));

    emit_progress(app, MembootProgress::LoadingImage);
    let padded = boot_img.len().div_ceil(fel::SECTOR_SIZE) * fel::SECTOR_SIZE;
    if padded as u32 > fel::TRANSFER_MAX_SIZE {
        return Err(RfError::ExecFailed("boot image too large".into()));
    }
    let mut kernel = boot_img.to_vec();
    kernel.resize(padded, 0);
    memboot_ops::write_memory(&mut transport, fel::TRANSFER_BASE, &kernel)?;

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
        thread::sleep(Duration::from_millis(500));
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
        // Hold off the poll thread for the duration so it does not contend for
        // the USB interface mid-transfer. Reset on every exit path.
        DEVICE_BUSY.store(true, Ordering::SeqCst);
        let result = run_memboot(&app, cache_dir);
        DEVICE_BUSY.store(false, Ordering::SeqCst);
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

use crate::device::bootscreen;

fn run_bootscreen(
    app: &AppHandle,
    cache_dir: std::path::PathBuf,
    png: &[u8],
) -> Result<(), RfError> {
    emit_progress(app, MembootProgress::FetchingPayload);
    blobs::ensure_blobs(&cache_dir)?;
    let stock = blobs::boot_img(&cache_dir)?;
    let custom = bootscreen::build_custom_bootimg(&stock, png)?;
    stage_boot_image(app, &cache_dir, &custom)
}

#[tauri::command]
fn memboot_bootscreen(app: AppHandle, png: Vec<u8>) {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    std::thread::spawn(move || {
        // Hold off the poll thread for the duration so it does not contend for
        // the USB interface mid-transfer. Reset on every exit path.
        DEVICE_BUSY.store(true, Ordering::SeqCst);
        let result = run_bootscreen(&app, cache_dir, &png);
        DEVICE_BUSY.store(false, Ordering::SeqCst);
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

const SHELL_PROGRESS_EVENT: &str = "shell-progress";

#[derive(Clone, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
enum ShellProgress {
    FetchingPayload,
    Membooting,
    WaitingForShell,
    Running,
    Done { stdout: String, exit_code: i32 },
    Failed { message: String },
}

fn emit_shell(app: &AppHandle, p: ShellProgress) {
    let _ = app.emit(SHELL_PROGRESS_EVENT, p);
}

/// Memboot the plain boot image (brings up the RNDIS gadget), then SSH to the
/// device and run the command. The default boot.img starts dropbear on
/// 169.254.13.37:22; no cmdline injection is needed.
fn run_shell(app: &AppHandle, cache_dir: std::path::PathBuf, command: &str) -> Result<(), RfError> {
    emit_shell(app, ShellProgress::FetchingPayload);
    blobs::ensure_blobs(&cache_dir)?;
    let fes1 = blobs::fes1();
    let uboot = to_sd_uboot(&blobs::uboot(&cache_dir)?);
    let boot_img = blobs::boot_img(&cache_dir)?;

    emit_shell(app, ShellProgress::Membooting);
    {
        let mut transport = open_fel()?;
        memboot_ops::init_dram(&mut transport, fes1)?;
        // DRAM settle after fes1 exec. 5s (vs memboot's 2s) — marginal devices
        // need longer before the first DRAM write lands.
        thread::sleep(Duration::from_millis(5000));
        let padded = boot_img.len().div_ceil(fel::SECTOR_SIZE) * fel::SECTOR_SIZE;
        if padded as u32 > fel::TRANSFER_MAX_SIZE {
            return Err(RfError::ExecFailed("boot image too large".into()));
        }
        let mut kernel = boot_img.clone();
        kernel.resize(padded, 0);
        memboot_ops::write_memory(&mut transport, fel::TRANSFER_BASE, &kernel)?;
        let cmd = format!("boota {:x}", fel::TRANSFER_BASE);
        memboot_ops::run_uboot_cmd(&mut transport, &uboot, &cmd)?;
    }

    emit_shell(app, ShellProgress::WaitingForShell);
    // Two-phase failure localization:
    //   - FEL never leaves the bus  -> image didn't boot (corrupt upload /
    //     failed boota) -> MembootTimeout, retry on a healthy session.
    //   - FEL left but SSH never reachable -> booted but no network shell
    //     (host RNDIS NIC unbound, RNDIS down) -> ShellNotFound.
    let deadline = Instant::now() + Duration::from_secs(15);
    while fel_present() {
        if Instant::now() >= deadline {
            return Err(RfError::MembootTimeout);
        }
        thread::sleep(Duration::from_millis(500));
    }
    netshell::wait_for_ssh(netshell::DEVICE_IP, Duration::from_secs(30))?;

    emit_shell(app, ShellProgress::Running);
    let out = netshell::run_ssh_command(netshell::DEVICE_IP, "root", command)?;
    emit_shell(
        app,
        ShellProgress::Done {
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            exit_code: out.exit_code,
        },
    );
    Ok(())
}

#[tauri::command]
fn open_shell_and_run(app: AppHandle, command: String) {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    std::thread::spawn(move || {
        DEVICE_BUSY.store(true, Ordering::SeqCst);
        let result = run_shell(&app, cache_dir, &command);
        DEVICE_BUSY.store(false, Ordering::SeqCst);
        if let Err(e) = result {
            emit_shell(
                &app,
                ShellProgress::Failed {
                    message: e.to_string(),
                },
            );
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
        .invoke_handler(tauri::generate_handler![
            get_device_status,
            memboot,
            memboot_bootscreen,
            open_shell_and_run
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            thread::spawn(move || {
                let mut monitor = Monitor::new(UsbProbe::new());
                loop {
                    // Skip probing while a memboot holds the interface.
                    if !DEVICE_BUSY.load(Ordering::SeqCst) {
                        if let Some(status) = monitor.tick() {
                            let _ = handle.emit(STATUS_EVENT, status);
                        }
                    }
                    thread::sleep(Duration::from_millis(1000));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
