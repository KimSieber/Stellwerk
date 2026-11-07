use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use stellwerk_core::ansicht::VergleichsSitzung;
use stellwerk_core::config::ConfigStore;
use stellwerk_core::secrets::Tresor;

/// Anwendungszustand. Später kommen SQLite-Store und Läufe hinzu.
pub struct AppState {
    pub store: ConfigStore,
    pub tresor: Box<dyn Tresor>,
    pub vergleiche: Vergleiche,
}

impl AppState {
    pub fn new(store: ConfigStore, tresor: Box<dyn Tresor>) -> Self {
        Self { store, tresor, vergleiche: Vergleiche::default() }
    }
}

/// Laufende und geöffnete Vergleiche. Je Stufe läuft höchstens ein Vergleich
/// (Schlüssel `strecke/stufe`); ein neuer bricht den alten ab.
#[derive(Default)]
pub struct Vergleiche {
    zaehler: AtomicU64,
    laufend: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub sitzungen: Mutex<HashMap<String, VergleichsSitzung>>,
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
        self.laufend.lock().unwrap().get(stufe).map(|s| s.store(true, Ordering::Relaxed)).is_some()
    }

    /// Legt die Sitzung ab und ersetzt ältere Sitzungen derselben Stufe. Liefert die Kennung.
    pub fn ablegen(&self, stufe: &str, sitzung: VergleichsSitzung) -> String {
        let n = self.zaehler.fetch_add(1, Ordering::Relaxed) + 1;
        let praefix = format!("{stufe}#");
        let id = format!("{praefix}{n}");
        let mut sitzungen = self.sitzungen.lock().unwrap();
        sitzungen.retain(|k, _| !k.starts_with(&praefix));
        sitzungen.insert(id.clone(), sitzung);
        id
    }
}
