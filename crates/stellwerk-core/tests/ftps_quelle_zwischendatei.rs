//! FTPS-Quelle (v0.6.5): Lässt sich keine Zwischendatei anlegen, scheitert die Datei mit
//! einer Meldung – eigenes Testprogramm, weil `TMPDIR` für den ganzen Prozess gilt.

#![cfg(unix)]

use std::collections::HashMap;

use stellwerk_core::endpoint::{Ftps, QuellEndpunkt};
use stellwerk_core::model::{FtpsEndpunkt, RelPath};
use stellwerk_testserver::{starten, Aufbau, BENUTZER, PASSWORT};

#[test]
fn ohne_zwischendatei_scheitert_die_datei_mit_meldung() {
    let s = starten(Aufbau::default());
    std::fs::write(s.ordner("logs").join("a.log"), "a").unwrap();
    let ziel = FtpsEndpunkt {
        host: "127.0.0.1".into(),
        port: s.port,
        pfad: "/logs".into(),
        benutzer: BENUTZER.into(),
        konto_ref: "k".into(),
        tls: Default::default(),
        zertifikat: Some(s.fingerprint.clone()),
    };
    let ftps = Ftps::new(&ziel, PASSWORT).unwrap();
    // Einziger Test in diesem Programm – kein anderer Thread liest die Umgebung
    std::env::set_var("TMPDIR", "/gibt/es/nicht");
    let e = ftps.als_quelle(HashMap::new()).oeffnen(&RelPath::new("a.log").unwrap()).err().unwrap();
    assert!(e.to_string().contains("/logs/a.log"), "{e}");
}
