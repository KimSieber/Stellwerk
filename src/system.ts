// Zugriff auf Systemfunktionen über Tauri-Plugins.
// Ordnerauswahl liefert nur einen Pfad – gelesen wird ausschließlich in Rust.

import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getVersion } from "@tauri-apps/api/app";
import { homeDir } from "@tauri-apps/api/path";

export const system = {
  /** Systemdialog zur Ordnerauswahl; `null`, wenn abgebrochen. */
  async ordnerWaehlen(start?: string): Promise<string | null> {
    const wahl = await open({ directory: true, multiple: false, defaultPath: start || undefined });
    return typeof wahl === "string" ? wahl : null;
  },
  /** Privaten SSH-Schlüssel wählen (v0.7.2b); startet in `~/.ssh`. `null`, wenn abgebrochen. */
  async schluesselWaehlen(): Promise<string | null> {
    const wahl = await open({ directory: false, multiple: false, defaultPath: `${await homeDir()}/.ssh` });
    return typeof wahl === "string" ? wahl : null;
  },
  /** Öffnet einen Link (Webseite, mailto:) im Standardprogramm. */
  oeffnen: (url: string) => openUrl(url),
  /** Versionsnummer der App aus tauri.conf.json. */
  version: () => getVersion(),
};
