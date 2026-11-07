import { describe, expect, it } from "vitest";
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
} from "lucide-react";

import { eintrag } from "../test/vergleich";
import { endung, symbolFuer } from "./dateiart";

const datei = (name: string) => symbolFuer(eintrag(`ordner/${name}`), false);

describe("Symbol je Dateiart", () => {
  it("Endung", () => {
    expect(endung("Index.HTML")).toBe("html");
    expect(endung("archiv.tar.gz")).toBe("gz");
    expect(endung(".htaccess")).toBe("htaccess");
    expect(endung("Makefile")).toBe("");
  });

  it("Ordner offen oder zu, Verknüpfung", () => {
    expect(symbolFuer(eintrag("bilder", "ordner"), false)).toBe(Folder);
    expect(symbolFuer(eintrag("bilder", "ordner"), true)).toBe(FolderOpen);
    expect(symbolFuer(eintrag("link", "symlink"), false)).toBe(FileSymlink);
  });

  it.each([
    ["index.html", FileCode],
    ["style.CSS", FileCode],
    ["sitemap.xml", FileCode],
    ["manifest.json", FileJson],
    ["deploy.sh", FileTerminal],
    [".htaccess", FileCog],
    ["config.yml", FileCog],
    ["server.pem", FileKey],
    ["dump.sql", FileDatabase],
    ["robots.txt", FileText],
    ["handbuch.pdf", FileText],
    ["logo.png", FileImage],
    ["paket.zip", FileArchive],
    ["daten.csv", FileSpreadsheet],
    ["ton.mp3", FileAudio],
    ["film.mov", FileVideo],
    ["schrift.woff2", FileType],
    ["notiz.textClipping", File],
    [".DS_Store", File],
    ["Makefile", File],
  ])("%s", (name, symbol) => {
    expect(datei(name)).toBe(symbol);
  });
});
