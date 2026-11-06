use stellwerk_core::config::ConfigStore;

/// Anwendungszustand. Später kommen SQLite-Store, laufende Vergleiche und Läufe hinzu.
pub struct AppState {
    pub store: ConfigStore,
}

impl AppState {
    pub fn new(store: ConfigStore) -> Self {
        Self { store }
    }
}
