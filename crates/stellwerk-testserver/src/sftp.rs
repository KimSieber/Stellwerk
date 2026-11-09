//! SFTP-Server für die automatischen Tests (v0.7.2). Das eigentliche Programm liegt in
//! `werkzeuge/sftp-testserver` (eigenes Projekt, siehe dort); es wird beim ersten Start
//! einmal gebaut. Pfade auf dem Server sind die echten des Rechners – Tests verwenden den
//! Temp-Ordner [`SftpServer::wurzel`] als absoluten Serverpfad.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;

pub use crate::{BENUTZER, PASSWORT};

/// Ein laufender Server; endet mit dem Test.
pub struct SftpServer {
    pub port: u16,
    /// Fingerabdruck des Host-Schlüssels wie bei OpenSSH (`SHA256:…`).
    pub fingerprint: String,
    wurzel: tempfile::TempDir,
    prozess: Child,
}

impl Drop for SftpServer {
    fn drop(&mut self) {
        let _ = self.prozess.kill();
        let _ = self.prozess.wait();
    }
}

impl SftpServer {
    /// Temp-Ordner des Tests – auch der absolute Serverpfad.
    pub fn wurzel(&self) -> &Path {
        self.wurzel.path()
    }

    /// Ordner unterhalb der Wurzel anlegen; liefert den Pfad (zugleich der Serverpfad).
    pub fn ordner(&self, rel: &str) -> PathBuf {
        let p = self.wurzel.path().join(rel);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// Serverpfad zu einem Ordner unterhalb der Wurzel.
    pub fn pfad(&self, rel: &str) -> String {
        self.wurzel.path().join(rel).display().to_string()
    }
}

/// Baut das Programm einmal je Testlauf; liefert seinen Pfad. Ein schon gebautes kann
/// über `STELLWERK_SFTP_TESTSERVER` vorgegeben werden.
fn programm() -> &'static Path {
    static PROGRAMM: OnceLock<PathBuf> = OnceLock::new();
    PROGRAMM.get_or_init(|| {
        if let Some(p) = std::env::var_os("STELLWERK_SFTP_TESTSERVER") {
            return PathBuf::from(p);
        }
        let projekt = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../werkzeuge/sftp-testserver");
        let mut cargo = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        cargo.args(["build", "--quiet", "--manifest-path"]).arg(projekt.join("Cargo.toml"));
        // Nicht mit den Einstellungen des äußeren Laufs bauen (z. B. Abdeckungsmessung)
        for v in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "LLVM_PROFILE_FILE", "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR"] {
            cargo.env_remove(v);
        }
        cargo.env("CARGO_TARGET_DIR", projekt.join("target"));
        let status = cargo.status().expect("cargo starten");
        assert!(status.success(), "SFTP-Testserver ließ sich nicht bauen");
        projekt.join("target/debug/sftp-testserver")
    })
}

/// Passphrase des Testschlüssels `mit` (v0.7.2b).
pub const PASSPHRASE: &str = "geheim-pp";

/// Testschlüssel, einmal je Lauf mit `ssh-keygen` erzeugt (v0.7.2b): `ohne` (ohne
/// Passphrase), `mit` (Passphrase [`PASSPHRASE`]) – beide kennt der Server – und
/// `fremd`, den er nicht kennt. Liefert den Pfad des privaten Schlüssels.
pub fn schluessel(name: &str) -> PathBuf {
    schluessel_ordner().path().join(name)
}

fn schluessel_ordner() -> &'static tempfile::TempDir {
    static ORDNER: OnceLock<tempfile::TempDir> = OnceLock::new();
    ORDNER.get_or_init(|| {
        let d = tempfile::tempdir().unwrap();
        for (name, pp) in [("ohne", ""), ("mit", PASSPHRASE), ("fremd", "")] {
            let status = Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-C", name, "-N", pp, "-f"])
                .arg(d.path().join(name))
                .status()
                .expect("ssh-keygen starten");
            assert!(status.success(), "ssh-keygen");
        }
        let erlaubt: String = ["ohne", "mit"].iter().map(|n| std::fs::read_to_string(d.path().join(format!("{n}.pub"))).unwrap()).collect();
        std::fs::write(d.path().join("authorized_keys"), erlaubt).unwrap();
        d
    })
}

/// Startet einen Server auf einem freien Port.
pub fn starten() -> SftpServer {
    let mut prozess = Command::new(programm())
        .arg(schluessel_ordner().path().join("authorized_keys"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().expect("SFTP-Testserver starten");
    let mut zeilen = BufReader::new(prozess.stdout.take().unwrap()).lines();
    let mut wert = |name: &str| {
        let z = zeilen.next().expect("Ausgabe des SFTP-Testservers").unwrap();
        z.strip_prefix(&format!("{name}=")).unwrap_or_else(|| panic!("unerwartete Zeile: {z}")).to_string()
    };
    let port = wert("PORT").parse().unwrap();
    let fingerprint = wert("FINGERPRINT");
    SftpServer { port, fingerprint, wurzel: tempfile::tempdir().unwrap(), prozess }
}
