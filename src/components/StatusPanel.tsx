import type { DeviceStatus } from "@/lib/device";

const DOT: Record<DeviceStatus["state"], string> = {
  disconnected: "bg-gray-400",
  detectedNoDriver: "bg-amber-500",
  connected: "bg-emerald-500",
};

export function StatusPanel({ status }: { status: DeviceStatus }) {
  return (
    <div className="flex items-start gap-3 rounded-lg border p-4">
      <span className={`mt-1 h-3 w-3 rounded-full ${DOT[status.state]}`} />
      <div className="text-sm">
        {status.state === "disconnected" && (
          <p className="font-medium">No device detected</p>
        )}
        {status.state === "detectedNoDriver" && (
          <div>
            <p className="font-medium">Device found — driver not bound</p>
            <p className="text-gray-500">
              Bind the WinUSB driver to the FEL device using Zadig, then reconnect.
            </p>
          </div>
        )}
        {status.state === "connected" && (
          <p className="font-medium">
            Connected — {status.soc.name} (FEL)
          </p>
        )}
      </div>
    </div>
  );
}
