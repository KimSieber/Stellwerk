// Gemeinsame Hilfen für Frontend-Tests.
import type { ReactElement } from "react";
import { render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { mockIPC } from "@tauri-apps/api/mocks";

/** Rendert mit frischem Query-Cache (keine Wiederholungen, kein Teilen zwischen Tests). */
export function mitQuery(ui: ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

export type Aufruf = { cmd: string; args: unknown };

/**
 * Ersetzt die Rust-Seite: `antworten` ordnet jedem Befehl eine Funktion zu.
 * Wirft die Funktion, kommt das beim Frontend als abgelehntes invoke an.
 * Liefert die Liste aller Aufrufe zur Prüfung.
 */
export function rustAttrappe(antworten: Record<string, (args: unknown) => unknown>): Aufruf[] {
  const aufrufe: Aufruf[] = [];
  mockIPC((cmd, args) => {
    aufrufe.push({ cmd, args });
    const f = antworten[cmd];
    if (!f) throw new Error(`unerwarteter Befehl: ${cmd}`);
    return f(args);
  });
  return aufrufe;
}
