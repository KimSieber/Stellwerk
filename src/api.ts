// Typisierte Aufrufe der Rust-Commands (ARCHITEKTUR Kap. 9).
// Die Typen spiegeln stellwerk-core von Hand; sobald die Command-Zahl wächst,
// werden sie per specta/tauri-specta generiert.

import { Channel, invoke } from "@tauri-apps/api/core";

export type Schwere = "fehler" | "warnung";

export interface Problem {
  schwere: Schwere;
  ort: string;
  meldung: string;
}

export interface BefehlsFehler {
  kategorie:
    | "validierung"
    | "konfiguration"
    | "nicht_gefunden"
    | "vorhanden"
    | "kein_verzeichnis"
    | "schluesselbund"
    | "nicht_unterstuetzt"
    | "abbruch"
    | "io";
  meldung: string;
  probleme?: Problem[];
}

export interface StufeKurz {
  id: string;
  name: string;
  quelle: string;
  ziel: string;
}

export interface StreckeUebersicht {
  id: string;
  name: string | null;
  stufen: StufeKurz[];
  probleme: Problem[];
  fehler: string | null;
}

// --- Vollständiges Modell (Spiegel von stellwerk_core::model) ---

export type RegelTyp = "ignorieren" | "schuetzen" | "nur_anlegen" | "ersetzen";

export interface Regel {
  muster: string;
  typ: RegelTyp;
  quelle?: string;
  aktiv?: boolean; // fehlt = an
}

export interface RegelEbene {
  standard_aus?: string[];
  eigene?: Regel[];
}

export interface FtpsEndpunkt {
  host: string;
  port: number;
  pfad: string;
  benutzer: string;
  tls: "explizit";
  konto_ref: string;
}

export type Endpunkt = { typ: "lokal"; pfad: string } | ({ typ: "ftps" } & FtpsEndpunkt);
export type Quelle = Endpunkt | { typ: "stufe"; stufe: string };

export type Rechte =
  | { modus: "nicht_setzen" }
  | { modus: "fest"; ordner?: string; dateien?: string }
  | {
      modus: "muster";
      ordner?: string;
      dateien?: string;
      regeln: { muster: string; modus: string }[];
    };

export interface Stufe {
  id: string;
  name: string;
  modus: "direkt" | "umschalten";
  quelle: Quelle;
  ziel: Endpunkt;
  rechte?: Rechte;
  nur_online: "herunterladen" | "ueberspringen";
  regeln?: RegelEbene;
}

export interface Strecke {
  id: string;
  name: string;
  version: number;
  regeln?: RegelEbene;
  stufen: Stufe[];
}

// --- Vergleich (Spiegel von stellwerk_core::diff / ::ansicht) ---

export type DiffStatus = "neu" | "geaendert" | "loeschen" | "gleich" | "geschuetzt" | "ignoriert" | "konflikt";

export type Warnung =
  | { art: "symlink"; ziel: string }
  | { art: "nur_online" }
  | { art: "konfliktkopie" }
  | { art: "gross_klein"; mit: string }
  | { art: "art_verschieden" }
  | { art: "ersatz_fehlt"; quelle: string }
  | { art: "lesefehler"; meldung: string }
  | { art: "enthaelt_bleibendes" };

export interface Eintrag {
  pfad: string;
  art: "datei" | "ordner" | "symlink";
  groesse: number;
  /** Nanosekunden seit 1970 – in JavaScript nur auf Mikrosekunden genau. */
  mtime_ns: number | null;
  nur_online: boolean;
  hash: string | null;
  warnungen: Warnung[];
}

export interface RegelTreffer {
  typ: RegelTyp;
  herkunft: { ebene: "global" | "strecke" | "stufe"; index: number; muster: string };
  ersatz: string | null;
}

/** Eine Zeile der Vergleichsansicht. */
export interface Zeile {
  pfad: string;
  status: DiffStatus;
  quelle: Eintrag | null;
  ziel: Eintrag | null;
  regel: RegelTreffer | null;
  warnungen: Warnung[];
  ausgewaehlt: boolean;
  name: string;
  tiefe: number;
  ordner: boolean;
  /** Ordner hat unter dem aktuellen Filter sichtbaren Inhalt. */
  kinder: boolean;
  zugeklappt: boolean;
  auswaehlbar: boolean;
  /** Eine Regel der Strecke/Stufe für genau diesen Pfad greift – entfernbar. */
  regel_entfernbar: boolean;
}

/** Wo eine Regel aus dem Kontextmenü gespeichert wird. */
export type RegelOrt = { ebene: "strecke" } | { ebene: "stufe"; stufe: string };

export interface Seite {
  zeilen: Zeile[];
  /** Anzahl aller sichtbaren Zeilen. */
  sichtbar: number;
}

export interface Zusammenfassung {
  neu: number;
  geaendert: number;
  loeschen: number;
  gleich: number;
  geschuetzt: number;
  ignoriert: number;
  konflikt: number;
  ausgewaehlt: number;
  bytes_uebertragen: number;
  nur_online: number;
  nur_online_bytes: number;
}

export type Filter = "aenderungen" | "alle" | "warnungen";

export interface Fortschritt {
  phase: "quelle" | "ziel" | "vergleich";
  anzahl: number;
}

export interface Lesefehler {
  seite: "quelle" | "ziel";
  pfad: string;
  meldung: string;
}

export interface VergleichStart {
  vergleich_id: string;
  quelle: string;
  ziel: string;
  zusammenfassung: Zusammenfassung;
  zeilen: number;
  lesefehler: Lesefehler[];
}

// --- Commands ---

export const api = {
  konfigOrt: () => invoke<string>("konfig_ort"),
  streckenListe: () => invoke<StreckeUebersicht[]>("strecken_liste"),
  globaleRegeln: () => invoke<Regel[]>("globale_regeln"),
  streckeLaden: (id: string) => invoke<Strecke>("strecke_laden", { id }),
  streckePruefen: (strecke: Strecke) => invoke<Problem[]>("strecke_pruefen", { strecke }),
  streckeAnlegen: (strecke: Strecke) => invoke<Problem[]>("strecke_anlegen", { strecke }),
  streckeSpeichern: (strecke: Strecke) => invoke<Problem[]>("strecke_speichern", { strecke }),
  streckeLoeschen: (id: string) => invoke<void>("strecke_loeschen", { id }),
  istOrdner: (pfad: string) => invoke<boolean>("ist_ordner", { pfad }),
  /** Passwort in den Schlüsselbund – es kommt nie zurück. */
  passwortSpeichern: (konto: string, passwort: string) => invoke<void>("passwort_speichern", { konto, passwort }),
  passwortVorhanden: (konto: string) => invoke<boolean>("passwort_vorhanden", { konto }),
  /** Scannt und vergleicht eine Stufe; verändert nichts. Fortschritt kommt gedrosselt. */
  vergleichStarten: (strecke: string, stufe: string, beiFortschritt: (f: Fortschritt) => void) =>
    invoke<VergleichStart>("vergleich_starten", { strecke, stufe, fortschritt: new Channel(beiFortschritt) }),
  vergleichAbbrechen: (strecke: string, stufe: string) => invoke<boolean>("vergleich_abbrechen", { strecke, stufe }),
  diffSeite: (vergleichId: string, offset: number, limit: number, filter: Filter, zugeklappt: string[]) =>
    invoke<Seite>("diff_seite", { vergleichId, offset, limit, filter, zugeklappt }),
  auswahlSetzen: (vergleichId: string, pfade: string[], an: boolean) =>
    invoke<Zusammenfassung>("auswahl_setzen", { vergleichId, pfade, an }),
  vergleichSchliessen: (vergleichId: string) => invoke<void>("vergleich_schliessen", { vergleichId }),
  /** Regel für genau `pfad` setzen und die Strecke speichern; liefert verbleibende Warnungen. */
  regelSetzen: (strecke: string, ort: RegelOrt, pfad: string, ordner: boolean, typ: RegelTyp, ersatz: string | null) =>
    invoke<Problem[]>("regel_setzen", { strecke, ort, pfad, ordner, typ, ersatz }),
  regelEntfernen: (strecke: string, ort: RegelOrt, pfad: string, ordner: boolean) =>
    invoke<Problem[]>("regel_entfernen", { strecke, ort, pfad, ordner }),
  /** Dateien der Quelle im selben Ordner – Vorschläge für „Ersetzen durch …“. */
  ersatzVorschlaege: (vergleichId: string, pfad: string) => invoke<string[]>("ersatz_vorschlaege", { vergleichId, pfad }),
};

/** Ereignis vom App-Menü „Über Stellwerk“ (siehe src-tauri/src/lib.rs). */
export const EREIGNIS_UEBER = "menue_ueber";

/** Fehlertext aus einem abgelehnten invoke. */
export function fehlerText(e: unknown): string {
  if (e && typeof e === "object" && "meldung" in e) return String((e as BefehlsFehler).meldung);
  return String(e);
}
