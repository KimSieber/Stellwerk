//! Stellwerk-Kern: die gesamte Logik der App, ohne Oberfläche.
//!
//! Aufbau gemäß `docs/ARCHITEKTUR.md`:
//! - [`model`]  – Strecke, Stufe, Endpunkt, Regel, relativer Pfad
//! - [`config`] – Laden/Speichern/Validieren der TOML-Konfiguration
//!
//! Folgende Module kommen in späteren Schritten hinzu:
//! `rules`, `endpoint`, `scan`, `diff`, `plan`, `exec`, `store`, `secrets`.

pub mod config;
pub mod error;
pub mod model;

pub use error::{Result, StellwerkError};
