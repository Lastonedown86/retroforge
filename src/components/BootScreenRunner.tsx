import { useEffect, useState } from "react";
import { membootBootscreen } from "@/lib/bootscreen";
import { onMembootProgress, type MembootProgress } from "@/lib/memboot";

const LABELS: Record<MembootProgress["phase"], string> = {
  fetchingPayload: "Building custom boot image…",
  connecting: "Connecting…",
  initDram: "Initializing DRAM…",
  loadingImage: "Loading boot image…",
  loadingUboot: "Loading U-Boot…",
  executing: "Executing…",
  waitingForExit: "Waiting for device to boot…",
  success: "Booted — check the TV.",
  failed: "Boot-screen memboot failed.",
};

export function BootScreenRunner({ connected }: { connected: boolean }) {
  const [progress, setProgress] = useState<MembootProgress | undefined>();

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

  async function onPick(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    // Reset the input so the same file can be re-selected later.
    e.target.value = "";
    if (!file) return;
    const bytes = new Uint8Array(await file.arrayBuffer());
    setProgress({ phase: "fetchingPayload" });
    try {
      await membootBootscreen(bytes);
    } catch {
      setProgress({ phase: "failed", message: "invoke failed" });
    }
  }

  return (
    <div className="mt-4 flex flex-col gap-2">
      <label className="rounded-md border px-4 py-2 text-sm font-medium disabled:opacity-50">
        Choose boot-screen PNG…
        <input
          type="file"
          accept="image/png"
          disabled={!connected || running}
          onChange={onPick}
          className="hidden"
        />
      </label>
      {progress && (
        <p className="text-sm text-gray-600">
          {LABELS[progress.phase]}
          {progress.phase === "failed" ? ` (${progress.message})` : ""}
        </p>
      )}
    </div>
  );
}
