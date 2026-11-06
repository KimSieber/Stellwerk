// Gemeinsame Hilfen für Frontend-Tests.
import { useState, type ReactElement } from "react";
import { render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { mockIPC } from "@tauri-apps/api/mocks";

import type { Strecke } from "../api";

/** Rendert mit frischem Query-Cache (keine Wiederholungen, kein Teilen zwischen Tests). */
export function mitQuery(ui: ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

export type Aufruf = { cmd: string; args: unknown };
export type Antworten = Record<string, (args: unknown) => unknown>;

/**
 * Ersetzt die Rust-Seite: `antworten` ordnet jedem Befehl eine Funktion zu.
 * Wirft die Funktion, kommt das beim Frontend als abgelehntes invoke an.
 * Ereignisse (`listen`/`emit`) werden ebenfalls nachgebildet.
 * Liefert die Liste aller Aufrufe zur Prüfung.
 */
export function rustAttrappe(antworten: Antworten): Aufruf[] {
  const aufrufe: Aufruf[] = [];
  mockIPC(
    (cmd, args) => {
      aufrufe.push({ cmd, args });
      const f = antworten[cmd];
      if (!f) throw new Error(`unerwarteter Befehl: ${cmd}`);
      return f(args);
    },
    { shouldMockEvents: true },
  );
  return aufrufe;
}

/** Aufrufe eines bestimmten Befehls. */
export function aufrufeVon(aufrufe: Aufruf[], cmd: string): Aufruf[] {
  return aufrufe.filter((a) => a.cmd === cmd);
}

/** Hält den Wert eines gesteuerten Bausteins, damit Eingaben wirken. */
export function Zustand<T>(props: { start: T; kind: (wert: T, setze: (w: T) => void) => ReactElement; beobachter?: (w: T) => void }) {
  const [wert, setWert] = useState(props.start);
  return props.kind(wert, (w) => {
    setWert(w);
    props.beobachter?.(w);
  });
}

/** Beispielstrecke wie in docs/ARCHITEKTUR.md. */
export function beispielStrecke(): Strecke {
  return {
    id: "mailwerk-web",
    name: "Mailwerk Webseite",
    version: 1,
    regeln: { standard_aus: [".git/"], eigene: [{ muster: "*.psd", typ: "ignorieren" }] },
    stufen: [
      {
        id: "spiegel",
        name: "Entwicklung → Spiegel",
        modus: "direkt",
        quelle: { typ: "lokal", pfad: "/Users/kim/Dev/Mailwerk" },
        ziel: { typ: "lokal", pfad: "/Users/kim/Server/mailwerk" },
        nur_online: "herunterladen",
      },
      {
        id: "server",
        name: "Spiegel → Server",
        modus: "umschalten",
        quelle: { typ: "stufe", stufe: "spiegel" },
        ziel: {
          typ: "ftps",
          host: "ftp.example.de",
          port: 21,
          pfad: "/httpdocs",
          benutzer: "kim",
          tls: "explizit",
          konto_ref: "stellwerk:mailwerk-web:server",
        },
        rechte: { modus: "fest", ordner: "755", dateien: "644" },
        nur_online: "herunterladen",
        regeln: { eigene: [{ muster: "config.php", typ: "ersetzen", quelle: "config.server.php" }] },
      },
    ],
  };
}
