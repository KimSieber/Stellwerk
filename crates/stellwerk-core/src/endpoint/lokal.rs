//! Lokaler Endpunkt (LocalFs): ein Ordner auf einem Laufwerk.
//!
//! Als Quelle wird nur gelesen. Als Ziel gilt (Verhalten aus v0.3):
//! - Temporärdatei `.<name>.stw-<lauf>` mit `create_new`, auf den Datenträger
//!   gebracht (`sync_all`), Rechte und Änderungszeit gesetzt, dann umbenannt.
//! - Neue Dateien erhalten die Rechte der Quelle, überschriebene behalten ihre.
//! - Veränderte Lage seit dem Vergleich wird nicht übergangen (s. einzelne Methoden).

use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use filetime::FileTime;

use super::{kopieren, temp_name, QuellDatei, QuellEndpunkt, ZielEndpunkt, ZielVerbindung};
use crate::error::{Result, StellwerkError};
use crate::model::RelPath;

/// Ein lokaler Ordner als Endpunkt.
#[derive(Debug, Clone)]
pub struct Lokal {
    wurzel: PathBuf,
}

impl Lokal {
    pub fn new(wurzel: impl Into<PathBuf>) -> Self {
        Self { wurzel: wurzel.into() }
    }

    fn pfad(&self, p: &RelPath) -> PathBuf {
        self.wurzel.join(p.as_str())
    }
}

fn io(p: &Path) -> impl FnOnce(std::io::Error) -> StellwerkError + '_ {
    move |e| StellwerkError::io(p, e)
}

impl QuellEndpunkt for Lokal {
    fn oeffnen(&self, p: &RelPath) -> Result<QuellDatei> {
        let q = self.pfad(p);
        let ein = File::open(&q).map_err(io(&q))?;
        let meta = ein.metadata().map_err(io(&q))?;
        if !meta.is_file() {
            return Err(StellwerkError::NichtUnterstuetzt(format!("{} ist keine Datei", q.display())));
        }
        Ok(QuellDatei {
            inhalt: Box::new(ein),
            groesse: meta.len(),
            geaendert: meta.modified().map_err(io(&q))?,
            modus: modus_von(&meta),
        })
    }
}

#[cfg(unix)]
fn modus_von(m: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(m.permissions().mode())
}

impl ZielEndpunkt for Lokal {
    fn verbinden(&self) -> Result<Box<dyn ZielVerbindung + '_>> {
        Ok(Box::new(LokaleVerbindung { ziel: self }))
    }
}

struct LokaleVerbindung<'a> {
    ziel: &'a Lokal,
}

impl ZielVerbindung for LokaleVerbindung<'_> {
    fn ordner_anlegen(&mut self, p: &RelPath) -> Result<()> {
        let z = self.ziel.pfad(p);
        match fs::create_dir(&z) {
            Err(e) if e.kind() == ErrorKind::AlreadyExists && ist_echter_ordner(&z) => Ok(()),
            r => r.map_err(io(&z)),
        }
    }

    fn datei_schreiben(&mut self, p: &RelPath, q: QuellDatei, neu: bool, lauf_id: &str, abbruch: &AtomicBool) -> Result<u64> {
        let z = self.ziel.pfad(p);
        let bisher = fs::symlink_metadata(&z).ok();
        if neu && bisher.is_some() {
            return Err(StellwerkError::Vorhanden(format!("{} – am Ziel inzwischen vorhanden, bitte neu vergleichen", z.display())));
        }
        let name = z.file_name().expect("Zielpfad endet auf einen Namen").to_string_lossy();
        let tmp = z.with_file_name(temp_name(&name, lauf_id));
        let mut aus = OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(io(&tmp))?;
        let QuellDatei { mut inhalt, geaendert, modus, .. } = q;
        let quelle_ort = Path::new(p.as_str());

        let ergebnis = (|| {
            let bytes = kopieren(&mut inhalt, &mut aus, abbruch, quelle_ort, &tmp)?;
            aus.sync_all().map_err(io(&tmp))?;
            // Neu: Rechte der Quelle; überschrieben: bisherige Rechte am Ziel
            let bits = bisher.as_ref().filter(|m| m.is_file()).and_then(modus_von).or(modus);
            bits.map(|b| rechte(&tmp, b)).transpose()?;
            filetime::set_file_mtime(&tmp, FileTime::from_system_time(geaendert)).map_err(io(&tmp))?;
            fs::rename(&tmp, &z).map_err(io(&z))?;
            Ok(bytes)
        })();
        if ergebnis.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        ergebnis
    }

    fn rechte_setzen(&mut self, p: &RelPath, bits: u32) -> Result<()> {
        rechte(&self.ziel.pfad(p), bits)
    }

    fn datei_loeschen(&mut self, p: &RelPath) -> Result<()> {
        let z = self.ziel.pfad(p);
        match fs::symlink_metadata(&z) {
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Ok(m) if m.is_dir() => {
                Err(StellwerkError::NichtUnterstuetzt(format!("{} ist inzwischen ein Ordner – bitte neu vergleichen", z.display())))
            }
            _ => fs::remove_file(&z).map_err(io(&z)),
        }
    }

    fn ordner_loeschen(&mut self, p: &RelPath) -> Result<()> {
        let z = self.ziel.pfad(p);
        match fs::remove_dir(&z) {
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            r => r.map_err(io(&z)),
        }
    }
}

fn ist_echter_ordner(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}

#[cfg(unix)]
fn rechte(z: &Path, bits: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(z, fs::Permissions::from_mode(bits)).map_err(io(z))
}
