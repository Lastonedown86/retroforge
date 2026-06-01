import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { type MembootProgress } from "@/lib/memboot";

/** Send PNG bytes to the backend; it builds a custom boot.img and memboots it. */
export function membootBootscreen(bytes: Uint8Array): Promise<void> {
  return invoke<void>("memboot_bootscreen", { png: Array.from(bytes) });
}

export function onBootscreenProgress(
  handler: (p: MembootProgress) => void,
): Promise<UnlistenFn> {
  return listen<MembootProgress>("bootscreen-progress", (e) =>
    handler(e.payload),
  );
}
