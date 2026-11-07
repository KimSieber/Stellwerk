//! Endpunkte (ARCHITEKTUR Kap. 5, Leitlinie A4).
//!
//! - **Quelle nur lesend:** [`QuellEndpunkt`] kann Dateien ausschließlich öffnen
//!   und lesen. Die Ausführung erhält die Quelle nur über diese Schnittstelle –
//!   auf die Quelle zu schreiben ist damit im Typsystem ausgeschlossen (E3).
//! - **Ziel schreibend:** [`ZielEndpunkt`] liefert je Worker eine eigene
//!   [`ZielVerbindung`] – lokal ein leichtes Objekt, bei FTPS eine eigene
//!   Server-Verbindung (eine Verbindung kann immer nur eine Datei übertragen).
//! - Endpunkte arbeiten **synchron**; parallel wird über den Worker-Pool (Threads).
//!
//! Abweichung vom Entwurf: kein `async_trait`, und der Scan bleibt vorerst im
//! Modul [`crate::scan`] (Quellen sind immer lokal).

pub mod ftps;
pub mod lokal;

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

pub use ftps::Ftps;
pub use lokal::Lokal;

use crate::error::{Result, StellwerkError};
use crate::model::RelPath;

/// Größe eines Kopierblocks; zwischen zwei Blöcken wird der Abbruch geprüft.
pub const BLOCK: usize = 1 << 20;

/// Eine geöffnete Quelldatei.
pub struct QuellDatei {
    pub inhalt: Box<dyn Read + Send>,
    pub groesse: u64,
    /// Änderungszeit der Quelle – wird am Ziel übernommen, wenn es das kann.
    pub geaendert: SystemTime,
    /// Unix-Rechte der Quelle (für neue Dateien in lokalen Zielen).
    pub modus: Option<u32>,
}

/// Quelle einer Stufe – nur lesend (A4).
pub trait QuellEndpunkt: Send + Sync {
    /// Öffnet eine Datei zum Lesen; Ordner, Links und Fehlendes sind Fehler.
    fn oeffnen(&self, p: &RelPath) -> Result<QuellDatei>;
}

/// Ziel einer Stufe – schreibend über je eine Verbindung pro Worker.
pub trait ZielEndpunkt: Send + Sync {
    fn verbinden(&self) -> Result<Box<dyn ZielVerbindung + '_>>;
}

/// Eine Verbindung zum Ziel. Alle Pfade relativ zur Wurzel der Stufe.
pub trait ZielVerbindung: Send {
    /// Legt einen Ordner an; ist er inzwischen da, ist das in Ordnung.
    fn ordner_anlegen(&mut self, p: &RelPath) -> Result<()>;
    /// Schreibt die Quelldatei nach `p` – nie als halbe Datei: erst als
    /// Temporärdatei ([`temp_name`]), dann umbenannt. `neu`: Am Ziel darf
    /// noch nichts liegen. Liefert die übertragenen Bytes.
    fn datei_schreiben(&mut self, p: &RelPath, q: QuellDatei, neu: bool, lauf_id: &str, abbruch: &AtomicBool) -> Result<u64>;
    fn rechte_setzen(&mut self, p: &RelPath, bits: u32) -> Result<()>;
    /// Löscht eine Datei (nie einen Ordner). Schon weg ist in Ordnung.
    fn datei_loeschen(&mut self, p: &RelPath) -> Result<()>;
    /// Löscht einen Ordner nur, wenn er leer ist. Schon weg ist in Ordnung.
    fn ordner_loeschen(&mut self, p: &RelPath) -> Result<()>;
}

/// Name der Temporärdatei im selben Ordner: `.<name>.stw-<lauf>`.
pub fn temp_name(name: &str, lauf_id: &str) -> String {
    format!(".{name}.stw-{lauf_id}")
}

/// Kopiert blockweise von `ein` nach `aus` und prüft vor jedem Block den Abbruch.
/// `ein_ort`/`aus_ort` benennen die Seiten in Fehlermeldungen.
pub fn kopieren(ein: &mut dyn Read, aus: &mut dyn Write, abbruch: &AtomicBool, ein_ort: &Path, aus_ort: &Path) -> Result<u64> {
    let mut puffer = vec![0u8; BLOCK];
    let mut bytes = 0u64;
    loop {
        if abbruch.load(Ordering::Relaxed) {
            return Err(StellwerkError::Abbruch);
        }
        let k = ein.read(&mut puffer).map_err(|e| StellwerkError::io(ein_ort, e))?;
        if k == 0 {
            return Ok(bytes);
        }
        aus.write_all(&puffer[..k]).map_err(|e| StellwerkError::io(aus_ort, e))?;
        bytes += k as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    #[test]
    fn temp_name_im_selben_ordner() {
        assert_eq!(temp_name("index.php", "mw5ha39c"), ".index.php.stw-mw5ha39c");
    }

    struct Kaputt;
    impl Read for Kaputt {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("Lesefehler"))
        }
    }
    impl Write for Kaputt {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("Schreibfehler"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn kopieren_benennt_die_fehlerseite() {
        let nein = AtomicBool::new(false);
        let (q, z) = (Path::new("/q"), Path::new("/z"));
        let mut aus = Vec::new();
        assert_eq!(kopieren(&mut Cursor::new(vec![7u8; BLOCK + 3]), &mut aus, &nein, q, z).unwrap(), BLOCK as u64 + 3);
        assert_eq!(aus.len(), BLOCK + 3);
        let e = kopieren(&mut Kaputt, &mut Vec::new(), &nein, q, z).unwrap_err();
        assert!(matches!(e, StellwerkError::Io { ref pfad, .. } if pfad == q), "{e}");
        let e = kopieren(&mut Cursor::new(vec![1u8]), &mut Kaputt, &nein, q, z).unwrap_err();
        assert!(matches!(e, StellwerkError::Io { ref pfad, .. } if pfad == z), "{e}");
        Kaputt.flush().unwrap();
        let ja = AtomicBool::new(true);
        assert!(matches!(kopieren(&mut Cursor::new(vec![1u8]), &mut aus, &ja, q, z), Err(StellwerkError::Abbruch)));
    }
}
