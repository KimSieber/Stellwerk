import { describe, expect, it, vi } from "vitest";
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Problem, Strecke } from "../api";
import { aufrufeVon, beispielStrecke, mitQuery, rustAttrappe, type Antworten } from "../test/hilfen";
import { ablageSenden } from "../test/ablage";
import { StreckeEditor } from "./StreckeEditor";

type Args = { strecke: Strecke };

/** Prüfung wie in Rust, vereinfacht: Name fehlt → Fehler, keine Stufe → Warnung. */
function pruefen(args: unknown): Problem[] {
  const s = (args as Args).strecke;
  const p: Problem[] = [];
  if (s.name.trim() === "") p.push({ schwere: "fehler", ort: "name", meldung: "Name fehlt" });
  if (s.stufen.length === 0) p.push({ schwere: "warnung", ort: "stufen", meldung: "Strecke hat noch keine Stufe" });
  return p;
}

function rust(extra: Antworten = {}) {
  return rustAttrappe({
    strecke_pruefen: pruefen,
    strecken_liste: () => [],
    strecke_laden: () => beispielStrecke(),
    strecke_anlegen: () => [],
    strecke_speichern: () => [],
    strecke_loeschen: () => null,
    passwort_vorhanden: () => false,
    passwort_speichern: () => null,
    globale_regeln: () => [
      { muster: ".DS_Store", typ: "ignorieren" },
      { muster: ".git/", typ: "ignorieren" },
    ],
    ...extra,
  });
}

function editor(id?: string) {
  const onSchliessen = vi.fn();
  const onAngelegt = vi.fn();
  mitQuery(<StreckeEditor id={id} onSchliessen={onSchliessen} onAngelegt={onAngelegt} />);
  return { onSchliessen, onAngelegt };
}

const speichernKnopf = () => screen.getByRole("button", { name: "Speichern" });

describe("Neue Strecke", () => {
  it("Kennung folgt dem Namen, bis sie von Hand geändert wird", async () => {
    rust();
    editor();
    expect(screen.getByRole("heading", { name: "Neue Strecke" })).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Name"), "Mailwerk Web");
    expect(screen.getByLabelText("Kennung")).toHaveValue("mailwerk-web");
    expect(screen.getByLabelText("Kennung")).not.toHaveAttribute("readonly");

    await userEvent.type(screen.getByLabelText("Kennung"), "2");
    await userEvent.type(screen.getByLabelText("Name"), "seite");
    expect(screen.getByLabelText("Kennung")).toHaveValue("mailwerk-web2");
  });

  it("Prüfmeldungen am Feld und oben; Speichern erst ohne Fehler", async () => {
    rust();
    editor();
    expect(await screen.findByText("1 Fehler, 0 Warnungen")).toBeInTheDocument();
    expect(screen.getByText("Name:")).toBeInTheDocument(); // lesbarer Ort in der Übersicht
    expect(screen.getAllByText("Name fehlt")).toHaveLength(2); // am Feld und in der Übersicht
    expect(speichernKnopf()).toBeDisabled();

    await userEvent.type(screen.getByLabelText("Name"), "A");
    await waitFor(() => expect(screen.queryByText("Name fehlt")).toBeNull());
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
  });

  it("legt an und meldet die Kennung", async () => {
    const a = rust();
    const { onAngelegt } = editor();
    await userEvent.type(screen.getByLabelText("Name"), "Test");
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
    await userEvent.click(speichernKnopf());
    await waitFor(() => expect(onAngelegt).toHaveBeenCalledWith("test"));
    const gesendet = (aufrufeVon(a, "strecke_anlegen")[0].args as Args).strecke;
    expect(gesendet).toMatchObject({ id: "test", name: "Test", version: 1 });
    expect(gesendet.stufen).toHaveLength(1);
    expect(aufrufeVon(a, "strecke_speichern")).toHaveLength(0);
  });

  it("vergebene Kennung wird sofort gemeldet", async () => {
    rust({ strecken_liste: () => [{ id: "test", name: "Test", stufen: [], probleme: [], fehler: null }] });
    editor();
    await userEvent.type(screen.getByLabelText("Name"), "Test");
    expect(await screen.findAllByText("Kennung ist bereits vergeben")).toHaveLength(2);
    expect(speichernKnopf()).toBeDisabled();
  });

  it("solange die Liste noch lädt, gilt die Kennung als frei (Rust prüft beim Anlegen)", async () => {
    rust({ strecken_liste: () => new Promise(() => {}) });
    editor();
    await userEvent.type(screen.getByLabelText("Name"), "Test");
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
    expect(screen.queryByText("Kennung ist bereits vergeben")).toBeNull();
  });

  it("Meldungen stehen im festen Kopfbereich bei den Knöpfen", async () => {
    rust({
      strecke_anlegen: () => {
        throw { kategorie: "validierung", meldung: "Konfiguration ungültig: Ordner existiert nicht" };
      },
    });
    editor();
    await userEvent.type(screen.getByLabelText("Name"), "Test");
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
    await userEvent.click(speichernKnopf());
    const meldung = await screen.findByText("Konfiguration ungültig: Ordner existiert nicht");
    expect(speichernKnopf().closest(".kopfbereich")).toContainElement(meldung);
    // Weitertippen nimmt die veraltete Fehlermeldung weg
    await userEvent.type(screen.getByLabelText("Name"), "x");
    await waitFor(() => expect(screen.queryByText(/Ordner existiert nicht/)).toBeNull());
  });

  it("Ablehnung durch Rust wird angezeigt", async () => {
    rust({
      strecke_anlegen: () => {
        throw { kategorie: "vorhanden", meldung: "Bereits vorhanden: Strecke 'test'" };
      },
    });
    const { onAngelegt } = editor();
    await userEvent.type(screen.getByLabelText("Name"), "Test");
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
    await userEvent.click(speichernKnopf());
    expect(await screen.findByText("Bereits vorhanden: Strecke 'test'")).toHaveClass("fehler");
    expect(onAngelegt).not.toHaveBeenCalled();
  });

  it("ungültige Eingabe, die Rust nicht einlesen kann", async () => {
    rust({
      strecke_pruefen: () => {
        throw "invalid args `strecke`: ungültiger Dateimodus '79x'";
      },
    });
    editor();
    expect(await screen.findByText(/Eingabe ungültig: invalid args/)).toBeInTheDocument();
    expect(speichernKnopf()).toBeDisabled();
  });

  it("Stufen hinzufügen, bearbeiten, entfernen; Regeln der Strecke", async () => {
    const a = rust();
    editor();
    await userEvent.type(screen.getByLabelText("Name"), "S");
    await userEvent.click(screen.getByRole("button", { name: "+ Stufe" }));
    expect(screen.getByRole("region", { name: "Stufe 2" })).toBeInTheDocument();
    const namen = screen.getAllByLabelText("Name der Stufe");
    await userEvent.clear(namen[1]);
    await userEvent.type(namen[1], "Server");

    const plusRegel = screen.getAllByRole("button", { name: "+ Regel" })[0]; // Strecke
    await userEvent.click(plusRegel);

    await userEvent.click(screen.getAllByRole("button", { name: "Stufe entfernen" })[0]);
    expect(screen.queryByRole("region", { name: "Stufe 2" })).toBeNull();

    await waitFor(() => {
      const letzte = (aufrufeVon(a, "strecke_pruefen").at(-1)!.args as Args).strecke;
      expect(letzte.stufen.map((s) => s.name)).toEqual(["Server"]);
      expect(letzte.regeln!.eigene).toEqual([{ muster: "", typ: "ignorieren" }]);
    });
  });

  it("alle Stufen entfernen ergibt eine Warnung", async () => {
    rust();
    editor();
    await userEvent.type(screen.getByLabelText("Name"), "S");
    await userEvent.click(screen.getByRole("button", { name: "Stufe entfernen" }));
    expect(await screen.findByText("0 Fehler, 1 Warnungen")).toBeInTheDocument();
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
  });
});

describe("Passwörter", () => {
  it("werden nach dem Speichern der Strecke in den Schlüsselbund gelegt – nie in die Konfiguration", async () => {
    const a = rust();
    editor("mailwerk-web");
    await userEvent.type(await screen.findByLabelText("Passwort"), "geheim");
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
    await userEvent.click(speichernKnopf());
    expect(await screen.findByText("Gespeichert.")).toBeInTheDocument();

    const reihenfolge = a.map((x) => x.cmd).filter((c) => c === "strecke_speichern" || c === "passwort_speichern");
    expect(reihenfolge).toEqual(["strecke_speichern", "passwort_speichern"]);
    expect(aufrufeVon(a, "passwort_speichern")[0].args).toEqual({
      konto: "stellwerk:mailwerk-web:server",
      passwort: "geheim",
    });
    expect(JSON.stringify(aufrufeVon(a, "strecke_speichern")[0].args)).not.toContain("geheim");
    expect(screen.getByLabelText("Passwort")).toHaveValue(""); // nach dem Speichern geleert
  });

  it("ohne Eingabe wird kein Passwort gespeichert", async () => {
    const a = rust();
    editor("mailwerk-web");
    await userEvent.type(await screen.findByLabelText("Name"), "!");
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
    await userEvent.click(speichernKnopf());
    await screen.findByText("Gespeichert.");
    expect(aufrufeVon(a, "passwort_speichern")).toHaveLength(0);
  });

  it("Passwort-Eingabe zählt als ungespeicherte Änderung", async () => {
    rust();
    const { onSchliessen } = editor("mailwerk-web");
    await userEvent.type(await screen.findByLabelText("Passwort"), "x");
    await userEvent.click(screen.getByRole("button", { name: "← Strecken" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Ungespeicherte Änderungen verwerfen?");
    expect(onSchliessen).not.toHaveBeenCalled();
  });

  it("falscher interner Name (z. B. Passwort im alten Feld) wird beim Speichern korrigiert", async () => {
    const alt = beispielStrecke();
    (alt.stufen[1].ziel as { konto_ref: string }).konto_ref = "Untv8-geheim";
    const a = rust({ strecke_laden: () => alt });
    editor("mailwerk-web");
    await userEvent.type(await screen.findByLabelText("Name"), "!");
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
    await userEvent.click(speichernKnopf());
    await screen.findByText("Gespeichert.");
    const gesendet = (aufrufeVon(a, "strecke_speichern")[0].args as Args).strecke;
    expect(gesendet.stufen[1].ziel).toMatchObject({ konto_ref: "stellwerk:mailwerk-web:server" });
    expect(JSON.stringify(gesendet)).not.toContain("Untv8");
  });

  it("Passwörter wandern beim Entfernen und Hinzufügen von Stufen mit", async () => {
    const a = rust();
    editor("mailwerk-web");
    await userEvent.type(await screen.findByLabelText("Passwort"), "geheim");
    await userEvent.click(screen.getByRole("button", { name: "+ Stufe" }));
    await userEvent.click(screen.getAllByRole("button", { name: "Stufe entfernen" })[0]);
    // Die FTPS-Stufe ist jetzt Stufe 1, ihr Passwort ist erhalten
    expect(screen.getByLabelText("Passwort")).toHaveValue("geheim");
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
    await userEvent.click(speichernKnopf());
    await screen.findByText("Gespeichert.");
    expect(aufrufeVon(a, "passwort_speichern")).toHaveLength(1);
  });

  it("Passwort eines Servers als Quelle kommt unter eigenem Namen in den Schlüsselbund (v0.6.5)", async () => {
    const mit = beispielStrecke();
    mit.stufen[0].quelle = { typ: "ftps", host: "h", port: 21, pfad: "/logs", benutzer: "b", tls: "explizit", konto_ref: "alt" };
    const a = rust({ strecke_laden: () => mit });
    editor("mailwerk-web");
    const [quelle] = await screen.findAllByLabelText("Passwort");
    await userEvent.type(quelle, "q-geheim");
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
    await userEvent.click(speichernKnopf());
    await screen.findByText("Gespeichert.");
    expect(aufrufeVon(a, "passwort_speichern").map((x) => x.args)).toEqual([{ konto: "stellwerk:mailwerk-web:spiegel:quelle", passwort: "q-geheim" }]);
    const gesendet = (aufrufeVon(a, "strecke_speichern")[0].args as Args).strecke;
    expect(gesendet.stufen[0].quelle).toMatchObject({ konto_ref: "stellwerk:mailwerk-web:spiegel:quelle" });
    expect(screen.getAllByLabelText("Passwort")[0]).toHaveValue("");
  });

  it("Fehler beim Schlüsselbund wird angezeigt", async () => {
    rust({
      passwort_speichern: () => {
        throw { kategorie: "schluesselbund", meldung: "Schlüsselbund: Zugriff verweigert" };
      },
    });
    editor("mailwerk-web");
    await userEvent.type(await screen.findByLabelText("Passwort"), "geheim");
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
    await userEvent.click(speichernKnopf());
    expect(await screen.findByText("Schlüsselbund: Zugriff verweigert")).toBeInTheDocument();
  });
});

describe("Übernommene Regeln", () => {
  it("auf Strecke und Stufe abschalten – landet in standard_aus", async () => {
    const a = rust();
    editor("mailwerk-web");
    await screen.findByRole("heading", { name: "Mailwerk Webseite" });
    // Beispiel: Strecke schaltet .git/ ab, hat eigene Regel *.psd
    expect(await screen.findByText("Standardregeln (global) – 1 von 2 wirksam")).toBeInTheDocument();
    const stufen = screen.getAllByText(/^Übernommene Regeln \(global und Strecke\)/);
    expect(stufen[0]).toHaveTextContent("2 von 2 wirksam"); // .DS_Store und *.psd

    const dsStore = screen.getAllByRole("checkbox", { name: ".DS_Store" });
    await userEvent.click(dsStore[0]); // Strecke
    await userEvent.click(screen.getAllByRole("checkbox", { name: "*.psd" })[1]); // Stufe 2

    await waitFor(() => {
      const letzte = (aufrufeVon(a, "strecke_pruefen").at(-1)!.args as Args).strecke;
      expect(letzte.regeln!.standard_aus).toEqual([".git/", ".DS_Store"]);
      expect(letzte.stufen[1].regeln!.standard_aus).toEqual(["*.psd"]);
    });
    // Auf Strecke abgeschaltet → bei den Stufen nicht mehr aufgeführt
    expect(screen.getAllByRole("checkbox", { name: ".DS_Store" })).toHaveLength(1);
  });

  it("solange die globalen Regeln laden, nur die Strecken-Regeln", async () => {
    rust({ globale_regeln: () => new Promise(() => {}) });
    editor("mailwerk-web");
    expect(await screen.findByText("Standardregeln (global) – 0 von 0 wirksam")).toBeInTheDocument();
  });
});

describe("Ordner aus dem Finder ablegen", () => {
  /** Lässt elementFromPoint immer das Ordnerfeld mit dem angegebenen Ort treffen. */
  function ablegen(ort: string, pfade: string[]) {
    const feld = document.querySelector(`[data-drop-target="${ort}"]`)!;
    document.elementFromPoint = (() => feld) as typeof document.elementFromPoint;
    act(() => ablageSenden({ type: "enter", paths: pfade, position: { x: 1, y: 1 } as never }));
    act(() => ablageSenden({ type: "drop", paths: pfade, position: { x: 1, y: 1 } as never }));
  }

  it("übernimmt einen Ordner als Pfad", async () => {
    const a = rust({ ist_ordner: () => true });
    editor("mailwerk-web");
    await screen.findByRole("heading", { name: "Mailwerk Webseite" });
    ablegen("stufen[0].ziel.pfad", ["/Users/kim/Spiegel"]);
    await waitFor(() => expect(screen.getAllByLabelText("Ordner")[1]).toHaveValue("/Users/kim/Spiegel"));
    expect(aufrufeVon(a, "ist_ordner")[0].args).toEqual({ pfad: "/Users/kim/Spiegel" });
    // gilt als Änderung
    await userEvent.click(screen.getByRole("button", { name: "← Strecken" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Ungespeicherte Änderungen");
  });

  it("weist Dateien ab – Meldung rot direkt unter dem Feld", async () => {
    rust({ ist_ordner: () => false });
    editor("mailwerk-web");
    await screen.findByRole("heading", { name: "Mailwerk Webseite" });
    ablegen("stufen[0].quelle.pfad", ["/Users/kim/notiz.txt"]);
    const meldung = await screen.findByText("„/Users/kim/notiz.txt“ ist kein Ordner – bitte einen Ordner ablegen.");
    expect(meldung).toHaveClass("feld-meldung", "fehler");
    const feld = screen.getAllByLabelText("Ordner")[0];
    expect(feld.closest(".feld")).toContainElement(meldung);
    expect(feld).toHaveValue("/Users/kim/Dev/Mailwerk");

    // Nächste Eingabe nimmt die Meldung weg
    await userEvent.type(screen.getByLabelText("Name"), "!");
    expect(screen.queryByText(/ist kein Ordner/)).toBeNull();
  });

  it("bei mehreren Objekten nur das erste – Hinweis am Feld", async () => {
    rust({ ist_ordner: () => true });
    editor("mailwerk-web");
    await screen.findByRole("heading", { name: "Mailwerk Webseite" });
    ablegen("stufen[0].ziel.pfad", ["/a", "/b"]);
    const hinweis = await screen.findByText("Mehrere Objekte abgelegt – übernommen wurde nur das erste.");
    expect(hinweis).toHaveClass("feld-meldung", "warnung");
    const feld = screen.getAllByLabelText("Ordner")[1];
    expect(feld.closest(".feld")).toContainElement(hinweis);
    expect(feld).toHaveValue("/a");
  });
});

describe("Vorhandene Strecke", () => {
  it("lädt, Kennung bleibt fest, Speichern überschreibt", async () => {
    const a = rust();
    editor("mailwerk-web");
    expect(screen.getByText("Strecke wird geladen …")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Mailwerk Webseite" })).toBeInTheDocument();
    expect(screen.getByLabelText("Kennung")).toHaveAttribute("readonly");
    expect(screen.getByText("Die Kennung ist der Dateiname und bleibt fest.")).toBeInTheDocument();
    expect(screen.getAllByRole("region")).toHaveLength(2);

    await userEvent.type(screen.getByLabelText("Name"), " neu");
    expect(screen.getByLabelText("Kennung")).toHaveValue("mailwerk-web");
    await waitFor(() => expect(speichernKnopf()).toBeEnabled());
    await userEvent.click(speichernKnopf());
    expect(await screen.findByText("Gespeichert.")).toBeInTheDocument();
    expect(speichernKnopf().closest(".kopfbereich")).toContainElement(screen.getByText("Gespeichert."));
    const gesendet = (aufrufeVon(a, "strecke_speichern")[0].args as Args).strecke;
    expect(gesendet.name).toBe("Mailwerk Webseite neu");
    expect(aufrufeVon(a, "strecke_anlegen")).toHaveLength(0);
    expect(aufrufeVon(a, "strecken_liste")).toHaveLength(0); // nur bei neuen Strecken nötig

    // Nach dem Speichern führt „Zurück“ ohne Rückfrage
    await userEvent.type(screen.getByLabelText("Name"), "!");
    expect(screen.queryByText("Gespeichert.")).toBeNull();
  });

  it("Titel zeigt die Kennung, solange der Name leer ist", async () => {
    rust({ strecke_laden: () => ({ ...beispielStrecke(), name: "" }) });
    editor("mailwerk-web");
    expect(await screen.findByRole("heading", { name: "mailwerk-web" })).toBeInTheDocument();
  });

  it("Ladefehler mit Weg zurück", async () => {
    rust({
      strecke_laden: () => {
        throw { kategorie: "konfiguration", meldung: "Konfigurationsfehler in x.toml" };
      },
    });
    const { onSchliessen } = editor("x");
    expect(await screen.findByText("Konfigurationsfehler in x.toml")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "← Strecken" }));
    expect(onSchliessen).toHaveBeenCalled();
  });

  it("Zurück: ohne Änderung sofort, mit Änderung nach Rückfrage", async () => {
    rust();
    const { onSchliessen } = editor("mailwerk-web");
    await screen.findByRole("heading", { name: "Mailwerk Webseite" });
    await userEvent.click(screen.getByRole("button", { name: "← Strecken" }));
    expect(onSchliessen).toHaveBeenCalledTimes(1);

    await userEvent.type(screen.getByLabelText("Name"), "x");
    await userEvent.click(screen.getByRole("button", { name: "← Strecken" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Ungespeicherte Änderungen verwerfen?");
    await userEvent.click(screen.getByRole("button", { name: "Weiter bearbeiten" }));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(onSchliessen).toHaveBeenCalledTimes(1);

    await userEvent.click(screen.getByRole("button", { name: "← Strecken" }));
    await userEvent.click(screen.getByRole("button", { name: "Verwerfen" }));
    expect(onSchliessen).toHaveBeenCalledTimes(2);
  });

  it("Löschen nur nach Bestätigung", async () => {
    const a = rust();
    const { onSchliessen } = editor("mailwerk-web");
    await userEvent.click(await screen.findByRole("button", { name: "Strecke löschen …" }));
    expect(screen.getByRole("alert")).toHaveTextContent("„Mailwerk Webseite“ wirklich löschen?");
    await userEvent.click(screen.getByRole("button", { name: "Abbrechen" }));
    expect(aufrufeVon(a, "strecke_loeschen")).toHaveLength(0);

    await userEvent.click(screen.getByRole("button", { name: "Strecke löschen …" }));
    await userEvent.click(screen.getByRole("button", { name: "Endgültig löschen" }));
    await waitFor(() => expect(onSchliessen).toHaveBeenCalled());
    expect(aufrufeVon(a, "strecke_loeschen")[0].args).toEqual({ id: "mailwerk-web" });
  });

  it("Löschfehler wird angezeigt", async () => {
    rust({
      strecke_loeschen: () => {
        throw { kategorie: "io", meldung: "Ein-/Ausgabefehler" };
      },
    });
    const { onSchliessen } = editor("mailwerk-web");
    await userEvent.click(await screen.findByRole("button", { name: "Strecke löschen …" }));
    await userEvent.click(screen.getByRole("button", { name: "Endgültig löschen" }));
    expect(await screen.findByText("Ein-/Ausgabefehler")).toBeInTheDocument();
    expect(onSchliessen).not.toHaveBeenCalled();
  });
});
