import "@testing-library/jest-dom";
import { vi } from "vitest";

// Stub out Tauri IPC so components that call invoke/listen don't throw in jsdom.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
