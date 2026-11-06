//! macOS-Schlüsselbund als [`Tresor`].
//!
//! Wie `start.rs` von der Abdeckungsmessung ausgenommen: Diese Datei ist nur
//! die dünne Anbindung an das System und lässt sich nur auf dem Mac mit dem
//! echten Schlüsselbund prüfen. Die Logik dahinter (wann gespeichert, gelesen,
//! entfernt wird) ist über den `SpeicherTresor` in den Tests abgedeckt.

use keyring::{Entry, Error};
use stellwerk_core::secrets::{Tresor, DIENST};
use stellwerk_core::{Result, StellwerkError};

pub struct Schluesselbund;

fn eintrag(konto: &str) -> Result<Entry> {
    Entry::new(DIENST, konto).map_err(fehler)
}

fn fehler(e: Error) -> StellwerkError {
    StellwerkError::Schluesselbund(e.to_string())
}

impl Tresor for Schluesselbund {
    fn setzen(&self, konto: &str, passwort: &str) -> Result<()> {
        eintrag(konto)?.set_password(passwort).map_err(fehler)
    }

    fn lesen(&self, konto: &str) -> Result<Option<String>> {
        match eintrag(konto)?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(fehler(e)),
        }
    }

    fn entfernen(&self, konto: &str) -> Result<()> {
        match eintrag(konto)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(fehler(e)),
        }
    }
}
