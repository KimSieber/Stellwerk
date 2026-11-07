// Typisierte Aufrufe der Rust-Commands (ARCHITEKTUR Kap. 9).
// Die Typen spiegeln stellwerk-core von Hand; sobald die Command-Zahl wächst,
// werden sie per specta/tauri-specta generiert.

import { invoke } from "@tauri-apps/api/core";

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
};

/** Ereignis vom App-Menü „Über Stellwerk“ (siehe src-tauri/src/lib.rs). */
export const EREIGNIS_UEBER = "menue_ueber";

/** Fehlertext aus einem abgelehnten invoke. */
export function fehlerText(e: unknown): string {
  if (e && typeof e === "object" && "meldung" in e) return String((e as BefehlsFehler).meldung);
  return String(e);
}
