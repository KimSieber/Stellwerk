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
    | "kein_manifest"
    | "abbruch"
    | "nicht_bestaetigt"
    | "datenbank"
    | "verbindung"
    | "anmeldung"
    | "zertifikat"
    | "io";
  meldung: string;
  probleme?: Problem[];
  /** Bei „zertifikat“: SHA-256 des nicht anerkannten Serverzertifikats. */
  fingerprint?: string;
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
  /** Bewusst bestätigtes Serverzertifikat (SHA-256), wenn das System es nicht anerkennt. */
  zertifikat?: string;
}

/** Ergebnis von „Verbindung testen“ (stellwerk_core::endpoint::ftps::Verbindungstest). */
export interface Verbindungstest {
  begruessung: string | null;
  verzeichnis: string;
  eintraege: number;
  faehigkeiten: { mlsd: boolean; mfmt: boolean; chmod: boolean | null; fortsetzen: boolean; utf8: boolean };
  zertifikat: { fingerprint: string; vom_system: boolean };
  /** Aus Läufen gelernt: Ersetzt der Server beim Umbenennen? `null`: noch nicht bekannt. */
  umbenennen_ueberschreibt: boolean | null;
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
  | { art: "enthaelt_bleibendes" }
  | { art: "inhalt_nicht_geprueft" };

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
  /** Nur bei FTPS-Zielen: Stand aus dem Manifest oder soeben vom Server gelesen. */
  ziel_stand: "manifest" | "server" | null;
}

// --- Übertragen „Direkt“ (Spiegel von stellwerk_core::plan / ::exec) ---

export interface PlanZusammenfassung {
  ordner_anlegen: number;
  kopieren: number;
  ueberschreiben: number;
  rechte: number;
  dateien_loeschen: number;
  ordner_loeschen: number;
  bytes: number;
}

export type PlanWarnung = { art: "weltschreibbar"; modus: string; anzahl: number };

/** Was vor dem Übertragen zur Bestätigung gezeigt wird. */
export interface PlanVorschau {
  zusammenfassung: PlanZusammenfassung;
  /** Alle Pfade, die am Ziel gelöscht werden. */
  loeschliste: string[];
  warnungen: PlanWarnung[];
  quelle: string;
  ziel: string;
  worker: number;
}

export type OpArt = "ordner_anlegen" | "kopieren" | "ueberschreiben" | "rechte_setzen" | "datei_loeschen" | "ordner_loeschen";

export interface ExecFortschritt {
  fertig: number;
  gesamt: number;
  bytes: number;
  bytes_gesamt: number;
  fehler: number;
}

export interface LaufErgebnis {
  erledigt: number;
  fehler: { nr: number; art: OpArt; pfad: string; meldung: string }[];
  uebersprungen: number;
  abgebrochen: boolean;
  gestoppt: boolean;
  bytes: number;
  journal_fehler: string | null;
}

export type LaufPhase = "geplant" | "laeuft" | "fertig" | "abgebrochen";
export type JournalStatus = "offen" | "erledigt" | "fehler" | "uebersprungen";

/** Ein Lauf aus der Datenbank (Protokoll der Streckenansicht). */
export interface Lauf {
  id: string;
  strecke_id: string;
  stufe_id: string;
  modus: "direkt" | "umschalten";
  phase: LaufPhase;
  /** Millisekunden seit 1970. */
  gestartet: number;
  beendet: number | null;
  zusammenfassung: PlanZusammenfassung;
  ergebnis: LaufErgebnis | null;
}

/** Ein Schritt eines Laufs. */
export interface JournalZeile {
  nr: number;
  op: OpArt;
  pfad: string;
  quelle: string | null;
  modus: string | null;
  groesse: number;
  status: JournalStatus;
  fehler: string | null;
  aktualisiert: number;
}

export interface LaufAntwort {
  lauf_id: string;
  ergebnis: LaufErgebnis;
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
  /** Liest nur. Ohne eingegebenes Passwort gilt das aus dem Schlüsselbund. */
  verbindungTesten: (ziel: Endpunkt, passwort: string) =>
    invoke<Verbindungstest>("verbindung_testen", { ziel, passwort: passwort || null }),
  /** Scannt und vergleicht eine Stufe; verändert nichts. Fortschritt kommt gedrosselt. */
  /** `server`: FTPS-Ziel vom Server einlesen statt gegen das Manifest (lokale Ziele: ohne Wirkung). */
  vergleichStarten: (strecke: string, stufe: string, beiFortschritt: (f: Fortschritt) => void, server = false) =>
    invoke<VergleichStart>("vergleich_starten", { strecke, stufe, fortschritt: new Channel(beiFortschritt), server }),
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
  /** Plan aus der aktuellen Auswahl – verändert nichts, Rust merkt ihn sich für `laufStarten`. */
  planErstellen: (vergleichId: string) => invoke<PlanVorschau>("plan_erstellen", { vergleichId }),
  /** Führt den zuletzt gezeigten Plan aus („Direkt“). Schreibt nur ins Ziel. */
  laufStarten: (
    vergleichId: string,
    loeschenBestaetigt: boolean,
    beiFehlerStoppen: boolean,
    beiFortschritt: (f: ExecFortschritt) => void,
  ) =>
    invoke<LaufAntwort>("lauf_starten", {
      vergleichId,
      loeschenBestaetigt,
      beiFehlerStoppen,
      fortschritt: new Channel(beiFortschritt),
    }),
  laufAbbrechen: (strecke: string, stufe: string) => invoke<boolean>("lauf_abbrechen", { strecke, stufe }),
  /** Läufe einer Stufe, neueste zuerst. */
  laeufe: (strecke: string, stufe: string, limit: number) => invoke<Lauf[]>("laeufe", { strecke, stufe, limit }),
  /** Jeder Schritt eines Laufs. */
  laufJournal: (laufId: string) => invoke<JournalZeile[]>("lauf_journal", { laufId }),
};

/** Ereignis vom App-Menü „Über Stellwerk“ (siehe src-tauri/src/lib.rs). */
export const EREIGNIS_UEBER = "menue_ueber";

/** Fehlertext aus einem abgelehnten invoke. */
export function fehlerText(e: unknown): string {
  if (e && typeof e === "object" && "meldung" in e) return String((e as BefehlsFehler).meldung);
  return String(e);
}
