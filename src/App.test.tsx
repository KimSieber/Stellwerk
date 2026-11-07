import { describe, expect, it, vi } from "vitest";
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { emit } from "@tauri-apps/api/event";

import App from "./App";
import { beispielStrecke, mitQuery, rustAttrappe } from "./test/hilfen";

const liste = [
  { id: "mailwerk-web", name: "Mailwerk Webseite", stufen: [], probleme: [], fehler: null },
];

function rust() {
  return rustAttrappe({
    strecken_liste: () => liste,
    konfig_ort: () => "/x",
    strecke_laden: () => beispielStrecke(),
    strecke_pruefen: () => [],
    strecke_anlegen: () => [],
    passwort_vorhanden: () => false,
    globale_regeln: () => [],
    "plugin:app|version": () => "0.1.0",
  });
}

describe("App", () => {
  it("zeigt Kopfzeile und Startseite", async () => {
    rust();
    mitQuery(<App />);
    expect(screen.getByRole("heading", { level: 1, name: "Stellwerk" })).toBeInTheDocument();
    expect(screen.getByText("Kontrollierte Transportstrecken für Dateien")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 2, name: "Strecken" })).toBeInTheDocument();
  });

  it("Startseite → Bearbeiten → zurück", async () => {
    rust();
    mitQuery(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "Bearbeiten" }));
    expect(await screen.findByLabelText("Kennung")).toHaveValue("mailwerk-web");
    await userEvent.click(screen.getByRole("button", { name: "← Strecken" }));
    expect(await screen.findByRole("heading", { level: 2, name: "Strecken" })).toBeInTheDocument();
  });

  it("neue Strecke: nach dem Anlegen wird sie zum Bearbeiten geöffnet", async () => {
    rust();
    mitQuery(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "+ Neue Strecke" }));
    expect(screen.getByRole("heading", { name: "Neue Strecke" })).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Name"), "Neu");
    await waitFor(() => expect(screen.getByRole("button", { name: "Speichern" })).toBeEnabled());
    await userEvent.click(screen.getByRole("button", { name: "Speichern" }));
    // Danach ist die Kennung fest (Bearbeiten einer vorhandenen Strecke)
    await waitFor(() => expect(screen.getByLabelText("Kennung")).toHaveAttribute("readonly"));
  });

  it("Menübefehl „Über Stellwerk“ öffnet den Dialog", async () => {
    rust();
    const { unmount } = mitQuery(<App />);
    await screen.findByRole("heading", { level: 2, name: "Strecken" });
    expect(screen.queryByRole("dialog")).toBeNull();
    await act(() => emit("menue_ueber"));
    expect(await screen.findByRole("dialog", { name: "Über Stellwerk" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Schließen" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    unmount(); // meldet den Ereignis-Empfänger ab
  });
});

describe("Einstiegspunkt main.tsx", () => {
  it("rendert die App in #root", async () => {
    rust();
    document.body.innerHTML = '<div id="root"></div>';
    vi.resetModules();
    await import("./main");
    expect(await screen.findByRole("heading", { level: 1, name: "Stellwerk" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Mailwerk Webseite" })).toBeInTheDocument();
  });
});
