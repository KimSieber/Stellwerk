//! SFTP-Server für die automatischen Tests von Stellwerk (v0.7.2) – ohne Docker.
//!
//! russh nimmt die SSH-Verbindung an (Passwort `kim`/`geheim`, zufälliger Ed25519-Host-
//! Schlüssel) und reicht den SFTP-Kanal an das `sftp-server` von OpenSSH weiter (macOS:
//! /usr/libexec/sftp-server, gehört zum System; Linux: Paket openssh-sftp-server).
//!
//! Aufruf: `sftp-testserver`. Schreibt `PORT=…` und
//! `FINGERPRINT=…` (je eine Zeile) und läuft, bis die Standardeingabe endet – so endet
//! er spätestens mit dem Test, der ihn gestartet hat.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use russh::keys::{Algorithm, HashAlg, PrivateKey};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

const BENUTZER: &str = "kim";
const PASSWORT: &str = "geheim";

/// `sftp-server` von OpenSSH.
fn sftp_server() -> PathBuf {
    ["/usr/libexec/sftp-server", "/usr/lib/openssh/sftp-server", "/usr/lib/ssh/sftp-server", "/usr/libexec/openssh/sftp-server"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .expect("sftp-server von OpenSSH nicht gefunden (Linux: Paket openssh-sftp-server)")
}

#[derive(Clone)]
struct Server;

impl russh::server::Server for Server {
    type Handler = Sitzung;
    fn new_client(&mut self, _: Option<SocketAddr>) -> Sitzung {
        Sitzung { kanaele: Arc::default() }
    }
}

struct Sitzung {
    kanaele: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
}

impl russh::server::Handler for Sitzung {
    type Error = russh::Error;

    async fn auth_password(&mut self, benutzer: &str, passwort: &str) -> Result<Auth, Self::Error> {
        Ok(if benutzer == BENUTZER && passwort == PASSWORT { Auth::Accept } else { Auth::reject() })
    }

    async fn channel_open_session(
        &mut self,
        kanal: Channel<Msg>,
        antwort: russh::server::ChannelOpenHandle,
        _s: &mut Session,
    ) -> Result<(), Self::Error> {
        self.kanaele.lock().await.insert(kanal.id(), kanal);
        antwort.accept().await;
        Ok(())
    }

    async fn subsystem_request(&mut self, id: ChannelId, name: &str, s: &mut Session) -> Result<(), Self::Error> {
        let kanal = self.kanaele.lock().await.remove(&id);
        let Some(kanal) = kanal.filter(|_| name == "sftp") else {
            s.channel_failure(id)?;
            return Ok(());
        };
        s.channel_success(id)?;
        let mut kind = tokio::process::Command::new(sftp_server())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let mut ein = kind.stdin.take().unwrap();
        let mut aus = kind.stdout.take().unwrap();
        let (mut r, mut w) = tokio::io::split(kanal.into_stream());
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut r, &mut ein).await;
        });
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut aus, &mut w).await;
            let _ = w.shutdown().await;
            drop(kind);
        });
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let schluessel = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("Host-Schlüssel");
    let fingerprint = schluessel.public_key().fingerprint(HashAlg::Sha256).to_string();
    let cfg = Arc::new(russh::server::Config { keys: vec![schluessel], ..Default::default() });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("Port");
    let port = listener.local_addr().expect("Adresse").port();
    println!("PORT={port}");
    println!("FINGERPRINT={fingerprint}");
    tokio::spawn(async move {
        let _ = Server.run_on_socket(cfg, &listener).await;
    });
    // Bis die Standardeingabe endet (der Test ist vorbei)
    let mut rest = Vec::new();
    let _ = tokio::io::stdin().read_to_end(&mut rest).await;
}
