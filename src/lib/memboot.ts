import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type MembootProgress =
  | { phase: "fetchingPayload" }
  | { phase: "connecting" }
  | { phase: "initDram" }
  | { phase: "loadingImage" }
  | { phase: "loadingUboot" }
  | { phase: "executing" }
  | { phase: "waitingForExit" }
  | { phase: "success" }
  | { phase: "failed"; message: string };

export function startMemboot(): Promise<void> {
  return invoke<void>("memboot");
}

export function onMembootProgress(
  handler: (p: MembootProgress) => void,
): Promise<UnlistenFn> {
  return listen<MembootProgress>("memboot-progress", (e) => handler(e.payload));
}
