import { describe, expect, it } from "vitest";

import { zeile } from "../test/vergleich";
import { fortschrittText, groesse, regelText, warnungText, zeilenTooltip, zeit } from "./format";

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
