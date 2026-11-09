//! Passwörter (ARCHITEKTUR Kap. 11, KONZEPT E7).
//!
//! Passwörter liegen nur im Schlüsselbund des Systems – nie in der
//! Konfiguration, im Protokoll oder in Antworten an die Oberfläche. Die
//! Konfiguration enthält nur den internen Namen des Eintrags (`konto_ref`),
//! den Stellwerk selbst vergibt.
//!
//! [`Tresor`] trennt die Logik vom echten Schlüsselbund: Die App nutzt den
//! macOS-Schlüsselbund (in `src-tauri`), Tests den [`SpeicherTresor`].

use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::Result;
use crate::model::{Endpunkt, Quelle, Strecke};

/// Dienstname der Einträge im Schlüsselbund.
pub const DIENST: &str = "de.sieber-bw.stellwerk";

/// Ablage für Passwörter, adressiert über den internen Namen (`konto_ref`).
pub trait Tresor: Send + Sync {
    fn setzen(&self, konto: &str, passwort: &str) -> Result<()>;
    /// Passwort lesen – nur für die Verbindung, nie für die Oberfläche.
    fn lesen(&self, konto: &str) -> Result<Option<String>>;
    /// Entfernt den Eintrag; fehlt er, ist das kein Fehler.
    fn entfernen(&self, konto: &str) -> Result<()>;

    fn vorhanden(&self, konto: &str) -> Result<bool> {
        Ok(self.lesen(konto)?.is_some())
    }
}

/// Interner Name des Schlüsselbund-Eintrags einer Stufe.
pub fn konto_ref(strecke_id: &str, stufe_id: &str) -> String {
    format!("stellwerk:{strecke_id}:{stufe_id}")
}

/// Alle Schlüsselbund-Einträge, die eine Strecke verwendet.
pub fn konten_der_strecke(s: &Strecke) -> Vec<String> {
    s.stufen
        .iter()
        .flat_map(|st| {
            // v0.6.5: auch ein Server als Quelle hat ein eigenes Konto
            let quelle = match &st.quelle {
                Quelle::Ftps(f) => Some(f.konto_ref.clone()),
                Quelle::Smb(s) => Some(s.konto_ref.clone()),
                Quelle::Sftp(s) => Some(s.konto_ref.clone()),
                _ => None,
            };
            let ziel = match &st.ziel {
                Endpunkt::Ftps(f) => Some(f.konto_ref.clone()),
                Endpunkt::Smb(s) => Some(s.konto_ref.clone()),
                Endpunkt::Sftp(s) => Some(s.konto_ref.clone()),
                Endpunkt::Lokal { .. } => None,
            };
            quelle.into_iter().chain(ziel)
        })
        .collect()
}

/// Tresor im Arbeitsspeicher – für Tests und als Rückfall ohne Schlüsselbund.
#[derive(Debug, Default)]
pub struct SpeicherTresor {
    eintraege: Mutex<HashMap<String, String>>,
}

impl Tresor for SpeicherTresor {
    fn setzen(&self, konto: &str, passwort: &str) -> Result<()> {
        self.eintraege.lock().expect("Sperre").insert(konto.to_string(), passwort.to_string());
        Ok(())
    }
    fn lesen(&self, konto: &str) -> Result<Option<String>> {
        Ok(self.eintraege.lock().expect("Sperre").get(konto).cloned())
    }
    fn entfernen(&self, konto: &str) -> Result<()> {
        self.eintraege.lock().expect("Sperre").remove(konto);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speicher_tresor() {
        let t = SpeicherTresor::default();
        assert!(!t.vorhanden("a").unwrap());
        t.setzen("a", "geheim").unwrap();
        assert!(t.vorhanden("a").unwrap());
        assert_eq!(t.lesen("a").unwrap().as_deref(), Some("geheim"));
        t.entfernen("a").unwrap();
        t.entfernen("a").unwrap(); // fehlender Eintrag ist kein Fehler
        assert_eq!(t.lesen("a").unwrap(), None);
    }

    #[test]
    fn namen() {
        assert_eq!(konto_ref("mailwerk-web", "server"), "stellwerk:mailwerk-web:server");
        assert_eq!(DIENST, "de.sieber-bw.stellwerk");
    }
}
