import { describe, expect, it, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { StreckeUebersicht } from "../api";
import { mitQuery, rustAttrappe } from "../test/hilfen";
import { StreckenListe } from "./StreckenListe";

const ORT = "/Users/kim/Library/Application Support/de.sieber-bw.stellwerk";

const mailwerk: StreckeUebersicht = {
  id: "mailwerk-web",
  name: "Mailwerk Webseite",
  stufen: [
    { id: "spiegel", name: "Entwicklung → Spiegel", quelle: "/Users/kim/Dev/Mailwerk", ziel: "/Users/kim/Server/mailwerk" },
    { id: "server", name: "Spiegel → Server", quelle: "Ziel von „spiegel“", ziel: "ftps://ftp.example.de/httpdocs" },
  ],
  probleme: [],
  fehler: null,
};

describe("Startseite – Strecken", () => {
  it("zeigt den Ladezustand", () => {
    rustAttrappe({ strecken_liste: () => new Promise(() => {}), konfig_ort: () => ORT });
    mitQuery(<StreckenListe onNeu={vi.fn()} onAnzeigen={vi.fn()} />);
    expect(screen.getByText("Strecken werden geladen …")).toBeInTheDocument();
  });

  it("zeigt einen Fehler der Rust-Seite", async () => {
    rustAttrappe({
      strecken_liste: () => {
        throw { kategorie: "konfiguration", meldung: "Konfigurationsfehler in config.toml: kaputt" };
      },
      konfig_ort: () => ORT,
    });
    mitQuery(<StreckenListe onNeu={vi.fn()} onAnzeigen={vi.fn()} />);
    expect(await screen.findByText("Konfigurationsfehler in config.toml: kaputt")).toHaveClass("fehler");
  });

  it("zeigt bei leerer Liste den Konfigurationsort", async () => {
    rustAttrappe({ strecken_liste: () => [], konfig_ort: () => ORT });
    mitQuery(<StreckenListe onNeu={vi.fn()} onAnzeigen={vi.fn()} />);
    expect(await screen.findByText("Noch keine Strecke angelegt.")).toBeInTheDocument();
    expect(await screen.findByText(`${ORT}/strecken/`)).toBeInTheDocument();
  });

  it("zeigt Platzhalter, solange der Konfigurationsort fehlt", async () => {
    rustAttrappe({ strecken_liste: () => [], konfig_ort: () => new Promise(() => {}) });
    mitQuery(<StreckenListe onNeu={vi.fn()} onAnzeigen={vi.fn()} />);
    expect(await screen.findByText("…/strecken/")).toBeInTheDocument();
  });

  it("zeigt Strecken als Karten mit nummerierten Stufen", async () => {
    rustAttrappe({ strecken_liste: () => [mailwerk], konfig_ort: () => ORT });
    mitQuery(<StreckenListe onNeu={vi.fn()} onAnzeigen={vi.fn()} />);

    const karte = (await screen.findByRole("heading", { name: "Mailwerk Webseite" })).closest("article")!;
    const k = within(karte);
    expect(k.getByText("mailwerk-web")).toBeInTheDocument();

    const stufen = k.getAllByRole("listitem");
    expect(stufen).toHaveLength(2);
    expect(within(stufen[0]).getByText("1")).toBeInTheDocument();
    expect(within(stufen[0]).getByText("Entwicklung → Spiegel")).toBeInTheDocument();
    expect(within(stufen[0]).getByText("/Users/kim/Dev/Mailwerk → /Users/kim/Server/mailwerk")).toBeInTheDocument();
    expect(within(stufen[1]).getByText("2")).toBeInTheDocument();
    expect(within(stufen[1]).getByTitle("Ziel von „spiegel“ → ftps://ftp.example.de/httpdocs")).toBeInTheDocument();

    // Keine Problemliste, wenn alles gültig ist
    expect(karte.querySelector(".probleme")).toBeNull();
  });

  it("zeigt Fehler und Warnungen der Validierung farblich getrennt", async () => {
    const mitProblemen: StreckeUebersicht = {
      ...mailwerk,
      probleme: [
        { schwere: "fehler", ort: "stufen[0].ziel.pfad", meldung: "Pfad fehlt" },
        { schwere: "warnung", ort: "stufen[1].rechte", meldung: "Modus 777 ist für alle schreibbar" },
      ],
    };
    rustAttrappe({ strecken_liste: () => [mitProblemen], konfig_ort: () => ORT });
    mitQuery(<StreckenListe onNeu={vi.fn()} onAnzeigen={vi.fn()} />);

    const fehler = (await screen.findByText("Pfad fehlt", { exact: false })).closest("li")!;
    expect(fehler).toHaveClass("fehler");
    expect(within(fehler).getByText("stufen[0].ziel.pfad")).toBeInTheDocument();
    const warnung = screen.getByText("Modus 777 ist für alle schreibbar", { exact: false }).closest("li")!;
    expect(warnung).toHaveClass("warnung");
  });

  it("zeigt defekte Dateien als eigene Karte, ohne die anderen zu verdecken", async () => {
    const defekt: StreckeUebersicht = {
      id: "kaputt",
      name: null,
      stufen: [],
      probleme: [],
      fehler: "Konfigurationsfehler in kaputt.toml: unerwartetes Dateiende",
    };
    rustAttrappe({ strecken_liste: () => [defekt, mailwerk], konfig_ort: () => ORT });
    mitQuery(<StreckenListe onNeu={vi.fn()} onAnzeigen={vi.fn()} />);

    const karte = (await screen.findByRole("heading", { name: "kaputt" })).closest("article")!;
    expect(karte).toHaveClass("defekt");
    expect(within(karte).getByText(/unerwartetes Dateiende/)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Mailwerk Webseite" })).toBeInTheDocument();
  });

  it("lädt auf Knopfdruck neu und sperrt den Knopf währenddessen", async () => {
    let anzahl = 0;
    let freigeben: (v: StreckeUebersicht[]) => void = () => {};
    rustAttrappe({
      strecken_liste: () => {
        anzahl += 1;
        if (anzahl === 1) return [];
        return new Promise<StreckeUebersicht[]>((r) => (freigeben = r));
      },
      konfig_ort: () => ORT,
    });
    mitQuery(<StreckenListe onNeu={vi.fn()} onAnzeigen={vi.fn()} />);
    const knopf = await screen.findByRole("button", { name: "Neu laden" });
    expect(knopf).toBeEnabled();

    await userEvent.click(knopf);
    await waitFor(() => expect(knopf).toBeDisabled());
    expect(anzahl).toBe(2);

    freigeben([mailwerk]);
    expect(await screen.findByRole("heading", { name: "Mailwerk Webseite" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Neu laden" })).toBeEnabled();
  });

  it("neue Strecke anlegen – oben und bei leerer Liste", async () => {
    rustAttrappe({ strecken_liste: () => [], konfig_ort: () => ORT });
    const neu = vi.fn();
    mitQuery(<StreckenListe onNeu={neu} onAnzeigen={vi.fn()} />);
    await userEvent.click(await screen.findByRole("button", { name: "Erste Strecke anlegen" }));
    await userEvent.click(screen.getByRole("button", { name: "+ Neue Strecke" }));
    expect(neu).toHaveBeenCalledTimes(2);
  });

  it("kein „Bearbeiten“ auf der Startseite (erst Strecke wählen); defekte Dateien lassen sich nicht öffnen", async () => {
    const defekt: StreckeUebersicht = { id: "kaputt", name: null, stufen: [], probleme: [], fehler: "kaputt" };
    rustAttrappe({ strecken_liste: () => [defekt, mailwerk], konfig_ort: () => ORT });
    mitQuery(<StreckenListe onNeu={vi.fn()} onAnzeigen={vi.fn()} />);
    expect(await screen.findByRole("button", { name: "Mailwerk Webseite öffnen" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Bearbeiten" })).toBeNull();
    expect(screen.getByText(/von Hand korrigiert/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "kaputt öffnen" })).toBeNull();
  });

  it("Klick oder Tastatur auf die Karte öffnet die Strecke; keine Knöpfe „Vergleichen“ mehr", async () => {
    rustAttrappe({ strecken_liste: () => [mailwerk], konfig_ort: () => ORT });
    const anzeigen = vi.fn();
    mitQuery(<StreckenListe onNeu={vi.fn()} onAnzeigen={anzeigen} />);
    const karte = await screen.findByRole("button", { name: "Mailwerk Webseite öffnen" });
    expect(screen.queryByRole("button", { name: /vergleichen/ })).toBeNull();
    await userEvent.click(within(karte).getByText("Entwicklung → Spiegel"));
    expect(anzeigen).toHaveBeenCalledWith(mailwerk);
    karte.focus();
    await userEvent.keyboard("{Enter}");
    await userEvent.keyboard(" ");
    await userEvent.keyboard("a");
    expect(anzeigen).toHaveBeenCalledTimes(3);
  });

  it("Kreise der Stufen nach dem zuletzt bekannten Stand (v0.6.6)", async () => {
    const lauf = {
      id: "l", strecke_id: "mailwerk-web", stufe_id: "server", modus: "umschalten" as const, phase: "abgebrochen" as const, gestartet: 0, beendet: 1,
      zusammenfassung: { ordner_anlegen: 0, kopieren: 1, ueberschreiben: 0, rechte: 0, dateien_loeschen: 0, ordner_loeschen: 0, bytes: 1 },
      ergebnis: { erledigt: 1, fehler: [], uebersprungen: 0, abgebrochen: false, gestoppt: false, bytes: 1, journal_fehler: null, unterbrochen: { grund: "programm_beendet" as const, phase: "umgeschaltet" as const } },
    };
    const stand = { zeit: new Date(2026, 10, 8, 23, 40).getTime(), neu: 0, geaendert: 0, loeschen: 0, konflikt: 0 };
    const mit: StreckeUebersicht = {
      ...mailwerk,
      stufen: [
        { ...mailwerk.stufen[0], stand },
        { ...mailwerk.stufen[1], stand: { ...stand, neu: 3, geaendert: 1, loeschen: 2, konflikt: 1 } },
        { ...mailwerk.stufen[1], id: "rest", stand, letzter_lauf: lauf },
        { ...mailwerk.stufen[1], id: "neu" },
      ],
    };
    rustAttrappe({ strecken_liste: () => [mit], konfig_ort: () => ORT });
    mitQuery(<StreckenListe onNeu={vi.fn()} onAnzeigen={vi.fn()} />);
    const kreise = await screen.findAllByText(/^[1-4]$/, { selector: ".stufe-nr" });
    expect(kreise.map((k) => k.className)).toEqual(["stufe-nr gleich", "stufe-nr differenz", "stufe-nr fehler", "stufe-nr unbekannt"]);
    expect(kreise[0]).toHaveAttribute("title", "Verglichen am 08.11.2026, 23:40 · nichts offen");
    expect(kreise[1]).toHaveAttribute("title", "Verglichen am 08.11.2026, 23:40 · 3 neu, 1 geändert, 2 zu löschen, 1 Konflikt");
    expect(kreise[2].title).toMatch(/^Letzter Lauf unterbrochen/);
    expect(kreise[3].title).toMatch(/^Stand unbekannt/);
  });
});
