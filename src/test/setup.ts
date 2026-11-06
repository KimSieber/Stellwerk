import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach } from "vitest";

afterEach(async () => {
  cleanup();
  // Abmeldungen von Ereignis-Empfängern laufen asynchron – erst danach die Attrappe entfernen.
  await new Promise((r) => setTimeout(r, 0));
  clearMocks();
});
