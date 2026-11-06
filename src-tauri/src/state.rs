use stellwerk_core::config::ConfigStore;
use stellwerk_core::secrets::Tresor;

/// Anwendungszustand. Später kommen SQLite-Store, laufende Vergleiche und Läufe hinzu.
pub struct AppState {
    pub store: ConfigStore,
    pub tresor: Box<dyn Tresor>,
}

impl AppState {
    pub fn new(store: ConfigStore, tresor: Box<dyn Tresor>) -> Self {
        Self { store, tresor }
    }
}
