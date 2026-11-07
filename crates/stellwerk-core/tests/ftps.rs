//! FTPS-Endpunkt gegen einen echten FTPS-Server im Test (libunftp, Übergabe v0.4 Kap. 7):
//! „Verbindung testen“, Zertifikatsprüfung, Anmeldung, Fähigkeiten.

use std::net::TcpListener;
use std::time::Duration;

use stellwerk_core::endpoint::ftps::{Faehigkeiten, Ftps};
use stellwerk_core::model::{FtpsEndpunkt, Tls};
use stellwerk_core::StellwerkError;
use stellwerk_testserver::{starten, Aufbau, TestServer, BENUTZER, PASSWORT};

fn ziel(port: u16, pfad: &str) -> FtpsEndpunkt {
    FtpsEndpunkt {
        host: "127.0.0.1".into(),
        port,
        pfad: pfad.into(),
        benutzer: BENUTZER.into(),
        tls: Tls::Explizit,
        konto_ref: "stellwerk:test:server".into(),
        zertifikat: None,
    }
}

/// Server mit Zertifikat einer Test-CA, die der Prüfung als Wurzel bekannt ist.
fn mit_ca(a: Aufbau) -> (TestServer, impl Fn(&FtpsEndpunkt, &str) -> stellwerk_core::Result<Ftps>) {
    let s = starten(Aufbau { mit_ca: true, ..a });
    let ca = s.ca_der.clone().unwrap();
    (s, move |z: &FtpsEndpunkt, pw: &str| Ftps::mit_wurzeln(z, pw, vec![ca.clone()]))
}

#[test]
fn verbindung_testen_mit_anerkanntem_zertifikat_tls12_und_tls13() {
    for tls13 in [false, true] {
        let (s, ftps) = mit_ca(Aufbau { site_help: Some((214, "CHMOD UMASK HELP")), tls13, ..Default::default() });
        let docs = s.ordner("httpdocs");
        std::fs::write(docs.join("index.php"), "x").unwrap();
        std::fs::create_dir(docs.join("css")).unwrap();

        let t = ftps(&ziel(s.port, "/httpdocs"), PASSWORT).unwrap().verbindung_testen().unwrap();
        assert_eq!(t.begruessung.as_deref(), Some("Stellwerk-Testserver"));
        assert_eq!(t.verzeichnis, "/httpdocs");
        assert_eq!(t.eintraege, 2, "Datenkanal (TLS) funktioniert");
        assert_eq!(
            t.faehigkeiten,
            Faehigkeiten { mlsd: true, mfmt: false, chmod: Some(true), fortsetzen: true, utf8: true }
        );
        assert!(t.zertifikat.vom_system, "TLS 1.3: {tls13}");
        assert_eq!(t.zertifikat.fingerprint, s.fingerprint);
    }
}

#[test]
fn site_help_ohne_chmod_oder_unbekannt() {
    for (help, erwartet) in [(Some((200, "UMASK HELP")), Some(false)), (None, None)] {
        let (s, ftps) = mit_ca(Aufbau { site_help: help, ..Default::default() });
        let t = ftps(&ziel(s.port, "/"), PASSWORT).unwrap().verbindung_testen().unwrap();
        assert_eq!(t.faehigkeiten.chmod, erwartet, "{help:?}");
        assert_eq!(t.eintraege, 0);
    }
}

#[test]
fn selbstsigniert_ohne_bestaetigung_meldet_den_fingerprint() {
    let s = starten(Aufbau::default());
    let e = Ftps::new(&ziel(s.port, "/"), PASSWORT).unwrap().verbindung_testen().unwrap_err();
    assert_eq!(e.kategorie(), "zertifikat", "{e}");
    match e {
        StellwerkError::Zertifikat { fingerprint, meldung } => {
            assert_eq!(fingerprint, s.fingerprint);
            assert!(!meldung.is_empty());
        }
        e => panic!("{e}"),
    }
}

#[test]
fn bestaetigter_fingerprint_laesst_genau_dieses_zertifikat_zu() {
    let s = starten(Aufbau::default());
    // Schreibweise egal (klein, ohne Doppelpunkte)
    let bestaetigt = s.fingerprint.to_lowercase().replace(':', "");
    let z = FtpsEndpunkt { zertifikat: Some(bestaetigt), ..ziel(s.port, "/") };
    let t = Ftps::new(&z, PASSWORT).unwrap().verbindung_testen().unwrap();
    assert!(!t.zertifikat.vom_system);
    assert_eq!(t.zertifikat.fingerprint, s.fingerprint);

    // Anderer Fingerprint: abgelehnt
    let z = FtpsEndpunkt { zertifikat: Some("00".repeat(32)), ..ziel(s.port, "/") };
    assert_eq!(Ftps::new(&z, PASSWORT).unwrap().verbindung_testen().unwrap_err().kategorie(), "zertifikat");
}

#[test]
fn falsches_passwort_oder_fehlender_serverpfad() {
    let (s, ftps) = mit_ca(Aufbau::default());
    let e = ftps(&ziel(s.port, "/"), "falsch").unwrap().verbindung_testen().unwrap_err();
    assert_eq!(e.kategorie(), "anmeldung", "{e}");
    assert!(!e.to_string().contains("falsch"), "Passwort erscheint nie in Meldungen");

    let e = ftps(&ziel(s.port, "/gibt-es-nicht"), PASSWORT).unwrap().verbindung_testen().unwrap_err();
    assert_eq!(e.kategorie(), "nicht_gefunden", "{e}");
    assert!(e.to_string().contains("/gibt-es-nicht"));
}

#[test]
fn server_ohne_tls_wird_abgewiesen() {
    let s = starten(Aufbau { ohne_tls: true, ..Default::default() });
    let e = Ftps::new(&ziel(s.port, "/"), PASSWORT).unwrap().verbindung_testen().unwrap_err();
    assert_eq!(e.kategorie(), "verbindung");
    assert!(e.to_string().contains("bietet kein TLS an"), "{e}");
}

#[test]
fn nicht_erreichbar_unbekannter_name_und_zeitlimit() {
    // Port ohne Server
    let frei = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let e = Ftps::new(&ziel(frei, "/"), PASSWORT).unwrap().verbindung_testen().unwrap_err();
    assert!(e.to_string().contains("nicht erreichbar"), "{e}");

    let z = FtpsEndpunkt { host: "gibt-es-nicht.invalid".into(), ..ziel(21, "/") };
    let e = Ftps::new(&z, PASSWORT).unwrap().verbindung_testen().unwrap_err();
    assert!(e.to_string().contains("nicht auflösbar"), "{e}");

    // Server nimmt an, antwortet aber nie
    let stumm = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = stumm.local_addr().unwrap().port();
    let ftps = Ftps::new(&ziel(port, "/"), PASSWORT).unwrap().zeitlimit(Duration::from_millis(200));
    let e = ftps.verbindung_testen().unwrap_err();
    assert_eq!(e.kategorie(), "verbindung", "{e}");
    assert!(e.to_string().contains("keine Begrüßung"), "{e}");
    drop(stumm);
}

/// Ein Gegenüber, das nur `antwort` sendet (und dann wartet bzw. auflegt).
fn gegenueber(antwort: &'static [u8]) -> (u16, std::thread::JoinHandle<()>) {
    use std::io::Write;
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let t = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.write_all(antwort).unwrap();
        std::thread::sleep(Duration::from_millis(300));
    });
    (port, t)
}

#[test]
fn kein_ftp_server_am_port_wird_verstaendlich_gemeldet() {
    // SFTP (SSH) – so meldet sich manitu auf Port 23
    let (port, t) = gegenueber(b"SSH-2.0-mod_sftp\r\n");
    let e = Ftps::new(&ziel(port, "/"), PASSWORT).unwrap().verbindung_testen().unwrap_err();
    t.join().unwrap();
    assert!(e.to_string().contains("SFTP-Server (SSH), kein FTP-Server. Für FTPS ist meist Port 21"), "{e}");

    // Etwas ganz anderes
    let (port, t) = gegenueber(b"HTTP/1.1 400 Bad Request\r\n\r\n");
    let e = Ftps::new(&ziel(port, "/"), PASSWORT).unwrap().verbindung_testen().unwrap_err();
    t.join().unwrap();
    assert!(e.to_string().contains("kein FTP-Server (Antwort beginnt mit „HTTP/1.1 400 Bad Request“)"), "{e}");

    // Legt sofort auf
    let (port, t) = gegenueber(b"");
    let e = Ftps::new(&ziel(port, "/"), PASSWORT).unwrap().verbindung_testen().unwrap_err();
    t.join().unwrap();
    assert!(e.to_string().contains("sofort geschlossen"), "{e}");

    // Beginnt mit Ziffern, ist aber kein gültiges FTP
    let (port, t) = gegenueber(b"220\r\n");
    let e = Ftps::new(&ziel(port, "/"), PASSWORT).unwrap().verbindung_testen().unwrap_err();
    t.join().unwrap();
    assert_eq!(e.kategorie(), "verbindung", "{e}");
}

#[test]
fn ungueltige_wurzel() {
    let e = Ftps::mit_wurzeln(&ziel(21, "/"), PASSWORT, vec![b"kein Zertifikat".to_vec()]).err().unwrap();
    assert!(e.to_string().contains("TLS kann nicht eingerichtet werden"), "{e}");
}

#[test]
fn tls_scheitert_ohne_zertifikatsproblem() {
    use std::io::{BufRead, BufReader, Write};
    // Sagt „AUTH TLS“ zu und legt dann auf – der Handshake scheitert, das Zertifikat ist nicht schuld
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let t = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.write_all(b"220 Hallo\r\n").unwrap();
        let mut zeile = String::new();
        BufReader::new(&s).read_line(&mut zeile).unwrap();
        assert!(zeile.starts_with("AUTH TLS"), "{zeile}");
        s.write_all(b"234 Bitte\r\n").unwrap();
    });
    let e = Ftps::new(&ziel(port, "/"), PASSWORT).unwrap().verbindung_testen().unwrap_err();
    t.join().unwrap();
    assert_eq!(e.kategorie(), "verbindung", "{e}");
}

#[test]
fn ein_frueher_abgelehntes_zertifikat_faerbt_spaetere_fehler_nicht() {
    use std::io::{BufRead, BufReader, Write};
    let s = starten(Aufbau::default());
    let server_port = s.port;
    // Erster Versuch: durchgereicht zum echten Server (selbstsigniert → abgelehnt);
    // zweiter Versuch: TLS scheitert, ohne dass ein Zertifikat geprüft wurde.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let t = std::thread::spawn(move || {
        let (client, _) = l.accept().unwrap();
        let server = std::net::TcpStream::connect(("127.0.0.1", server_port)).unwrap();
        let (mut c_lesen, mut s_schreiben) = (client.try_clone().unwrap(), server.try_clone().unwrap());
        let (mut s_lesen, mut c_schreiben) = (server.try_clone().unwrap(), client.try_clone().unwrap());
        // Wer zuerst auflegt, trennt auch die andere Seite
        let zurueck = std::thread::spawn(move || {
            drop(std::io::copy(&mut s_lesen, &mut c_schreiben));
            let _ = c_schreiben.shutdown(std::net::Shutdown::Both);
        });
        drop(std::io::copy(&mut c_lesen, &mut s_schreiben));
        let _ = server.shutdown(std::net::Shutdown::Both);
        zurueck.join().unwrap();
        let (mut s, _) = l.accept().unwrap();
        s.write_all(b"220 Hallo\r\n").unwrap();
        BufReader::new(&s).read_line(&mut String::new()).unwrap();
        s.write_all(b"234 Bitte\r\n").unwrap();
    });
    let ftps = Ftps::new(&ziel(port, "/"), PASSWORT).unwrap();
    assert_eq!(ftps.verbindung_testen().unwrap_err().kategorie(), "zertifikat");
    let e = ftps.verbindung_testen().unwrap_err();
    t.join().unwrap();
    assert_eq!(e.kategorie(), "verbindung", "{e}");
}
