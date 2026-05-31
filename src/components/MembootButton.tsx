import { useEffect, useState } from "react";
import {
  onMembootProgress,
  startMemboot,
  type MembootProgress,
} from "@/lib/memboot";

const LABELS: Record<MembootProgress["phase"], string> = {
  fetchingPayload: "Fetching boot payload…",
  connecting: "Connecting…",
  initDram: "Initializing DRAM…",
  loadingImage: "Loading boot image…",
  loadingUboot: "Loading U-Boot…",
  executing: "Executing…",
  waitingForExit: "Waiting for device to boot…",
  success: "Booted — check the TV.",
  failed: "Memboot failed.",
};

export function MembootButton({
  connected,
  initialPhase,
}: {
  connected: boolean;
  initialPhase?: MembootProgress;
}) {
  const [progress, setProgress] = useState<MembootProgress | undefined>(initialPhase);

  useEffect(() => {
    const unlisten = onMembootProgress(setProgress);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const running =
    progress !== undefined &&
    progress.phase !== "success" &&
    progress.phase !== "failed";

  return (
    <div className="mt-4 flex flex-col gap-2">
      <button
        type="button"
        disabled={!connected || running}
        onClick={() => {
          setProgress({ phase: "fetchingPayload" });
          startMemboot().catch(() => setProgress({ phase: "failed", message: "invoke failed" }));
        }}
        className="rounded-md border px-4 py-2 text-sm font-medium disabled:opacity-50"
      >
        Memboot (test)
      </button>
      {progress && (
        <p className="text-sm text-gray-600">
          {LABELS[progress.phase]}
          {progress.phase === "failed" ? ` (${progress.message})` : ""}
        </p>
      )}
    </div>
  );
}
