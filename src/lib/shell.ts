import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ShellProgress =
  | { phase: "fetchingPayload" }
  | { phase: "membooting" }
  | { phase: "waitingForShell" }
  | { phase: "running" }
  | { phase: "done"; stdout: string; exitCode: number }
  | { phase: "failed"; message: string };

export function openShellAndRun(command: string): Promise<void> {
  return invoke<void>("open_shell_and_run", { command });
}

export function onShellProgress(
  handler: (p: ShellProgress) => void,
): Promise<UnlistenFn> {
  return listen<ShellProgress>("shell-progress", (e) => handler(e.payload));
}
