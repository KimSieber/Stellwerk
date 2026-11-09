//! Datenmodell (ARCHITEKTUR Kap. 4).

mod eintrag;
mod path;
mod regel;
mod strecke;

pub use eintrag::{Eintrag, EintragArt, Hash, Warnung};
pub use path::{RelPath, RelPathError};
pub use regel::{Regel, RegelEbene, RegelTyp};
pub use strecke::{
    Dateimodus, Endpunkt, FtpsEndpunkt, Modus, NurOnline, Quelle, Rechte, RechteMuster, SftpEndpunkt, SmbEndpunkt, Stufe,
    Strecke, Tls, KONFIG_VERSION,
};
