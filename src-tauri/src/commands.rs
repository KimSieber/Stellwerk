//! Befehle für das Frontend (ARCHITEKTUR Kap. 9). Hier wird nur übersetzt, nicht gerechnet.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use serde_json::json;
use stellwerk_core::endpoint::ftps::{Verbindungstest, FTPS_WORKER};
use stellwerk_core::endpoint::sftp::{kennung as sftp_kennung, SFTP_WORKER};
use stellwerk_core::endpoint::smb::{kennung as smb_kennung, SMB_WORKER};
use stellwerk_core::endpoint::{Ftps, Lokal, QuellEndpunkt, ServerLesen, Sftp, Smb, ZielEndpunkt};
use stellwerk_core::exec::{direkt_ausfuehren, standard_worker, ExecFortschritt, LaufErgebnis, Optionen, Uebertragen};
use stellwerk_core::plan::{plan_erstellen as plan_erstellen_aus, OpArt, Operation, Plan, PlanWarnung, PlanZusammenfassung};
use stellwerk_core::store::{
    jetzt_ms, Datenbank, JournalStatus, JournalZeile, Lauf, LaufPhase, ManifestEintrag, ProtokollEbene, Protokollieren,
};

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use stellwerk_core::ansicht::{
    ftps_kennung, server_kennung, stufe_vergleichen_entfernt_mit_zugang, stufe_vergleichen_mit_zugang, stufe_vergleichen_vom_server_mit_zugang, ziel_kennung, Drossel,
    EntfernterStand, Filter, Fortschritt, Lesefehler, Seite,
    StufenVergleich, VergleichsSitzung,
};
use stellwerk_core::diff::Zusammenfassung;
use stellwerk_core::model::RelPath;

use stellwerk_core::config::{effektive_quelle, pruefe_ordner, regel_entfernen as entfernen, regel_setzen as setzen, validiere_strecke, GlobalConfig, Problem, RegelOrt};
use stellwerk_core::secrets::konten_der_strecke;
use stellwerk_core::model::{Endpunkt, Modus, Quelle, Regel, RegelTyp, Strecke};
use stellwerk_core::StellwerkError;

use crate::state::{AppState, OffenerVergleich};

/// Fehler, wie ihn das Frontend erhält.
#[derive(Debug, Serialize)]
pub struct BefehlsFehler {
    pub kategorie: &'static str,
    pub meldung: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub probleme: Vec<Problem>,
    /// Bei nicht anerkanntem Serverzertifikat: SHA-256 zum bewussten Bestätigen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

impl From<StellwerkError> for BefehlsFehler {
    fn from(e: StellwerkError) -> Self {
        let kategorie = e.kategorie();
        let meldung = e.to_string();
        let (probleme, fingerprint) = match e {
            StellwerkError::Validierung(p) => (p, None),
            StellwerkError::Zertifikat { fingerprint, .. } | StellwerkError::HostSchluessel { fingerprint, .. } => (vec![], Some(fingerprint)),
            _ => (vec![], None),
        };
        Self { kategorie, meldung, probleme, fingerprint }
    }
}

pub(crate) type Antwort<T> = Result<T, BefehlsFehler>;

/// Konfigurationsprüfung und Prüfung der lokalen Ordner zusammen.
fn alle_probleme(s: &Strecke, global: &GlobalConfig) -> Vec<Problem> {
    let mut p = validiere_strecke(s, global);
    p.extend(pruefe_ordner(s));
    p
}

/// Vor dem Schreiben: Fehler (auch fehlende Ordner) verhindern das Speichern.
fn speicherbar(state: &AppState, s: &Strecke) -> Result<(), StellwerkError> {
    let p = alle_probleme(s, &state.store.global_laden()?);
    if p.iter().any(Problem::ist_fehler) {
        return Err(StellwerkError::Validierung(p));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct StufeKurz {
    pub id: String,
    pub name: String,
    pub quelle: String,
    pub ziel: String,
    /// Ergebnis des letzten erfolgreichen Vergleichs (v0.6.6) – `None`: unbekannt.
    pub stand: Option<StufenStand>,
    /// Der neueste Lauf – zeigt, ob Reste am Ziel liegen können.
    pub letzter_lauf: Option<Lauf>,
}

/// Was der letzte Vergleich einer Stufe ergab (Stufen-Status `letzter_vergleich`). Verfällt
/// nach einer Übertragung (eigene Stufe und die, die ihr Ziel lesen) und beim Speichern der Strecke.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StufenStand {
    pub zeit: i64,
    pub neu: usize,
    pub geaendert: usize,
    pub loeschen: usize,
    pub konflikt: usize,
}

pub(crate) const LETZTER_VERGLEICH: &str = "letzter_vergleich";

/// Stände verfallen nach einem Lauf (oder Schritt) auf `stufe`: ihr eigener und der aller
/// Stufen, die ihr Ziel als Quelle lesen – deren Quelle hat sich geändert.
pub(crate) fn staende_nach_lauf(state: &AppState, strecke: &str, stufe: &str) -> Result<(), StellwerkError> {
    let s = state.store.strecke_laden(strecke)?;
    let db = state.datenbank.lock().unwrap();
    db.status_entfernen(strecke, Some(stufe), LETZTER_VERGLEICH)?;
    for st in s.stufen.iter().filter(|st| matches!(&st.quelle, Quelle::Stufe { stufe: v } if v == stufe)) {
        db.status_entfernen(strecke, Some(&st.id), LETZTER_VERGLEICH)?;
    }
    Ok(())
}

/// Eintrag der Startseite. Defekte Dateien erscheinen mit `fehler`.
#[derive(Debug, Serialize)]
pub struct StreckeUebersicht {
    pub id: String,
    pub name: Option<String>,
    /// Gruppe der Startseite (v0.8.1); `None` = ohne Gruppe.
    pub gruppe: Option<String>,
    pub stufen: Vec<StufeKurz>,
    pub probleme: Vec<Problem>,
    pub fehler: Option<String>,
}

fn quelle_kurz(q: &Quelle) -> String {
    match q {
        Quelle::Lokal { pfad } => pfad.display().to_string(),
        Quelle::Ftps(f) => format!("ftps://{}{}", f.host, f.pfad),
        Quelle::Smb(e) => smb_kurz(e),
        Quelle::Sftp(f) => format!("sftp://{}{}", f.host, f.pfad),
        Quelle::Stufe { stufe } => format!("Ziel von „{stufe}“"),
    }
}

fn ziel_kurz(z: &Endpunkt) -> String {
    match z {
        Endpunkt::Lokal { pfad } => pfad.display().to_string(),
        Endpunkt::Ftps(f) => format!("ftps://{}{}", f.host, f.pfad),
        Endpunkt::Smb(e) => smb_kurz(e),
        Endpunkt::Sftp(f) => format!("sftp://{}{}", f.host, f.pfad),
    }
}

/// `smb://host/freigabe/pfad` – ohne Standard-Port.
fn smb_kurz(e: &stellwerk_core::model::SmbEndpunkt) -> String {
    format!("smb://{}/{}{}", e.host, e.freigabe, e.pfad.trim_end_matches('/'))
}

#[tauri::command]
pub fn konfig_ort(state: State<'_, AppState>) -> String {
    state.store.wurzel().display().to_string()
}

#[tauri::command]
pub fn strecken_liste(state: State<'_, AppState>) -> Antwort<Vec<StreckeUebersicht>> {
    let global = state.store.global_laden()?;
    let liste = state.store.strecken_liste()?;
    let db = state.datenbank.lock().unwrap();
    let mut ergebnis = Vec::new();
    for e in liste {
        ergebnis.push(match e.ergebnis {
            Ok(s) => {
                let mut stufen = Vec::new();
                for st in &s.stufen {
                    // Nur gespeichert, nie neu verglichen (Startseite, v0.6.6)
                    let stand = db.status(&s.id, &st.id, LETZTER_VERGLEICH)?.and_then(|w| serde_json::from_str(&w).ok());
                    let letzter_lauf = db.laeufe(&s.id, &st.id, 1)?.into_iter().next();
                    stufen.push(StufeKurz {
                        id: st.id.clone(),
                        name: st.name.clone(),
                        quelle: quelle_kurz(&st.quelle),
                        ziel: ziel_kurz(&st.ziel),
                        stand,
                        letzter_lauf,
                    });
                }
                StreckeUebersicht {
                    id: e.id,
                    name: Some(s.name.clone()),
                    gruppe: s.gruppe_bereinigt(),
                    stufen,
                    probleme: alle_probleme(&s, &global),
                    fehler: None,
                }
            }
            Err(err) => StreckeUebersicht {
                id: e.id,
                name: None,
                gruppe: None,
                stufen: vec![],
                probleme: vec![],
                fehler: Some(err.to_string()),
            },
        });
    }
    Ok(ergebnis)
}

/// Globale Standardregeln (Ebene „Global“), z. B. `.DS_Store`, `.git/`.
#[tauri::command]
pub fn globale_regeln(state: State<'_, AppState>) -> Antwort<Vec<Regel>> {
    Ok(state.store.global_laden()?.regeln.standard)
}

#[tauri::command]
pub fn strecke_laden(state: State<'_, AppState>, id: String) -> Antwort<Strecke> {
    Ok(state.store.strecke_laden(&id)?)
}

#[tauri::command]
pub fn strecke_pruefen(state: State<'_, AppState>, strecke: Strecke) -> Antwort<Vec<Problem>> {
    let global = state.store.global_laden()?;
    Ok(alle_probleme(&strecke, &global))
}

/// Legt eine neue Strecke an; eine vorhandene Kennung wird nie überschrieben.
#[tauri::command]
pub fn strecke_anlegen(state: State<'_, AppState>, strecke: Strecke) -> Antwort<Vec<Problem>> {
    speicherbar(&state, &strecke)?;
    Ok(state.store.strecke_anlegen(&strecke)?)
}

/// Speichert und liefert verbleibende Warnungen.
#[tauri::command]
pub fn strecke_speichern(state: State<'_, AppState>, strecke: Strecke) -> Antwort<Vec<Problem>> {
    speicherbar(&state, &strecke)?;
    let probleme = state.store.strecke_speichern(&strecke)?;
    // Quelle, Ziel oder Regeln können sich geändert haben – die Stände gelten nicht mehr (v0.6.6)
    state.datenbank.lock().unwrap().status_entfernen(&strecke.id, None, LETZTER_VERGLEICH)?;
    Ok(probleme)
}

/// Löscht die Strecke und die Passwörter ihrer FTPS-Ziele. Dateien werden nie gelöscht.
#[tauri::command]
pub fn strecke_loeschen(state: State<'_, AppState>, id: String) -> Antwort<()> {
    // Eine defekte Datei lässt sich trotzdem löschen – dann ohne Passwörter
    let konten = state.store.strecke_laden(&id).map(|s| konten_der_strecke(&s)).unwrap_or_default();
    state.store.strecke_loeschen(&id)?;
    state.datenbank.lock().unwrap().status_entfernen(&id, None, LETZTER_VERGLEICH)?;
    for konto in konten {
        state.tresor.entfernen(&konto)?;
    }
    Ok(())
}

/// Ob ein (z. B. per Drag-and-drop abgelegter) Pfad ein vorhandener Ordner ist.
/// Liest nur die Dateiart, nichts sonst.
#[tauri::command]
pub fn ist_ordner(pfad: String) -> bool {
    std::path::Path::new(&pfad).is_dir()
}

/// Legt ein Passwort im Schlüsselbund ab. Es wird nie zurückgegeben.
#[tauri::command]
pub fn passwort_speichern(state: State<'_, AppState>, konto: String, passwort: String) -> Antwort<()> {
    Ok(state.tresor.setzen(&konto, &passwort)?)
}

/// Nur ob ein Passwort hinterlegt ist – nie das Passwort selbst.
#[tauri::command]
pub fn passwort_vorhanden(state: State<'_, AppState>, konto: String) -> Antwort<bool> {
    Ok(state.tresor.vorhanden(&konto)?)
}

/// „Verbindung testen“ im Editor (FTPS-Ziel): TLS, Zertifikat, Anmeldung, Serverpfad,
/// Datenkanal und Fähigkeiten. Liest nur – auf dem Server wird nichts verändert, daher
/// kein Protokolleintrag (E8). Passwort: das eingegebene (noch nicht gespeicherte),
/// sonst das aus dem Schlüsselbund. Es erscheint in keiner Antwort.
/// Zusätzlich, was Stellwerk über den Server dieser Stufe gelernt hat.
#[tauri::command]
pub async fn verbindung_testen(
    state: State<'_, AppState>,
    ziel: Endpunkt,
    passwort: Option<String>,
) -> Antwort<VerbindungstestAntwort> {
    let Endpunkt::Ftps(ziel) = ziel else {
        return Err(StellwerkError::NichtUnterstuetzt("Verbindungstest gibt es nur für FTPS-Ziele".into()).into());
    };
    let passwort = match passwort.filter(|p| !p.is_empty()) {
        Some(p) => p,
        None => hinterlegtes_passwort(&state, &ziel.konto_ref)?,
    };
    let umbenennen_ueberschreibt = match stufe_aus_konto(&ziel.konto_ref) {
        Some((strecke, stufe)) => gelernt_umbenennen(&state.datenbank.lock().unwrap(), strecke, stufe)?,
        None => None,
    };
    let ftps = Ftps::new(&ziel, &passwort)?;
    let test = tauri::async_runtime::spawn_blocking(move || ftps.verbindung_testen()).await.expect("Verbindungstest beendet");
    Ok(VerbindungstestAntwort { test: test?, umbenennen_ueberschreibt })
}

/// „Verbindung testen“ für eine Samba-Freigabe (v0.7.1b): anmelden und den Ordner
/// lesen, nichts verändern. Passwort: das eingegebene, sonst das aus dem Schlüsselbund.
#[tauri::command]
pub async fn smb_verbindung_testen(
    state: State<'_, AppState>,
    ziel: Endpunkt,
    passwort: Option<String>,
) -> Antwort<stellwerk_core::endpoint::smb::SmbVerbindungstest> {
    let Endpunkt::Smb(ziel) = ziel else {
        return Err(StellwerkError::NichtUnterstuetzt("Dieser Verbindungstest gilt nur für Samba-Freigaben".into()).into());
    };
    let passwort = match passwort.filter(|p| !p.is_empty()) {
        Some(p) => p,
        None => hinterlegtes_passwort(&state, &ziel.konto_ref)?,
    };
    let smb = Smb::new(&ziel, &passwort);
    let test = tauri::async_runtime::spawn_blocking(move || smb.verbindung_testen()).await.expect("Verbindungstest beendet");
    Ok(test?)
}

/// „Verbindung testen“ für einen SFTP-Server (v0.7.2): Host-Schlüssel, Anmeldung,
/// Ordner – liest nur. Ist der Host-Schlüssel nicht bestätigt, kommt sein Fingerabdruck
/// im Fehler (`fingerprint`) zum Bestätigen. Passwort bzw. Passphrase des SSH-Schlüssels (v0.7.2b):
/// eingegeben, sonst aus dem Schlüsselbund.
#[tauri::command]
pub async fn sftp_verbindung_testen(
    state: State<'_, AppState>,
    ziel: Endpunkt,
    passwort: Option<String>,
) -> Antwort<stellwerk_core::endpoint::sftp::SftpVerbindungstest> {
    let Endpunkt::Sftp(ziel) = ziel else {
        return Err(StellwerkError::NichtUnterstuetzt("Dieser Verbindungstest gilt nur für SFTP-Server".into()).into());
    };
    let sftp = match passwort.filter(|p| !p.is_empty()) {
        Some(p) => Sftp::new(&ziel, &p),
        None => sftp_zugang(&state, &ziel)?,
    };
    let test = tauri::async_runtime::spawn_blocking(move || sftp.verbindung_testen()).await.expect("Verbindungstest beendet");
    Ok(test?)
}

/// Antwort von `verbindung_testen`.
#[derive(Debug, Serialize)]
pub struct VerbindungstestAntwort {
    #[serde(flatten)]
    pub test: Verbindungstest,
    /// Gelernt aus Läufen: Ersetzt der Server beim Umbenennen eine vorhandene Datei?
    /// `None`: noch nicht bekannt (oder Ziel noch keiner Stufe zugeordnet).
    pub umbenennen_ueberschreibt: Option<bool>,
}

/// Schlüssel in `stufen_status`: Ersetzt der Server beim Umbenennen (`ja`/`nein`)?
pub(crate) const UMBENENNEN_UEBERSCHREIBT: &str = "umbenennen_ueberschreibt";

/// Strecke und Stufe aus dem Konto `stellwerk:<strecke>:<stufe>`.
fn stufe_aus_konto(konto: &str) -> Option<(&str, &str)> {
    let mut teile = konto.split(':');
    match (teile.next(), teile.next(), teile.next(), teile.next()) {
        (Some("stellwerk"), Some(strecke), Some(stufe), None) => Some((strecke, stufe)),
        _ => None,
    }
}

/// Gelerntes `umbenennen_ueberschreibt` der Stufe; Unbekanntes gilt als nicht gelernt.
pub(crate) fn gelernt_umbenennen(db: &Datenbank, strecke: &str, stufe: &str) -> Result<Option<bool>, StellwerkError> {
    Ok(match db.status(strecke, stufe, UMBENENNEN_UEBERSCHREIBT)?.as_deref() {
        Some("ja") => Some(true),
        Some("nein") => Some(false),
        _ => None,
    })
}

/// Passwort aus dem Schlüsselbund; fehlt es → „nicht gefunden“ mit Hinweis.
pub(crate) fn hinterlegtes_passwort(state: &AppState, konto: &str) -> Result<String, StellwerkError> {
    state
        .tresor
        .lesen(konto)?
        .ok_or_else(|| StellwerkError::NichtGefunden("Passwort – bitte eingeben (im Schlüsselbund ist keines hinterlegt)".into()))
}

/// Zugang zu einem SFTP-Server mit dem Passwort aus dem Schlüsselbund – oder bei einem
/// SSH-Schlüssel (v0.7.2b) mit dessen Passphrase; die darf fehlen.
pub(crate) fn sftp_zugang(state: &AppState, e: &stellwerk_core::model::SftpEndpunkt) -> Result<Sftp, StellwerkError> {
    let geheim = match e.schluessel {
        Some(_) => state.tresor.lesen(&e.konto_ref)?.unwrap_or_default(),
        None => hinterlegtes_passwort(state, &e.konto_ref)?,
    };
    Ok(Sftp::new(e, &geheim))
}

#[tauri::command]
pub fn passwort_entfernen(state: State<'_, AppState>, konto: String) -> Antwort<()> {
    Ok(state.tresor.entfernen(&konto)?)
}

// --- Vergleich (v0.2) ---------------------------------------------------------

/// Antwort auf `vergleich_starten`; die Zeilen holt die Oberfläche mit `diff_seite`.
#[derive(Debug, Serialize)]
pub struct VergleichStart {
    pub vergleich_id: String,
    /// Aufgelöste Ordner (bei „Ziel einer Vorstufe“ deren Zielordner).
    pub quelle: String,
    pub ziel: String,
    pub zusammenfassung: Zusammenfassung,
    /// Anzahl aller Zeilen ohne Filter.
    pub zeilen: usize,
    pub lesefehler: Vec<Lesefehler>,
    /// Nur bei FTPS-Zielen: woher der Stand des Ziels kommt (`manifest` | `server`).
    pub ziel_stand: Option<&'static str>,
    /// Dateien mit „Inhalt nicht geprüft“ – für „Inhalte prüfen“.
    pub ungeprueft: Ungeprueft,
    /// Modus der Stufe, wie er jetzt konfiguriert ist (v0.6.3: Anzeige in der Vergleichsansicht).
    pub modus: Modus,
}

/// Anzahl und Größe der Dateien, deren Inhalt am Ziel unbekannt ist.
#[derive(Debug, Default, Serialize)]
pub struct Ungeprueft {
    pub anzahl: usize,
    pub bytes: u64,
}

/// Wann der Server zuletzt vollständig eingelesen wurde (`stufen_status`).
const MANIFEST_SEIT: &str = "manifest_seit";
/// Stufen-Status (v0.6.2): „ja“ – der nächste Vergleich liest den Server, auch ohne
/// „Mit Server vergleichen“ (gesetzt, wenn ein Lauf unterbrochen wurde: Reste am Server
/// kennt der letzte bekannte Stand nicht).
pub(crate) const SERVER_LESEN: &str = "server_lesen";

/// Scannt Quelle und Ziel einer Stufe und vergleicht sie. Verändert nichts (E4) –
/// auch am Server nicht. FTPS-Ziel: ohne `server` gegen das Manifest (kein
/// Serverkontakt; nie eingelesen → `kein_manifest`), mit `server` wird der Server
/// eingelesen und das Manifest abgeglichen. Fortschritt kommt gedrosselt über
/// `fortschritt`. Läuft außerhalb des Hauptthreads (async), damit
/// `vergleich_abbrechen` jederzeit ankommt.
#[tauri::command]
pub async fn vergleich_starten(
    state: State<'_, AppState>,
    strecke: String,
    stufe: String,
    fortschritt: Channel<Fortschritt>,
    server: Option<bool>,
) -> Antwort<VergleichStart> {
    offen_pruefen(&state, &strecke, &stufe)?;
    let s = state.store.strecke_laden(&strecke)?;
    let global = state.store.global_laden()?;
    let schluessel = format!("{strecke}/{stufe}");
    let abbruch = state.vergleiche.anmelden(&schluessel);
    let mut drossel = Drossel::neu(Duration::from_millis(100));
    let mut melden = |f: Fortschritt| {
        if drossel.durchlassen(&f, Instant::now()) {
            // Ein geschlossenes Fenster ist kein Grund, den Vergleich abzubrechen
            let _ = fortschritt.send(f);
        }
    };
    let modus = s.stufen.iter().find(|st| st.id == stufe).map(|st| st.modus).unwrap_or_default();
    // Ziel auf einem Server (FTPS, v0.7.2 auch SFTP): Vergleich über den letzten bekannten Stand
    let ziel_server: Option<Quelle> =
        s.stufen.iter().find(|st| st.id == stufe).map(|st| st.ziel.clone().into()).filter(|z| server_kennung(z).is_some());
    // Passwörter von Samba-Freigaben (v0.7.1)
    let zugang = |konto: &str| hinterlegtes_passwort(&state, konto);
    // v0.6.5: Quelle auf einem Server (direkt oder über „Ziel einer Vorstufe“)
    let quelle_server =
        s.stufen.iter().position(|st| st.id == stufe).and_then(|i| effektive_quelle(&s, i)).filter(|q| server_kennung(q).is_some());
    let ergebnis = match (&quelle_server, &ziel_server) {
        (Some(q), None) => vom_server(&state, &s, &stufe, &global, q, &abbruch, &mut melden).map(|v| (v, None)),
        (None, None) => stufe_vergleichen_mit_zugang(&s, &stufe, &global, &zugang, &abbruch, &mut melden).map(|v| (v, None)),
        // Server-Ziel – die Quelle lokal oder (v0.6.5b) ebenfalls ein Server
        (q, Some(f)) => vergleichen_entfernt(&state, &s, &stufe, &global, f, q.as_ref(), server.unwrap_or(false), &abbruch, &mut melden)
            .map(|(v, mit_server)| (v, Some(if mit_server { "server" } else { "manifest" }))),
    };
    state.vergleiche.abmelden(&schluessel, &abbruch);
    let (v, ziel_stand) = ergebnis?;
    // Für die Startseite merken (v0.6.6)
    let z = &v.vergleich.zusammenfassung;
    let stand = StufenStand { zeit: jetzt_ms(), neu: z.neu, geaendert: z.geaendert, loeschen: z.loeschen, konflikt: z.konflikt };
    state.datenbank.lock().unwrap().status_setzen(&strecke, &stufe, LETZTER_VERGLEICH, &json!(stand).to_string(), jetzt_ms())?;

    let sitzung = VergleichsSitzung::neu(v.vergleich);
    let zusammenfassung = sitzung.zusammenfassung().clone();
    let liste = sitzung.ungeprueft();
    let ungeprueft = Ungeprueft { anzahl: liste.len(), bytes: liste.iter().map(|(_, b)| b).sum() };
    let zeilen = sitzung.anzahl();
    let (quelle, ziel) = (v.quelle.display().to_string(), v.ziel.display().to_string());
    let offen = OffenerVergleich { sitzung, strecke, stufe, quelle: v.quelle, ziel: v.ziel, plan: None };
    Ok(VergleichStart {
        vergleich_id: state.vergleiche.ablegen(&schluessel, offen),
        quelle,
        ziel,
        zusammenfassung,
        zeilen,
        lesefehler: v.lesefehler,
        ziel_stand,
        ungeprueft,
        modus,
    })
}

/// Ergebnis von „Inhalte prüfen“.
#[derive(Debug, Serialize)]
pub struct InhaltePruefung {
    /// Dateien, deren Inhalt jetzt bekannt ist.
    pub geprueft: usize,
    pub fehler: Vec<crate::umschalten::SchrittFehler>,
}

/// „Inhalte prüfen“ (nur Server-Ziele: FTPS, SFTP): Dateien mit unbekanntem Inhalt einmal herunterladen,
/// hashen und den Hash im letzten bekannten Stand vermerken. Liest am Server nur.
/// Danach vergleicht die Ansicht neu (ohne Serverkontakt). Abbrechen wie ein Vergleich.
#[tauri::command]
pub async fn inhalte_pruefen(state: State<'_, AppState>, vergleich_id: String, fortschritt: Channel<usize>) -> Antwort<InhaltePruefung> {
    let (strecke, stufe, pfade) = mit_vergleich(&state, &vergleich_id, |o| {
        Ok((o.strecke.clone(), o.stufe.clone(), o.sitzung.ungeprueft().into_iter().map(|(p, _)| p).collect::<Vec<_>>()))
    })?;
    let s = state.store.strecke_laden(&strecke)?;
    let ziel = s.stufen.into_iter().find(|st| st.id == stufe).map(|st| Quelle::from(st.ziel));
    let Some(server) = ziel.as_ref().map(|z| ServerEp::neu(&state, z)).transpose()?.flatten() else {
        return Err(StellwerkError::NichtUnterstuetzt("„Inhalte prüfen“ gibt es nur für Server-Ziele – bitte neu vergleichen".into()).into());
    };
    let schluessel = format!("{strecke}/{stufe}");
    let abbruch = state.vergleiche.anmelden(&schluessel);
    let r = server.inhalte_hashen(&pfade, &abbruch, &mut |n| drop(fortschritt.send(n)));
    state.vergleiche.abmelden(&schluessel, &abbruch);
    let db = state.datenbank.lock().unwrap();
    let mut bekannt: std::collections::HashMap<RelPath, ManifestEintrag> =
        db.manifest(&strecke, &stufe)?.into_iter().map(|e| (e.pfad.clone(), e)).collect();
    let mut erg = InhaltePruefung { geprueft: 0, fehler: vec![] };
    for (pfad, h) in r? {
        match h {
            Ok(g) => {
                let leer = ManifestEintrag { pfad: pfad.clone(), ist_ordner: false, groesse: 0, hash: None, mtime_ns: None, lauf_id: None };
                let alt = bekannt.remove(&pfad).unwrap_or(leer);
                db.manifest_setzen(&strecke, &stufe, &ManifestEintrag { groesse: g.bytes, hash: Some(g.hash), ..alt })?;
                erg.geprueft += 1;
            }
            Err(meldung) => erg.fehler.push(crate::umschalten::SchrittFehler { pfad: pfad.to_string(), meldung }),
        }
    }
    Ok(erg)
}

/// Stufe mit Quelle auf einem Server vergleichen (v0.6.5) – mit dem Passwort der Quelle.
fn vom_server(
    state: &AppState,
    s: &Strecke,
    stufe: &str,
    global: &GlobalConfig,
    quelle: &Quelle,
    abbruch: &AtomicBool,
    melden: &mut dyn FnMut(Fortschritt),
) -> Result<StufenVergleich, StellwerkError> {
    let server = ServerEp::neu(state, quelle)?.expect("Quelle ist ein Server");
    let zugang = |konto: &str| hinterlegtes_passwort(state, konto);
    Ok(stufe_vergleichen_vom_server_mit_zugang(s, stufe, global, server.lesen(), None, &zugang, abbruch, melden)?.vergleich)
}

/// Vergleich mit einem Ziel-Server – die Quelle lokal oder (v0.6.5b) ein Server.
#[allow(clippy::too_many_arguments)]
fn mit_stand(
    s: &Strecke,
    stufe: &str,
    global: &GlobalConfig,
    quelle: Option<&ServerEp>,
    stand: EntfernterStand,
    zugang: stellwerk_core::ansicht::Zugang,
    abbruch: &AtomicBool,
    melden: &mut dyn FnMut(Fortschritt),
) -> Result<stellwerk_core::ansicht::EntfernterVergleich, StellwerkError> {
    match quelle {
        Some(q) => stufe_vergleichen_vom_server_mit_zugang(s, stufe, global, q.lesen(), Some(stand), zugang, abbruch, melden),
        None => stufe_vergleichen_entfernt_mit_zugang(s, stufe, global, stand, zugang, abbruch, melden),
    }
}

/// Stufe mit Server-Ziel (FTPS, SFTP) vergleichen. Die Datenbank ist nie während des Netzzugriffs gesperrt.
/// Liefert auch, ob der Server gelesen wurde (gewünscht oder nach einem unterbrochenen Lauf).
#[allow(clippy::too_many_arguments)]
fn vergleichen_entfernt(
    state: &AppState,
    s: &Strecke,
    stufe: &str,
    global: &GlobalConfig,
    ziel: &Quelle,
    quelle: Option<&Quelle>,
    mit_server: bool,
    abbruch: &AtomicBool,
    melden: &mut dyn FnMut(Fortschritt),
) -> Result<(StufenVergleich, bool), StellwerkError> {
    // Quelle auf einem Server: vom Server gelesen, mit eigenem Passwort
    let quelle = match quelle {
        Some(q) => ServerEp::neu(state, q)?,
        None => None,
    };
    let (seit, manifest, lesen) = {
        let db = state.datenbank.lock().unwrap();
        (db.status(&s.id, stufe, MANIFEST_SEIT)?, db.manifest(&s.id, stufe)?, db.status(&s.id, stufe, SERVER_LESEN)?)
    };
    let mit_server = mit_server || lesen.as_deref() == Some("ja");
    // Eine Quelle auf einer Samba-Freigabe braucht ihr Passwort (v0.7.1)
    let zugang = |konto: &str| hinterlegtes_passwort(state, konto);
    if !mit_server {
        if seit.is_none() {
            return Err(StellwerkError::KeinManifest);
        }
        let v = mit_stand(s, stufe, global, quelle.as_ref(), EntfernterStand::Manifest(&manifest), &zugang, abbruch, melden)?;
        return Ok((v.vergleich, false));
    }
    let server = ServerEp::neu(state, ziel)?.expect("Ziel ist ein Server");
    let v = mit_stand(s, stufe, global, quelle.as_ref(), EntfernterStand::Server { server: server.lesen(), manifest: &manifest }, &zugang, abbruch, melden)?;
    // Mit Server gibt es immer ein abgeglichenes Manifest
    let neu = v.neues_manifest.unwrap_or_default();
    let mut db = state.datenbank.lock().unwrap();
    db.manifest_ersetzen(&s.id, stufe, &neu)?;
    db.status_setzen(&s.id, stufe, MANIFEST_SEIT, &jetzt_ms().to_string(), jetzt_ms())?;
    db.status_setzen(&s.id, stufe, SERVER_LESEN, "nein", jetzt_ms())?;
    Ok((v.vergleich, true))
}

/// Bricht den laufenden Vergleich einer Stufe ab; `false`, wenn keiner läuft.
#[tauri::command]
pub fn vergleich_abbrechen(state: State<'_, AppState>, strecke: String, stufe: String) -> bool {
    state.vergleiche.abbrechen(&format!("{strecke}/{stufe}"))
}

/// Führt `f` mit dem offenen Vergleich `id` aus; unbekannt → „nicht gefunden“.
fn mit_vergleich<T>(
    state: &AppState,
    id: &str,
    f: impl FnOnce(&mut OffenerVergleich) -> Result<T, StellwerkError>,
) -> Antwort<T> {
    let mut sitzungen = state.vergleiche.sitzungen.lock().unwrap();
    let offen = sitzungen
        .get_mut(id)
        .ok_or_else(|| StellwerkError::NichtGefunden(format!("Vergleich „{id}“ – bitte neu vergleichen")))?;
    Ok(f(offen)?)
}

/// Wie [`mit_vergleich`], nur mit der Sitzung.
fn mit_sitzung<T>(
    state: &AppState,
    id: &str,
    f: impl FnOnce(&mut VergleichsSitzung) -> Result<T, StellwerkError>,
) -> Antwort<T> {
    mit_vergleich(state, id, |o| f(&mut o.sitzung))
}

/// Sichtbare Zeilen ab `offset` (Filter und zugeklappte Ordner wertet Rust aus).
#[tauri::command]
pub fn diff_seite(
    state: State<'_, AppState>,
    vergleich_id: String,
    offset: usize,
    limit: usize,
    filter: Filter,
    zugeklappt: Vec<RelPath>,
) -> Antwort<Seite> {
    let zu: HashSet<RelPath> = zugeklappt.into_iter().collect();
    mit_sitzung(&state, &vergleich_id, |s| Ok(s.seite(offset, limit, filter, &zu)))
}

/// Häkchen setzen/entfernen; ein Ordner wirkt auf seinen Inhalt. Liefert die neue Zusammenfassung.
#[tauri::command]
pub fn auswahl_setzen(
    state: State<'_, AppState>,
    vergleich_id: String,
    pfade: Vec<RelPath>,
    an: bool,
) -> Antwort<Zusammenfassung> {
    mit_vergleich(&state, &vergleich_id, |o| {
        // Eine andere Auswahl braucht einen neuen Plan und eine neue Bestätigung
        o.plan = None;
        o.sitzung.auswahl_setzen(&pfade, an).cloned()
    })
}

/// Gibt den Speicher eines Vergleichs frei (beim Verlassen der Ansicht).
#[tauri::command]
pub fn vergleich_schliessen(state: State<'_, AppState>, vergleich_id: String) {
    state.vergleiche.sitzungen.lock().unwrap().remove(&vergleich_id);
}

// --- Kontextmenü „Regel setzen“ (v0.2) ------------------------------------------

/// Setzt eine Regel für genau `pfad` auf der Ebene `ort`, prüft und speichert die
/// Strecke wie der Editor. Liefert verbleibende Warnungen. Danach neu vergleichen.
#[tauri::command]
pub fn regel_setzen(
    state: State<'_, AppState>,
    strecke: String,
    ort: RegelOrt,
    pfad: RelPath,
    ordner: bool,
    typ: RegelTyp,
    ersatz: Option<RelPath>,
) -> Antwort<Vec<Problem>> {
    let mut s = state.store.strecke_laden(&strecke)?;
    setzen(&mut s, &ort, &pfad, ordner, typ, ersatz.as_ref())?;
    speicherbar(&state, &s)?;
    Ok(state.store.strecke_speichern(&s)?)
}

/// Entfernt die Regel für genau `pfad` auf der Ebene `ort` und speichert.
#[tauri::command]
pub fn regel_entfernen(
    state: State<'_, AppState>,
    strecke: String,
    ort: RegelOrt,
    pfad: RelPath,
    ordner: bool,
) -> Antwort<Vec<Problem>> {
    let mut s = state.store.strecke_laden(&strecke)?;
    entfernen(&mut s, &ort, &pfad, ordner)?;
    speicherbar(&state, &s)?;
    Ok(state.store.strecke_speichern(&s)?)
}

/// Dateien der Quelle im selben Ordner wie `pfad` – Vorschläge für „Ersetzen durch …“.
#[tauri::command]
pub fn ersatz_vorschlaege(state: State<'_, AppState>, vergleich_id: String, pfad: RelPath) -> Antwort<Vec<RelPath>> {
    mit_sitzung(&state, &vergleich_id, |s| Ok(s.quelldateien_neben(&pfad)))
}

// --- Übertragen „Direkt“ (v0.3) ------------------------------------------------

/// Was vor dem Übertragen zur Bestätigung gezeigt wird.
#[derive(Debug, Serialize)]
pub struct PlanVorschau {
    pub zusammenfassung: PlanZusammenfassung,
    /// Alle Pfade, die gelöscht werden (Sammelbestätigung, E5).
    pub loeschliste: Vec<RelPath>,
    pub warnungen: Vec<PlanWarnung>,
    pub quelle: String,
    pub ziel: String,
    /// So viele Dateien werden gleichzeitig geschrieben.
    pub worker: usize,
    /// Modus der Stufe („Umschalten“: erst Schattendateien, dann umbenennen).
    pub modus: Modus,
}

/// Erstellt den Plan aus der aktuellen Auswahl und merkt ihn für `lauf_starten`.
/// Verändert nichts.
#[tauri::command]
pub fn plan_erstellen(state: State<'_, AppState>, vergleich_id: String) -> Antwort<PlanVorschau> {
    let strecken = state.store.clone();
    mit_vergleich(&state, &vergleich_id, |o| {
        let s = strecken.strecke_laden(&o.strecke)?;
        let i = s
            .stufen
            .iter()
            .position(|st| st.id == o.stufe)
            .ok_or_else(|| StellwerkError::NichtGefunden(format!("Stufe „{}“ – bitte neu vergleichen", o.stufe)))?;
        let st = &s.stufen[i];
        offen_pruefen(&state, &o.strecke, &o.stufe)?;
        let quelle = effektive_quelle(&s, i);
        let gleiche_ordner = ziel_kennung(&st.ziel) == o.ziel && quelle.as_ref().and_then(quell_kennung).as_deref() == Some(o.quelle.as_path());
        if !gleiche_ordner {
            return Err(StellwerkError::NichtUnterstuetzt(
                "Quelle oder Ziel der Stufe wurden seit dem Vergleich geändert – bitte neu vergleichen".into(),
            ));
        }
        let plan = plan_erstellen_aus(o.sitzung.eintraege(), &st.rechte)?;
        let vorschau = PlanVorschau {
            zusammenfassung: plan.zusammenfassung.clone(),
            loeschliste: plan.loeschliste.clone(),
            warnungen: plan.warnungen.clone(),
            quelle: o.quelle.display().to_string(),
            ziel: o.ziel.display().to_string(),
            worker: worker_fuer(quelle.as_ref().and_then(server_kennung).is_some(), &st.ziel),
            modus: st.modus,
        };
        o.plan = Some(plan);
        Ok(vorschau)
    })
}

#[derive(Debug, Serialize)]
pub struct LaufAntwort {
    pub lauf_id: String,
    pub ergebnis: LaufErgebnis,
    /// Phase nach dem Start – bei „Umschalten manuell“ `hochgeladen`.
    pub phase: LaufPhase,
}

/// Wartet auf der Stufe ein Lauf auf seinen nächsten Schritt, ist sie gesperrt.
pub(crate) fn offen_pruefen(state: &AppState, strecke: &str, stufe: &str) -> Result<(), StellwerkError> {
    match state.datenbank.lock().unwrap().offener_lauf(strecke, stufe)? {
        Some(l) => Err(StellwerkError::LaufOffen(format!(
            "Auf dieser Stufe wartet ein Lauf auf „{}“ – bitte erst in der Streckenansicht abschließen",
            if l.phase == LaufPhase::Hochgeladen { "Umschalten" } else { "Übernehmen" }
        ))),
        None => Ok(()),
    }
}

/// Ein Server (FTPS oder SFTP) mit Passwort aus dem Schlüsselbund (v0.7.2).
pub(crate) enum ServerEp {
    Ftps(Ftps),
    Sftp(Sftp),
}

impl ServerEp {
    /// `None`, wenn der Endpunkt kein Server ist.
    pub(crate) fn neu(state: &AppState, q: &Quelle) -> Result<Option<Self>, StellwerkError> {
        Ok(match q {
            Quelle::Ftps(f) => Some(Self::Ftps(Ftps::new(f, &hinterlegtes_passwort(state, &f.konto_ref)?)?)),
            Quelle::Sftp(f) => Some(Self::Sftp(sftp_zugang(state, f)?)),
            _ => None,
        })
    }

    pub(crate) fn lesen(&self) -> &dyn ServerLesen {
        match self {
            Self::Ftps(f) => f,
            Self::Sftp(s) => s,
        }
    }

    #[allow(clippy::type_complexity)]
    fn inhalte_hashen(
        &self,
        pfade: &[RelPath],
        abbruch: &AtomicBool,
        fortschritt: &mut dyn FnMut(usize),
    ) -> Result<Vec<(RelPath, Result<stellwerk_core::endpoint::Geschrieben, String>)>, StellwerkError> {
        match self {
            Self::Ftps(f) => f.inhalte_hashen(pfade, abbruch, fortschritt),
            Self::Sftp(s) => s.inhalte_hashen(pfade, abbruch, fortschritt),
        }
    }
}

/// Das Ziel einer Stufe als Endpunkt – FTPS mit Passwort und Gelerntem.
pub(crate) enum ZielEp {
    Lokal(Lokal),
    Ftps(Ftps),
    Smb(Smb),
    Sftp(Sftp),
}

impl ZielEp {
    pub(crate) fn neu(state: &AppState, ziel: &Endpunkt, strecke: &str, stufe: &str) -> Result<Self, StellwerkError> {
        Ok(match ziel {
            Endpunkt::Ftps(f) => {
                let gelernt = gelernt_umbenennen(&state.datenbank.lock().unwrap(), strecke, stufe)?;
                Self::Ftps(Ftps::new(f, &hinterlegtes_passwort(state, &f.konto_ref)?)?.mit_umbenennen_ueberschreibt(gelernt))
            }
            Endpunkt::Lokal { pfad } => Self::Lokal(Lokal::new(pfad)),
            Endpunkt::Smb(e) => Self::Smb(Smb::new(e, &hinterlegtes_passwort(state, &e.konto_ref)?)),
            Endpunkt::Sftp(e) => Self::Sftp(sftp_zugang(state, e)?),
        })
    }

    pub(crate) fn ep(&self) -> &dyn ZielEndpunkt {
        match self {
            Self::Lokal(l) => l,
            Self::Ftps(f) => f,
            Self::Smb(s) => s,
            Self::Sftp(s) => s,
        }
    }

    pub(crate) fn ftps(&self) -> Option<&Ftps> {
        match self {
            Self::Lokal(_) | Self::Smb(_) | Self::Sftp(_) => None,
            Self::Ftps(f) => Some(f),
        }
    }

    /// Ziel auf einem Server (FTPS, SFTP) – der letzte bekannte Stand wird fortgeschrieben.
    pub(crate) fn ist_server(&self) -> bool {
        matches!(self, Self::Ftps(_) | Self::Sftp(_))
    }

    /// Gelerntes (Überschreiben beim Umbenennen) speichern, sobald bekannt.
    pub(crate) fn gelerntes_speichern(&self, db: &Datenbank, strecke: &str, stufe: &str) -> Result<(), StellwerkError> {
        match self.ftps().and_then(Ftps::umbenennen_ueberschreibt) {
            Some(g) => db.status_setzen(strecke, stufe, UMBENENNEN_UEBERSCHREIBT, if g { "ja" } else { "nein" }, jetzt_ms()),
            None => Ok(()),
        }
    }
}

/// Führt den zuletzt bestätigten Plan aus („Direkt“). Schreibt nur ins Ziel (E3).
/// Löschungen müssen ausdrücklich bestätigt sein (E5). Jede Operation steht im
/// Journal, Beginn, Fehler und Ende im Protokoll (E8). Danach muss neu verglichen werden.
#[tauri::command]
pub async fn lauf_starten(
    state: State<'_, AppState>,
    vergleich_id: String,
    loeschen_bestaetigt: bool,
    bei_fehler_stoppen: bool,
    fortschritt: Channel<ExecFortschritt>,
) -> Antwort<LaufAntwort> {
    let (plan, strecke, stufe, quelle, ziel, zeiten) = mit_vergleich(&state, &vergleich_id, |o| {
        // Der Plan gilt genau einmal – ein zweiter Start braucht eine neue Bestätigung
        let plan = o.plan.take().ok_or_else(|| {
            StellwerkError::NichtBestaetigt("Übertragung – bitte „Übertragen …“ erneut öffnen und bestätigen".into())
        })?;
        Ok((plan, o.strecke.clone(), o.stufe.clone(), o.quelle.clone(), o.ziel.clone(), quell_zeiten(&o.sitzung)))
    })?;
    if plan.hat_loeschungen() && !loeschen_bestaetigt {
        return Err(StellwerkError::NichtBestaetigt(format!("{} Löschungen", plan.loeschliste.len())).into());
    }
    let (ziel, modus) = ziel_der_stufe(&state, &strecke, &stufe, &ziel)?;
    let quelle = QuellEp::neu(&state, &strecke, &stufe, &quelle, zeiten)?;
    offen_pruefen(&state, &strecke, &stufe)?;
    let schluessel = format!("{strecke}/{stufe}");
    let abbruch = state
        .laeufe
        .anmelden(&schluessel)
        .ok_or_else(|| StellwerkError::Vorhanden(format!("Übertragung für „{schluessel}“ läuft bereits")))?;
    let ergebnis = if modus.schaltet_um() {
        crate::umschalten::starten(&state, &plan, (&strecke, &stufe), modus, &quelle, &ziel, bei_fehler_stoppen, &abbruch, &fortschritt)
    } else {
        uebertragen(&state, &plan, &strecke, &stufe, &quelle, &ziel, bei_fehler_stoppen, &abbruch, &fortschritt)
    };
    state.laeufe.abmelden(&schluessel);
    let ergebnis = ergebnis?;
    staende_nach_lauf(&state, &strecke, &stufe)?;
    Ok(ergebnis)
}

/// So viele Dateien werden gleichzeitig geschrieben – bei einer Quelle auf einem
/// Server höchstens so viele, wie dort Verbindungen erlaubt sind.
pub(crate) fn worker_fuer(quelle_server: bool, ziel: &Endpunkt) -> usize {
    let n = match ziel {
        Endpunkt::Lokal { .. } => standard_worker(),
        Endpunkt::Ftps(_) => FTPS_WORKER,
        Endpunkt::Smb(_) => SMB_WORKER,
        Endpunkt::Sftp(_) => SFTP_WORKER,
    };
    if quelle_server { n.min(FTPS_WORKER) } else { n }
}

/// Kennung einer aufgelösten Quelle wie im Vergleich: Ordner bzw. `ftps://…`.
fn quell_kennung(q: &Quelle) -> Option<PathBuf> {
    // `Stufe` kommt aufgelöst nicht vor
    match q { Quelle::Lokal { pfad } => Some(pfad.clone()), Quelle::Ftps(f) => Some(ftps_kennung(f)), Quelle::Smb(e) => Some(PathBuf::from(smb_kennung(e))), Quelle::Sftp(e) => Some(PathBuf::from(sftp_kennung(e))), Quelle::Stufe { .. } => None }
}

/// Änderungszeit der Quelle je Pfad aus dem Vergleich – eine Quelle auf dem Server
/// übernimmt sie beim Schreiben (v0.6.5).
fn quell_zeiten(sitzung: &VergleichsSitzung) -> HashMap<RelPath, i64> {
    sitzung.eintraege().iter().filter_map(|e| e.quelle.as_ref()).filter_map(|q| Some((q.pfad.clone(), q.mtime_ns?))).collect()
}

/// Quelle eines Laufs: lokaler Ordner oder Server (v0.6.5), wie sie jetzt konfiguriert
/// ist – sie muss dieselbe sein wie beim Vergleich.
pub(crate) enum QuellEp {
    Lokal(PathBuf),
    Ftps { ftps: Ftps, zeiten: HashMap<RelPath, i64>, kennung: PathBuf },
    /// Samba-Freigabe (v0.7.1) – liest direkt, wie ein lokaler Ordner.
    Smb { smb: Smb, kennung: PathBuf },
    /// SFTP-Server (v0.7.2) – die Änderungszeit kommt vom Server.
    Sftp { sftp: Sftp, kennung: PathBuf },
}

impl QuellEp {
    pub(crate) fn neu(state: &AppState, strecke: &str, stufe: &str, kennung: &Path, zeiten: HashMap<RelPath, i64>) -> Result<Self, StellwerkError> {
        let s = state.store.strecke_laden(strecke)?;
        let geaendert = || StellwerkError::NichtUnterstuetzt("Quelle der Stufe wurde seit dem Vergleich geändert – bitte neu vergleichen".into());
        let i = s.stufen.iter().position(|st| st.id == stufe).ok_or_else(geaendert)?;
        match effektive_quelle(&s, i) {
            Some(Quelle::Lokal { pfad }) if pfad == kennung => Ok(Self::Lokal(pfad)),
            Some(Quelle::Ftps(f)) if ftps_kennung(&f) == kennung => {
                let ftps = Ftps::new(&f, &hinterlegtes_passwort(state, &f.konto_ref)?)?;
                Ok(Self::Ftps { ftps, zeiten, kennung: kennung.to_path_buf() })
            }
            Some(Quelle::Sftp(e)) if Path::new(&sftp_kennung(&e)) == kennung => {
                let sftp = sftp_zugang(state, &e)?;
                Ok(Self::Sftp { sftp, kennung: kennung.to_path_buf() })
            }
            Some(Quelle::Smb(e)) if Path::new(&smb_kennung(&e)) == kennung => {
                let smb = Smb::new(&e, &hinterlegtes_passwort(state, &e.konto_ref)?);
                Ok(Self::Smb { smb, kennung: kennung.to_path_buf() })
            }
            _ => Err(geaendert()),
        }
    }

    pub(crate) fn ist_server(&self) -> bool {
        matches!(self, Self::Ftps { .. } | Self::Sftp { .. })
    }

    /// Ordner bzw. `ftps://…` – fürs Protokoll.
    pub(crate) fn kennung(&self) -> &Path {
        match self {
            Self::Lokal(p) => p,
            Self::Ftps { kennung, .. } | Self::Smb { kennung, .. } | Self::Sftp { kennung, .. } => kennung,
        }
    }

    pub(crate) fn ep(&self) -> Box<dyn QuellEndpunkt + '_> {
        match self {
            Self::Lokal(p) => Box::new(Lokal::new(p)),
            Self::Ftps { ftps, zeiten, .. } => Box::new(ftps.als_quelle(zeiten.clone())),
            Self::Smb { smb, .. } => Box::new(smb),
            Self::Sftp { sftp, .. } => Box::new(sftp.als_quelle()),
        }
    }
}

/// Ziel und Modus der Stufe, wie sie jetzt konfiguriert ist – das Ziel muss
/// dasselbe sein wie beim Vergleich.
pub(crate) fn ziel_der_stufe(state: &AppState, strecke: &str, stufe: &str, kennung: &Path) -> Result<(Endpunkt, Modus), StellwerkError> {
    let s = state.store.strecke_laden(strecke)?;
    s.stufen
        .into_iter()
        .find(|st| st.id == stufe)
        .map(|st| (st.ziel, st.modus))
        .filter(|(z, _)| ziel_kennung(z) == kennung)
        .ok_or_else(|| StellwerkError::NichtUnterstuetzt("Ziel der Stufe wurde seit dem Vergleich geändert – bitte neu vergleichen".into()))
}

/// Nach einer erledigten Operation am Server: den letzten bekannten Stand (Manifest)
/// fortschreiben – Datei mit Hash und Zeit der Quelle, neuer Ordner, Gelöschtes heraus.
pub(crate) fn manifest_fortschreiben(
    db: &Datenbank,
    ort: (&str, &str, &str),
    op: &Operation,
    uebertragen: Option<&Uebertragen>,
) -> Result<(), StellwerkError> {
    let (strecke, stufe, lauf_id) = ort;
    let eintrag = |ist_ordner, groesse, hash, mtime_ns| ManifestEintrag {
        pfad: op.pfad.clone(),
        ist_ordner,
        groesse,
        hash,
        mtime_ns,
        lauf_id: Some(lauf_id.to_string()),
    };
    match (op.art, uebertragen) {
        (OpArt::Kopieren | OpArt::Ueberschreiben, Some(u)) => {
            db.manifest_setzen(strecke, stufe, &eintrag(false, u.geschrieben.bytes, Some(u.geschrieben.hash), u.quelle_mtime_ns))
        }
        (OpArt::OrdnerAnlegen, _) => db.manifest_setzen(strecke, stufe, &eintrag(true, 0, None, None)),
        (OpArt::DateiLoeschen | OpArt::OrdnerLoeschen, _) => db.manifest_entfernen(strecke, stufe, &op.pfad),
        // Rechte stehen nicht im Manifest; erledigte Dateien kommen immer mit `uebertragen`
        _ => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn uebertragen(
    state: &AppState,
    plan: &Plan,
    strecke: &str,
    stufe: &str,
    quelle: &QuellEp,
    ziel: &Endpunkt,
    bei_fehler_stoppen: bool,
    abbruch: &AtomicBool,
    fortschritt: &Channel<ExecFortschritt>,
) -> Result<LaufAntwort, StellwerkError> {
    let db = &state.datenbank;
    let worker = worker_fuer(quelle.ist_server(), ziel);
    // Alles, was scheitern kann, bevor der Lauf angelegt wird
    let zep = ZielEp::neu(state, ziel, strecke, stufe)?;
    let (ziel_ep, server) = (zep.ep(), zep.ist_server());
    let ziel = ziel_kennung(ziel);
    let lauf_id = db.lock().unwrap().lauf_anlegen(strecke, stufe, Modus::Direkt, plan, jetzt_ms())?;
    let ort = (strecke, stufe, lauf_id.as_str());
    {
        let db = db.lock().unwrap();
        db.protokollieren(&eintrag(
            ort,
            ProtokollEbene::Info,
            "lauf_gestartet",
            None,
            json!({
                "modus": "direkt", "quelle": quelle.kennung(), "ziel": ziel, "worker": worker,
                "bei_fehler_stoppen": bei_fehler_stoppen, "plan": plan.zusammenfassung,
            }),
        ))?;
        db.lauf_phase(&lauf_id, LaufPhase::Laeuft, jetzt_ms(), None)?;
    }

    let opt = Optionen { worker, bei_fehler_stoppen, fortschritt_abstand: Duration::from_millis(100) };
    let ergebnis = direkt_ausfuehren(
        plan,
        quelle.ep().as_ref(),
        ziel_ep,
        &lauf_id,
        &opt,
        abbruch,
        &mut |op, status, meldung, uebertragen, hinweis| {
            let db = db.lock().unwrap();
            db.journal_setzen(&lauf_id, op.nr, status, meldung, hinweis, jetzt_ms())?;
            if server && status == JournalStatus::Erledigt {
                manifest_fortschreiben(&db, ort, op, uebertragen)?;
            }
            if status == JournalStatus::Fehler {
                let details = json!({ "op": op.art, "meldung": meldung });
                db.protokollieren(&eintrag(ort, ProtokollEbene::Fehler, "operation_fehler", Some(op.pfad.as_str()), details))?;
            }
            Ok(())
        },
        // Ein geschlossenes Fenster ist kein Grund, die Übertragung abzubrechen
        &mut |f| drop(fortschritt.send(f)),
    );

    let (phase, ebene) = if ergebnis.abgebrochen {
        (LaufPhase::Abgebrochen, ProtokollEbene::Warnung)
    } else if ergebnis.vollstaendig() {
        (LaufPhase::Fertig, ProtokollEbene::Info)
    } else {
        (LaufPhase::Fertig, ProtokollEbene::Fehler)
    };
    let zusammen = json!(ergebnis);
    let db = db.lock().unwrap();
    zep.gelerntes_speichern(&db, strecke, stufe)?;
    db.lauf_phase(&lauf_id, phase, jetzt_ms(), Some(&zusammen))?;
    db.protokollieren(&eintrag(ort, ebene, "lauf_beendet", None, zusammen))?;
    Ok(LaufAntwort { lauf_id, ergebnis, phase })
}

/// Protokolleintrag zu einem Lauf; `ort` = (Strecke, Stufe, Lauf).
pub(crate) fn eintrag<'a>(
    ort: (&'a str, &'a str, &'a str),
    ebene: ProtokollEbene,
    aktion: &'a str,
    pfad: Option<&'a str>,
    details: serde_json::Value,
) -> Protokollieren<'a> {
    Protokollieren {
        zeit: jetzt_ms(),
        ebene: Some(ebene),
        strecke_id: Some(ort.0),
        stufe_id: Some(ort.1),
        lauf_id: Some(ort.2),
        aktion,
        pfad,
        details: Some(details),
    }
}

/// Läufe einer Stufe, neueste zuerst (Protokoll in der Streckenansicht).
#[tauri::command]
pub fn laeufe(state: State<'_, AppState>, strecke: String, stufe: String, limit: usize) -> Antwort<Vec<Lauf>> {
    Ok(state.datenbank.lock().unwrap().laeufe(&strecke, &stufe, limit)?)
}

/// Jeder Schritt eines Laufs in Plan-Reihenfolge; unbekannter Lauf → leer.
#[tauri::command]
pub fn lauf_journal(state: State<'_, AppState>, lauf_id: String) -> Antwort<Vec<JournalZeile>> {
    Ok(state.datenbank.lock().unwrap().journal(&lauf_id)?)
}

/// Bricht die laufende Übertragung einer Stufe ab; `false`, wenn keine läuft.
#[tauri::command]
pub fn lauf_abbrechen(state: State<'_, AppState>, strecke: String, stufe: String) -> bool {
    state.laeufe.abbrechen(&format!("{strecke}/{stufe}"))
}
