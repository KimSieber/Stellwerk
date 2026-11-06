import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";

import App from "./App";
import { mitQuery, rustAttrappe } from "./test/hilfen";

describe("App", () => {
  it("zeigt Kopfzeile und Startseite", async () => {
    rustAttrappe({ strecken_liste: () => [], konfig_ort: () => "/x" });
    mitQuery(<App />);
    expect(screen.getByRole("heading", { level: 1, name: "Stellwerk" })).toBeInTheDocument();
    expect(screen.getByText("Kontrollierte Transportstrecken für Dateien")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 2, name: "Strecken" })).toBeInTheDocument();
  });
});

describe("Einstiegspunkt main.tsx", () => {
  it("rendert die App in #root", async () => {
    rustAttrappe({ strecken_liste: () => [], konfig_ort: () => "/x" });
    document.body.innerHTML = '<div id="root"></div>';
    vi.resetModules();
    await import("./main");
    expect(await screen.findByRole("heading", { level: 1, name: "Stellwerk" })).toBeInTheDocument();
    expect(await screen.findByText("Noch keine Strecke angelegt.")).toBeInTheDocument();
  });
});
