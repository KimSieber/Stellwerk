//! Stellwerk-Kern: die gesamte Logik der App, ohne Oberfläche.
//!
//! Aufbau gemäß `docs/ARCHITEKTUR.md`:
//! - [`model`]  – Strecke, Stufe, Endpunkt, Regel, relativer Pfad
//! - [`config`] – Laden/Speichern/Validieren der TOML-Konfiguration
//! - [`rules`]  – Regel-Engine (Ebenen, gitignore-Muster, Herkunft)
//! - [`scan`]   – Erfassen lokaler Verzeichnisse
//! - [`diff`]   – Vergleich Quelle ↔ Ziel (Vorschau)
//! - [`ansicht`] – Vergleich einer Stufe starten, Zeilen für die Vergleichsansicht, Auswahl
//! - [`plan`]   – angehakte Zeilen → geordnete Operationen mit Abhängigkeiten
//! - [`endpoint`] – Quelle (nur lesend) und Ziel (schreibend): lokal, FTPS
//! - [`exec`]   – Ausführung „Direkt“ über einen Worker-Pool
//! - [`secrets`] – Passwörter im Schlüsselbund
//! - [`store`]  – Laufzeitdaten in SQLite: Läufe, Journal, Protokoll

pub mod ansicht;
pub mod config;
pub mod diff;
pub mod endpoint;
pub mod error;
pub mod exec;
pub mod model;
pub mod plan;
pub mod rules;
pub mod scan;
pub mod secrets;
pub mod store;

pub use error::{Result, StellwerkError};
