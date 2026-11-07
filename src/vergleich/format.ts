// Darstellung der Vergleichsdaten – reine Funktionen ohne Rechnerei über die Daten (A1).

import type { DiffStatus, ExecFortschritt, Fortschritt, JournalStatus, Lauf, OpArt, PlanZusammenfassung, RegelTreffer, Warnung, Zeile, Zusammenfassung } from "../api";

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

/** Zahl mit Einzahl/Mehrzahl: „1 Datei“, „3 Dateien“. */
export function anzahl(n: number, einzahl: string, mehrzahl: string): string {
  return `${zahl.format(n)} ${n === 1 ? einzahl : mehrzahl}`;
}

/** Teile der Plan-Zusammenfassung, nur was vorkommt – z. B. „2 Dateien neu“. */
export function planTeile(z: PlanZusammenfassung): string[] {
  const teile: [number, string][] = [
    [z.ordner_anlegen, `${anzahl(z.ordner_anlegen, "Ordner", "Ordner")} anlegen`],
    [z.kopieren, `${anzahl(z.kopieren, "Datei", "Dateien")} neu`],
    [z.ueberschreiben, `${anzahl(z.ueberschreiben, "Datei", "Dateien")} überschreiben`],
    [z.rechte, `Rechte für ${anzahl(z.rechte, "Eintrag", "Einträge")} setzen`],
    [z.dateien_loeschen, `${anzahl(z.dateien_loeschen, "Datei", "Dateien")} löschen`],
    [z.ordner_loeschen, `${anzahl(z.ordner_loeschen, "Ordner", "Ordner")} löschen`],
  ];
  return teile.filter(([n]) => n > 0).map(([, text]) => text);
}

export const OP_TEXT: Record<OpArt, string> = {
  ordner_anlegen: "Ordner anlegen",
  kopieren: "Kopieren",
  ueberschreiben: "Überschreiben",
  rechte_setzen: "Rechte setzen",
  datei_loeschen: "Löschen",
  ordner_loeschen: "Ordner löschen",
};

/** „12 von 40 · 1,5 MB von 3 MB“ */
export function execFortschrittText(f: ExecFortschritt | null): string {
  if (f === null) return "Übertragung wird vorbereitet …";
  return `${zahl.format(f.fertig)} von ${zahl.format(f.gesamt)} · ${groesse(f.bytes)} von ${groesse(f.bytes_gesamt)}`;
}

/** Zu tun ist, was neu, geändert, zu löschen oder ein Konflikt ist – Geschütztes und Ignoriertes ist gewollt. */
export function offen(z: Zusammenfassung): number {
  return z.neu + z.geaendert + z.loeschen + z.konflikt;
}

/** Status einer Stufe: „gleich“ oder „3 neu · 2 geändert · 1 löschen · 1 Konflikt“. */
export function stufenText(z: Zusammenfassung): string {
  if (offen(z) === 0) return "gleich";
  const teile: [number, string][] = [
    [z.neu, "neu"],
    [z.geaendert, "geändert"],
    [z.loeschen, "löschen"],
    [z.konflikt, z.konflikt === 1 ? "Konflikt" : "Konflikte"],
  ];
  return teile
    .filter(([n]) => n > 0)
    .map(([n, t]) => `${zahl.format(n)} ${t}`)
    .join(" · ");
}

const uhr = new Intl.DateTimeFormat("de-DE", { hour: "2-digit", minute: "2-digit" });

/** Uhrzeit „22:31“. */
export function uhrzeit(d: Date): string {
  return uhr.format(d);
}

/** Startzeit eines Laufs (Millisekunden) als „07.11.2026, 21:40“. */
export function laufZeit(ms: number): string {
  return datum.format(new Date(ms));
}

/** Anzahl aller Operationen eines Plans. */
export function schritte(z: PlanZusammenfassung): number {
  return z.ordner_anlegen + z.kopieren + z.ueberschreiben + z.rechte + z.dateien_loeschen + z.ordner_loeschen;
}

/** Kurzfassung eines Laufs für das Protokoll und ob er ohne Befund war. */
export function laufText(l: Lauf): { text: string; art: "ok" | "warnung" | "fehler" } {
  const n = schritte(l.zusammenfassung);
  const e = l.ergebnis;
  if (l.phase === "geplant" || l.phase === "laeuft" || e === null) return { text: "nicht abgeschlossen", art: "fehler" };
  const von = `${zahl.format(e.erledigt)} von ${anzahl(n, "Schritt", "Schritten")}`;
  if (e.abgebrochen) return { text: `abgebrochen · ${von}`, art: "warnung" };
  if (e.fehler.length > 0) return { text: `${anzahl(e.fehler.length, "Fehler", "Fehler")} · ${von}`, art: "fehler" };
  if (e.journal_fehler !== null) return { text: `Journal unvollständig · ${von}`, art: "fehler" };
  return { text: `${anzahl(e.erledigt, "Schritt", "Schritte")} · ${groesse(e.bytes)}`, art: "ok" };
}

export const JOURNAL_TEXT: Record<JournalStatus, string> = {
  offen: "offen",
  erledigt: "erledigt",
  fehler: "Fehler",
  uebersprungen: "nicht ausgeführt",
};
