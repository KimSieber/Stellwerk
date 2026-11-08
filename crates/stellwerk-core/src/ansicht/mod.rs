//! Vergleichsansicht (KONZEPT Kap. 4): Vergleich einer Stufe starten und das
//! Ergebnis so aufbereiten, dass die Oberfläche nur noch anzeigt (A1).

mod ablauf;
mod sitzung;
mod sortierung;

pub use ablauf::{
    manifest_abgleichen, stufe_vergleichen, stufe_vergleichen_entfernt, Drossel, EntfernterStand, EntfernterVergleich, Fortschritt,
    Lesefehler, Phase, Seitenname, StufenVergleich, ziel_kennung,
};
pub use sitzung::{Filter, Seite, VergleichsSitzung, Zeile};
pub use sortierung::{namen_vergleichen, pfade_vergleichen};
