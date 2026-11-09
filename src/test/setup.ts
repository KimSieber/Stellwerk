import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, vi } from "vitest";

import { fensterAttrappe } from "./ablage";
import { appFensterAttrappe, fensterZu } from "./fenster";

// Kein echtes Tauri-Fenster im Test: Drag-and-drop über die Attrappe
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: fensterAttrappe }));
// Kein echtes App-Fenster: Schließen und Beenden über die Attrappe (v0.6.1)
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: appFensterAttrappe }));

afterEach(async () => {
  cleanup();
  // Abmeldungen von Ereignis-Empfängern laufen asynchron – erst danach die Attrappe entfernen.
  await new Promise((r) => setTimeout(r, 0));
  clearMocks();
  fensterZu.mockClear();
  localStorage.clear(); // z. B. aufgeklappte Gruppen der Startseite (v0.8.1)
});
