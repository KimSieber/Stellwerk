import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, vi } from "vitest";

import { fensterAttrappe } from "./ablage";

// Kein echtes Tauri-Fenster im Test: Drag-and-drop über die Attrappe
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: fensterAttrappe }));

afterEach(async () => {
  cleanup();
  // Abmeldungen von Ereignis-Empfängern laufen asynchron – erst danach die Attrappe entfernen.
  await new Promise((r) => setTimeout(r, 0));
  clearMocks();
});
