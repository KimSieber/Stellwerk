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
//! Modul [`crate::scan`]; eine Quelle auf einem Server liest [`FtpsQuelle`] (v0.6.5).

pub mod ftps;
pub mod lokal;
pub mod smb;

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

pub use ftps::{Ftps, FtpsQuelle};
pub use lokal::Lokal;
pub use smb::Smb;

use crate::error::{Result, StellwerkError};
use crate::model::{Hash, RelPath};

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

/// Ergebnis einer geschriebenen Datei: übertragene Bytes und BLAKE3-Hash des Inhalts
/// (für das Manifest entfernter Ziele, E11) – beim Kopieren mitgerechnet, ohne zweites Lesen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geschrieben {
    pub bytes: u64,
    pub hash: Hash,
}

/// Quelle einer Stufe – nur lesend (A4).
pub trait QuellEndpunkt: Send + Sync {
    /// Öffnet eine Datei zum Lesen; Ordner, Links und Fehlendes sind Fehler.
    fn oeffnen(&self, p: &RelPath) -> Result<QuellDatei>;
}

/// Eine geliehene Quelle ist ebenfalls eine Quelle.
impl<T: QuellEndpunkt + ?Sized> QuellEndpunkt for &T {
    fn oeffnen(&self, p: &RelPath) -> Result<QuellDatei> {
        (**self).oeffnen(p)
    }
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
    /// noch nichts liegen. Liefert Bytes und Hash des Inhalts.
    fn datei_schreiben(&mut self, p: &RelPath, q: QuellDatei, neu: bool, lauf_id: &str, abbruch: &AtomicBool) -> Result<Geschrieben>;
    fn rechte_setzen(&mut self, p: &RelPath, bits: u32) -> Result<()>;
    /// Löscht eine Datei (nie einen Ordner). Schon weg ist in Ordnung.
    fn datei_loeschen(&mut self, p: &RelPath) -> Result<()>;
    /// Löscht einen Ordner nur, wenn er leer ist. Schon weg ist in Ordnung.
    fn ordner_loeschen(&mut self, p: &RelPath) -> Result<()>;
    /// Benennt eine Datei um („Umschalten“). `ersetzen`: Eine Datei unter `nach`
    /// wird ersetzt; sonst ist sie ein Fehler.
    fn umbenennen(&mut self, von: &RelPath, nach: &RelPath, ersetzen: bool) -> Result<()>;
    /// Liegt unter `p` eine Datei (kein Ordner)?
    fn ist_datei(&mut self, p: &RelPath) -> Result<bool>;
    /// Bemerkenswertes seit dem letzten Abruf, fürs Journal – z. B. dass die
    /// Datenverbindung wiederholt werden musste. Die Ausführung fragt nach jeder Operation.
    fn hinweis(&mut self) -> Option<String> {
        None
    }
}

/// Name der Temporärdatei im selben Ordner: `.<name>.stw-<lauf>`.
pub fn temp_name(name: &str, lauf_id: &str) -> String {
    format!(".{name}.stw-{lauf_id}")
}

/// Kopiert blockweise von `ein` nach `aus`, prüft vor jedem Block den Abbruch und
/// rechnet dabei den Hash mit. `ein_ort`/`aus_ort` benennen die Seiten in Fehlermeldungen.
pub fn kopieren(ein: &mut dyn Read, aus: &mut dyn Write, abbruch: &AtomicBool, ein_ort: &Path, aus_ort: &Path) -> Result<Geschrieben> {
    let mut puffer = vec![0u8; BLOCK];
    let mut bytes = 0u64;
    let mut h = blake3::Hasher::new();
    loop {
        if abbruch.load(Ordering::Relaxed) {
            return Err(StellwerkError::Abbruch);
        }
        let k = ein.read(&mut puffer).map_err(|e| StellwerkError::io(ein_ort, e))?;
        if k == 0 {
            return Ok(Geschrieben { bytes, hash: Hash(*h.finalize().as_bytes()) });
        }
        aus.write_all(&puffer[..k]).map_err(|e| StellwerkError::io(aus_ort, e))?;
        h.update(&puffer[..k]);
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
        let inhalt = vec![7u8; BLOCK + 3];
        let g = kopieren(&mut Cursor::new(inhalt.clone()), &mut aus, &nein, q, z).unwrap();
        assert_eq!(g, Geschrieben { bytes: BLOCK as u64 + 3, hash: Hash(*blake3::hash(&inhalt).as_bytes()) });
        assert_eq!(aus, inhalt);
        let e = kopieren(&mut Kaputt, &mut Vec::new(), &nein, q, z).unwrap_err();
        assert!(matches!(e, StellwerkError::Io { ref pfad, .. } if pfad == q), "{e}");
        let e = kopieren(&mut Cursor::new(vec![1u8]), &mut Kaputt, &nein, q, z).unwrap_err();
        assert!(matches!(e, StellwerkError::Io { ref pfad, .. } if pfad == z), "{e}");
        Kaputt.flush().unwrap();
        let ja = AtomicBool::new(true);
        assert!(matches!(kopieren(&mut Cursor::new(vec![1u8]), &mut aus, &ja, q, z), Err(StellwerkError::Abbruch)));
    }
}
