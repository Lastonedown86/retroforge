import { useEffect, useState } from "react";
import { StatusPanel } from "@/components/StatusPanel";
import {
  getDeviceStatus,
  onDeviceStatusChanged,
  type DeviceStatus,
} from "@/lib/device";

export default function App() {
  const [status, setStatus] = useState<DeviceStatus>({ state: "disconnected" });

  useEffect(() => {
    getDeviceStatus().then(setStatus).catch(() => {});
    const unlisten = onDeviceStatusChanged(setStatus);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <main className="mx-auto max-w-md p-8">
      <h1 className="mb-4 text-xl font-bold">RetroForge</h1>
      <StatusPanel status={status} />
    </main>
  );
}
