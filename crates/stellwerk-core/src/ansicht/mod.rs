//! Vergleichsansicht (KONZEPT Kap. 4): Vergleich einer Stufe starten und das
//! Ergebnis so aufbereiten, dass die Oberfläche nur noch anzeigt (A1).

mod ablauf;
mod sitzung;
mod sortierung;

pub use ablauf::{stufe_vergleichen, Drossel, Fortschritt, Lesefehler, Phase, Seitenname, StufenVergleich};
pub use sitzung::{Filter, Seite, VergleichsSitzung, Zeile};
pub use sortierung::{namen_vergleichen, pfade_vergleichen};
