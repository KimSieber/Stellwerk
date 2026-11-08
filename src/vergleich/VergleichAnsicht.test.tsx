import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Zeile } from "../api";
import { aufrufeVon, rustAttrappe, type Antworten } from "../test/hilfen";
import { eintrag, kanalSenden, listenGroesse, seiteAus, start, summe, zeile } from "../test/vergleich";
import { VergleichAnsicht } from "./VergleichAnsicht";
import { SEITE } from "./useZeilen";

const ZEILEN: Zeile[] = [
  zeile("bilder", { ordner: true, kinder: true, quelle: eintrag("bilder", "ordner") }),
  zeile("bilder/a.png"),
  zeile("alt.txt", { status: "loeschen", quelle: null, ziel: eintrag("alt.txt") }),
  zeile("config.php", {
    status: "geaendert",
    quelle: eintrag("config.server.php"),
    ziel: eintrag("config.php"),
    regel: { typ: "ersetzen", herkunft: { ebene: "stufe", index: 0, muster: "config.php" }, ersatz: "config.server.php" },
  }),
  zeile("link", { status: "konflikt", quelle: eintrag("link", "symlink"), ausgewaehlt: false, auswaehlbar: false, warnungen: [{ art: "symlink", ziel: "x" }] }),
];

function rust(extra: Antworten = {}) {
  return rustAttrappe({
    vergleich_starten: () => start(),
    diff_seite: seiteAus(ZEILEN),
    auswahl_setzen: () => summe({ ausgewaehlt: 1, bytes_uebertragen: 1500 }),
    vergleich_abbrechen: () => false,
    vergleich_schliessen: () => null,
    ...extra,
  });
}

function ansicht(zurueck = vi.fn()) {
  return render(<VergleichAnsicht strecke="web" stufe="spiegel" titel="Web · Entwicklung → Spiegel" onZurueck={zurueck} />);
}

const zeileVon = (pfad: string) => document.querySelector(`[data-pfad="${pfad}"]`) as HTMLElement;

beforeEach(() => listenGroesse());
afterEach(() => vi.restoreAllMocks());

describe("Vergleichsansicht – Ablauf", () => {
  it("zeigt den Fortschritt und lässt sich abbrechen", async () => {
    let kanal: { id: number } = { id: 0 };
    const aufrufe = rust({
      vergleich_starten: (a) => {
        kanal = (a as { fortschritt: { id: number } }).fortschritt;
        return new Promise(() => {});
      },
    });
    ansicht();
    expect(screen.getByText("Vergleich wird vorbereitet …")).toBeInTheDocument();
    expect(aufrufeVon(aufrufe, "vergleich_starten")[0].args).toMatchObject({ strecke: "web", stufe: "spiegel" });
    act(() => kanalSenden(kanal, 0, { phase: "ziel", anzahl: 42 }));
    expect(screen.getByText("Ziel wird gelesen … 42")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Übertragen …" })).toBeDisabled();

    await userEvent.click(screen.getByRole("button", { name: "Abbrechen" }));
    expect(aufrufeVon(aufrufe, "vergleich_abbrechen")[0].args).toEqual({ strecke: "web", stufe: "spiegel" });
  });

  it("Abbruch und Fehler werden gemeldet; neu vergleichen startet erneut", async () => {
    let n = 0;
    const aufrufe = rust({
      vergleich_starten: () => {
        n += 1;
        if (n === 1) throw { kategorie: "abbruch", meldung: "Abgebrochen" };
        if (n === 2) throw { kategorie: "kein_verzeichnis", meldung: "Kein Verzeichnis: /weg" };
        return start();
      },
    });
    ansicht();
    expect(await screen.findByText("Vergleich abgebrochen.")).toHaveClass("hinweis");
    await userEvent.click(screen.getByRole("button", { name: "Neu vergleichen" }));
    expect(await screen.findByText("Kein Verzeichnis: /weg")).toHaveClass("fehler");
    await userEvent.click(screen.getByRole("button", { name: "Neu vergleichen" }));
    expect(await screen.findByText("/Users/kim/Dev/web")).toBeInTheDocument();
    expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(3);
  });

  it("Zurück nach dem Vergleich gibt den Stand zurück – freigeben ist Sache der Streckenansicht", async () => {
    const aufrufe = rust();
    const zurueck = vi.fn();
    ansicht(zurueck);
    await screen.findByText("/Users/kim/Dev/web");
    await userEvent.click(screen.getByRole("button", { name: "← Zurück" }));
    expect(zurueck).toHaveBeenCalledWith(start(), false);
    expect(aufrufeVon(aufrufe, "vergleich_schliessen")).toHaveLength(0);
    expect(aufrufeVon(aufrufe, "vergleich_abbrechen")).toHaveLength(0);
  });

  it("mit fertigem Ergebnis wird beim Öffnen nicht verglichen – auch nicht im StrictMode", async () => {
    const aufrufe = rust();
    const zurueck = vi.fn();
    const fertig = start({ vergleich_id: "web/spiegel#7", quelle: "/schon/da" });
    render(
      <StrictMode>
        <VergleichAnsicht strecke="web" stufe="spiegel" titel="Web" start={fertig} onZurueck={zurueck} />
      </StrictMode>,
    );
    expect(screen.getByText("/schon/da")).toBeInTheDocument();
    expect(await screen.findByRole("checkbox", { name: "bilder/a.png übertragen" })).toBeInTheDocument();
    expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(0);
    expect(aufrufeVon(aufrufe, "diff_seite")[0].args).toMatchObject({ vergleichId: "web/spiegel#7" });
    await userEvent.click(screen.getByRole("button", { name: "Neu vergleichen" }));
    await screen.findByText("/Users/kim/Dev/web");
    expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(1);
    await userEvent.click(screen.getByRole("button", { name: "← Zurück" }));
    expect(zurueck).toHaveBeenCalledWith(start(), false);
  });

  it("Zurück während des Vergleichs bricht ihn ab", async () => {
    const aufrufe = rust({ vergleich_starten: () => new Promise(() => {}) });
    const zurueck = vi.fn();
    ansicht(zurueck);
    await userEvent.click(screen.getByRole("button", { name: "← Zurück" }));
    expect(zurueck).toHaveBeenCalledWith(null, false);
    expect(aufrufeVon(aufrufe, "vergleich_abbrechen")).toEqual([
      { cmd: "vergleich_abbrechen", args: { strecke: "web", stufe: "spiegel" } },
    ]);
    expect(aufrufeVon(aufrufe, "vergleich_schliessen")).toHaveLength(0);
  });

  it("Zurück nach einem Fehler meldet nichts an Rust", async () => {
    const aufrufe = rust({
      vergleich_starten: () => {
        throw { kategorie: "io", meldung: "kaputt" };
      },
    });
    ansicht();
    await screen.findByText("kaputt");
    await userEvent.click(screen.getByRole("button", { name: "← Zurück" }));
    expect(aufrufeVon(aufrufe, "vergleich_abbrechen")).toHaveLength(0);
    expect(aufrufeVon(aufrufe, "vergleich_schliessen")).toHaveLength(0);
  });

  it("StrictMode (Entwicklungsmodus): der Vergleich bleibt nutzbar", async () => {
    // Nachbildung der Rust-Seite mit Zustand: Geschlossene Vergleiche sind weg,
    // ein neuer Vergleich ersetzt den alten derselben Stufe.
    let n = 0;
    const offen = new Set<string>();
    const seite = seiteAus(ZEILEN);
    rust({
      vergleich_starten: async () => {
        n += 1;
        const id = `web/spiegel#${n}`;
        offen.clear();
        offen.add(id);
        return start({ vergleich_id: id });
      },
      vergleich_schliessen: (a) => offen.delete((a as { vergleichId: string }).vergleichId),
      diff_seite: (a) => {
        if (!offen.has((a as { vergleichId: string }).vergleichId)) {
          throw { kategorie: "nicht_gefunden", meldung: "Vergleich – bitte neu vergleichen" };
        }
        return seite(a);
      },
    });
    render(
      <StrictMode>
        <VergleichAnsicht strecke="web" stufe="spiegel" titel="Web" onZurueck={vi.fn()} />
      </StrictMode>,
    );
    await screen.findByRole("checkbox", { name: "bilder/a.png übertragen" });
    await userEvent.click(screen.getByRole("button", { name: "Nur Änderungen" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Nur Änderungen" })).toHaveClass("aktiv"));
    await waitFor(() => expect(screen.getByText("5 Zeilen")).toBeInTheDocument());
    expect(screen.queryByText(/bitte neu vergleichen/)).toBeNull();
  });

  it("späte Ergebnisse nach „Zurück“ werden verworfen und freigegeben", async () => {
    let fertig: (v: unknown) => void = () => {};
    let kanal: { id: number } = { id: 0 };
    const aufrufe = rust({
      vergleich_starten: (a) => {
        kanal = (a as { fortschritt: { id: number } }).fortschritt;
        return new Promise((ok) => (fertig = ok));
      },
    });
    const zurueck = vi.fn();
    const { unmount } = ansicht(zurueck);
    await userEvent.click(screen.getByRole("button", { name: "← Zurück" }));
    unmount();
    kanalSenden(kanal, 0, { phase: "quelle", anzahl: 1 }); // kommt nach dem Verlassen – ohne Wirkung
    fertig(start({ vergleich_id: "spät" }));
    await waitFor(() => expect(aufrufeVon(aufrufe, "vergleich_schliessen")[0]?.args).toEqual({ vergleichId: "spät" }));
  });

  it("Fehler nach dem Verlassen bleiben folgenlos", async () => {
    let scheitern: (e: unknown) => void = () => {};
    const aufrufe = rust({ vergleich_starten: () => new Promise((_, nein) => (scheitern = nein)) });
    const { unmount } = ansicht();
    unmount();
    scheitern({ kategorie: "abbruch", meldung: "Abgebrochen" });
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.textContent).toBe("");
    expect(aufrufeVon(aufrufe, "vergleich_schliessen")).toHaveLength(0);
  });

  it("StrictMode: genau ein Auftrag je Öffnen – kein Wettlauf zweier Vergleiche", async () => {
    // Nachbildung von Rust: Ein neuer Auftrag derselben Stufe bricht den laufenden ab.
    const laufend: ((e: unknown) => void)[] = [];
    const aufrufe = rust({
      vergleich_starten: () =>
        new Promise((ok, nein) => {
          laufend.splice(0).forEach((abbrechen) => abbrechen({ kategorie: "abbruch", meldung: "Abgebrochen" }));
          laufend.push(nein);
          setTimeout(() => ok(start()), 5);
        }),
    });
    render(
      <StrictMode>
        <VergleichAnsicht strecke="web" stufe="spiegel" titel="Web" onZurueck={vi.fn()} />
      </StrictMode>,
    );
    expect(await screen.findByText("/Users/kim/Dev/web")).toBeInTheDocument();
    expect(screen.queryByText("Vergleich abgebrochen.")).toBeNull();
    expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(1);
    // „Neu vergleichen“ startet wieder genau einen Auftrag
    await userEvent.click(screen.getByRole("button", { name: "Neu vergleichen" }));
    await screen.findByText("/Users/kim/Dev/web");
    expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(2);
  });
});

describe("Vergleichsansicht – Ergebnis", () => {
  it("zeigt Wege, Summen und die Zeilen nebeneinander", async () => {
    rust({
      vergleich_starten: () =>
        start({ zusammenfassung: summe({ neu: 2, loeschen: 1, ausgewaehlt: 3, bytes_uebertragen: 3000, nur_online: 1, nur_online_bytes: 1500 }) }),
    });
    ansicht();
    const quellPfad = await screen.findByText("/Users/kim/Dev/web");
    expect(quellPfad).toHaveAttribute("title", "/Users/kim/Dev/web");
    expect(quellPfad.closest("fieldset")).toHaveTextContent("Quelle (wird nur gelesen)");
    expect(screen.getByText("/Users/kim/Server/web").closest("fieldset")).toHaveTextContent(/^Ziel/);
    expect(screen.getByText("löschen", { selector: ".summe" }).querySelector("b")).toHaveTextContent("1");
    expect(screen.getByText(/Ausgewählt:/)).toHaveTextContent(
      "Ausgewählt: 3 · 3 KB zu übertragen · davon 1 „nur online“ (1,5 KB), werden vorher geladen",
    );
    expect(await screen.findByText("5 Zeilen")).toBeInTheDocument();

    const bild = zeileVon("bilder/a.png");
    expect(bild).toHaveClass("status-neu");
    const [quelle, , ziel] = bild.querySelectorAll(".seite, .status");
    expect(quelle).toHaveTextContent("a.png");
    expect(quelle).toHaveTextContent("1,5 KB");
    expect(quelle).toHaveStyle({ paddingLeft: "24px" }); // 8 px Rand + 16 px je Ebene
    expect(within(bild.querySelector(".mitte")!).getByRole("checkbox")).toBeInTheDocument(); // Häkchen gehört zur Aktion
    const kopf = document.querySelector(".kopfzeile")!;
    expect([...kopf.querySelectorAll(".kopf-text")].map((k) => k.textContent)).toEqual(["Quelle", "Aktion", "Ziel"]);
    expect(ziel).toHaveClass("seite-leer");

    const alt = zeileVon("alt.txt");
    expect(alt.querySelector(".seite")).toHaveClass("seite-leer");
    expect(within(alt).getByText("löschen")).toBeInTheDocument();

    expect(zeileVon("config.php")).toHaveTextContent("config.php ← config.server.php");
    expect(zeileVon("config.php")).toHaveAttribute("title", expect.stringContaining("Ersetzen durch → config.server.php"));
    expect(zeileVon("bilder").querySelector(".symbol")).toHaveClass("lucide-folder-open");
    expect(zeileVon("bilder/a.png").querySelector(".symbol")).toHaveClass("lucide-file-image");
    expect(zeileVon("bilder").querySelector(".groesse")).toBeNull();
    expect(zeileVon("link").querySelector(".symbol")).toHaveClass("lucide-file-symlink");
    expect(within(zeileVon("link")).getByLabelText("Hinweis")).toHaveAttribute("title", expect.stringContaining("Symbolischer Link"));
    expect(within(zeileVon("link")).getByRole("checkbox")).toBeDisabled();
  });

  it("nicht lesbare Einträge werden aufgelistet", async () => {
    rust({
      vergleich_starten: () =>
        start({
          lesefehler: [
            { seite: "quelle", pfad: "/q/sock", meldung: "nicht unterstützter Dateityp" },
            { seite: "ziel", pfad: "/z/x", meldung: "kein Zugriff" },
          ],
        }),
    });
    ansicht();
    expect(await screen.findByText("2 Einträge nicht lesbar – nicht im Vergleich")).toBeInTheDocument();
    expect(screen.getByText("/q/sock").closest("li")).toHaveTextContent("Quelle: /q/sock – nicht unterstützter Dateityp");
    expect(screen.getByText("/z/x").closest("li")).toHaveTextContent("Ziel: /z/x – kein Zugriff");
  });

  it("Häkchen setzen: Rust rechnet, Zeilen werden neu geholt", async () => {
    const aufrufe = rust();
    ansicht();
    const haken = await screen.findByRole("checkbox", { name: "bilder/a.png übertragen" });
    const vorher = aufrufeVon(aufrufe, "diff_seite").length;
    await userEvent.click(haken);
    expect(aufrufeVon(aufrufe, "auswahl_setzen")[0].args).toEqual({ vergleichId: "web/spiegel#1", pfade: ["bilder/a.png"], an: false });
    await waitFor(() => expect(screen.getByText(/Ausgewählt:/)).toHaveTextContent("Ausgewählt: 1 · 1,5 KB"));
    expect(aufrufeVon(aufrufe, "diff_seite").length).toBeGreaterThan(vorher);
  });

  it("Fehler beim Setzen der Auswahl wird gezeigt und beim nächsten Erfolg entfernt", async () => {
    let n = 0;
    rust({
      auswahl_setzen: () => {
        n += 1;
        if (n === 1) throw { kategorie: "nicht_gefunden", meldung: "Vergleich – bitte neu vergleichen" };
        return summe();
      },
    });
    ansicht();
    const haken = await screen.findByRole("checkbox", { name: "bilder/a.png übertragen" });
    await userEvent.click(haken);
    expect(await screen.findByText("Vergleich – bitte neu vergleichen")).toHaveClass("fehler");
    await userEvent.click(haken);
    await waitFor(() => expect(screen.queryByText("Vergleich – bitte neu vergleichen")).toBeNull());
  });

  it("Ordner zu- und aufklappen", async () => {
    const aufrufe = rust();
    ansicht();
    await userEvent.click(await screen.findByRole("button", { name: "bilder zuklappen" }));
    await waitFor(() => expect(zeileVon("bilder/a.png")).toBeNull());
    expect(aufrufeVon(aufrufe, "diff_seite").at(-1)!.args).toMatchObject({ zugeklappt: ["bilder"] });
    const auf = await screen.findByRole("button", { name: "bilder aufklappen" });
    expect(auf.querySelector("svg")).not.toBeNull(); // Dreieck, kein Textzeichen
    expect(auf).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(auf);
    expect(await screen.findByRole("checkbox", { name: "bilder/a.png übertragen" })).toBeInTheDocument();
    expect(aufrufeVon(aufrufe, "diff_seite").at(-1)!.args).toMatchObject({ zugeklappt: [] });
  });

  it("startet mit „Alle“; Filter wechseln; leere Ergebnisse je Filter", async () => {
    const aufrufe = rust({ diff_seite: () => ({ zeilen: [], sichtbar: 0 }) });
    ansicht();
    expect(await screen.findByText("Keine Einträge.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Alle" })).toHaveAttribute("aria-pressed", "true");
    expect(aufrufeVon(aufrufe, "diff_seite")[0].args).toMatchObject({ filter: "alle" });
    const knoepfe = within(screen.getByRole("group", { name: "Filter" })).getAllByRole("button");
    expect(knoepfe.map((k) => k.textContent)).toEqual(["Alle", "Nur Änderungen", "Warnungen"]);
    await userEvent.click(screen.getByRole("button", { name: "Nur Änderungen" }));
    expect(await screen.findByText("Keine Änderungen – Quelle und Ziel stimmen überein.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Nur Änderungen" })).toHaveClass("aktiv");
    await userEvent.click(screen.getByRole("button", { name: "Warnungen" }));
    await waitFor(() => expect(aufrufeVon(aufrufe, "diff_seite").at(-1)!.args).toMatchObject({ filter: "warnungen" }));
  });

  it("Zeilenzahl in Einzahl und Mehrzahl", async () => {
    rust({ diff_seite: seiteAus([zeile("a.txt")]) });
    ansicht();
    expect(await screen.findByText("1 Zeile")).toBeInTheDocument();
  });

  it("Fehler beim Laden der Zeilen wird gezeigt", async () => {
    rust({
      diff_seite: () => {
        throw { kategorie: "nicht_gefunden", meldung: "Vergleich „x“ – bitte neu vergleichen" };
      },
    });
    ansicht();
    expect(await screen.findByText("Vergleich „x“ – bitte neu vergleichen")).toHaveClass("fehler");
    expect(screen.getByText("… Zeilen")).toBeInTheDocument();
  });
});

describe("Vergleichsansicht – große Listen", () => {
  const viele = Array.from({ length: 450 }, (_, i) => zeile(`datei-${String(i).padStart(3, "0")}.txt`));

  it("lädt weitere Seiten erst bei Bedarf und zeigt bis dahin Platzhalter", async () => {
    listenGroesse(SEITE * 28 + 400); // Sichtfenster reicht über die erste Seite hinaus
    let freigeben: () => void = () => {};
    const antwort = seiteAus(viele);
    const aufrufe = rust({
      diff_seite: (a) => {
        const { offset } = a as { offset: number };
        if (offset === 0) return antwort(a);
        return new Promise((ok) => (freigeben = () => ok(antwort(a))));
      },
    });
    ansicht();
    expect(await screen.findByText("450 Zeilen")).toBeInTheDocument();
    await waitFor(() => expect(document.querySelector(".vergleich-zeile.laedt")).not.toBeNull());
    // Seite 2 wird genau einmal angefordert, auch wenn mehrfach gezeichnet wird
    expect(aufrufeVon(aufrufe, "diff_seite").filter((x) => (x.args as { offset: number }).offset === SEITE)).toHaveLength(1);
    act(() => freigeben());
    expect(await screen.findByRole("checkbox", { name: "datei-200.txt übertragen" })).toBeInTheDocument();
    expect(document.querySelector(".vergleich-zeile.laedt")).toBeNull();
  });

  it("veraltete Antworten werden verworfen, bis dahin bleiben die alten Zeilen stehen", async () => {
    let freigeben: () => void = () => {};
    const antwort = seiteAus(ZEILEN);
    let langsam = false;
    rust({
      diff_seite: (a) => {
        if (!langsam) return antwort(a);
        return new Promise((ok) => (freigeben = () => ok({ zeilen: [zeile("veraltet")], sichtbar: 1 })));
      },
    });
    ansicht();
    await screen.findByRole("checkbox", { name: "bilder/a.png übertragen" });
    langsam = true;
    await userEvent.click(screen.getByRole("button", { name: "Nur Änderungen" }));
    // alte Zeilen bleiben stehen, solange die neue Antwort aussteht
    expect(screen.getByRole("checkbox", { name: "bilder/a.png übertragen" })).toBeInTheDocument();
    const alteFreigabe = freigeben;
    langsam = false;
    await userEvent.click(screen.getByRole("button", { name: "Alle" }));
    act(() => alteFreigabe());
    await new Promise((r) => setTimeout(r, 0));
    expect(zeileVon("veraltet")).toBeNull();
  });
});

describe("Vergleichsansicht – Regel setzen per Rechtsklick", () => {
  const rechtsklick = async (pfad: string) => {
    const el = await waitFor(() => {
      expect(zeileVon(pfad)).not.toBeNull();
      return zeileVon(pfad);
    });
    fireEvent.contextMenu(el, { clientX: 30, clientY: 40 });
    return screen.getByRole("menu");
  };
  const stufeOrt = { ebene: "stufe", stufe: "spiegel" };

  it("Regel setzen speichert und vergleicht neu – Filter bleibt erhalten", async () => {
    const aufrufe = rust({ regel_setzen: () => [] });
    ansicht();
    await screen.findByRole("checkbox", { name: "bilder/a.png übertragen" });
    await userEvent.click(screen.getByRole("button", { name: "Nur Änderungen" }));
    const menue = await rechtsklick("alt.txt");
    await userEvent.click(within(within(menue).getByRole("group", { name: "Für diese Stufe" })).getByRole("menuitem", { name: "Schützen" }));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(aufrufeVon(aufrufe, "regel_setzen")[0].args).toEqual({
      strecke: "web", ort: stufeOrt, pfad: "alt.txt", ordner: false, typ: "schuetzen", ersatz: null,
    });
    await waitFor(() => expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(2));
    await screen.findByRole("checkbox", { name: "bilder/a.png übertragen" });
    expect(screen.getByRole("button", { name: "Nur Änderungen" })).toHaveAttribute("aria-pressed", "true");
    expect(aufrufeVon(aufrufe, "diff_seite").at(-1)!.args).toMatchObject({ filter: "aenderungen" });
  });

  it("Regel entfernen (Ebene der vorhandenen Regel); Ordner für die ganze Strecke", async () => {
    const zeilen = [
      ...ZEILEN.slice(0, 2),
      zeile("robots.txt", {
        regel: { typ: "schuetzen", herkunft: { ebene: "strecke", index: 0, muster: "/robots.txt" }, ersatz: null },
        regel_entfernbar: true,
      }),
    ];
    const aufrufe = rust({ diff_seite: seiteAus(zeilen), regel_entfernen: () => [], regel_setzen: () => [] });
    ansicht();
    await userEvent.click(within(await rechtsklick("robots.txt")).getByRole("menuitem", { name: "Regel entfernen (Strecke)" }));
    expect(aufrufeVon(aufrufe, "regel_entfernen")[0].args).toEqual({
      strecke: "web", ort: { ebene: "strecke" }, pfad: "robots.txt", ordner: false,
    });
    await waitFor(() => expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(2));

    const menue = await rechtsklick("bilder");
    await userEvent.click(within(within(menue).getByRole("group", { name: "Für alle Stufen der Strecke" })).getByRole("menuitem", { name: "Ignorieren" }));
    expect(aufrufeVon(aufrufe, "regel_setzen")[0].args).toMatchObject({ ort: { ebene: "strecke" }, pfad: "bilder", ordner: true });
  });

  it("„Ersetzen durch …“ mit Vorschlägen aus dem Ordner der Quelle", async () => {
    const aufrufe = rust({ ersatz_vorschlaege: () => ["config.server.php"], regel_setzen: () => [] });
    ansicht();
    const menue = await rechtsklick("bilder/a.png");
    await userEvent.click(within(within(menue).getByRole("group", { name: "Für diese Stufe" })).getByRole("menuitem", { name: "Ersetzen durch …" }));
    expect(aufrufeVon(aufrufe, "ersatz_vorschlaege")[0].args).toEqual({ vergleichId: "web/spiegel#1", pfad: "bilder/a.png" });
    // erst abbrechen – nichts passiert
    await userEvent.click(await screen.findByRole("button", { name: "Abbrechen" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(aufrufeVon(aufrufe, "regel_setzen")).toHaveLength(0);
    // dann übernehmen
    await userEvent.click(within(await rechtsklick("bilder/a.png")).getAllByRole("menuitem", { name: "Ersetzen durch …" })[1]);
    expect(await screen.findByLabelText("Ersatzdatei (Pfad in der Quelle)")).toHaveValue("config.server.php");
    await userEvent.click(screen.getByRole("button", { name: "Übernehmen" }));
    expect(aufrufeVon(aufrufe, "regel_setzen")[0].args).toEqual({
      strecke: "web", ort: { ebene: "strecke" }, pfad: "bilder/a.png", ordner: false, typ: "ersetzen", ersatz: "config.server.php",
    });
    await waitFor(() => expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(2));
  });

  it("Fehler werden gezeigt und nach dem nächsten Erfolg entfernt; ohne Erfolg kein Neu-Vergleich", async () => {
    let n = 0;
    const aufrufe = rust({
      regel_setzen: () => {
        n += 1;
        if (n === 1) throw { kategorie: "validierung", meldung: "Ungültige Konfiguration", probleme: [] };
        return [];
      },
      ersatz_vorschlaege: () => {
        throw { kategorie: "nicht_gefunden", meldung: "Vergleich weg" };
      },
    });
    ansicht();
    const klick = async (name: string) =>
      userEvent.click(within(within(await rechtsklick("alt.txt")).getByRole("group", { name: "Für diese Stufe" })).getByRole("menuitem", { name }));
    await klick("Ignorieren");
    expect(await screen.findByText(/^Regel nicht gespeichert: /)).toHaveClass("fehler");
    expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(1);
    await klick("Ersetzen durch …");
    expect(await screen.findByText("Regel nicht gespeichert: Vergleich weg")).toBeInTheDocument();
    await klick("Ignorieren");
    await waitFor(() => expect(screen.queryByText(/Regel nicht gespeichert/)).toBeNull());
    await waitFor(() => expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(2));
  });
});

describe("Vergleichsansicht – Übertragen", () => {
  const vorschau = {
    zusammenfassung: { ordner_anlegen: 0, kopieren: 1, ueberschreiben: 0, rechte: 0, dateien_loeschen: 0, ordner_loeschen: 0, bytes: 1500 },
    loeschliste: [],
    warnungen: [],
    quelle: "/q",
    ziel: "/z",
    worker: 4,
    modus: "direkt" as const,
  };
  const ergebnis = { erledigt: 1, fehler: [], uebersprungen: 0, abgebrochen: false, gestoppt: false, bytes: 1500, journal_fehler: null };

  it("öffnet den Dialog erst nach dem Vergleich; Abbrechen schließt ohne Übertragung", async () => {
    const aufrufe = rust({ plan_erstellen: () => vorschau });
    ansicht();
    expect(screen.getByRole("button", { name: "Übertragen …" })).toBeDisabled();
    await screen.findByText("/Users/kim/Dev/web");
    await userEvent.click(screen.getByRole("button", { name: "Übertragen …" }));
    const d = await screen.findByRole("dialog", { name: "Übertragen" });
    expect(aufrufeVon(aufrufe, "plan_erstellen")[0].args).toEqual({ vergleichId: "web/spiegel#1" });
    await within(d).findByText("1 Datei neu");
    await userEvent.click(within(d).getByRole("button", { name: "Abbrechen" }));
    expect(screen.queryByRole("dialog", { name: "Übertragen" })).toBeNull();
    expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(1);
  });

  it("nach der Übertragung wird automatisch neu verglichen; Zurück meldet die Übertragung", async () => {
    const aufrufe = rust({ plan_erstellen: () => vorschau, lauf_starten: () => ({ lauf_id: "l1", ergebnis, phase: "fertig" }) });
    const zurueck = vi.fn();
    ansicht(zurueck);
    await screen.findByText("/Users/kim/Dev/web");
    await userEvent.click(screen.getByRole("button", { name: "Übertragen …" }));
    await userEvent.click(await screen.findByRole("button", { name: "Übertragen" }));
    await userEvent.click(await screen.findByRole("button", { name: "Schließen und neu vergleichen" }));
    expect(screen.queryByRole("dialog", { name: "Übertragen" })).toBeNull();
    await waitFor(() => expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(2));
    await screen.findByText("/Users/kim/Dev/web");
    await userEvent.click(screen.getByRole("button", { name: "← Zurück" }));
    expect(zurueck).toHaveBeenCalledWith(start(), true);
  });
  it("Umschalten: der ganze Ablauf im Dialog, danach wird neu verglichen", async () => {
    const aufrufe = rust({
      plan_erstellen: () => ({ ...vorschau, modus: "umschalten" }),
      lauf_starten: () => ({ lauf_id: "l1", ergebnis, phase: "hochgeladen" }),
      lauf_weiter: (a) => ({ lauf_id: "l1", phase: (a as { schritt: string }).schritt === "umschalten" ? "umgeschaltet" : "fertig", fehler: [] }),
    });
    ansicht(vi.fn());
    await screen.findByText("/Users/kim/Dev/web");
    await userEvent.click(screen.getByRole("button", { name: "Übertragen …" }));
    await userEvent.click(await screen.findByRole("button", { name: "Übertragen" }));
    await userEvent.click(await screen.findByRole("button", { name: "Fertig – neu vergleichen" }));
    expect(screen.queryByRole("dialog", { name: "Übertragen" })).toBeNull();
    await waitFor(() => expect(aufrufeVon(aufrufe, "vergleich_starten")).toHaveLength(2));
  });
});

describe("Vergleichsansicht – FTPS-Ziel", () => {
  const ftps = (ziel_stand: "manifest" | "server") => start({ ziel: "ftps://ftp.example.de:21/web/", ziel_stand });
  const server = (aufrufe: ReturnType<typeof rust>) =>
    aufrufeVon(aufrufe, "vergleich_starten").map((a) => (a.args as { server: boolean }).server);

  it("Inhalte prüfen: nur bei unbekanntem Inhalt; danach Meldung und neu verglichen ohne Server", async () => {
    let n = 0;
    const aufrufe = rust({
      vergleich_starten: () => {
        n += 1;
        return n === 1 ? start({ ...ftps("server"), ungeprueft: { anzahl: 2, bytes: 6 } }) : ftps("manifest");
      },
      inhalte_pruefen: () => ({ geprueft: 2, fehler: [] }),
    });
    ansicht();
    await screen.findByRole("region", { name: "Inhalte prüfen" });
    await userEvent.click(screen.getByRole("button", { name: "Inhalte prüfen …" }));
    await userEvent.click(screen.getByRole("button", { name: "Herunterladen und prüfen" }));
    expect(await screen.findByText("Inhalt von 2 Dateien geprüft")).toHaveClass("pruef-meldung");
    await waitFor(() => expect(server(aufrufe)).toEqual([false, false]));
    await screen.findByText(/letzter bekannter Stand/);
    expect(screen.queryByRole("region", { name: "Inhalte prüfen" })).toBeNull();
    // Der nächste Vergleich per Knopf nimmt die Meldung weg
    await userEvent.click(screen.getByRole("button", { name: "Schnell vergleichen" }));
    expect(screen.queryByText("Inhalt von 2 Dateien geprüft")).toBeNull();
    await screen.findByText(/letzter bekannter Stand/);
    await userEvent.click(screen.getByRole("button", { name: "Mit Server vergleichen" }));
    await waitFor(() => expect(server(aufrufe)).toEqual([false, false, false, true]));
  });

  it("noch nie gelesen: „Mit Server vergleichen …“ liest den Server", async () => {
    const aufrufe = rust({
      vergleich_starten: (a) => {
        if (!(a as { server: boolean }).server) throw { kategorie: "kein_manifest", meldung: "x" };
        return ftps("server");
      },
    });
    ansicht();
    expect(await screen.findByText("Noch nie mit dem Server verglichen.")).toHaveClass("hinweis");
    await userEvent.click(screen.getByRole("button", { name: "Mit Server vergleichen …" }));
    expect(await screen.findByText("Stand des Servers: soeben vollständig gelesen.")).toBeInTheDocument();
    expect(server(aufrufe)).toEqual([false, true]);
  });

  it("gegen das Manifest: Hinweis, „Mit Server vergleichen“, „Schnell vergleichen“ ohne Server; Übertragen möglich", async () => {
    const aufrufe = rust({ vergleich_starten: (a) => ftps((a as { server: boolean }).server ? "server" : "manifest") });
    ansicht();
    expect(await screen.findByText(/letzter bekannter Stand – der Server wurde nicht gelesen/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Übertragen …" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Neu vergleichen" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Mit Server vergleichen" }));
    expect(await screen.findByText("Stand des Servers: soeben vollständig gelesen.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Schnell vergleichen" }));
    expect(await screen.findByText(/letzter bekannter Stand/)).toBeInTheDocument();
    expect(server(aufrufe)).toEqual([false, true, false]);
  });

  it("lokales Ziel: „Neu vergleichen“, kein „Mit Server vergleichen“, kein Hinweis", async () => {
    rust();
    ansicht();
    expect(await screen.findByText("/Users/kim/Dev/web")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Neu vergleichen" })).not.toHaveAttribute("title");
    expect(screen.queryByRole("button", { name: "Mit Server vergleichen" })).toBeNull();
    expect(document.querySelector(".ziel-stand")).toBeNull();
    expect(screen.getByRole("button", { name: "Übertragen …" })).toBeEnabled();
  });
});
