import { describe, expect, it } from "vitest";
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
    mitQuery(<StreckenListe />);
    expect(screen.getByText("Strecken werden geladen …")).toBeInTheDocument();
  });

  it("zeigt einen Fehler der Rust-Seite", async () => {
    rustAttrappe({
      strecken_liste: () => {
        throw { kategorie: "konfiguration", meldung: "Konfigurationsfehler in config.toml: kaputt" };
      },
      konfig_ort: () => ORT,
    });
    mitQuery(<StreckenListe />);
    expect(await screen.findByText("Konfigurationsfehler in config.toml: kaputt")).toHaveClass("fehler");
  });

  it("zeigt bei leerer Liste den Konfigurationsort", async () => {
    rustAttrappe({ strecken_liste: () => [], konfig_ort: () => ORT });
    mitQuery(<StreckenListe />);
    expect(await screen.findByText("Noch keine Strecke angelegt.")).toBeInTheDocument();
    expect(await screen.findByText(`${ORT}/strecken/`)).toBeInTheDocument();
  });

  it("zeigt Platzhalter, solange der Konfigurationsort fehlt", async () => {
    rustAttrappe({ strecken_liste: () => [], konfig_ort: () => new Promise(() => {}) });
    mitQuery(<StreckenListe />);
    expect(await screen.findByText("…/strecken/")).toBeInTheDocument();
  });

  it("zeigt Strecken als Karten mit nummerierten Stufen", async () => {
    rustAttrappe({ strecken_liste: () => [mailwerk], konfig_ort: () => ORT });
    mitQuery(<StreckenListe />);

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
    mitQuery(<StreckenListe />);

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
    mitQuery(<StreckenListe />);

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
    mitQuery(<StreckenListe />);
    const knopf = await screen.findByRole("button", { name: "Neu laden" });
    expect(knopf).toBeEnabled();

    await userEvent.click(knopf);
    await waitFor(() => expect(knopf).toBeDisabled());
    expect(anzahl).toBe(2);

    freigeben([mailwerk]);
    expect(await screen.findByRole("heading", { name: "Mailwerk Webseite" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Neu laden" })).toBeEnabled();
  });
});
