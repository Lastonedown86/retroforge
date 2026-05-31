mod device;
mod error;

use std::thread;
use std::time::Duration;

use tauri::Emitter;

use crate::device::usb::UsbProbe;
use crate::device::{DeviceStatus, Monitor};

/// Event name carrying live status changes to the frontend.
const STATUS_EVENT: &str = "device-status-changed";

/// One-shot status read for initial render. Builds a throwaway Monitor so a
/// single probe maps through the same `ProbeOutcome -> DeviceStatus` path.
#[tauri::command]
fn get_device_status() -> DeviceStatus {
    let mut m = Monitor::new(UsbProbe);
    m.tick().unwrap_or(DeviceStatus::Disconnected)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_device_status])
        .setup(|app| {
            let handle = app.handle().clone();
            thread::spawn(move || {
                let mut monitor = Monitor::new(UsbProbe);
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
