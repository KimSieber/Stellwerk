//! Datenmodell (ARCHITEKTUR Kap. 4).

mod path;
mod regel;
mod strecke;

pub use path::{RelPath, RelPathError};
pub use regel::{Regel, RegelEbene, RegelTyp};
pub use strecke::{
    Dateimodus, Endpunkt, FtpsEndpunkt, Modus, NurOnline, Quelle, Rechte, RechteMuster, Stufe,
    Strecke, Tls, KONFIG_VERSION,
};
