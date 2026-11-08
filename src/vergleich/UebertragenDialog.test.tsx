import { describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { LaufAntwort, LaufErgebnis, PlanVorschau } from "../api";
import { aufrufeVon, rustAttrappe, type Antworten } from "../test/hilfen";
import { kanalSenden } from "../test/vergleich";
import { UebertragenDialog } from "./UebertragenDialog";

function plan(teil: Partial<PlanVorschau> = {}): PlanVorschau {
  return {
    zusammenfassung: { ordner_anlegen: 1, kopieren: 2, ueberschreiben: 0, rechte: 0, dateien_loeschen: 1, ordner_loeschen: 0, bytes: 3000 },
    loeschliste: ["alt.txt"],
    warnungen: [],
    quelle: "/Users/kim/Dev/web",
    ziel: "/Users/kim/Server/web",
    worker: 8,
    modus: "direkt",
    ...teil,
  };
}

function ergebnis(teil: Partial<LaufErgebnis> = {}): LaufErgebnis {
  return { erledigt: 4, fehler: [], uebersprungen: 0, abgebrochen: false, gestoppt: false, bytes: 3000, journal_fehler: null, ...teil };
}

const OHNE_LOESCHEN = plan({
  loeschliste: [],
  zusammenfassung: { ordner_anlegen: 0, kopieren: 1, ueberschreiben: 0, rechte: 0, dateien_loeschen: 0, ordner_loeschen: 0, bytes: 10 },
});

/** Lauf, den der Test selbst beendet; `kanal` ist der Fortschrittskanal. */
function steuerbarerLauf() {
  const s = { kanal: { id: 0 }, fertig: (_a: LaufAntwort) => {}, fehler: (_e: unknown) => {} };
  const antwort = (args: unknown) => {
    s.kanal = (args as { fortschritt: { id: number } }).fortschritt;
    return new Promise((ok, nein) => {
      s.fertig = ok;
      s.fehler = nein;
    });
  };
  return { s, antwort };
}

function dialog(antworten: Antworten) {
  const aufrufe = rustAttrappe({ lauf_abbrechen: () => true, ...antworten });
  const onAbbrechen = vi.fn();
  const onFertig = vi.fn();
  const r = render(<UebertragenDialog vergleichId="web/spiegel#1" strecke="web" stufe="spiegel" onAbbrechen={onAbbrechen} onFertig={onFertig} />);
  return { aufrufe, onAbbrechen, onFertig, r };
}

const knopf = (name: string) => screen.getByRole("button", { name });
const hintergrund = () => document.querySelector(".dialog-hintergrund") as HTMLElement;

describe("Übertragen – Bestätigung", () => {
  it("zeigt den Plan; Löschungen müssen ausdrücklich bestätigt werden", async () => {
    const { aufrufe } = dialog({ plan_erstellen: () => plan() });
    expect(screen.getByText("Plan wird erstellt …")).toBeInTheDocument();
    expect(await screen.findByText("1 Ordner anlegen")).toBeInTheDocument();
    expect(aufrufeVon(aufrufe, "plan_erstellen")[0].args).toEqual({ vergleichId: "web/spiegel#1" });
    expect(screen.getByText("2 Dateien neu")).toBeInTheDocument();
    expect(screen.getByText("1 Datei löschen")).toBeInTheDocument();
    expect(screen.getByText("3 KB")).toBeInTheDocument();
    expect(screen.getByText("/Users/kim/Server/web")).toBeInTheDocument();
    expect(screen.getByText(/bis zu 8 gleichzeitig/)).toBeInTheDocument();

    expect(screen.getByText("1 Eintrag wird am Ziel gelöscht")).toBeInTheDocument();
    expect(screen.getByText("alt.txt")).toBeInTheDocument();
    expect(knopf("Übertragen")).toBeDisabled();
    await userEvent.click(screen.getByLabelText("Löschungen bestätigen"));
    expect(knopf("Übertragen")).toBeEnabled();
    await userEvent.click(screen.getByLabelText("Löschungen bestätigen"));
    expect(knopf("Übertragen")).toBeDisabled();
  });

  it("mehrere Löschungen und Warnung bei weltschreibbaren Rechten", async () => {
    dialog({
      plan_erstellen: () =>
        plan({ loeschliste: ["a", "b"], warnungen: [{ art: "weltschreibbar", modus: "777", anzahl: 3 }] }),
    });
    expect(await screen.findByText("2 Einträge werden am Ziel gelöscht")).toBeInTheDocument();
    expect(screen.getByText("⚠ Rechte 777 für 3 Einträge – für alle schreibbar")).toBeInTheDocument();
  });

  it("nichts ausgewählt: nichts zu übertragen", async () => {
    const leer = { ordner_anlegen: 0, kopieren: 0, ueberschreiben: 0, rechte: 0, dateien_loeschen: 0, ordner_loeschen: 0, bytes: 0 };
    dialog({ plan_erstellen: () => plan({ loeschliste: [], zusammenfassung: leer }) });
    expect(await screen.findByText("Nichts ausgewählt – es gibt nichts zu übertragen.")).toBeInTheDocument();
    expect(knopf("Übertragen")).toBeDisabled();
    expect(screen.queryByLabelText("Beim ersten Fehler anhalten")).toBeNull();
  });

  it("Abbrechen, Klick daneben und Escape-freier Klick im Dialog", async () => {
    const { onAbbrechen } = dialog({ plan_erstellen: () => plan() });
    await screen.findByText("1 Ordner anlegen");
    await userEvent.click(screen.getByRole("dialog"));
    expect(onAbbrechen).not.toHaveBeenCalled();
    await userEvent.click(hintergrund());
    expect(onAbbrechen).toHaveBeenCalledTimes(1);
    await userEvent.click(knopf("Abbrechen"));
    expect(onAbbrechen).toHaveBeenCalledTimes(2);
  });

  it("Fehler beim Planen: Meldung, Schließen ohne neu zu vergleichen", async () => {
    const { onAbbrechen, onFertig } = dialog({
      plan_erstellen: () => {
        throw { kategorie: "nicht_unterstuetzt", meldung: "„Mit Umschalten“ kommt später" };
      },
    });
    expect(await screen.findByText("„Mit Umschalten“ kommt später")).toHaveClass("fehler");
    await userEvent.click(hintergrund());
    expect(onAbbrechen).toHaveBeenCalledTimes(1);
    await userEvent.click(knopf("Schließen"));
    expect(onAbbrechen).toHaveBeenCalledTimes(2);
    expect(onFertig).not.toHaveBeenCalled();
  });

  it("Ergebnis nach dem Schließen wird nicht mehr angezeigt", async () => {
    let ok: (p: PlanVorschau) => void = () => {};
    let nein: (e: unknown) => void = () => {};
    const zuerst = dialog({ plan_erstellen: () => new Promise((a) => (ok = a)) });
    zuerst.r.unmount();
    await act(async () => ok(plan()));
    const danach = dialog({ plan_erstellen: () => new Promise((_a, b) => (nein = b)) });
    danach.r.unmount();
    await act(async () => nein({ kategorie: "io", meldung: "x" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("Übertragen – Lauf", () => {
  it("überträgt mit Bestätigung, zeigt Fortschritt und Ergebnis", async () => {
    const { s, antwort } = steuerbarerLauf();
    const { aufrufe, onFertig, onAbbrechen } = dialog({ plan_erstellen: () => plan(), lauf_starten: antwort });
    await screen.findByText("1 Ordner anlegen");
    await userEvent.click(screen.getByLabelText("Löschungen bestätigen"));
    await userEvent.click(knopf("Übertragen"));

    expect(aufrufeVon(aufrufe, "lauf_starten")[0].args).toMatchObject({
      vergleichId: "web/spiegel#1",
      loeschenBestaetigt: true,
      beiFehlerStoppen: false,
    });
    expect(screen.getByText("Übertragung wird vorbereitet …")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuemax", "0");
    act(() => kanalSenden(s.kanal, 0, { fertig: 2, gesamt: 4, bytes: 1500, bytes_gesamt: 3000, fehler: 0 }));
    expect(screen.getByText("2 von 4 · 1,5 KB von 3 KB")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "2");
    expect((screen.getByRole("progressbar").firstChild as HTMLElement).style.width).toBe("50%");

    // Während des Laufs lässt sich der Dialog nicht wegklicken
    await userEvent.click(hintergrund());
    expect(onAbbrechen).not.toHaveBeenCalled();

    await act(async () => s.fertig({ lauf_id: "mw5ha39c", ergebnis: ergebnis(), phase: "fertig" }));
    expect(screen.getByText("✓ 4 Schritte erledigt · 3 KB übertragen")).toHaveClass("ok");
    expect(screen.getByText("Lauf mw5ha39c · jeder Schritt steht im Journal.")).toBeInTheDocument();
    // Späte Fortschrittsmeldungen ändern nichts mehr
    act(() => kanalSenden(s.kanal, 1, { fertig: 4, gesamt: 4, bytes: 3000, bytes_gesamt: 3000, fehler: 0 }));
    expect(screen.getByText("✓ 4 Schritte erledigt · 3 KB übertragen")).toBeInTheDocument();

    await userEvent.click(hintergrund());
    expect(onAbbrechen).not.toHaveBeenCalled();
    await userEvent.click(knopf("Schließen und neu vergleichen"));
    expect(onFertig).toHaveBeenCalled();
  });

  it("ohne Löschungen, mit „beim ersten Fehler anhalten“, Abbrechen während des Laufs", async () => {
    const { s, antwort } = steuerbarerLauf();
    const { aufrufe } = dialog({ plan_erstellen: () => OHNE_LOESCHEN, lauf_starten: antwort });
    await screen.findByText("1 Datei neu");
    expect(screen.queryByLabelText("Löschungen bestätigen")).toBeNull();
    await userEvent.click(screen.getByLabelText("Beim ersten Fehler anhalten"));
    await userEvent.click(knopf("Übertragen"));
    expect(aufrufeVon(aufrufe, "lauf_starten")[0].args).toMatchObject({ loeschenBestaetigt: false, beiFehlerStoppen: true });

    await userEvent.click(knopf("Abbrechen"));
    expect(aufrufeVon(aufrufe, "lauf_abbrechen")[0].args).toEqual({ strecke: "web", stufe: "spiegel" });
    expect(screen.getByText("Wird abgebrochen – laufende Dateien werden sauber beendet …")).toBeInTheDocument();
    expect(knopf("Abbrechen")).toBeDisabled();

    await act(async () => s.fertig({ lauf_id: "l", ergebnis: ergebnis({ erledigt: 1, uebersprungen: 2, abgebrochen: true, bytes: 0 }), phase: "abgebrochen" }));
    expect(screen.getByText("1 Schritt erledigt · 0 B übertragen")).not.toHaveClass("ok");
    expect(screen.getByText(/^Abgebrochen/)).toBeInTheDocument();
    expect(screen.getByText("2 Schritte nicht ausgeführt.")).toBeInTheDocument();
  });

  it("Fehler je Datei, Anhalten und Journalfehler werden gezeigt", async () => {
    const fehler = [
      { nr: 1, art: "kopieren" as const, pfad: "index.php", meldung: "Ein-/Ausgabefehler" },
      { nr: 3, art: "datei_loeschen" as const, pfad: "alt.txt", meldung: "gesperrt" },
    ];
    dialog({
      plan_erstellen: () => OHNE_LOESCHEN,
      lauf_starten: () => ({ lauf_id: "l", ergebnis: ergebnis({ erledigt: 2, fehler, gestoppt: true, journal_fehler: "Platte voll" }), phase: "fertig" }),
    });
    await userEvent.click(await screen.findByRole("button", { name: "Übertragen" }));
    expect(await screen.findByText("2 Fehler")).toBeInTheDocument();
    expect(screen.getByText("Nach dem ersten Fehler angehalten.")).toBeInTheDocument();
    expect(screen.getByText("Journal konnte nicht geschrieben werden: Platte voll")).toBeInTheDocument();
    expect(screen.getByText("index.php").closest("li")).toHaveTextContent("Kopieren index.php – Ein-/Ausgabefehler");
    expect(screen.getByText("alt.txt").closest("li")).toHaveTextContent("Löschen alt.txt – gesperrt");
  });

  it("abgewiesener Lauf: Meldung, Schließen vergleicht neu", async () => {
    const { onFertig, onAbbrechen } = dialog({
      plan_erstellen: () => OHNE_LOESCHEN,
      lauf_starten: () => {
        throw { kategorie: "vorhanden", meldung: "Übertragung läuft bereits" };
      },
    });
    await userEvent.click(await screen.findByRole("button", { name: "Übertragen" }));
    expect(await screen.findByText("Übertragung läuft bereits")).toHaveClass("fehler");
    await userEvent.click(hintergrund());
    expect(onAbbrechen).not.toHaveBeenCalled();
    await userEvent.click(knopf("Schließen"));
    expect(onFertig).toHaveBeenCalled();
  });

  it("Umschalten: Hinweis zum Modus und zum Stand nach dem Hochladen", async () => {
    for (const [modus, phase, text, art] of [
      ["umschalten_manuell", "hochgeladen", "Hochgeladen als Schattendateien – die Live-Seite ist unverändert.", "warnung"],
      ["umschalten", "umgeschaltet", "Umschalten angehalten – Rollback in der Streckenansicht.", "fehler"],
    ] as const) {
      const { r, onFertig } = dialog({ plan_erstellen: () => plan({ ...OHNE_LOESCHEN, modus }), lauf_starten: () => ({ lauf_id: "l", ergebnis: ergebnis(), phase }) });
      expect(await screen.findByText(/^Mit Umschalten/)).toHaveClass("modus-hinweis");
      await userEvent.click(knopf("Übertragen"));
      expect(await screen.findByText(new RegExp(`^${text}`))).toHaveClass(art);
      await userEvent.click(knopf("Weiter in der Streckenansicht"));
      expect(onFertig).toHaveBeenCalledWith(true);
      r.unmount();
    }
  });
});
