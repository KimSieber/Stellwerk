import { describe, expect, it } from "vitest";

import { summe, zeile } from "../test/vergleich";
import type { Lauf } from "../api";
import { anzahl, execFortschrittText, JOURNAL_TEXT, laufText, laufZeit, offen, schritte, stufenText, uhrzeit, fortschrittText, groesse, OP_TEXT, planTeile, regelText, warnungText, zeilenTooltip, zeit } from "./format";

describe("Darstellung", () => {
  it("Größen mit Einheit und deutschem Komma", () => {
    expect(groesse(0)).toBe("0 B");
    expect(groesse(999)).toBe("999 B");
    expect(groesse(1500)).toBe("1,5 KB");
    expect(groesse(12_345_678)).toBe("12,3 MB");
    expect(groesse(2e12)).toBe("2 TB");
    expect(groesse(3e15)).toBe("3.000 TB");
  });

  it("Zeit als Datum mit Uhrzeit; ohne Zeit leer", () => {
    const ns = new Date(2026, 10, 7, 15, 30).getTime() * 1e6;
    expect(zeit(ns)).toBe("07.11.2026, 15:30");
    expect(zeit(null)).toBe("");
  });

  it("Herkunft der Regel", () => {
    expect(regelText({ typ: "schuetzen", herkunft: { ebene: "stufe", index: 0, muster: "config.php" }, ersatz: null })).toBe(
      "Schützen – Regel der Stufe: config.php",
    );
    expect(
      regelText({ typ: "ersetzen", herkunft: { ebene: "strecke", index: 2, muster: "config.php" }, ersatz: "config.server.php" }),
    ).toBe("Ersetzen durch → config.server.php – Regel der Strecke: config.php");
    expect(regelText({ typ: "ignorieren", herkunft: { ebene: "global", index: 0, muster: ".DS_Store" }, ersatz: null })).toBe(
      "Ignorieren – Standardregel: .DS_Store",
    );
    expect(regelText({ typ: "nur_anlegen", herkunft: { ebene: "stufe", index: 1, muster: ".htaccess" }, ersatz: null })).toMatch(
      /^Nur anlegen/,
    );
  });

  it("jede Warnung hat einen Text", () => {
    expect(warnungText({ art: "symlink", ziel: "../x" })).toContain("„../x“");
    expect(warnungText({ art: "nur_online" })).toContain("nur online");
    expect(warnungText({ art: "konfliktkopie" })).toContain("Konfliktkopie");
    expect(warnungText({ art: "gross_klein", mit: "bild.jpg" })).toContain("„bild.jpg“");
    expect(warnungText({ art: "art_verschieden" })).toContain("Ordner");
    expect(warnungText({ art: "ersatz_fehlt", quelle: "config.server.php" })).toContain("„config.server.php“ fehlt");
    expect(warnungText({ art: "lesefehler", meldung: "kein Zugriff" })).toBe("Nicht lesbar: kein Zugriff");
    expect(warnungText({ art: "enthaelt_bleibendes" })).toContain("bleibt");
    expect(warnungText({ art: "inhalt_nicht_geprueft" })).toBe("Inhalt nicht geprüft – nur Größe verglichen");
  });

  it("Tooltip: Pfad, Regel, Hinweise", () => {
    expect(zeilenTooltip(zeile("a/b.txt"))).toBe("a/b.txt");
    const z = zeile("logs", {
      regel: { typ: "schuetzen", herkunft: { ebene: "stufe", index: 0, muster: "logs/" }, ersatz: null },
      warnungen: [{ art: "enthaelt_bleibendes" }],
    });
    expect(zeilenTooltip(z).split("\n")).toEqual(["logs", "Schützen – Regel der Stufe: logs/", expect.stringContaining("bleibt")]);
  });

  it("Fortschritt je Phase", () => {
    expect(fortschrittText(null)).toBe("Vergleich wird vorbereitet …");
    expect(fortschrittText({ phase: "quelle", anzahl: 1234 })).toBe("Quelle wird gelesen … 1.234");
    expect(fortschrittText({ phase: "ziel", anzahl: 5 })).toBe("Ziel wird gelesen … 5");
    expect(fortschrittText({ phase: "vergleich", anzahl: 7 })).toBe("Einträge werden verglichen … 7");
  });
});

describe("Übertragen", () => {
  it("Anzahl mit Einzahl und Mehrzahl", () => {
    expect(anzahl(1, "Datei", "Dateien")).toBe("1 Datei");
    expect(anzahl(0, "Datei", "Dateien")).toBe("0 Dateien");
    expect(anzahl(12345, "Datei", "Dateien")).toBe("12.345 Dateien");
  });

  it("Plan-Teile nur für Vorkommendes", () => {
    const leer = { ordner_anlegen: 0, kopieren: 0, ueberschreiben: 0, rechte: 0, dateien_loeschen: 0, ordner_loeschen: 0, bytes: 0 };
    expect(planTeile(leer)).toEqual([]);
    expect(planTeile({ ordner_anlegen: 1, kopieren: 2, ueberschreiben: 1, rechte: 3, dateien_loeschen: 1, ordner_loeschen: 2, bytes: 9 })).toEqual([
      "1 Ordner anlegen",
      "2 Dateien neu",
      "1 Datei überschreiben",
      "Rechte für 3 Einträge setzen",
      "1 Datei löschen",
      "2 Ordner löschen",
    ]);
    expect(planTeile({ ...leer, rechte: 1 })).toEqual(["Rechte für 1 Eintrag setzen"]);
  });

  it("Fortschritt der Übertragung", () => {
    expect(execFortschrittText(null)).toBe("Übertragung wird vorbereitet …");
    expect(execFortschrittText({ fertig: 12, gesamt: 40, bytes: 1500, bytes_gesamt: 3_000_000, fehler: 0 })).toBe(
      "12 von 40 · 1,5 KB von 3 MB",
    );
  });

  it("Texte der Operationen", () => {
    expect(Object.values(OP_TEXT)).toEqual(["Ordner anlegen", "Kopieren", "Überschreiben", "Rechte setzen", "Löschen", "Ordner löschen"]);
  });
});

describe("Streckenansicht", () => {
  it("offen zählt nur, was zu tun ist", () => {
    expect(offen(summe({ neu: 1, geaendert: 2, loeschen: 3, konflikt: 4, gleich: 9, geschuetzt: 9, ignoriert: 9 }))).toBe(10);
  });

  it("Status einer Stufe", () => {
    expect(stufenText(summe({ gleich: 5, geschuetzt: 2 }))).toBe("gleich");
    expect(stufenText(summe({ neu: 1 }))).toBe("1 neu");
    expect(stufenText(summe({ geaendert: 2, konflikt: 1 }))).toBe("2 geändert · 1 Konflikt");
    expect(stufenText(summe({ loeschen: 1234, konflikt: 2 }))).toBe("1.234 löschen · 2 Konflikte");
  });

  it("Zeiten", () => {
    expect(uhrzeit(new Date(2026, 10, 7, 9, 5))).toBe("09:05");
    expect(laufZeit(new Date(2026, 10, 7, 21, 40).getTime())).toBe("07.11.2026, 21:40");
  });

  it("Kurzfassung eines Laufs", () => {
    const z = { ordner_anlegen: 1, kopieren: 1, ueberschreiben: 1, rechte: 1, dateien_loeschen: 1, ordner_loeschen: 1, bytes: 9 };
    expect(schritte(z)).toBe(6);
    const e = { erledigt: 6, fehler: [], uebersprungen: 0, abgebrochen: false, gestoppt: false, bytes: 1500, journal_fehler: null };
    const l: Lauf = { id: "l", strecke_id: "s", stufe_id: "t", modus: "direkt", phase: "fertig", gestartet: 0, beendet: 1, zusammenfassung: z, ergebnis: e };
    expect(laufText(l)).toEqual({ text: "6 Schritte · 1,5 KB", art: "ok" });
    expect(laufText({ ...l, ergebnis: { ...e, erledigt: 1, bytes: 0 } })).toEqual({ text: "1 Schritt · 0 B", art: "ok" });
    expect(laufText({ ...l, phase: "geplant" })).toEqual({ text: "nicht abgeschlossen", art: "fehler" });
    expect(laufText({ ...l, phase: "laeuft" }).text).toBe("nicht abgeschlossen");
    expect(laufText({ ...l, ergebnis: null }).text).toBe("nicht abgeschlossen");
    expect(laufText({ ...l, phase: "abgebrochen", ergebnis: { ...e, erledigt: 2, abgebrochen: true } })).toEqual({
      text: "abgebrochen · 2 von 6 Schritten",
      art: "warnung",
    });
    expect(laufText({ ...l, ergebnis: { ...e, erledigt: 5, journal_fehler: "voll" } })).toEqual({ text: "Journal unvollständig · 5 von 6 Schritten", art: "fehler" });
    const einSchritt = { ...z, kopieren: 0, ueberschreiben: 0, rechte: 0, dateien_loeschen: 0, ordner_loeschen: 0 };
    expect(laufText({ ...l, zusammenfassung: einSchritt, ergebnis: { ...e, erledigt: 0, fehler: [{ nr: 0, art: "kopieren", pfad: "a", meldung: "m" }] } }).text).toBe(
      "1 Fehler · 0 von 1 Schritt",
    );
  });

  it("Läufe mit Umschalten: wartend, angehalten, zurückgerollt, Reste", () => {
    const z = { ordner_anlegen: 1, kopieren: 2, ueberschreiben: 1, rechte: 0, dateien_loeschen: 1, ordner_loeschen: 1, bytes: 10 };
    const e = { erledigt: 4, fehler: [], uebersprungen: 0, abgebrochen: false, gestoppt: false, bytes: 10, journal_fehler: null };
    const l: Lauf = { id: "l", strecke_id: "s", stufe_id: "t", modus: "umschalten_manuell", phase: "hochgeladen", gestartet: 0, beendet: null, zusammenfassung: z, ergebnis: e };
    const f = { nr: 1, pfad: "a", meldung: "m" };
    expect(laufText(l)).toEqual({ text: "bereitgestellt · wartet auf Umschalten", art: "warnung" });
    expect(laufText({ ...l, ergebnis: { ...e, fehler: [{ ...f, art: "kopieren" }] } })).toEqual({ text: "Bereitstellen unvollständig · 1 Fehler", art: "fehler" });
    expect(laufText({ ...l, phase: "umgeschaltet" })).toEqual({ text: "umgeschaltet · wartet auf Übernehmen", art: "warnung" });
    const angehalten = { ...e, umschalten: { erledigt: 1, fehler: [f, f], gestoppt: true } };
    expect(laufText({ ...l, phase: "umgeschaltet", ergebnis: angehalten })).toEqual({ text: "Umschalten angehalten · 2 Fehler", art: "fehler" });
    expect(laufText({ ...l, phase: "zurueckgerollt" })).toEqual({ text: "zurückgerollt", art: "warnung" });
    const reste = { ...e, uebernehmen: { erledigt: 1, fehler: [f], gestoppt: false } };
    expect(laufText({ ...l, phase: "fertig", ergebnis: reste })).toEqual({ text: "übernommen · 1 Rest nicht entfernt", art: "warnung" });
    expect(laufText({ ...l, phase: "fertig", ergebnis: { ...e, uebernehmen: { erledigt: 3, fehler: [], gestoppt: false } } }).art).toBe("ok");
  });

  it("unterbrochene Läufe (v0.6.1): klar markiert, mit Phase und Grund", () => {
    const z = { ordner_anlegen: 0, kopieren: 1, ueberschreiben: 0, rechte: 0, dateien_loeschen: 0, ordner_loeschen: 0, bytes: 1 };
    const e = { erledigt: 1, fehler: [], uebersprungen: 0, abgebrochen: false, gestoppt: false, bytes: 1, journal_fehler: null };
    const l: Lauf = { id: "l", strecke_id: "s", stufe_id: "t", modus: "umschalten", phase: "abgebrochen", gestartet: 0, beendet: 1, zusammenfassung: z, ergebnis: e };
    const mit = (grund: "programm_beendet" | "aufgegeben", phase: "hochgeladen" | "umgeschaltet") => laufText({ ...l, ergebnis: { ...e, unterbrochen: { grund, phase } } });
    expect(mit("programm_beendet", "umgeschaltet")).toEqual({ text: "unterbrochen nach dem Umschalten (Programm beendet) · Stand am Ziel prüfen", art: "fehler" });
    expect(mit("aufgegeben", "hochgeladen").text).toBe("unterbrochen nach dem Bereitstellen (Rollback gescheitert) · Stand am Ziel prüfen");
  });

  it("Journal-Status", () => {
    expect(Object.values(JOURNAL_TEXT)).toEqual(["offen", "erledigt", "Fehler", "nicht ausgeführt", "hochgeladen", "umgeschaltet", "zurückgenommen"]);
  });
});
