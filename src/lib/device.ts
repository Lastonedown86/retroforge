import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface SocInfo {
  socId: number;
  name: string;
}

export type DeviceStatus =
  | { state: "disconnected" }
  | { state: "detectedNoDriver" }
  | { state: "connected"; soc: SocInfo };

export function getDeviceStatus(): Promise<DeviceStatus> {
  return invoke<DeviceStatus>("get_device_status");
}

export function onDeviceStatusChanged(
  handler: (status: DeviceStatus) => void,
): Promise<UnlistenFn> {
  return listen<DeviceStatus>("device-status-changed", (event) =>
    handler(event.payload),
  );
}
