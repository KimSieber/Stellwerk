use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::regel::RegelEbene;

/// Aktuelle Version des Konfigurationsformats einer Strecke.
pub const KONFIG_VERSION: u32 = 1;

/// Eine Strecke: Kette von Stufen (KONZEPT E2). Eine Datei je Strecke.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strecke {
    /// Kennung, zugleich Dateiname (`<id>.toml`). Erlaubt: `a-z`, `0-9`, `-`.
    pub id: String,
    pub name: String,
    #[serde(default = "konfig_version")]
    pub version: u32,
    /// Regeln der Ebene „Strecke“.
    #[serde(default, skip_serializing_if = "RegelEbene::ist_leer")]
    pub regeln: RegelEbene,
    #[serde(default)]
    pub stufen: Vec<Stufe>,
}

fn konfig_version() -> u32 {
    KONFIG_VERSION
}

impl Strecke {
    pub fn stufe(&self, id: &str) -> Option<&Stufe> {
        self.stufen.iter().find(|s| s.id == id)
    }
}

/// Eine Stufe verbindet eine Quelle mit einem Ziel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stufe {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub modus: Modus,
    pub quelle: Quelle,
    pub ziel: Endpunkt,
    #[serde(default, skip_serializing_if = "Rechte::ist_nicht_setzen")]
    pub rechte: Rechte,
    #[serde(default)]
    pub nur_online: NurOnline,
    /// Regeln der Ebene „Stufe“.
    #[serde(default, skip_serializing_if = "RegelEbene::ist_leer")]
    pub regeln: RegelEbene,
}

/// Deployment-Modus (KONZEPT Kap. 7, Entscheidungen v0.5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modus {
    #[default]
    Direkt,
    /// Umschalten automatisch: Hochladen, Umschalten und Übernehmen in einem Zug.
    Umschalten,
    /// Umschalten manuell: jede Phase per Knopf.
    UmschaltenManuell,
}

impl Modus {
    pub fn schaltet_um(self) -> bool {
        self != Modus::Direkt
    }
}

/// Umgang mit Dropbox-Dateien „nur online“ (KONZEPT Kap. 8).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NurOnline {
    #[default]
    Herunterladen,
    Ueberspringen,
}

/// Quelle einer Stufe. Wird zur Laufzeit immer nur lesend geöffnet (E3/A4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum Quelle {
    Lokal { pfad: PathBuf },
    Ftps(FtpsEndpunkt),
    /// Samba-Freigabe mit eigener Anmeldung (v0.7.1).
    Smb(SmbEndpunkt),
    /// SFTP-Server (v0.7.2).
    Sftp(SftpEndpunkt),
    /// Das Ziel einer vorhergehenden Stufe ist die Quelle dieser Stufe.
    Stufe { stufe: String },
}

/// Ziel einer Stufe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum Endpunkt {
    Lokal { pfad: PathBuf },
    Ftps(FtpsEndpunkt),
    Smb(SmbEndpunkt),
    Sftp(SftpEndpunkt),
}

impl From<Endpunkt> for Quelle {
    fn from(e: Endpunkt) -> Self {
        match e {
            Endpunkt::Lokal { pfad } => Quelle::Lokal { pfad },
            Endpunkt::Ftps(f) => Quelle::Ftps(f),
            Endpunkt::Smb(s) => Quelle::Smb(s),
            Endpunkt::Sftp(s) => Quelle::Sftp(s),
        }
    }
}

/// FTPS-Endpunkt. Kein Passwort – nur `konto_ref` als Schlüssel in den Schlüsselbund (E7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FtpsEndpunkt {
    pub host: String,
    #[serde(default = "ftp_port")]
    pub port: u16,
    /// Absoluter Pfad auf dem Server, z. B. `/httpdocs`.
    pub pfad: String,
    pub benutzer: String,
    #[serde(default)]
    pub tls: Tls,
    pub konto_ref: String,
    /// Bewusst bestätigtes Serverzertifikat (SHA-256, `AB:CD:…`), wenn die
    /// Prüfung des Betriebssystems es nicht anerkennt – z. B. selbstsigniert beim
    /// Hoster (ARCHITEKTUR Kap. 11). Ohne Eintrag gilt nur die Systemprüfung.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zertifikat: Option<String>,
}

fn ftp_port() -> u16 {
    21
}

/// Samba-Freigabe (SMB 2/3), angesprochen ohne eingehängtes Laufwerk (v0.7.1).
/// Wie bei FTPS kein Passwort – nur `konto_ref` in den Schlüsselbund (E7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmbEndpunkt {
    pub host: String,
    #[serde(default = "smb_port")]
    pub port: u16,
    /// Name der Freigabe, z. B. `manitu`.
    pub freigabe: String,
    /// Ordner innerhalb der Freigabe, mit `/` beginnend (`/` = ganze Freigabe).
    #[serde(default = "smb_pfad")]
    pub pfad: String,
    pub benutzer: String,
    pub konto_ref: String,
}

/// SFTP-Server (SSH File Transfer Protocol, v0.7.2). Wie bei FTPS kein Passwort –
/// nur `konto_ref` in den Schlüsselbund (E7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SftpEndpunkt {
    pub host: String,
    #[serde(default = "sftp_port")]
    pub port: u16,
    /// Absoluter Pfad auf dem Server, z. B. `/web/sieber-bw.de`.
    pub pfad: String,
    pub benutzer: String,
    pub konto_ref: String,
    /// Bewusst bestätigter Host-Schlüssel (`SHA256:…`, wie OpenSSH ihn anzeigt). Ohne
    /// Eintrag verbindet Stellwerk nicht – der Schlüssel muss erst bestätigt werden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostschluessel: Option<String>,
}

fn sftp_port() -> u16 {
    22
}

fn smb_port() -> u16 {
    445
}

fn smb_pfad() -> String {
    "/".into()
}

/// TLS-Variante. Klartext-FTP wird bewusst nicht angeboten (E6).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tls {
    #[default]
    Explizit,
}

/// Dateirechte am Ziel (KONZEPT Kap. 8). Standard: nicht setzen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "modus", rename_all = "snake_case")]
pub enum Rechte {
    #[default]
    NichtSetzen,
    /// Feste Werte für Ordner und/oder Dateien.
    Fest {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ordner: Option<Dateimodus>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dateien: Option<Dateimodus>,
    },
    /// Regeln nach Muster (z. B. `*.sh` → 755), optional mit Vorgabewerten.
    Muster {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ordner: Option<Dateimodus>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dateien: Option<Dateimodus>,
        #[serde(default)]
        regeln: Vec<RechteMuster>,
    },
}

impl Rechte {
    pub fn ist_nicht_setzen(&self) -> bool {
        matches!(self, Rechte::NichtSetzen)
    }

    /// Alle in dieser Einstellung vorkommenden Modi (für Prüfungen).
    pub fn alle_modi(&self) -> Vec<Dateimodus> {
        match self {
            Rechte::NichtSetzen => vec![],
            Rechte::Fest { ordner, dateien } => ordner.iter().chain(dateien).copied().collect(),
            Rechte::Muster { ordner, dateien, regeln } => ordner
                .iter()
                .chain(dateien)
                .copied()
                .chain(regeln.iter().map(|r| r.modus))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RechteMuster {
    pub muster: String,
    pub modus: Dateimodus,
}

/// Unix-Dateimodus, in der Konfiguration als Oktal-Zeichenkette (`"755"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Dateimodus(u32);

impl Dateimodus {
    pub fn new(bits: u32) -> Option<Self> {
        (bits <= 0o777).then_some(Self(bits))
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    /// Für alle schreibbar (z. B. 777) – erzeugt eine Warnung, kein Verbot.
    pub fn ist_weltschreibbar(self) -> bool {
        self.0 & 0o002 != 0
    }
}

impl TryFrom<String> for Dateimodus {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        let t = s.trim();
        let t = t.strip_prefix("0o").unwrap_or(t);
        if t.is_empty() || t.len() > 4 || !t.chars().all(|c| ('0'..='7').contains(&c)) {
            return Err(format!("ungültiger Dateimodus '{s}' (erwartet z. B. \"755\")"));
        }
        let bits = u32::from_str_radix(t, 8).expect("höchstens vier Oktalziffern – oben geprüft");
        Dateimodus::new(bits).ok_or_else(|| format!("Dateimodus '{s}' außerhalb 000–777"))
    }
}

impl From<Dateimodus> for String {
    fn from(m: Dateimodus) -> Self {
        m.to_string()
    }
}

impl fmt::Display for Dateimodus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:03o}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dateimodus_parsen() {
        assert_eq!(Dateimodus::try_from("755".to_string()).unwrap().bits(), 0o755);
        assert_eq!(Dateimodus::try_from("0644".to_string()).unwrap().bits(), 0o644);
        assert_eq!(Dateimodus::try_from("0o600".to_string()).unwrap().to_string(), "600");
        assert!(Dateimodus::try_from("778".to_string()).is_err());
        assert!(Dateimodus::try_from("1777".to_string()).is_err());
        assert!(Dateimodus::try_from("rwx".to_string()).is_err());
        assert!(Dateimodus::try_from("777".to_string()).unwrap().ist_weltschreibbar());
        assert!(!Dateimodus::try_from("755".to_string()).unwrap().ist_weltschreibbar());
        assert!(Dateimodus::try_from(String::new()).is_err());
        assert!(Dateimodus::try_from(" 644 ".to_string()).is_ok());
        assert_eq!(Dateimodus::new(0o1000), None);
        assert_eq!(String::from(Dateimodus::new(0o7).unwrap()), "007");
    }

    #[test]
    fn alle_modi_je_rechte_variante() {
        let m = |b| Dateimodus::new(b).unwrap();
        assert!(Rechte::NichtSetzen.alle_modi().is_empty());
        assert!(Rechte::NichtSetzen.ist_nicht_setzen());
        let fest = Rechte::Fest { ordner: Some(m(0o755)), dateien: None };
        assert_eq!(fest.alle_modi(), vec![m(0o755)]);
        assert!(!fest.ist_nicht_setzen());
        let muster = Rechte::Muster {
            ordner: None,
            dateien: Some(m(0o644)),
            regeln: vec![RechteMuster { muster: "*.sh".into(), modus: m(0o755) }],
        };
        assert_eq!(muster.alle_modi(), vec![m(0o644), m(0o755)]);
    }

    #[test]
    fn endpunkt_wird_zur_quelle() {
        let lokal = Endpunkt::Lokal { pfad: "/x".into() };
        assert_eq!(Quelle::from(lokal), Quelle::Lokal { pfad: "/x".into() });
        let f = FtpsEndpunkt {
            host: "h".into(),
            port: 21,
            pfad: "/".into(),
            benutzer: "b".into(),
            tls: Tls::Explizit,
            konto_ref: "k".into(),
            zertifikat: None,
        };
        assert_eq!(Quelle::from(Endpunkt::Ftps(f.clone())), Quelle::Ftps(f));
        let s: SmbEndpunkt =
            toml::from_str("host = \"pi\"\nfreigabe = \"f\"\nbenutzer = \"b\"\nkonto_ref = \"k\"").unwrap();
        assert_eq!((s.port, s.pfad.as_str()), (445, "/"));
        assert_eq!(Quelle::from(Endpunkt::Smb(s.clone())), Quelle::Smb(s));
        let s: SftpEndpunkt = toml::from_str("host = \"h\"\npfad = \"/\"\nbenutzer = \"b\"\nkonto_ref = \"k\"").unwrap();
        assert_eq!((s.port, s.hostschluessel.as_deref()), (22, None));
        assert_eq!(Quelle::from(Endpunkt::Sftp(s.clone())), Quelle::Sftp(s));
    }

    #[test]
    fn strecke_findet_stufe() {
        let s = Strecke { id: "s".into(), name: "S".into(), version: 1, regeln: Default::default(), stufen: vec![] };
        assert!(s.stufe("x").is_none());
    }
}
