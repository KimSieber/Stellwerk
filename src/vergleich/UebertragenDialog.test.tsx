import { describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { LaufAntwort, LaufErgebnis, PlanVorschau } from "../api";
import { aufrufeVon, rustAttrappe, type Antworten } from "../test/hilfen";
import { kanalSenden } from "../test/vergleich";
import { laufOffen } from "../beenden";
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

});

/** Schritte („lauf_weiter“), die der Test selbst beendet. */
function steuerbareSchritte() {
  const offen: { schritt: string; ok: (v: unknown) => void; nein: (e: unknown) => void }[] = [];
  const antwort = (args: unknown) =>
    new Promise((ok, nein) => offen.push({ schritt: (args as { schritt: string }).schritt, ok, nein }));
  return { offen, antwort };
}

const stand = (phase: string, fehler: { pfad: string; meldung: string }[] = []) => ({ lauf_id: "L", phase, fehler });
const kachel = (titel: string) => screen.getByText(titel, { selector: ".kachel-kopf b" }).closest("li") as HTMLElement;
const zustaende = () => ["Bereitstellen", "Umschalten", "Übernehmen"].map((t) => kachel(t).className.replace("kachel ", ""));
const knoepfeUnten = () => [...document.querySelectorAll(".knoepfe.unten button")].map((b) => b.textContent);

describe("Übertragen – Umschalten manuell (v0.6.1)", () => {
  it("drei Schritte nacheinander, nur der aktuelle ist aktiv; danach nur noch „Fertig“", async () => {
    const lauf = steuerbarerLauf();
    const schritte = steuerbareSchritte();
    const { aufrufe, onFertig, onAbbrechen } = dialog({
      plan_erstellen: () => plan({ modus: "umschalten_manuell" }),
      lauf_starten: lauf.antwort,
      lauf_weiter: schritte.antwort,
    });
    expect(await screen.findByText(/^Mit Umschalten \(manuell\)/)).toHaveClass("modus-hinweis");
    expect(zustaende()).toEqual(["aktiv", "aus", "aus"]);
    expect(kachel("Bereitstellen")).toHaveAttribute("aria-current", "step");
    expect(kachel("Umschalten")).toHaveTextContent("→");
    // Erst nach dem Bestätigen der Löschungen startbar; vorher nur „Abbrechen“
    expect(knopf("Bereitstellen")).toBeDisabled();
    expect(knopf("Umschalten")).toBeDisabled();
    expect(knopf("Übernehmen")).toBeDisabled();
    expect(knoepfeUnten()).toEqual(["Abbrechen"]);
    await userEvent.click(screen.getByLabelText("Löschungen bestätigen"));
    expect(laufOffen()).toBe(false);

    await userEvent.click(knopf("Bereitstellen"));
    expect(aufrufeVon(aufrufe, "lauf_starten")[0].args).toMatchObject({ loeschenBestaetigt: true, beiFehlerStoppen: false });
    expect(zustaende()).toEqual(["laeuft", "aus", "aus"]);
    expect(knopf("Bereitstellen …")).toBeDisabled();
    expect(screen.getByLabelText("Löschungen bestätigen")).toBeDisabled();
    expect(screen.getByLabelText("Beim ersten Fehler anhalten")).toBeDisabled();
    expect(knoepfeUnten()).toEqual(["Rollback – alles zurück"]);
    expect(laufOffen()).toBe(true);
    act(() => kanalSenden(lauf.s.kanal, 0, { fertig: 1, gesamt: 4, bytes: 1000, bytes_gesamt: 3000, fehler: 0 }));
    expect(screen.getByText("1 von 4 · 1 KB von 3 KB")).toBeInTheDocument();
    // Kein Aussteigen: Klick daneben tut nichts
    await userEvent.click(hintergrund());
    expect(onAbbrechen).not.toHaveBeenCalled();

    await act(async () => lauf.s.fertig({ lauf_id: "mw5", ergebnis: ergebnis(), phase: "hochgeladen" }));
    expect(screen.getByText("✓ Bereitgestellt (4 Schritte · 3 KB) – die Live-Seite ist unverändert.")).toHaveClass("ok");
    expect(zustaende()).toEqual(["erledigt", "aktiv", "aus"]);
    expect(kachel("Bereitstellen")).toHaveTextContent("✓");
    expect(knopf("Bereitstellen")).toBeDisabled();
    expect(screen.getByText("Lauf mw5 · jeder Schritt steht im Journal.")).toBeInTheDocument();
    expect(knopf("Rollback – alles zurück")).toBeEnabled();

    await userEvent.click(knopf("Umschalten"));
    expect(schritte.offen[0].schritt).toBe("umschalten");
    expect(aufrufeVon(aufrufe, "lauf_weiter")[0].args).toEqual({ laufId: "mw5", schritt: "umschalten" });
    expect(zustaende()).toEqual(["erledigt", "laeuft", "aus"]);
    expect(screen.getByText("Wird umgeschaltet …")).toBeInTheDocument();
    expect(knopf("Rollback – alles zurück")).toBeDisabled();
    await act(async () => schritte.offen[0].ok(stand("umgeschaltet")));
    expect(screen.getByText(/^✓ Umgeschaltet – die neuen Dateien sind live/)).toBeInTheDocument();
    expect(zustaende()).toEqual(["erledigt", "erledigt", "aktiv"]);

    await userEvent.click(knopf("Übernehmen"));
    expect(zustaende()).toEqual(["erledigt", "erledigt", "laeuft"]);
    expect(screen.getByText("Sicherungen werden entfernt …")).toBeInTheDocument();
    await act(async () => schritte.offen[1].ok(stand("fertig")));
    expect(screen.getByText("✓ Übernommen – fertig.")).toBeInTheDocument();
    expect(zustaende()).toEqual(["erledigt", "erledigt", "erledigt"]);
    expect(knoepfeUnten()).toEqual(["Fertig – neu vergleichen"]);
    expect(laufOffen()).toBe(false);
    await userEvent.click(knopf("Fertig – neu vergleichen"));
    expect(onFertig).toHaveBeenCalled();
  });

  it("vor dem Start: Abbrechen und Klick daneben schließen", async () => {
    const { onAbbrechen } = dialog({ plan_erstellen: () => plan({ ...OHNE_LOESCHEN, modus: "umschalten_manuell" }) });
    await userEvent.click(await screen.findByRole("button", { name: "Abbrechen" }));
    await userEvent.click(hintergrund());
    expect(onAbbrechen).toHaveBeenCalledTimes(2);
  });

  it("Verbindungsfehler bei einem Schritt: Meldung, Schritt bleibt; alles zurück scheitert – erneut versuchen", async () => {
    const schritte = steuerbareSchritte();
    const { aufrufe } = dialog({
      plan_erstellen: () => plan({ ...OHNE_LOESCHEN, modus: "umschalten_manuell" }),
      lauf_starten: () => ({ lauf_id: "L", ergebnis: ergebnis(), phase: "hochgeladen" }),
      lauf_weiter: schritte.antwort,
    });
    await userEvent.click(await screen.findByRole("button", { name: "Bereitstellen" }));
    await userEvent.click(await screen.findByRole("button", { name: "Umschalten" }));
    await act(async () => schritte.offen[0].nein({ kategorie: "verbindung", meldung: "Verbindung weg" }));
    expect(screen.getByText("Verbindung weg")).toHaveClass("fehler");
    expect(zustaende()).toEqual(["erledigt", "aktiv", "aus"]);
    expect(knopf("Umschalten")).toBeEnabled();

    // Alles zurück (vor dem Umschalten = Abbrechen); während es läuft, ist nichts anderes möglich
    await userEvent.click(knopf("Rollback – alles zurück"));
    expect(schritte.offen[1].schritt).toBe("abbrechen");
    expect(screen.getByText("Wird zurückgenommen …")).toBeInTheDocument();
    expect(knopf("Umschalten")).toBeDisabled();
    expect(screen.queryByText("Verbindung weg")).toBeNull();
    await act(async () => schritte.offen[1].nein({ kategorie: "verbindung", meldung: "Wieder weg" }));
    expect(knoepfeUnten()).toEqual(["Schließen – Lauf als unterbrochen markieren", "Rollback erneut versuchen"]);

    await userEvent.click(knopf("Rollback erneut versuchen"));
    await act(async () => schritte.offen[2].ok(stand("abgebrochen")));
    expect(screen.getByText("✓ Alles zurückgenommen – die Schattendateien sind entfernt, die Live-Seite war nie berührt.")).toBeInTheDocument();
    expect(zustaende()).toEqual(["aus", "aus", "aus"]);
    expect(knoepfeUnten()).toEqual(["Schließen und neu vergleichen"]);
    expect(aufrufeVon(aufrufe, "lauf_weiter").map((a) => (a.args as { schritt: string }).schritt)).toEqual(["umschalten", "abbrechen", "abbrechen"]);
  });

  it("Rollback nach dem Umschalten stellt den alten Stand her", async () => {
    dialog({
      plan_erstellen: () => plan({ ...OHNE_LOESCHEN, modus: "umschalten_manuell" }),
      lauf_starten: () => ({ lauf_id: "L", ergebnis: ergebnis(), phase: "hochgeladen" }),
      lauf_weiter: (a) => stand((a as { schritt: string }).schritt === "umschalten" ? "umgeschaltet" : "zurueckgerollt"),
    });
    await userEvent.click(await screen.findByRole("button", { name: "Bereitstellen" }));
    await userEvent.click(await screen.findByRole("button", { name: "Umschalten" }));
    await userEvent.click(await screen.findByRole("button", { name: "Rollback – alles zurück" }));
    expect(await screen.findByText("✓ Alles zurückgenommen – der alte Stand ist wiederhergestellt.")).toBeInTheDocument();
  });

  it("Rollback scheitert endgültig: Lauf als unterbrochen markieren und schließen (auch wenn das scheitert)", async () => {
    for (const aufgeben of [() => null, () => Promise.reject({ kategorie: "datenbank", meldung: "x" })]) {
      const { aufrufe, onFertig, r } = dialog({
        plan_erstellen: () => plan({ ...OHNE_LOESCHEN, modus: "umschalten_manuell" }),
        lauf_starten: () => ({ lauf_id: "L", ergebnis: ergebnis(), phase: "hochgeladen" }),
        lauf_weiter: (a) => {
          if ((a as { schritt: string }).schritt === "umschalten") return stand("umgeschaltet");
          throw { kategorie: "verbindung", meldung: "Server antwortet nicht" };
        },
        lauf_aufgeben: aufgeben,
      });
      await userEvent.click(await screen.findByRole("button", { name: "Bereitstellen" }));
      await userEvent.click(await screen.findByRole("button", { name: "Umschalten" }));
      await userEvent.click(await screen.findByRole("button", { name: "Rollback – alles zurück" }));
      expect(await screen.findByText("Server antwortet nicht")).toBeInTheDocument();
      await userEvent.click(knopf("Schließen – Lauf als unterbrochen markieren"));
      await vi.waitFor(() => expect(onFertig).toHaveBeenCalled());
      expect(aufrufeVon(aufrufe, "lauf_aufgeben")[0].args).toEqual({ laufId: "L" });
      r.unmount();
    }
  });
});

describe("Übertragen – Umschalten automatisch (v0.6.1)", () => {
  it("läuft von selbst durch – die Schritte sind nur Anzeige", async () => {
    const lauf = steuerbarerLauf();
    const schritte = steuerbareSchritte();
    const { aufrufe, onFertig } = dialog({ plan_erstellen: () => plan({ ...OHNE_LOESCHEN, modus: "umschalten" }), lauf_starten: lauf.antwort, lauf_weiter: schritte.antwort });
    expect(await screen.findByText(/^Mit Umschalten \(automatisch\)/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Bereitstellen" })).toBeNull();
    expect(knoepfeUnten()).toEqual(["Abbrechen", "Übertragen"]);
    await userEvent.click(knopf("Übertragen"));
    expect(zustaende()).toEqual(["laeuft", "aus", "aus"]);
    await act(async () => lauf.s.fertig({ lauf_id: "L", ergebnis: ergebnis(), phase: "hochgeladen" }));
    expect(zustaende()).toEqual(["erledigt", "laeuft", "aus"]);
    await act(async () => schritte.offen[0].ok(stand("umgeschaltet")));
    expect(zustaende()).toEqual(["erledigt", "erledigt", "laeuft"]);
    await act(async () => schritte.offen[1].ok(stand("fertig", [{ pfad: "index.php.bak-L", meldung: "gesperrt" }])));
    expect(screen.getByText(/1 Sicherung wurde nicht entfernt/)).toHaveClass("warnung");
    expect(screen.getByText("index.php.bak-L").closest("li")).toHaveTextContent("Übernehmen: index.php.bak-L – gesperrt");
    expect(aufrufeVon(aufrufe, "lauf_weiter").map((a) => (a.args as { schritt: string }).schritt)).toEqual(["umschalten", "uebernehmen"]);
    await userEvent.click(knopf("Fertig – neu vergleichen"));
    expect(onFertig).toHaveBeenCalled();
  });

  it("Fehler beim Bereitstellen: kein Umschalten, nur alles zurück", async () => {
    const fehler = [{ nr: 1, art: "kopieren" as const, pfad: "a.css", meldung: "weg" }];
    const { aufrufe } = dialog({
      plan_erstellen: () => plan({ ...OHNE_LOESCHEN, modus: "umschalten" }),
      lauf_starten: () => ({ lauf_id: "L", ergebnis: ergebnis({ fehler }), phase: "hochgeladen" }),
      lauf_weiter: () => stand("abgebrochen"),
    });
    await userEvent.click(await screen.findByRole("button", { name: "Übertragen" }));
    expect(await screen.findByText(/^Nicht alles wurde bereitgestellt/)).toHaveClass("fehler");
    expect(zustaende()).toEqual(["fehler", "aus", "aus"]);
    expect(kachel("Bereitstellen")).toHaveTextContent("✕");
    expect(screen.getByText("a.css").closest("li")).toHaveTextContent("Bereitstellen: Kopieren a.css – weg");
    expect(aufrufeVon(aufrufe, "lauf_weiter")).toEqual([]);
    await userEvent.click(knopf("Rollback – alles zurück"));
    expect(await screen.findByText(/^✓ Alles zurückgenommen/)).toBeInTheDocument();
  });

  it("Fehler beim Umschalten: angehalten; Rollback nimmt nicht alles zurück", async () => {
    dialog({
      plan_erstellen: () => plan({ ...OHNE_LOESCHEN, modus: "umschalten" }),
      lauf_starten: () => ({ lauf_id: "L", ergebnis: ergebnis(), phase: "hochgeladen" }),
      lauf_weiter: (a) =>
        (a as { schritt: string }).schritt === "umschalten"
          ? stand("umgeschaltet", [{ pfad: "css/site.css", meldung: "inzwischen vorhanden" }])
          : stand("zurueckgerollt", [{ pfad: "neu.txt", meldung: "gesperrt" }]),
    });
    await userEvent.click(await screen.findByRole("button", { name: "Übertragen" }));
    expect(await screen.findByText(/^Beim Umschalten ist ein Fehler aufgetreten/)).toHaveClass("fehler");
    expect(zustaende()).toEqual(["erledigt", "fehler", "aus"]);
    await userEvent.click(knopf("Rollback – alles zurück"));
    expect(await screen.findByText(/^1 Eintrag konnte nicht zurückgenommen werden/)).toHaveClass("warnung");
    expect(screen.getByText("css/site.css").closest("li")).toHaveTextContent("Umschalten: css/site.css – inzwischen vorhanden");
    expect(screen.getByText("neu.txt").closest("li")).toHaveTextContent("Zurücknehmen: neu.txt – gesperrt");
  });

  it("Rollback während des Bereitstellens bricht ab und räumt auf", async () => {
    const lauf = steuerbarerLauf();
    const { aufrufe } = dialog({ plan_erstellen: () => plan({ ...OHNE_LOESCHEN, modus: "umschalten" }), lauf_starten: lauf.antwort });
    await userEvent.click(await screen.findByRole("button", { name: "Übertragen" }));
    await userEvent.click(knopf("Rollback – alles zurück"));
    expect(aufrufeVon(aufrufe, "lauf_abbrechen")[0].args).toEqual({ strecke: "web", stufe: "spiegel" });
    expect(screen.getByText("Wird zurückgenommen – laufende Dateien werden sauber beendet …")).toBeInTheDocument();
    expect(knopf("Rollback – alles zurück")).toBeDisabled();
    await act(async () => lauf.s.fertig({ lauf_id: "L", ergebnis: ergebnis({ abgebrochen: true }), phase: "abgebrochen" }));
    expect(screen.getByText(/^✓ Alles zurückgenommen – die Schattendateien/)).toBeInTheDocument();
  });

  it("abgewiesener Start: Meldung, neuer Versuch möglich", async () => {
    dialog({
      plan_erstellen: () => plan({ ...OHNE_LOESCHEN, modus: "umschalten" }),
      lauf_starten: () => {
        throw { kategorie: "vorhanden", meldung: "Übertragung läuft bereits" };
      },
    });
    await userEvent.click(await screen.findByRole("button", { name: "Übertragen" }));
    expect(await screen.findByText("Übertragung läuft bereits")).toHaveClass("fehler");
    expect(knoepfeUnten()).toEqual(["Abbrechen", "Übertragen"]);
    expect(zustaende()).toEqual(["aktiv", "aus", "aus"]);
  });
});
