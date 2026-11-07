// Darstellung der Vergleichsdaten – reine Funktionen ohne Rechnerei über die Daten (A1).

import type { DiffStatus, Fortschritt, RegelTreffer, Warnung, Zeile } from "../api";

const zahl = new Intl.NumberFormat("de-DE", { maximumFractionDigits: 1 });

/** Bytes lesbar: „512 B“, „1,5 KB“, „12,3 MB“ (Basis 1000 wie im Finder). */
export function groesse(bytes: number): string {
  const einheiten = ["B", "KB", "MB", "GB", "TB"];
  let wert = bytes;
  let i = 0;
  while (wert >= 1000 && i < einheiten.length - 1) {
    wert /= 1000;
    i++;
  }
  return `${zahl.format(wert)} ${einheiten[i]}`;
}

const datum = new Intl.DateTimeFormat("de-DE", {
  day: "2-digit",
  month: "2-digit",
  year: "numeric",
  hour: "2-digit",
  minute: "2-digit",
});

/** Änderungszeit (Nanosekunden seit 1970) als „07.11.2026, 15:30“; ohne Zeit leer. */
export function zeit(mtimeNs: number | null): string {
  return mtimeNs === null ? "" : datum.format(new Date(mtimeNs / 1e6));
}

export const STATUS_TEXT: Record<DiffStatus, string> = {
  neu: "neu",
  geaendert: "geändert",
  loeschen: "löschen",
  gleich: "gleich",
  geschuetzt: "geschützt",
  ignoriert: "ignoriert",
  konflikt: "Konflikt",
};

export const REGEL_TEXT: Record<RegelTreffer["typ"], string> = {
  ignorieren: "Ignorieren",
  schuetzen: "Schützen",
  nur_anlegen: "Nur anlegen",
  ersetzen: "Ersetzen durch",
};

const EBENE_TEXT: Record<RegelTreffer["herkunft"]["ebene"], string> = {
  global: "Standardregel",
  strecke: "Regel der Strecke",
  stufe: "Regel der Stufe",
};

/** Herkunft der greifenden Regel, z. B. „Schützen – Regel der Stufe: config.php“. */
export function regelText(r: RegelTreffer): string {
  const ersatz = r.ersatz ? ` → ${r.ersatz}` : "";
  return `${REGEL_TEXT[r.typ]}${ersatz} – ${EBENE_TEXT[r.herkunft.ebene]}: ${r.herkunft.muster}`;
}

export function warnungText(w: Warnung): string {
  switch (w.art) {
    case "symlink":
      return `Symbolischer Link auf „${w.ziel}“ – wird nicht verfolgt`;
    case "nur_online":
      return "Dropbox „nur online“ – wird vor dem Übertragen geladen";
    case "konfliktkopie":
      return "Dropbox-Konfliktkopie – wird nicht übertragen";
    case "gross_klein":
      return `Unterscheidet sich nur in Groß-/Kleinschreibung von „${w.mit}“`;
    case "art_verschieden":
      return "Datei auf der einen, Ordner auf der anderen Seite";
    case "ersatz_fehlt":
      return `Ersatzdatei „${w.quelle}“ fehlt in der Quelle`;
    case "lesefehler":
      return `Nicht lesbar: ${w.meldung}`;
    case "enthaelt_bleibendes":
      return "Ordner bleibt, weil darin etwas bestehen bleibt";
  }
}

/** Tooltip einer Zeile: Pfad, greifende Regel, Hinweise. */
export function zeilenTooltip(z: Zeile): string {
  return [z.pfad, ...(z.regel ? [regelText(z.regel)] : []), ...z.warnungen.map(warnungText)].join("\n");
}

const PHASE_TEXT: Record<Fortschritt["phase"], string> = {
  quelle: "Quelle wird gelesen",
  ziel: "Ziel wird gelesen",
  vergleich: "Einträge werden verglichen",
};

export function fortschrittText(f: Fortschritt | null): string {
  return f === null ? "Vergleich wird vorbereitet …" : `${PHASE_TEXT[f.phase]} … ${zahl.format(f.anzahl)}`;
}
