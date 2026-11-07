//! Stellwerk-Kern: die gesamte Logik der App, ohne Oberfläche.
//!
//! Aufbau gemäß `docs/ARCHITEKTUR.md`:
//! - [`model`]  – Strecke, Stufe, Endpunkt, Regel, relativer Pfad
//! - [`config`] – Laden/Speichern/Validieren der TOML-Konfiguration
//! - [`rules`]  – Regel-Engine (Ebenen, gitignore-Muster, Herkunft)
//! - [`scan`]   – Erfassen lokaler Verzeichnisse
//! - [`diff`]   – Vergleich Quelle ↔ Ziel (Vorschau)
//! - [`ansicht`] – Vergleich einer Stufe starten, Zeilen für die Vergleichsansicht, Auswahl
//! - [`secrets`] – Passwörter im Schlüsselbund
//!
//! Folgende Module kommen in späteren Schritten hinzu:
//! `endpoint`, `plan`, `exec`, `store`.

pub mod ansicht;
pub mod config;
pub mod diff;
pub mod error;
pub mod model;
pub mod rules;
pub mod scan;
pub mod secrets;

pub use error::{Result, StellwerkError};
