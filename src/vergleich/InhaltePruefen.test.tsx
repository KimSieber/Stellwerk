import { describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { aufrufeVon, rustAttrappe, type Antworten } from "../test/hilfen";
import { kanalSenden } from "../test/vergleich";
import { InhaltePruefen, pruefMeldung } from "./InhaltePruefen";

function baustein(antworten: Antworten) {
  const aufrufe = rustAttrappe({ vergleich_abbrechen: () => true, ...antworten });
  const onGeprueft = vi.fn();
  render(<InhaltePruefen vergleichId="web/server#3" strecke="web" stufe="server" ungeprueft={{ anzahl: 37, bytes: 12_400_000 }} onGeprueft={onGeprueft} />);
  return { aufrufe, onGeprueft };
}

/** Prüfung, die der Test selbst beendet. */
function steuerbar() {
  const s = { kanal: { id: 0 }, ok: (_: unknown) => {}, nein: (_: unknown) => {} };
  const antwort = (args: unknown) => {
    s.kanal = (args as { fortschritt: { id: number } }).fortschritt;
    return new Promise((ok, nein) => {
      s.ok = ok;
      s.nein = nein;
    });
  };
  return { s, antwort };
}

const knopf = (name: string) => screen.getByRole("button", { name });

describe("Inhalte prüfen", () => {
  it("erst nach Bestätigung; Fortschritt; danach meldet es das Ergebnis", async () => {
    const { s, antwort } = steuerbar();
    const { aufrufe, onGeprueft } = baustein({ inhalte_pruefen: antwort });
    expect(screen.getByRole("region", { name: "Inhalte prüfen" })).toHaveTextContent("37 Dateien mit gleicher Größe – Inhalt nicht geprüft (12,4 MB)");
    await userEvent.click(knopf("Inhalte prüfen …"));
    expect(screen.getByText(/einmal vom Server herunter/)).toHaveTextContent("Lädt 37 Dateien (12,4 MB)");
    await userEvent.click(knopf("Abbrechen"));
    expect(aufrufeVon(aufrufe, "inhalte_pruefen")).toHaveLength(0);

    await userEvent.click(knopf("Inhalte prüfen …"));
    await userEvent.click(knopf("Herunterladen und prüfen"));
    expect(aufrufeVon(aufrufe, "inhalte_pruefen")[0].args).toMatchObject({ vergleichId: "web/server#3" });
    expect(screen.getByText("Prüfe … 0 von 37")).toBeInTheDocument();
    act(() => kanalSenden(s.kanal, 0, 12));
    expect(screen.getByText("Prüfe … 12 von 37")).toBeInTheDocument();
    const ergebnis = { geprueft: 37, fehler: [] };
    await act(async () => s.ok(ergebnis));
    expect(onGeprueft).toHaveBeenCalledWith(ergebnis);
    // Späte Meldungen ändern nichts mehr
    act(() => kanalSenden(s.kanal, 1, 37));
  });

  it("Abbrechen und Fehler: Meldung, danach erneut möglich", async () => {
    const { s, antwort } = steuerbar();
    const { aufrufe } = baustein({ inhalte_pruefen: antwort });
    await userEvent.click(knopf("Inhalte prüfen …"));
    await userEvent.click(knopf("Herunterladen und prüfen"));
    await userEvent.click(knopf("Abbrechen"));
    expect(aufrufeVon(aufrufe, "vergleich_abbrechen")[0].args).toEqual({ strecke: "web", stufe: "server" });
    expect(screen.getByText("Wird abgebrochen …")).toBeInTheDocument();
    expect(knopf("Abbrechen")).toBeDisabled();
    await act(async () => s.nein({ kategorie: "abbruch", meldung: "Abgebrochen" }));
    expect(screen.getByText("Prüfung abgebrochen.")).toHaveClass("fehler");

    await userEvent.click(knopf("Inhalte prüfen …"));
    await userEvent.click(knopf("Herunterladen und prüfen"));
    await act(async () => s.nein({ kategorie: "verbindung", meldung: "Verbindung: weg" }));
    expect(screen.getByText("Verbindung: weg")).toHaveClass("fehler");
    expect(knopf("Inhalte prüfen …")).toBeInTheDocument();
  });

  it("Rückmeldung mit und ohne nicht lesbare Dateien", () => {
    expect(pruefMeldung({ geprueft: 1, fehler: [] })).toBe("Inhalt von 1 Datei geprüft");
    expect(pruefMeldung({ geprueft: 3, fehler: [{ pfad: "a.css", meldung: "550" }, { pfad: "b.js", meldung: "550" }] })).toBe(
      "Inhalt von 3 Dateien geprüft · 2 Dateien nicht lesbar: a.css, b.js",
    );
  });
});
