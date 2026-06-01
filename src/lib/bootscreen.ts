import { invoke } from "@tauri-apps/api/core";

/** Send PNG bytes to the backend; it builds a custom boot.img and memboots it. */
export function membootBootscreen(bytes: Uint8Array): Promise<void> {
  return invoke<void>("memboot_bootscreen", { png: Array.from(bytes) });
}
