import { StrictMode } from "react";
import { describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import type { JournalZeile, Lauf, StreckeUebersicht, VergleichStart } from "../api";
import { aufrufeVon, rustAttrappe, type Antworten } from "../test/hilfen";
import { kanalSenden, start, summe } from "../test/vergleich";
import { NEU_VERGLEICHEN_NACH_MS, StreckeAnsicht } from "./StreckeAnsicht";

// Die Vergleichsansicht ist eigens getestet; hier zählt nur, was sie zurückmeldet.
vi.mock("../vergleich/VergleichAnsicht", () => ({
  VergleichAnsicht: (p: {
    stufe: string;
    titel: string;
    start?: VergleichStart;
    onZurueck: (stand: VergleichStart | null, uebertragen: boolean) => void;
  }) => (
    <div data-testid="vergleich" data-stufe={p.stufe} data-start={p.start?.vergleich_id ?? "ohne"}>
      <h2>{p.titel}</h2>
      <button onClick={() => p.onZurueck(p.start ?? null, false)}>zurück unverändert</button>
      <button onClick={() => p.onZurueck(start({ vergleich_id: `${p.stufe}#neu`, zusammenfassung: summe() }), false)}>
        zurück neu verglichen
      </button>
      <button onClick={() => p.onZurueck(start({ vergleich_id: `${p.stufe}#nach`, zusammenfassung: summe() }), true)}>
        zurück nach Übertragung
      </button>
      <button onClick={() => p.onZurueck(null, true)}>zurück nach Übertragung ohne Vergleich</button>
      <button onClick={() => p.onZurueck(null, false)}>zurück abgebrochen</button>
    </div>
  ),
}));

const STRECKE: StreckeUebersicht = {
  id: "web",
  name: "Test mit Mahlwerk",
  stufen: [
    { id: "eins", name: "Repo → Test 1", quelle: "/repo", ziel: "/test1" },
    { id: "zwei", name: "Test 1 → Test 2", quelle: "Ziel von „eins“", ziel: "/test2" },
    { id: "server", name: "Test 2 → Server", quelle: "Ziel von „zwei“", ziel: "ftps://ftp.example.de/httpdocs" },
  ],
  probleme: [],
  fehler: null,
};

const GLEICH = summe({ gleich: 12 });
const DIFF = summe({ neu: 3, geaendert: 2, loeschen: 1, konflikt: 1, ausgewaehlt: 6, bytes_uebertragen: 48_000 });

const KEIN_MANIFEST = { kategorie: "kein_manifest", meldung: "Noch nie mit dem Server verglichen" };
const NIE = "Noch nie mit dem Server verglichen.";

/** Vergleich, den der Test beendet. */
function haengend() {
  const s = { kanal: { id: 0 }, ok: (_v: VergleichStart) => {}, nein: (_e: unknown) => {} };
  const antwort = (args: unknown) => {
    s.kanal = (args as { fortschritt: { id: number } }).fortschritt;
    return new Promise((ok, nein) => {
      s.ok = ok;
      s.nein = nein;
    });
  };
  return { s, antwort };
}

function rust(antworten: Antworten = {}) {
  return rustAttrappe({
    vergleich_starten: (a) => {
      const { stufe, server } = a as { stufe: string; server: boolean };
      // Der Server wurde noch nie gelesen – erst „Mit Server vergleichen“
      if (stufe === "server" && !server) throw KEIN_MANIFEST;
      return start({ vergleich_id: `${stufe}#1`, zusammenfassung: stufe === "eins" ? GLEICH : DIFF });
    },
    vergleich_abbrechen: () => true,
    vergleich_schliessen: () => null,
    laeufe: () => [],
    lauf_journal: () => [],
    ...antworten,
  });
}

function ansicht(strecke = STRECKE) {
  const zurueck = vi.fn();
  const bearbeiten = vi.fn();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <StrictMode>
      <QueryClientProvider client={client}>
        <StreckeAnsicht strecke={strecke} onZurueck={zurueck} onBearbeiten={bearbeiten} />
      </QueryClientProvider>
    </StrictMode>,
  );
  return { zurueck, bearbeiten };
}

const zeile = (name: string) => screen.getByRole("heading", { level: 3, name }).closest("li") as HTMLElement;
const karte = (name: string) => zeile(name).querySelector(".stufen-karte") as HTMLElement;
const gestartet = (aufrufe: ReturnType<typeof rust>) =>
  aufrufeVon(aufrufe, "vergleich_starten").map((a) => (a.args as { stufe: string }).stufe);

describe("Streckenansicht – Status je Stufe", () => {
  it("vergleicht beim Öffnen alle Stufen nacheinander – genau einmal, auch im StrictMode", async () => {
    const erste = haengend();
    let n = 0;
    const aufrufe = rust({
      vergleich_starten: (a) => {
        n += 1;
        if (n === 1) return erste.antwort(a);
        if ((a as { stufe: string }).stufe === "server") throw KEIN_MANIFEST;
        return start({ vergleich_id: "zwei#1", zusammenfassung: DIFF });
      },
    });
    ansicht();
    expect(screen.getByRole("heading", { level: 2, name: "Test mit Mahlwerk" })).toBeInTheDocument();
    expect(within(zeile("Repo → Test 1")).getByText("Vergleich wird vorbereitet …")).toBeInTheDocument();
    expect(within(zeile("Test 1 → Test 2")).getByText("Vergleich wartet …")).toBeInTheDocument();
    expect(within(zeile("Test 2 → Server")).getByText("Vergleich wartet …")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Alle neu vergleichen" })).toBeDisabled();
    expect(within(karte("Repo → Test 1")).getByRole("button", { name: "Vergleichen …" })).toBeDisabled();

    act(() => kanalSenden(erste.s.kanal, 0, { phase: "ziel", anzahl: 42 }));
    expect(within(zeile("Repo → Test 1")).getByText("Ziel wird gelesen … 42")).toBeInTheDocument();
    expect(gestartet(aufrufe)).toEqual(["eins"]);

    await act(async () => erste.s.ok(start({ vergleich_id: "eins#1", zusammenfassung: GLEICH })));
    await waitFor(() => expect(within(zeile("Test 1 → Test 2")).getByText("3 neu · 2 geändert · 1 löschen · 1 Konflikt")).toBeInTheDocument());
    await waitFor(() => expect(within(zeile("Test 2 → Server")).getByText(NIE)).toHaveClass("hinweis"));
    expect(gestartet(aufrufe)).toEqual(["eins", "zwei", "server"]);
    expect(aufrufeVon(aufrufe, "vergleich_starten")[0].args).toMatchObject({ strecke: "web", stufe: "eins", server: false });

    expect(within(zeile("Repo → Test 1")).getByText("gleich")).toBeInTheDocument();
    expect(karte("Repo → Test 1")).toHaveClass("status-gleich");
    expect(karte("Test 1 → Test 2")).toHaveClass("status-differenz");
    expect(karte("Test 2 → Server")).toHaveClass("status-grau");
    expect(zeile("Test 1 → Test 2").querySelector(".stufen-stand")).toHaveTextContent(/· 48 KB · verglichen \d\d:\d\d$/);
    expect(zeile("Repo → Test 1").querySelector(".stufen-stand")).not.toHaveTextContent("KB");
    expect(within(karte("Repo → Test 1")).getByRole("button", { name: "Ansehen & übertragen …" })).toBeEnabled();
    expect(within(karte("Test 1 → Test 2")).getByTitle("Ziel von „eins“ → /test2")).toBeInTheDocument();
  });

  it("Fehler und Abbruch eines Vergleichs; danach lässt sich die Stufe selbst vergleichen", async () => {
    rust({
      vergleich_starten: (a) => {
        if ((a as { stufe: string }).stufe === "eins") throw { kategorie: "kein_verzeichnis", meldung: "Kein Verzeichnis: /test1" };
        throw { kategorie: "abbruch", meldung: "Abgebrochen" };
      },
    });
    ansicht();
    expect(await within(zeile("Repo → Test 1")).findByText("Kein Verzeichnis: /test1")).toHaveClass("fehler");
    expect(await within(zeile("Test 1 → Test 2")).findByText("Vergleich abgebrochen.")).toHaveClass("hinweis");
    expect(karte("Repo → Test 1")).toHaveClass("status-fehler");
    await userEvent.click(within(karte("Repo → Test 1")).getByRole("button", { name: "Vergleichen …" }));
    expect(screen.getByTestId("vergleich")).toHaveAttribute("data-start", "ohne");
  });

  it("„Alle neu vergleichen“ vergleicht alle Stufen erneut", async () => {
    const aufrufe = rust();
    ansicht();
    const knopf = screen.getByRole("button", { name: "Alle neu vergleichen" });
    await waitFor(() => expect(knopf).toBeEnabled());
    await userEvent.click(knopf);
    await waitFor(() => expect(gestartet(aufrufe)).toEqual(["eins", "zwei", "server", "eins", "zwei", "server"]));
  });

  it("FTPS noch nie gelesen: „Mit Server vergleichen …“ liest den Server, danach wie lokal", async () => {
    const aufrufe = rust();
    ansicht({ ...STRECKE, stufen: [STRECKE.stufen[2]] });
    expect(await within(zeile("Test 2 → Server")).findByText(NIE)).toBeInTheDocument();
    expect(karte("Test 2 → Server")).toHaveClass("status-grau");
    expect(within(karte("Test 2 → Server")).queryByRole("button", { name: "Vergleichen …" })).toBeNull();
    await userEvent.click(within(karte("Test 2 → Server")).getByRole("button", { name: "Mit Server vergleichen …" }));
    expect(await within(zeile("Test 2 → Server")).findByText("3 neu · 2 geändert · 1 löschen · 1 Konflikt")).toBeInTheDocument();
    expect(aufrufeVon(aufrufe, "vergleich_starten").map((a) => (a.args as { server: boolean }).server)).toEqual([false, true]);
    expect(within(karte("Test 2 → Server")).getByRole("button", { name: "Ansehen & übertragen …" })).toBeEnabled();
  });

  it("ohne Stufen: nichts zu vergleichen", () => {
    const aufrufe = rust();
    ansicht({ ...STRECKE, stufen: [] });
    expect(screen.getByRole("button", { name: "Alle neu vergleichen" })).toBeDisabled();
    expect(gestartet(aufrufe)).toEqual([]);
  });

  it("Bearbeiten", async () => {
    rust();
    const { bearbeiten } = ansicht();
    await userEvent.click(screen.getByRole("button", { name: "Bearbeiten" }));
    expect(bearbeiten).toHaveBeenCalled();
  });
});

describe("Streckenansicht – zurück im Vordergrund", () => {
  const spaeter = (ms: number) => {
    const echt = Date.now;
    const jetzt = echt();
    return vi.spyOn(Date, "now").mockImplementation(() => jetzt + ms);
  };
  const sichtbar = (zustand: "visible" | "hidden") => {
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => zustand });
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
  };
  const fokus = () =>
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });

  async function geladen() {
    const aufrufe = rust();
    const r = ansicht();
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Ansehen & übertragen …" })).toHaveLength(2));
    await waitFor(() => expect(within(zeile("Test 2 → Server")).getByText(NIE)).toBeInTheDocument());
    return { aufrufe, ...r };
  }

  it("vergleicht Stufen neu, deren Vergleich älter als eine Minute ist – FTPS ohne Server", async () => {
    const { aufrufe } = await geladen();
    const uhr = spaeter(NEU_VERGLEICHEN_NACH_MS + 1000);
    fokus();
    // Die nie gelesene FTPS-Stufe hat keinen Vergleich – sie bleibt, wie sie ist
    await waitFor(() => expect(gestartet(aufrufe)).toEqual(["eins", "zwei", "server", "eins", "zwei"]));
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Ansehen & übertragen …" })).toHaveLength(2));
    expect(aufrufeVon(aufrufe, "vergleich_starten").every((a) => (a.args as { server: boolean }).server === false)).toBe(true);
    // Sichtbar werden wirkt genauso; die Vergleiche sind jetzt frisch
    uhr.mockRestore();
    sichtbar("visible");
    fokus();
    expect(gestartet(aufrufe)).toHaveLength(5);
  });

  it("frische Vergleiche, verdecktes Fenster, geöffnete Stufe: nichts", async () => {
    const { aufrufe } = await geladen();
    fokus();
    expect(gestartet(aufrufe)).toHaveLength(3);
    const uhr = spaeter(NEU_VERGLEICHEN_NACH_MS + 1000);
    sichtbar("hidden");
    expect(gestartet(aufrufe)).toHaveLength(3);
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "visible" });
    await userEvent.click(within(karte("Repo → Test 1")).getByRole("button", { name: "Ansehen & übertragen …" }));
    fokus();
    expect(gestartet(aufrufe)).toHaveLength(3);
    // Zurück in der Übersicht: jetzt schon
    await userEvent.click(screen.getByRole("button", { name: "zurück unverändert" }));
    fokus();
    await waitFor(() => expect(gestartet(aufrufe)).toEqual(["eins", "zwei", "server", "eins", "zwei"]));
    uhr.mockRestore();
  });

  it("während verglichen wird: nichts dazu; nach dem Verlassen hört die Ansicht nicht mehr hin", async () => {
    const zweite = haengend();
    let n = 0;
    const aufrufe = rust({
      vergleich_starten: (a) => {
        n += 1;
        if (n === 2) return zweite.antwort(a);
        return start({ vergleich_id: `${(a as { stufe: string }).stufe}#1`, zusammenfassung: GLEICH });
      },
    });
    const { zurueck } = ansicht({ ...STRECKE, stufen: STRECKE.stufen.slice(0, 2) });
    await waitFor(() => expect(within(zeile("Repo → Test 1")).getByText("gleich")).toBeInTheDocument());
    const uhr = spaeter(NEU_VERGLEICHEN_NACH_MS + 1000);
    fokus();
    expect(gestartet(aufrufe)).toEqual(["eins", "zwei"]);
    await act(async () => zweite.s.ok(start({ vergleich_id: "zwei#1", zusammenfassung: GLEICH })));
    await waitFor(() => expect(within(zeile("Test 1 → Test 2")).getByText("gleich")).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "← Strecken" }));
    expect(zurueck).toHaveBeenCalled();
    uhr.mockRestore();
  });
});

describe("Streckenansicht – Stufe öffnen und zurück", () => {
  async function fertigGeladen() {
    const aufrufe = rust();
    const r = ansicht();
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Ansehen & übertragen …" })).toHaveLength(2));
    await waitFor(() => expect(within(zeile("Test 2 → Server")).getByText(NIE)).toBeInTheDocument());
    return { aufrufe, ...r };
  }
  const oeffnen = (stufe: string) => userEvent.click(within(karte(stufe)).getByRole("button", { name: "Ansehen & übertragen …" }));

  it("öffnet den fertigen Vergleich ohne neuen Lauf; unverändert zurück behält die Zeit", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 10, 7, 10, 0));
    const { aufrufe } = await fertigGeladen();
    const zeitVorher = zeile("Test 1 → Test 2").querySelector(".stufen-stand")!.textContent;
    expect(zeitVorher).toContain("verglichen 10:00");
    vi.setSystemTime(new Date(2026, 10, 7, 10, 5));
    await oeffnen("Test 1 → Test 2");
    const v = screen.getByTestId("vergleich");
    expect(v).toHaveAttribute("data-start", "zwei#1");
    expect(screen.getByRole("heading", { level: 2, name: "Test mit Mahlwerk · Test 1 → Test 2" })).toBeInTheDocument();
    const laeufeVorher = aufrufeVon(aufrufe, "laeufe").length;
    await userEvent.click(screen.getByRole("button", { name: "zurück unverändert" }));
    expect(zeile("Test 1 → Test 2").querySelector(".stufen-stand")!.textContent).toBe(zeitVorher);
    expect(gestartet(aufrufe)).toEqual(["eins", "zwei", "server"]);
    // Protokoll der Stufe wird neu geladen
    await waitFor(() => expect(aufrufeVon(aufrufe, "laeufe").length).toBeGreaterThan(laeufeVorher));
     vi.useRealTimers();
  });

  it("in der Detailansicht neu verglichen: neuer Stand mit neuer Zeit", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 10, 7, 10, 0));
    await fertigGeladen();
    vi.setSystemTime(new Date(2026, 10, 7, 10, 5));
    await oeffnen("Test 1 → Test 2");
    await userEvent.click(screen.getByRole("button", { name: "zurück neu verglichen" }));
    expect(within(zeile("Test 1 → Test 2")).getByText("gleich")).toBeInTheDocument();
    expect(zeile("Test 1 → Test 2").querySelector(".stufen-stand")).toHaveTextContent("verglichen 10:05");
    vi.useRealTimers();
  });

  it("nach einer Übertragung wird die folgende Stufe neu verglichen, auch eine FTPS-Stufe (gegen das Manifest)", async () => {
    const { aufrufe } = await fertigGeladen();
    await oeffnen("Repo → Test 1");
    await userEvent.click(screen.getByRole("button", { name: "zurück nach Übertragung" }));
    await waitFor(() => expect(gestartet(aufrufe)).toEqual(["eins", "zwei", "server", "zwei"]));
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Ansehen & übertragen …" })).toHaveLength(2));
    await oeffnen("Test 1 → Test 2");
    await userEvent.click(screen.getByRole("button", { name: "zurück nach Übertragung" }));
    await waitFor(() => expect(gestartet(aufrufe)).toEqual(["eins", "zwei", "server", "zwei", "server"]));
    expect(aufrufeVon(aufrufe, "vergleich_starten").at(-1)!.args).toMatchObject({ server: false });
  });

  it("nach einer Übertragung in der letzten Stufe: nichts weiter zu vergleichen", async () => {
    const aufrufe = rust();
    ansicht({ ...STRECKE, stufen: STRECKE.stufen.slice(0, 2) });
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Ansehen & übertragen …" })).toHaveLength(2));
    await oeffnen("Test 1 → Test 2");
    await userEvent.click(screen.getByRole("button", { name: "zurück nach Übertragung" }));
    await new Promise((r) => setTimeout(r, 0));
    expect(gestartet(aufrufe)).toEqual(["eins", "zwei"]);
  });

  it("Übertragung ohne fertigen Vergleich: eigene und folgende Stufe neu", async () => {
    const { aufrufe } = await fertigGeladen();
    await oeffnen("Repo → Test 1");
    await userEvent.click(screen.getByRole("button", { name: "zurück nach Übertragung ohne Vergleich" }));
    await waitFor(() => expect(gestartet(aufrufe)).toEqual(["eins", "zwei", "server", "eins", "zwei"]));
  });

  it("abgebrochen zurück: Stufe gilt als nicht verglichen", async () => {
    await fertigGeladen();
    await oeffnen("Test 1 → Test 2");
    await userEvent.click(screen.getByRole("button", { name: "zurück abgebrochen" }));
    expect(within(zeile("Test 1 → Test 2")).getByText("Nicht verglichen.")).toBeInTheDocument();
    expect(karte("Test 1 → Test 2")).toHaveClass("status-grau");
  });

  it("während im Hintergrund noch verglichen wird: neue Aufträge reihen sich ein", async () => {
    const zweite = haengend();
    const aufrufe = rust({
      vergleich_starten: (a) => {
        const { stufe } = a as { stufe: string };
        if (stufe === "zwei" && aufrufeVon(aufrufe, "vergleich_starten").length === 2) return zweite.antwort(a);
        return start({ vergleich_id: `${stufe}#1`, zusammenfassung: GLEICH });
      },
    });
    ansicht();
    await waitFor(() => expect(within(karte("Repo → Test 1")).getByRole("button", { name: "Ansehen & übertragen …" })).toBeEnabled());
    await oeffnen("Repo → Test 1");
    await userEvent.click(screen.getByRole("button", { name: "zurück nach Übertragung" }));
    expect(gestartet(aufrufe)).toEqual(["eins", "zwei"]);
    await act(async () => zweite.s.ok(start({ vergleich_id: "zwei#1", zusammenfassung: DIFF })));
    await waitFor(() => expect(gestartet(aufrufe)).toEqual(["eins", "zwei", "server", "zwei"]));
  });
});

describe("Streckenansicht – Verlassen", () => {
  it("bricht den laufenden Vergleich ab, vergleicht nichts mehr und gibt Fertiges frei", async () => {
    const zweite = haengend();
    const aufrufe = rust({
      vergleich_starten: (a) => {
        const { stufe } = a as { stufe: string };
        if (stufe === "zwei") return zweite.antwort(a);
        return start({ vergleich_id: "eins#1", zusammenfassung: GLEICH });
      },
    });
    const { zurueck } = ansicht();
    await waitFor(() => expect(gestartet(aufrufe)).toEqual(["eins", "zwei"]));
    await userEvent.click(screen.getByRole("button", { name: "← Strecken" }));
    expect(zurueck).toHaveBeenCalled();
    expect(aufrufeVon(aufrufe, "vergleich_abbrechen")[0].args).toEqual({ strecke: "web", stufe: "zwei" });
    expect(aufrufeVon(aufrufe, "vergleich_schliessen").map((a) => a.args)).toEqual([{ vergleichId: "eins#1" }]);
    // Ein spätes Ergebnis gehört niemandem mehr – es wird freigegeben
    await act(async () => zweite.s.ok(start({ vergleich_id: "zwei#spät" })));
    await waitFor(() => expect(aufrufeVon(aufrufe, "vergleich_schliessen").map((a) => a.args)).toContainEqual({ vergleichId: "zwei#spät" }));
  });

  it("späte Fehler und wartende Stufen bleiben nach dem Verlassen folgenlos", async () => {
    const erste = haengend();
    const aufrufe = rust({ vergleich_starten: (a) => erste.antwort(a) });
    ansicht();
    await userEvent.click(screen.getByRole("button", { name: "← Strecken" }));
    await act(async () => erste.s.nein({ kategorie: "abbruch", meldung: "Abgebrochen" }));
    expect(gestartet(aufrufe)).toEqual(["eins"]);
    expect(aufrufeVon(aufrufe, "vergleich_schliessen")).toHaveLength(0);
  });

  it("nach Ende aller Vergleiche wird beim Verlassen nichts abgebrochen", async () => {
    const aufrufe = rust();
    ansicht();
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Ansehen & übertragen …" })).toHaveLength(2));
    await waitFor(() => expect(within(zeile("Test 2 → Server")).getByText(NIE)).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "← Strecken" }));
    expect(aufrufeVon(aufrufe, "vergleich_abbrechen")).toHaveLength(0);
    expect(aufrufeVon(aufrufe, "vergleich_schliessen")).toHaveLength(2);
  });
});

describe("Streckenansicht – Protokoll je Stufe", () => {
  const plan = { ordner_anlegen: 1, kopieren: 2, ueberschreiben: 0, rechte: 0, dateien_loeschen: 1, ordner_loeschen: 0, bytes: 3000 };
  const ergebnis = { erledigt: 4, fehler: [], uebersprungen: 0, abgebrochen: false, gestoppt: false, bytes: 3000, journal_fehler: null };
  const lauf = (id: string, teil: Partial<Lauf> = {}): Lauf => ({
    id,
    strecke_id: "web",
    stufe_id: "eins",
    modus: "direkt",
    phase: "fertig",
    gestartet: new Date(2026, 10, 7, 21, 40).getTime(),
    beendet: null,
    zusammenfassung: plan,
    ergebnis,
    ...teil,
  });
  const schritt = (nr: number, teil: Partial<JournalZeile>): JournalZeile => ({
    nr,
    op: "kopieren",
    pfad: "a.txt",
    quelle: "a.txt",
    modus: null,
    groesse: 1,
    status: "erledigt",
    fehler: null,
    hinweis: null,
    aktualisiert: 0,
    ...teil,
  });

  it("zeigt die Läufe je Stufe, neueste oben, und jeden Schritt eines Laufs", async () => {
    const aufrufe = rust({
      laeufe: (a) =>
        (a as { stufe: string }).stufe === "eins"
          ? [
              lauf("neu"),
              lauf("fehler", { ergebnis: { ...ergebnis, erledigt: 3, fehler: [{ nr: 1, art: "kopieren", pfad: "x", meldung: "kaputt" }] } }),
              lauf("abgebrochen", { phase: "abgebrochen", ergebnis: { ...ergebnis, erledigt: 1, abgebrochen: true } }),
            ]
          : [],
      lauf_journal: () => [
        schritt(0, { op: "ordner_anlegen", pfad: "css", quelle: null, groesse: 0 }),
        schritt(1, { pfad: "config.php", quelle: "config.server.php", status: "fehler", fehler: "kein Zugriff", hinweis: "Datenverbindung 3-mal mit neuem Port wiederholt" }),
        schritt(3, { pfad: "b.txt", quelle: "b.txt", hinweis: "Datenverbindung 1-mal mit neuem Port wiederholt" }),
        schritt(2, { op: "rechte_setzen", pfad: "a.txt", quelle: null, modus: "644", status: "uebersprungen" }),
      ],
    });
    ansicht();
    const p = within(within(zeile("Repo → Test 1")).getByLabelText("Protokoll"));
    const eintraege = await p.findAllByRole("listitem");
    expect(eintraege.map((e) => e.textContent)).toEqual([
      "✓ 07.11.2026, 21:40 · 4 Schritte · 3 KB",
      "✗ 07.11.2026, 21:40 · 1 Fehler · 3 von 4 Schritten",
      "⚠ 07.11.2026, 21:40 · abgebrochen · 1 von 4 Schritten",
    ]);
    expect(aufrufeVon(aufrufe, "laeufe").find((a) => (a.args as { stufe: string }).stufe === "eins")!.args).toEqual({
      strecke: "web",
      stufe: "eins",
      limit: 50,
    });
    expect(await within(within(zeile("Test 1 → Test 2")).getByLabelText("Protokoll")).findByText("Noch nichts übertragen.")).toBeInTheDocument();

    expect(aufrufeVon(aufrufe, "lauf_journal")).toHaveLength(0);
    await userEvent.click(p.getByText(/4 Schritte · 3 KB/));
    const schritte = (await screen.findAllByRole("listitem")).filter((li) => li.closest(".journal"));
    expect(aufrufeVon(aufrufe, "lauf_journal")[0].args).toEqual({ laufId: "neu" });
    expect(schritte.map((s) => s.textContent)).toEqual([
      "Ordner anlegen css erledigt",
      "Kopieren config.php ← config.server.php Fehler – kein Zugriff · ⚠ Datenverbindung 3-mal mit neuem Port wiederholt",
      "Kopieren b.txt erledigt · ⚠ Datenverbindung 1-mal mit neuem Port wiederholt",
      "Rechte setzen a.txt 644 nicht ausgeführt",
    ]);
    expect(schritte[2].querySelector(".journal-hinweis")).not.toBeNull();
    expect(schritte[1]).toHaveClass("journal-fehler");
    // Zuklappen und wieder auf: kein erneutes Laden nötig
    await userEvent.click(p.getByText(/4 Schritte · 3 KB/));
    expect(document.querySelector(".journal")).toBeNull();
  });

  it("Laden und Fehler beim Protokoll und beim Journal", async () => {
    rust({
      laeufe: (a) => {
        if ((a as { stufe: string }).stufe === "zwei") throw { kategorie: "datenbank", meldung: "Datenbank: kaputt" };
        if ((a as { stufe: string }).stufe === "server") return new Promise(() => {});
        return [{ ...lauf("l1"), phase: "laeuft", ergebnis: null }, lauf("l2")];
      },
      lauf_journal: (a) => {
        if ((a as { laufId: string }).laufId === "l1") throw { kategorie: "datenbank", meldung: "Journal kaputt" };
        return new Promise(() => {});
      },
    });
    ansicht();
    expect(await within(zeile("Test 1 → Test 2")).findByText("Datenbank: kaputt")).toHaveClass("fehler");
    expect(within(within(zeile("Test 2 → Server")).getByLabelText("Protokoll")).getByText("…")).toBeInTheDocument();
    const p = within(within(zeile("Repo → Test 1")).getByLabelText("Protokoll"));
    await userEvent.click(await p.findByText(/nicht abgeschlossen/));
    expect(await p.findByText("Journal kaputt")).toHaveClass("fehler");
    await userEvent.click(p.getByText(/4 Schritte/));
    expect(p.getAllByText("…").length).toBeGreaterThan(0);
  });
});
