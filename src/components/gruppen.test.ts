import { describe, expect, it } from "vitest";

import type { StreckeUebersicht } from "../api";
import { gruppenNamen, gruppieren, OHNE_GRUPPE, sortiert, suchen, suchWoerter, vergleichDeutsch } from "./gruppen";

function strecke(name: string | null, gruppe?: string | null, id = (name ?? "x").toLowerCase()): StreckeUebersicht {
  return { id, name, gruppe, stufen: [], probleme: [], fehler: name === null ? "kaputt" : null };
}

const namen = (l: StreckeUebersicht[]) => l.map((s) => s.name ?? s.id);

describe("deutsche Sortierung", () => {
  it("sortiert Umlaute richtig, Groß/Klein egal und Zahlen natürlich", () => {
    expect(["Zug", "Ärger", "apfel", "Test 10", "Test 2", "Bahn"].sort(vergleichDeutsch)).toEqual([
      "apfel",
      "Ärger",
      "Bahn",
      "Test 2",
      "Test 10",
      "Zug",
    ]);
  });

  it("sortiert Strecken nach Namen, defekte nach Kennung", () => {
    expect(namen(sortiert([strecke("Mailwerk"), strecke(null, null, "kaputt"), strecke("Abfahrtstafel")]))).toEqual([
      "Abfahrtstafel",
      "kaputt",
      "Mailwerk",
    ]);
  });
});

describe("Gruppen", () => {
  it("Namen: alphabetisch, ohne Leere, ohne Doppelte – erste Schreibweise gewinnt", () => {
    const liste = [
      strecke("A", " Webseiten "),
      strecke("B", "webseiten"),
      strecke("C", "APIs"),
      strecke("D", "  "),
      strecke("E", null),
      strecke("F"),
    ];
    expect(gruppenNamen(liste)).toEqual(["APIs", "Webseiten"]);
  });

  it("ohne vergebene Gruppe: keine Gruppierung", () => {
    expect(gruppieren([strecke("A"), strecke("B", "")])).toEqual([]);
  });

  it("Gruppen alphabetisch, Strecken je Gruppe alphabetisch, „Ohne Gruppe“ am Ende", () => {
    const g = gruppieren([
      strecke("Mailwerk Webseite", "Webseiten"),
      strecke("Versuchsstrecke"),
      strecke("Garagentor Api", "APIs"),
      strecke("GymGuard Webseite", "webseiten "),
      strecke("Abfahrtstafel Api", "APIs"),
      strecke(null, null, "kaputt"),
    ]);
    expect(g.map((x) => [x.name, namen(x.strecken)])).toEqual([
      ["APIs", ["Abfahrtstafel Api", "Garagentor Api"]],
      ["Webseiten", ["GymGuard Webseite", "Mailwerk Webseite"]],
      [OHNE_GRUPPE, ["kaputt", "Versuchsstrecke"]],
    ]);
  });

  it("ohne „Ohne Gruppe“, wenn jede Strecke eine Gruppe hat", () => {
    expect(gruppieren([strecke("A", "X")]).map((x) => x.name)).toEqual(["X"]);
  });
});

describe("Suche (v0.8.2)", () => {
  const liste = [
    strecke("Mailwerk Webseite", "Webseiten", "mailwerk"),
    strecke("GymGuard Webseite", "Webseiten", "gymguard-webseite"),
    strecke("AWS-Müllabfuhr Api", "APIs", "aws-muellabfuhr-app"),
    strecke("Versuchsstrecke", null, "test-mit-mahlwerk"),
  ];

  it("zerlegt den Begriff in Wörter", () => {
    expect(suchWoerter("  Mail  WEB ")).toEqual(["mail", "web"]);
    expect(suchWoerter("   ")).toEqual([]);
  });

  it("findet in Name, Kennung und Gruppe – Groß/Klein egal, Ergebnis alphabetisch", () => {
    expect(namen(suchen(liste, suchWoerter("w")))).toEqual([
      "AWS-Müllabfuhr Api",
      "GymGuard Webseite",
      "Mailwerk Webseite",
      "Versuchsstrecke",
    ]);
    expect(namen(suchen(liste, suchWoerter("MÜLL")))).toEqual(["AWS-Müllabfuhr Api"]);
    expect(namen(suchen(liste, suchWoerter("muell")))).toEqual(["AWS-Müllabfuhr Api"]); // Kennung
    expect(namen(suchen(liste, suchWoerter("apis")))).toEqual(["AWS-Müllabfuhr Api"]); // Gruppe
    expect(namen(suchen(liste, suchWoerter("mahlwerk")))).toEqual(["Versuchsstrecke"]);
  });

  it("mehrere Wörter müssen alle vorkommen", () => {
    expect(namen(suchen(liste, suchWoerter("web mail")))).toEqual(["Mailwerk Webseite"]);
    expect(suchen(liste, suchWoerter("web api"))).toEqual([]);
  });

  it("defekte Datei: gefunden über die Kennung", () => {
    expect(namen(suchen([strecke(null, null, "kaputt")], suchWoerter("kap")))).toEqual(["kaputt"]);
  });
});
