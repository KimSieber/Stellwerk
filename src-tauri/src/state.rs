use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use stellwerk_core::ansicht::VergleichsSitzung;
use stellwerk_core::config::ConfigStore;
use stellwerk_core::plan::Plan;
use stellwerk_core::secrets::Tresor;
use stellwerk_core::store::Datenbank;

/// Anwendungszustand.
pub struct AppState {
    pub store: ConfigStore,
    pub tresor: Box<dyn Tresor>,
    pub vergleiche: Vergleiche,
    pub laeufe: Laeufe,
    /// Laufzeitdaten (Läufe, Journal, Protokoll) im Konfigurationsordner.
    pub datenbank: Mutex<Datenbank>,
}

impl AppState {
    /// Öffnet die Laufzeitdatenbank im Konfigurationsordner (legt sie beim ersten Mal an)
    /// und markiert dort offene Läufe als unterbrochen.
    pub fn new(store: ConfigStore, tresor: Box<dyn Tresor>) -> stellwerk_core::Result<Self> {
        let datenbank = Datenbank::im_ordner(store.wurzel())?;
        // v0.6.1: Wer mitten im Umschalten beendet hat, findet den Lauf als unterbrochen vor
        crate::umschalten::offene_aufgeben(&datenbank)?;
        let datenbank = Mutex::new(datenbank);
        Ok(Self { store, tresor, vergleiche: Vergleiche::default(), laeufe: Laeufe::default(), datenbank })
    }
}

/// Ein abgeschlossener Vergleich, wie ihn die Ansicht offen hält.
pub struct OffenerVergleich {
    pub sitzung: VergleichsSitzung,
    pub strecke: String,
    pub stufe: String,
    /// Aufgelöste Ordner zum Zeitpunkt des Vergleichs.
    pub quelle: PathBuf,
    pub ziel: PathBuf,
    /// Der zuletzt zur Bestätigung gezeigte Plan; jede Änderung der Auswahl verwirft ihn.
    pub plan: Option<Plan>,
}

/// Laufende und geöffnete Vergleiche. Je Stufe läuft höchstens ein Vergleich
/// (Schlüssel `strecke/stufe`); ein neuer bricht den alten ab.
#[derive(Default)]
pub struct Vergleiche {
    zaehler: AtomicU64,
    laufend: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub sitzungen: Mutex<HashMap<String, OffenerVergleich>>,
}

impl Vergleiche {
    /// Meldet einen Vergleich der Stufe an und liefert seinen Abbruch-Schalter.
    /// Ein noch laufender Vergleich derselben Stufe wird abgebrochen.
    pub fn anmelden(&self, stufe: &str) -> Arc<AtomicBool> {
        let neu = Arc::new(AtomicBool::new(false));
        if let Some(alt) = self.laufend.lock().unwrap().insert(stufe.to_string(), neu.clone()) {
            alt.store(true, Ordering::Relaxed);
        }
        neu
    }

    /// Meldet ab – aber nur, wenn inzwischen kein neuerer Vergleich angemeldet ist.
    pub fn abmelden(&self, stufe: &str, schalter: &Arc<AtomicBool>) {
        let mut laufend = self.laufend.lock().unwrap();
        if laufend.get(stufe).is_some_and(|s| Arc::ptr_eq(s, schalter)) {
            laufend.remove(stufe);
        }
    }

    /// Bricht den laufenden Vergleich der Stufe ab; `false`, wenn keiner läuft.
    pub fn abbrechen(&self, stufe: &str) -> bool {
        abbrechen(&self.laufend, stufe)
    }

    /// Legt den Vergleich ab und ersetzt ältere derselben Stufe. Liefert die Kennung.
    pub fn ablegen(&self, stufe: &str, offen: OffenerVergleich) -> String {
        let n = self.zaehler.fetch_add(1, Ordering::Relaxed) + 1;
        let praefix = format!("{stufe}#");
        let id = format!("{praefix}{n}");
        let mut sitzungen = self.sitzungen.lock().unwrap();
        sitzungen.retain(|k, _| !k.starts_with(&praefix));
        sitzungen.insert(id.clone(), offen);
        id
    }
}

/// Laufende Übertragungen. Anders als beim Vergleich bricht ein zweiter Start
/// den ersten **nicht** ab – er wird abgewiesen.
#[derive(Default)]
pub struct Laeufe {
    laufend: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl Laeufe {
    /// Meldet eine Übertragung der Stufe an; `None`, wenn dort schon eine läuft.
    pub fn anmelden(&self, stufe: &str) -> Option<Arc<AtomicBool>> {
        let mut laufend = self.laufend.lock().unwrap();
        if laufend.contains_key(stufe) {
            return None;
        }
        let neu = Arc::new(AtomicBool::new(false));
        laufend.insert(stufe.to_string(), neu.clone());
        Some(neu)
    }

    pub fn abmelden(&self, stufe: &str) {
        self.laufend.lock().unwrap().remove(stufe);
    }

    /// Bricht die laufende Übertragung der Stufe ab; `false`, wenn keine läuft.
    pub fn abbrechen(&self, stufe: &str) -> bool {
        abbrechen(&self.laufend, stufe)
    }
}

fn abbrechen(laufend: &Mutex<HashMap<String, Arc<AtomicBool>>>, stufe: &str) -> bool {
    laufend.lock().unwrap().get(stufe).map(|s| s.store(true, Ordering::Relaxed)).is_some()
}
