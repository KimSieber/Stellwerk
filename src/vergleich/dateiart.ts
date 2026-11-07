// Symbol je Dateiart (nur Darstellung). Einheitliche Strich-Icons von Lucide
// (ISC-Lizenz) statt der Icons der verknüpften Programme wie im Finder:
// gleich auf macOS und Windows, hell wie dunkel.

import {
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileCog,
  FileDatabase,
  FileImage,
  FileJson,
  FileKey,
  FileSpreadsheet,
  FileSymlink,
  FileTerminal,
  FileText,
  FileType,
  FileVideo,
  Folder,
  FolderOpen,
  type LucideIcon,
} from "lucide-react";

import type { Eintrag } from "../api";

const GRUPPEN: [LucideIcon, string[]][] = [
  [FileCode, ["html", "htm", "php", "phtml", "js", "mjs", "cjs", "ts", "tsx", "jsx", "css", "scss", "sass", "less", "xml", "vue", "svelte", "py", "rb", "go", "rs", "java", "c", "h", "cpp", "swift", "kt"]],
  [FileJson, ["json", "jsonc", "webmanifest"]],
  [FileTerminal, ["sh", "bash", "zsh", "command", "bat", "cmd", "ps1"]],
  [FileCog, ["htaccess", "htpasswd", "ini", "conf", "cfg", "env", "toml", "yaml", "yml", "plist", "properties"]],
  [FileKey, ["pem", "key", "crt", "cer", "p12", "pfx"]],
  [FileDatabase, ["sql", "sqlite", "db"]],
  [FileText, ["txt", "md", "markdown", "log", "rtf", "pdf", "doc", "docx", "odt", "pages"]],
  [FileImage, ["png", "jpg", "jpeg", "gif", "svg", "webp", "ico", "bmp", "tif", "tiff", "heic", "avif"]],
  [FileArchive, ["zip", "gz", "tgz", "tar", "bz2", "xz", "7z", "rar"]],
  [FileSpreadsheet, ["csv", "tsv", "xls", "xlsx", "ods", "numbers"]],
  [FileAudio, ["mp3", "wav", "m4a", "aac", "flac", "ogg"]],
  [FileVideo, ["mp4", "mov", "webm", "mkv", "avi", "m4v"]],
  [FileType, ["woff", "woff2", "ttf", "otf", "eot"]],
];

const NACH_ENDUNG = new Map(GRUPPEN.flatMap(([symbol, endungen]) => endungen.map((e) => [e, symbol] as const)));

/** Endung in Kleinbuchstaben; bei Punktdateien wie „.htaccess“ der Name ohne Punkt. */
export function endung(name: string): string {
  const punkt = name.lastIndexOf(".");
  return punkt === -1 ? "" : name.slice(punkt + 1).toLowerCase();
}

/** Symbol für einen Eintrag; Ordner offen, wenn ihr Inhalt sichtbar ist. */
export function symbolFuer(e: Eintrag, offen: boolean): LucideIcon {
  if (e.art === "ordner") return offen ? FolderOpen : Folder;
  if (e.art === "symlink") return FileSymlink;
  const name = e.pfad.slice(e.pfad.lastIndexOf("/") + 1);
  return NACH_ENDUNG.get(endung(name)) ?? File;
}
