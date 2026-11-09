//! Vergleichsansicht (KONZEPT Kap. 4): Vergleich einer Stufe starten und das
//! Ergebnis so aufbereiten, dass die Oberfläche nur noch anzeigt (A1).

mod ablauf;
mod sitzung;
mod sortierung;

pub use ablauf::{
    ftps_kennung, manifest_abgleichen, stufe_vergleichen, stufe_vergleichen_entfernt, stufe_vergleichen_entfernt_mit_zugang,
    stufe_vergleichen_mit_zugang, stufe_vergleichen_vom_server, stufe_vergleichen_vom_server_mit_zugang, Drossel,
    EntfernterStand, EntfernterVergleich, Fortschritt, Lesefehler, Ordner, Phase, Seitenname, StufenVergleich, Zugang, server_kennung,
    ziel_kennung,
};
pub use sitzung::{Filter, Seite, VergleichsSitzung, Zeile};
pub use sortierung::{namen_vergleichen, pfade_vergleichen};
