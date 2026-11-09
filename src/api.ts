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
    | "keine_rechte"
    | "lauf_offen"
    | "zertifikat"
    | "host_schluessel"
    | "io";
  meldung: string;
  probleme?: Problem[];
  /** Bei „zertifikat“: SHA-256 des nicht anerkannten Serverzertifikats; bei „host_schluessel“: `SHA256:…` des Host-Schlüssels. */
  fingerprint?: string;
}

export interface StufeKurz {
  id: string;
  name: string;
  quelle: string;
  ziel: string;
  /** Ergebnis des letzten Vergleichs (v0.6.6); `null`: unbekannt. */
  stand?: StufenStand | null;
  /** Der neueste Lauf – zeigt, ob Reste am Ziel liegen können. */
  letzter_lauf?: Lauf | null;
}

/** Was der letzte Vergleich einer Stufe ergab – verfällt nach Übertragung und Speichern. */
export interface StufenStand {
  /** Millisekunden seit 1970. */
  zeit: number;
  neu: number;
  geaendert: number;
  loeschen: number;
  konflikt: number;
}

export interface StreckeUebersicht {
  id: string;
  name: string | null;
  /** Gruppe der Startseite (v0.8.1); fehlt/`null` = ohne Gruppe. */
  gruppe?: string | null;
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

/** Samba-Freigabe (v0.7.1), angesprochen ohne eingehängtes Laufwerk. */
export interface SmbEndpunkt {
  host: string;
  port: number;
  freigabe: string;
  /** Ordner in der Freigabe, mit „/“ beginnend. */
  pfad: string;
  benutzer: string;
  konto_ref: string;
}

/** SFTP-Server (v0.7.2): SSH-Dateiübertragung, Anmeldung mit Passwort. */
export interface SftpEndpunkt {
  host: string;
  port: number;
  pfad: string;
  benutzer: string;
  konto_ref: string;
  /** Bewusst bestätigter Host-Schlüssel (`SHA256:…`); ohne ihn verbindet Stellwerk nicht. */
  hostschluessel?: string;
  /** Privater SSH-Schlüssel (Pfad) statt Passwort (v0.7.2b); im Schlüsselbund steht dann dessen Passphrase. */
  schluessel?: string;
}

/** Ergebnis von „Verbindung testen“ für einen SFTP-Server (v0.7.2). */
export interface SftpVerbindungstest {
  /** Kennung der Server-Software, z. B. `SSH-2.0-OpenSSH_9.6`. */
  server: string | null;
  verzeichnis: string;
  eintraege: number;
  hostschluessel: string;
}

/** Ergebnis von „Verbindung testen“ für eine Samba-Freigabe (v0.7.1b). */
export interface SmbVerbindungstest {
  /** `smb://host:port/freigabe/ordner` */
  ordner: string;
  eintraege: number;
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

export type Endpunkt =
  | { typ: "lokal"; pfad: string }
  | ({ typ: "ftps" } & FtpsEndpunkt)
  | ({ typ: "smb" } & SmbEndpunkt)
  | ({ typ: "sftp" } & SftpEndpunkt);
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
  modus: Modus;
  quelle: Quelle;
  ziel: Endpunkt;
  rechte?: Rechte;
  nur_online: "herunterladen" | "ueberspringen";
  regeln?: RegelEbene;
}

export interface Strecke {
  id: string;
  name: string;
  /** Freie Gruppe für die Startseite (v0.8.1). */
  gruppe?: string;
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
  /** Dateien mit „Inhalt nicht geprüft“ (nur Größe bekannt) – für „Inhalte prüfen“. */
  ungeprueft: { anzahl: number; bytes: number };
  /** Modus der Stufe, wie er jetzt konfiguriert ist (v0.6.3). */
  modus: Modus;
}

/** Ergebnis von „Inhalte prüfen“. */
export interface InhaltePruefung {
  geprueft: number;
  fehler: { pfad: string; meldung: string }[];
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
  modus: Modus;
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

/** Deployment-Modus einer Stufe. */
export type Modus = "direkt" | "umschalten" | "umschalten_manuell";

export type LaufPhase = "geplant" | "laeuft" | "fertig" | "abgebrochen" | "hochgeladen" | "umgeschaltet" | "zurueckgerollt";
export type JournalStatus = "offen" | "erledigt" | "fehler" | "uebersprungen" | "hochgeladen" | "umgeschaltet" | "zurueckgerollt";

/** Ergebnis einer Phase von „Umschalten“ (Umschalten, Übernehmen, Rollback, Abbrechen). */
export interface PhasenErgebnis {
  erledigt: number;
  fehler: { nr: number; pfad: string; meldung: string }[];
  gestoppt: boolean;
}

/** Ergebnis eines Laufs; bei „Umschalten“ zusätzlich je Phase. */
export type LaufErgebnisGesamt = LaufErgebnis & {
  umschalten?: PhasenErgebnis;
  uebernehmen?: PhasenErgebnis;
  rollback?: PhasenErgebnis;
  abbrechen?: PhasenErgebnis;
  /** v0.6.1: Lauf wurde mitten im Ablauf verlassen (Programm beendet bzw. aufgegeben). */
  unterbrochen?: { grund: "programm_beendet" | "aufgegeben"; phase: LaufPhase };
};

/** Nächster Schritt eines wartenden Laufs. */
export type Schritt = "umschalten" | "uebernehmen" | "rollback" | "abbrechen";

export interface LaufStand {
  lauf_id: string;
  phase: LaufPhase;
  fehler: { pfad: string; meldung: string }[];
}

/** Ein Lauf aus der Datenbank (Protokoll der Streckenansicht). */
export interface Lauf {
  id: string;
  strecke_id: string;
  stufe_id: string;
  modus: Modus;
  phase: LaufPhase;
  /** Millisekunden seit 1970. */
  gestartet: number;
  beendet: number | null;
  zusammenfassung: PlanZusammenfassung;
  ergebnis: LaufErgebnisGesamt | null;
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
  /** Bemerkenswertes ohne Fehler, z. B. eine wiederholte Datenverbindung. */
  hinweis: string | null;
  aktualisiert: number;
}

export interface LaufAntwort {
  lauf_id: string;
  ergebnis: LaufErgebnis;
  /** Phase nach dem Start – bei „Umschalten“ `hochgeladen` (bereitgestellt) bzw. nach einem Abbruch `abgebrochen`. */
  phase: LaufPhase;
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
  /** Samba-Freigabe: anmelden und Ordner lesen (v0.7.1b). Ohne Eingabe gilt das Passwort aus dem Schlüsselbund. */
  smbVerbindungTesten: (ziel: Endpunkt, passwort: string) =>
    invoke<SmbVerbindungstest>("smb_verbindung_testen", { ziel, passwort: passwort || null }),
  /** SFTP-Server: anmelden, Host-Schlüssel prüfen, Ordner lesen (v0.7.2). Ohne Eingabe gilt das Passwort aus dem Schlüsselbund. */
  sftpVerbindungTesten: (ziel: Endpunkt, passwort: string) =>
    invoke<SftpVerbindungstest>("sftp_verbindung_testen", { ziel, passwort: passwort || null }),
  /** Scannt und vergleicht eine Stufe; verändert nichts. Fortschritt kommt gedrosselt. */
  /** `server`: FTPS-Ziel vom Server einlesen statt gegen das Manifest (lokale Ziele: ohne Wirkung). */
  vergleichStarten: (strecke: string, stufe: string, beiFortschritt: (f: Fortschritt) => void, server = false) =>
    invoke<VergleichStart>("vergleich_starten", { strecke, stufe, fortschritt: new Channel(beiFortschritt), server }),
  vergleichAbbrechen: (strecke: string, stufe: string) => invoke<boolean>("vergleich_abbrechen", { strecke, stufe }),
  /** FTPS: Dateien mit unbekanntem Inhalt herunterladen und hashen; abbrechen wie ein Vergleich. */
  inhaltePruefen: (vergleichId: string, beiFortschritt: (n: number) => void) =>
    invoke<InhaltePruefung>("inhalte_pruefen", { vergleichId, fortschritt: new Channel(beiFortschritt) }),
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
  /** Nächster Schritt eines wartenden Laufs („Umschalten“). */
  laufWeiter: (laufId: string, schritt: Schritt) => invoke<LaufStand>("lauf_weiter", { laufId, schritt }),
  /** Wartenden Lauf als unterbrochen markieren (wenn „alles zurück“ gescheitert ist) – verändert am Ziel nichts. */
  laufAufgeben: (laufId: string) => invoke<null>("lauf_aufgeben", { laufId }),
  /** Läufe einer Stufe, neueste zuerst. */
  laeufe: (strecke: string, stufe: string, limit: number) => invoke<Lauf[]>("laeufe", { strecke, stufe, limit }),
  /** Jeder Schritt eines Laufs. */
  laufJournal: (laufId: string) => invoke<JournalZeile[]>("lauf_journal", { laufId }),
};

/** Ereignis vom App-Menü „Über Stellwerk“ (siehe src-tauri/src/lib.rs). */
export const EREIGNIS_UEBER = "menue_ueber";
/** Ereignis vom App-Menü „Stellwerk beenden“ (⌘Q) – das Frontend fragt bei offenem Lauf nach. */
export const EREIGNIS_BEENDEN = "menue_beenden";

/** Fehlertext aus einem abgelehnten invoke. */
export function fehlerText(e: unknown): string {
  if (e && typeof e === "object" && "meldung" in e) return String((e as BefehlsFehler).meldung);
  return String(e);
}
