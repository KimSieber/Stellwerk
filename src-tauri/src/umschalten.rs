//! Modus „Umschalten“ in der Tauri-Hülle (v0.5): Lauf anlegen, Phasen ausführen,
//! Journal, Protokoll und letzten bekannten Stand (Manifest) pflegen.
//!
//! - **automatisch:** Hochladen → Umschalten → Übernehmen in einem Zug.
//! - **manuell:** nach dem Hochladen wartet der Lauf (`hochgeladen`); die weiteren
//!   Schritte kommen über [`lauf_weiter`].
//! - **Fehler:** Beim Hochladen wird nie umgeschaltet – der Lauf bleibt `hochgeladen`
//!   und bietet „Abbrechen“. Beim Umschalten stoppt er (`umgeschaltet`) und bietet
//!   „Rollback“. Ein Abbruch durch den Nutzer beim Hochladen räumt sofort auf.
//! - Solange ein Lauf `hochgeladen` oder `umgeschaltet` ist, ist die Stufe gesperrt
//!   (kein Vergleich, keine Übertragung).
//! - **Manifest (FTPS):** wird beim Umschalten fortgeschrieben (vorher gesichert),
//!   beim Rollback aus der Sicherung zurückgesetzt, beim Übernehmen bereinigt.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::ipc::Channel;
use tauri::State;

use stellwerk_core::ansicht::ziel_kennung;
use stellwerk_core::endpoint::Lokal;
use stellwerk_core::exec::umschalten::{self as phasen, plan_aus_journal, PhasenErgebnis};
use stellwerk_core::exec::{ExecFortschritt, Optionen};
use stellwerk_core::model::{Endpunkt, Modus};
use stellwerk_core::plan::{OpArt, Plan};
use stellwerk_core::store::{jetzt_ms, JournalStatus, JournalZeile, Lauf, LaufPhase, ManifestEintrag, ProtokollEbene};
use stellwerk_core::StellwerkError;

use crate::commands::{eintrag, manifest_fortschreiben, worker_fuer, Antwort, LaufAntwort, ZielEp};
use crate::state::AppState;

/// Nächster Schritt eines wartenden Laufs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Schritt {
    Umschalten,
    Uebernehmen,
    Rollback,
    Abbrechen,
}

/// Ein fehlgeschlagener Schritt einer Phase.
#[derive(Debug, Clone, Serialize)]
pub struct SchrittFehler {
    pub pfad: String,
    pub meldung: String,
}

/// Stand eines Laufs nach einem Schritt.
#[derive(Debug, Serialize)]
pub struct LaufStand {
    pub lauf_id: String,
    pub phase: LaufPhase,
    pub fehler: Vec<SchrittFehler>,
}

type Ort<'a> = (&'a str, &'a str);

/// Startet einen Lauf mit „Umschalten“: Phase 1, bei „automatisch“ und ohne Fehler
/// gleich Umschalten und Übernehmen.
#[allow(clippy::too_many_arguments)]
pub(crate) fn starten(
    state: &AppState,
    plan: &Plan,
    (strecke, stufe): Ort,
    modus: Modus,
    quelle: &Path,
    ziel: &Endpunkt,
    bei_fehler_stoppen: bool,
    abbruch: &AtomicBool,
    fortschritt: &Channel<ExecFortschritt>,
) -> Result<LaufAntwort, StellwerkError> {
    let db = &state.datenbank;
    let worker = worker_fuer(ziel);
    let zep = ZielEp::neu(state, ziel, strecke, stufe)?;
    let kennung = ziel_kennung(ziel);
    let lauf_id = db.lock().unwrap().lauf_anlegen(strecke, stufe, modus, plan, jetzt_ms())?;
    let ort = (strecke, stufe, lauf_id.as_str());
    {
        let db = db.lock().unwrap();
        let details = json!({
            "modus": modus, "quelle": quelle, "ziel": kennung, "worker": worker,
            "bei_fehler_stoppen": bei_fehler_stoppen, "plan": plan.zusammenfassung,
        });
        db.protokollieren(&eintrag(ort, ProtokollEbene::Info, "lauf_gestartet", None, details))?;
        db.lauf_phase(&lauf_id, LaufPhase::Laeuft, jetzt_ms(), None)?;
    }

    let opt = Optionen { worker, bei_fehler_stoppen, fortschritt_abstand: Duration::from_millis(100) };
    let ergebnis = phasen::hochladen(
        plan,
        &Lokal::new(quelle),
        zep.ep(),
        &lauf_id,
        &opt,
        abbruch,
        &mut |op, status, meldung, uebertragen, hinweis| {
            let db = db.lock().unwrap();
            let status = if status == JournalStatus::Erledigt { JournalStatus::Hochgeladen } else { status };
            db.journal_setzen(&lauf_id, op.nr, status, meldung, hinweis, jetzt_ms())?;
            if let Some(u) = uebertragen {
                db.journal_uebertragen(&lauf_id, op.nr, u.geschrieben.hash, u.quelle_mtime_ns)?;
            }
            if status == JournalStatus::Fehler {
                let details = json!({ "op": op.art, "meldung": meldung });
                db.protokollieren(&eintrag(ort, ProtokollEbene::Fehler, "operation_fehler", Some(op.pfad.as_str()), details))?;
            }
            Ok(())
        },
        &mut |f| drop(fortschritt.send(f)),
    );
    let mut stand = json!(ergebnis);
    stand["ziel"] = json!(kennung);
    {
        let db = db.lock().unwrap();
        zep.gelerntes_speichern(&db, strecke, stufe)?;
        db.lauf_phase(&lauf_id, LaufPhase::Hochgeladen, jetzt_ms(), Some(&stand))?;
        let ebene = if ergebnis.vollstaendig() { ProtokollEbene::Info } else { ProtokollEbene::Fehler };
        db.protokollieren(&eintrag(ort, ebene, "hochgeladen", None, json!(ergebnis)))?;
    }

    let lauf = db.lock().unwrap().lauf(&lauf_id)?.expect("eben angelegt");
    let phase = if ergebnis.abgebrochen {
        // Abbruch durch den Nutzer: sofort aufräumen – die Live-Seite war nie berührt
        schritt_ausfuehren(state, &lauf, Schritt::Abbrechen, &zep)?.0
    } else if !ergebnis.vollstaendig() || modus == Modus::UmschaltenManuell {
        LaufPhase::Hochgeladen
    } else {
        let (phase, erg) = schritt_ausfuehren(state, &lauf, Schritt::Umschalten, &zep)?;
        if erg.vollstaendig() {
            let lauf = db.lock().unwrap().lauf(&lauf_id)?.expect("Lauf besteht");
            schritt_ausfuehren(state, &lauf, Schritt::Uebernehmen, &zep)?.0
        } else {
            phase
        }
    };
    Ok(LaufAntwort { lauf_id, ergebnis, phase })
}

/// Nächster Schritt eines wartenden Laufs („Umschalten manuell“ oder nach einem
/// Fehler): Umschalten, Übernehmen, Rollback oder Abbrechen. Das Ziel der Stufe
/// muss noch dasselbe sein wie beim Hochladen.
#[tauri::command]
pub async fn lauf_weiter(state: State<'_, AppState>, lauf_id: String, schritt: Schritt) -> Antwort<LaufStand> {
    let gefunden = state.datenbank.lock().unwrap().lauf(&lauf_id)?;
    let lauf = gefunden.ok_or_else(|| StellwerkError::NichtGefunden(format!("Lauf „{lauf_id}“")))?;
    let kennung = lauf.ergebnis.as_ref().and_then(|e| e["ziel"].as_str()).unwrap_or_default().to_string();
    let (ziel, _) = crate::commands::ziel_der_stufe(&state, &lauf.strecke_id, &lauf.stufe_id, Path::new(&kennung))
        .map_err(|_| StellwerkError::NichtUnterstuetzt("Das Ziel der Stufe wurde seit dem Hochladen geändert – bitte die Angaben zurücksetzen".into()))?;
    let zep = ZielEp::neu(&state, &ziel, &lauf.strecke_id, &lauf.stufe_id)?;
    let schluessel = format!("{}/{}", lauf.strecke_id, lauf.stufe_id);
    state
        .laeufe
        .anmelden(&schluessel)
        .ok_or_else(|| StellwerkError::Vorhanden(format!("Übertragung für „{schluessel}“ läuft bereits")))?;
    let r = schritt_ausfuehren(&state, &lauf, schritt, &zep);
    state.laeufe.abmelden(&schluessel);
    let (phase, erg) = r?;
    let fehler = erg.fehler.into_iter().map(|(_, pfad, meldung)| SchrittFehler { pfad: pfad.to_string(), meldung }).collect();
    Ok(LaufStand { lauf_id, phase, fehler })
}

/// Welcher Schritt in welcher Phase erlaubt ist, und ob das Journal ihn zulässt.
fn erlaubt(lauf: &Lauf, schritt: Schritt, journal: &[JournalZeile]) -> Result<(), StellwerkError> {
    let alle = |arten: &[OpArt], st: JournalStatus| journal.iter().filter(|z| arten.contains(&z.op)).all(|z| z.status == st);
    let ok = match (lauf.phase, schritt) {
        (LaufPhase::Hochgeladen, Schritt::Umschalten) => {
            alle(&[OpArt::OrdnerAnlegen, OpArt::Kopieren, OpArt::Ueberschreiben, OpArt::RechteSetzen], JournalStatus::Hochgeladen)
        }
        (LaufPhase::Umgeschaltet, Schritt::Uebernehmen) => {
            alle(&[OpArt::Kopieren, OpArt::Ueberschreiben, OpArt::DateiLoeschen], JournalStatus::Umgeschaltet)
        }
        (LaufPhase::Hochgeladen, Schritt::Abbrechen) | (LaufPhase::Umgeschaltet, Schritt::Rollback) => true,
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(StellwerkError::NichtUnterstuetzt(format!("„{}“ ist in diesem Stand des Laufs nicht möglich", schritt_text(schritt))))
    }
}

fn schritt_text(s: Schritt) -> &'static str {
    match s {
        Schritt::Umschalten => "Umschalten",
        Schritt::Uebernehmen => "Übernehmen",
        Schritt::Rollback => "Rollback",
        Schritt::Abbrechen => "Abbrechen",
    }
}

/// Führt einen Schritt aus: Journal, Manifest, Phase und Protokoll.
fn schritt_ausfuehren(state: &AppState, lauf: &Lauf, schritt: Schritt, zep: &ZielEp) -> Result<(LaufPhase, PhasenErgebnis), StellwerkError> {
    let db = &state.datenbank;
    let (strecke, stufe, id) = (lauf.strecke_id.as_str(), lauf.stufe_id.as_str(), lauf.id.as_str());
    let ort = (strecke, stufe, id);
    let journal = db.lock().unwrap().journal(id)?;
    erlaubt(lauf, schritt, &journal)?;
    let plan = plan_aus_journal(&journal);
    let zeilen: HashMap<usize, &JournalZeile> = journal.iter().map(|z| (z.nr, z)).collect();
    let ftps = zep.ftps().is_some();
    if ftps && schritt == Schritt::Umschalten {
        db.lock().unwrap().manifest_sichern(strecke, stufe, id)?;
    }
    let ok_status = match schritt {
        Schritt::Umschalten => JournalStatus::Umgeschaltet,
        Schritt::Uebernehmen => JournalStatus::Erledigt,
        Schritt::Rollback | Schritt::Abbrechen => JournalStatus::Zurueckgerollt,
    };
    let f = match schritt {
        Schritt::Umschalten => phasen::umschalten,
        Schritt::Uebernehmen => phasen::uebernehmen,
        Schritt::Rollback => phasen::zurueckrollen,
        Schritt::Abbrechen => phasen::abbrechen,
    };
    let erg = f(&plan, zep.ep(), id, &mut |op, status, meldung| {
        let db = db.lock().unwrap();
        let status = if status == JournalStatus::Erledigt { ok_status } else { status };
        db.journal_setzen(id, op.nr, status, meldung, zeilen[&op.nr].hinweis.as_deref(), jetzt_ms())?;
        if status == JournalStatus::Fehler {
            let details = json!({ "op": op.art, "schritt": schritt, "meldung": meldung });
            db.protokollieren(&eintrag(ort, ProtokollEbene::Fehler, "operation_fehler", Some(op.pfad.as_str()), details))?;
        }
        if ftps && status == JournalStatus::Umgeschaltet {
            manifest_umgeschaltet(&db, ort, zeilen[&op.nr])?;
        }
        if ftps && schritt == Schritt::Uebernehmen && op.art == OpArt::OrdnerLoeschen && status == JournalStatus::Erledigt {
            manifest_fortschreiben(&db, ort, op, None)?;
        }
        Ok(())
    });
    let erg = erg?;

    let mut db = db.lock().unwrap();
    let phase = match schritt {
        Schritt::Umschalten => {
            // Neue Ordner gibt es seit dem Hochladen – jetzt gehören sie zum bekannten Stand
            for z in journal.iter().filter(|z| z.op == OpArt::OrdnerAnlegen && ftps) {
                manifest_umgeschaltet(&db, ort, z)?;
            }
            LaufPhase::Umgeschaltet
        }
        Schritt::Uebernehmen => {
            // Was nur hochgeladen oder umgeschaltet war, ist damit erledigt
            for z in db.journal(id)?.iter().filter(|z| matches!(z.status, JournalStatus::Hochgeladen | JournalStatus::Umgeschaltet)) {
                db.journal_setzen(id, z.nr, JournalStatus::Erledigt, None, z.hinweis.as_deref(), jetzt_ms())?;
            }
            db.manifest_sicherung_verwerfen(id)?;
            LaufPhase::Fertig
        }
        Schritt::Rollback => {
            if ftps {
                db.manifest_zuruecksetzen(strecke, stufe, id)?;
            }
            LaufPhase::Zurueckgerollt
        }
        Schritt::Abbrechen => LaufPhase::Abgebrochen,
    };
    // Das Ergebnis steht seit dem Hochladen im Lauf; ein leeres (null) würde beim ersten Schlüssel zum Objekt
    let mut stand = lauf.ergebnis.clone().unwrap_or_default();
    stand[match schritt {
        Schritt::Umschalten => "umschalten",
        Schritt::Uebernehmen => "uebernehmen",
        Schritt::Rollback => "rollback",
        Schritt::Abbrechen => "abbrechen",
    }] = phase_json(&erg);
    zep.gelerntes_speichern(&db, strecke, stufe)?;
    db.lauf_phase(id, phase, jetzt_ms(), Some(&stand))?;
    let ebene = if erg.vollstaendig() { ProtokollEbene::Info } else { ProtokollEbene::Fehler };
    db.protokollieren(&eintrag(ort, ebene, schritt_aktion(schritt), None, phase_json(&erg)))?;
    Ok((phase, erg))
}

fn schritt_aktion(s: Schritt) -> &'static str {
    match s {
        Schritt::Umschalten => "umgeschaltet",
        Schritt::Uebernehmen => "uebernommen",
        Schritt::Rollback => "zurueckgerollt",
        Schritt::Abbrechen => "abgebrochen",
    }
}

fn phase_json(e: &PhasenErgebnis) -> Value {
    json!({
        "erledigt": e.erledigt.len(),
        "fehler": e.fehler.iter().map(|(nr, p, m)| json!({ "nr": nr, "pfad": p, "meldung": m })).collect::<Vec<_>>(),
        "gestoppt": e.gestoppt,
    })
}

/// Nach dem Umschalten einer Operation: Datei mit Hash und Zeit aus dem Hochladen,
/// neuer Ordner, gelöschte Datei heraus.
fn manifest_umgeschaltet(db: &stellwerk_core::store::Datenbank, ort: (&str, &str, &str), z: &JournalZeile) -> Result<(), StellwerkError> {
    let (strecke, stufe, lauf_id) = ort;
    match z.op {
        OpArt::Kopieren | OpArt::Ueberschreiben => db.manifest_setzen(
            strecke,
            stufe,
            &ManifestEintrag { pfad: z.pfad.clone(), ist_ordner: false, groesse: z.groesse, hash: z.hash, mtime_ns: z.mtime_ns, lauf_id: Some(lauf_id.into()) },
        ),
        OpArt::OrdnerAnlegen => db.manifest_setzen(
            strecke,
            stufe,
            &ManifestEintrag { pfad: z.pfad.clone(), ist_ordner: true, groesse: 0, hash: None, mtime_ns: None, lauf_id: Some(lauf_id.into()) },
        ),
        // Umgeschaltet werden sonst nur zu löschende Dateien
        _ => db.manifest_entfernen(strecke, stufe, &z.pfad),
    }
}
