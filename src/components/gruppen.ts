// Gruppen der Startseite (v0.8.1): frei vergebene Gruppe je Strecke, nichts zentral angelegt.

import type { StreckeUebersicht } from "../api";

/**
 * Deutsche Sortierung: Umlaute richtig einsortiert, Groß/Klein nur als letzter Unterschied,
 * Zahlen natürlich („Test 2“ vor „Test 10“).
 */
const sortierer = new Intl.Collator("de", { numeric: true });

export function vergleichDeutsch(a: string, b: string): number {
  return sortierer.compare(a, b);
}

/** Schlüssel für „Ohne Gruppe“ beim Merken des Auf-/Zu-Zustands. */
export const OHNE_GRUPPE = "";

export type Gruppe = {
  /** Gruppenname; `OHNE_GRUPPE` für Strecken ohne Gruppe. */
  name: string;
  strecken: StreckeUebersicht[];
};

/** Gruppe ohne Rand-Leerzeichen; leer zählt als „ohne Gruppe“. */
function gruppeVon(s: StreckeUebersicht): string {
  return (s.gruppe ?? "").trim();
}

/** „Webseiten“ und „webseiten“ gelten als dieselbe Gruppe. */
function schluessel(gruppe: string): string {
  return gruppe.toLocaleLowerCase("de");
}

/** Name zum Sortieren; defekte Dateien haben keinen Namen, dann zählt die Kennung. */
function anzeigeName(s: StreckeUebersicht): string {
  return s.name ?? s.id;
}

/** Strecken alphabetisch nach Namen. */
export function sortiert(liste: StreckeUebersicht[]): StreckeUebersicht[] {
  return [...liste].sort((a, b) => vergleichDeutsch(anzeigeName(a), anzeigeName(b)));
}

/** Alle vergebenen Gruppen, alphabetisch und ohne Doppelte (Vorschläge im Editor). */
export function gruppenNamen(liste: StreckeUebersicht[]): string[] {
  const namen = new Map<string, string>();
  for (const s of liste) {
    const g = gruppeVon(s);
    // Bei abweichender Groß-/Kleinschreibung gewinnt die erste Schreibweise
    if (g !== "" && !namen.has(schluessel(g))) namen.set(schluessel(g), g);
  }
  return [...namen.values()].sort(vergleichDeutsch);
}

/**
 * Strecken nach Gruppen. Gruppen alphabetisch, „Ohne Gruppe“ am Ende, Strecken je Gruppe alphabetisch.
 * Hat keine Strecke eine Gruppe, ist das Ergebnis leer – die Startseite bleibt dann flach.
 */
export function gruppieren(liste: StreckeUebersicht[]): Gruppe[] {
  const namen = gruppenNamen(liste);
  if (namen.length === 0) return [];
  const gruppen: Gruppe[] = namen.map((name) => ({
    name,
    strecken: sortiert(liste.filter((s) => schluessel(gruppeVon(s)) === schluessel(name))),
  }));
  const ohne = sortiert(liste.filter((s) => gruppeVon(s) === ""));
  if (ohne.length > 0) gruppen.push({ name: OHNE_GRUPPE, strecken: ohne });
  return gruppen;
}
