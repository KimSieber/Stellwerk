//! Diagnose für FTPS-Ziele (kein Teil der App, nicht in der Abdeckung):
//! lädt eine Stufe aus Kims Konfiguration, nimmt das Passwort aus dem
//! Schlüsselbund und schreibt genau wie Stellwerk eine kleine Testdatei
//! `stellwerk-diagnose.txt` ins Ziel – danach wird sie wieder gelöscht.
//! Jeder FTP-Befehl und jede Antwort erscheint mit Zeitstempel; die Zeile mit
//! dem Passwort wird unterdrückt.
//!
//! Aufruf (im Projektordner):
//!   cargo run -p stellwerk --example ftps_diagnose -- <strecke> <stufe>

use std::io::Cursor;
use std::sync::atomic::AtomicBool;
use std::time::{Instant, SystemTime};

use stellwerk_core::config::ConfigStore;
use stellwerk_core::endpoint::{Ftps, QuellDatei, ZielEndpunkt};
use stellwerk_core::model::{Endpunkt, RelPath};

struct Protokoll {
    start: Instant,
}

impl log::Log for Protokoll {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.target().starts_with("suppaftp") || (m.target().starts_with("rustls") && m.level() <= log::Level::Debug)
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let text = r.args().to_string();
        let text = if text.contains("PASS ") { "CC OUT: PASS ********".to_string() } else { text };
        println!("[{:>7.3} s] {:<5} {:<28} {text}", self.start.elapsed().as_secs_f64(), r.level(), r.target());
    }

    fn flush(&self) {}
}

fn schritt(start: Instant, text: &str) {
    println!("\n[{:>7.3} s] ==== {text}", start.elapsed().as_secs_f64());
}

fn passwort(konto: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        let e = keyring::Entry::new(stellwerk_core::secrets::DIENST, konto).expect("Schlüsselbund");
        e.get_password().expect("Kein Passwort im Schlüsselbund – bitte in Stellwerk im Editor hinterlegen")
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = konto;
        std::env::var("STELLWERK_PASSWORT").expect("STELLWERK_PASSWORT setzen (nur außerhalb von macOS)")
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, strecke, stufe] = args.as_slice() else {
        eprintln!("Aufruf: cargo run -p stellwerk --example ftps_diagnose -- <strecke> <stufe>");
        std::process::exit(2);
    };
    let start = Instant::now();
    log::set_boxed_logger(Box::new(Protokoll { start })).expect("Logger");
    log::set_max_level(log::LevelFilter::Trace);

    let wurzel = dirs_wurzel();
    let s = ConfigStore::new(&wurzel).strecke_laden(strecke).expect("Strecke laden");
    let st = s.stufen.iter().find(|x| &x.id == stufe).expect("Stufe nicht gefunden");
    let Endpunkt::Ftps(f) = &st.ziel else {
        eprintln!("Stufe „{stufe}“ hat kein FTPS-Ziel");
        std::process::exit(2);
    };
    println!("Ziel: ftps://{}:{}{} als {}", f.host, f.port, f.pfad, f.benutzer);

    let ftps = Ftps::new(f, &passwort(&f.konto_ref)).expect("FTPS einrichten");
    schritt(start, "Verbinden und anmelden");
    let mut v = match ftps.verbinden() {
        Ok(v) => v,
        Err(e) => return fehler(start, e),
    };

    let inhalt = b"Stellwerk-Diagnose\n".to_vec();
    let q = QuellDatei { groesse: inhalt.len() as u64, inhalt: Box::new(Cursor::new(inhalt)), geaendert: SystemTime::now(), modus: None };
    let pfad = RelPath::new("stellwerk-diagnose.txt").expect("Pfad");
    schritt(start, "Datei schreiben (Temporärdatei, Zeit setzen, umbenennen)");
    match v.datei_schreiben(&pfad, q, true, "diagnose", &AtomicBool::new(false)) {
        Ok(g) => println!("    → geschrieben: {} Bytes", g.bytes),
        Err(e) => return fehler(start, e),
    }
    schritt(start, "Testdatei wieder löschen");
    if let Err(e) = v.datei_loeschen(&pfad) {
        return fehler(start, e);
    }
    schritt(start, "FERTIG – alles in Ordnung");
}

fn fehler(start: Instant, e: stellwerk_core::StellwerkError) {
    schritt(start, &format!("FEHLER: {e}"));
    std::process::exit(1);
}

/// Konfigurationsordner wie in der App (macOS: Application Support).
fn dirs_wurzel() -> std::path::PathBuf {
    let home = std::env::var("HOME").expect("HOME");
    #[cfg(target_os = "macos")]
    return std::path::Path::new(&home).join("Library/Application Support/de.sieber-bw.stellwerk");
    #[cfg(not(target_os = "macos"))]
    return std::path::Path::new(&home).join(".local/share/de.sieber-bw.stellwerk");
}
