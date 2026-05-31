import { useEffect, useState } from "react";
import { onShellProgress, openShellAndRun, type ShellProgress } from "@/lib/shell";

function labelFor(phase: ShellProgress["phase"]): string {
  switch (phase) {
    case "fetchingPayload":
      return "Fetching boot payload…";
    case "membooting":
      return "Membooting into shell…";
    case "waitingForShell":
      return "Waiting for shell…";
    case "running":
      return "Running…";
    default:
      return "";
  }
}

export function ShellRunner({
  connected,
  initialProgress,
}: {
  connected: boolean;
  initialProgress?: ShellProgress;
}) {
  const [progress, setProgress] = useState<ShellProgress | undefined>(initialProgress);

  useEffect(() => {
    const unlisten = onShellProgress(setProgress);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const running =
    progress !== undefined && progress.phase !== "done" && progress.phase !== "failed";

  return (
    <div className="mt-4 flex flex-col gap-2">
      <button
        type="button"
        disabled={!connected || running}
        onClick={() => {
          setProgress({ phase: "fetchingPayload" });
          openShellAndRun("uname -a").catch(() =>
            setProgress({ phase: "failed", message: "invoke failed" }),
          );
        }}
        className="rounded-md border px-4 py-2 text-sm font-medium disabled:opacity-50"
      >
        Run `uname -a` on device
      </button>
      {progress?.phase === "done" && (
        <pre className="rounded bg-gray-100 p-2 text-xs">
          {progress.stdout}
          {"\n"}exit {progress.exitCode}
        </pre>
      )}
      {progress?.phase === "failed" && (
        <p className="text-sm text-red-600">Failed: {progress.message}</p>
      )}
      {progress && progress.phase !== "done" && progress.phase !== "failed" && (
        <p className="text-sm text-gray-600">{labelFor(progress.phase)}</p>
      )}
    </div>
  );
}
