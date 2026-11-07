import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { RegelTreffer, Zeile } from "../api";
import { zeile } from "../test/vergleich";
import { ErsatzDialog, RegelMenue } from "./RegelMenue";

const regel = (ebene: RegelTreffer["herkunft"]["ebene"], typ: RegelTreffer["typ"], ersatz: string | null = null): RegelTreffer => ({
  typ,
  herkunft: { ebene, index: 0, muster: "/x" },
  ersatz,
});

function menue(z: Zeile, x = 10, y = 20) {
  const onAktion = vi.fn();
  const onSchliessen = vi.fn();
  render(<RegelMenue z={z} x={x} y={y} onAktion={onAktion} onSchliessen={onSchliessen} />);
  return { onAktion, onSchliessen };
}

const gruppe = (name: string) => within(screen.getByRole("group", { name }));

describe("Kontextmenü „Regel setzen“", () => {
  it("bietet je Ebene die Regeln an; bei Dateien auch „Ersetzen durch …“", async () => {
    const { onAktion, onSchliessen } = menue(zeile("css/site.css"));
    expect(screen.getByRole("menu", { name: "Regel für css/site.css" })).toHaveTextContent("Regel für site.css");
    for (const name of ["Für diese Stufe", "Für alle Stufen der Strecke"]) {
      expect(gruppe(name).getAllByRole("menuitem").map((b) => b.textContent)).toEqual([
        "Ignorieren",
        "Schützen",
        "Nur anlegen",
        "Ersetzen durch …",
      ]);
    }
    expect(screen.queryByText(/Regel entfernen/)).toBeNull();

    await userEvent.click(gruppe("Für alle Stufen der Strecke").getByRole("menuitem", { name: "Schützen" }));
    expect(onSchliessen).toHaveBeenCalled();
    expect(onAktion).toHaveBeenCalledWith({ art: "setzen", ebene: "strecke", typ: "schuetzen" });
  });

  it("Ordner: ohne „Ersetzen durch“, mit Hinweis auf den Inhalt", async () => {
    const { onAktion } = menue(zeile("logs", { ordner: true }));
    expect(screen.getByRole("menu")).toHaveTextContent("Regel für logs (Ordner mit Inhalt)");
    expect(screen.queryByRole("menuitem", { name: /Ersetzen/ })).toBeNull();
    await userEvent.click(gruppe("Für diese Stufe").getByRole("menuitem", { name: "Ignorieren" }));
    expect(onAktion).toHaveBeenCalledWith({ art: "setzen", ebene: "stufe", typ: "ignorieren" });
  });

  it("„Ersetzen durch …“ öffnet den Dialog-Schritt", async () => {
    const { onAktion } = menue(zeile("config.php"));
    await userEvent.click(gruppe("Für diese Stufe").getByRole("menuitem", { name: "Ersetzen durch …" }));
    expect(onAktion).toHaveBeenCalledWith({ art: "ersetzen", ebene: "stufe" });
  });

  it("vorhandene Regel für genau diesen Pfad: abgehakt und entfernbar", async () => {
    const { onAktion } = menue(zeile("robots.txt", { regel: regel("stufe", "schuetzen"), regel_entfernbar: true }));
    const aktiv = gruppe("Für diese Stufe").getByRole("menuitem", { name: "✓Schützen" });
    expect(aktiv).toBeDisabled();
    expect(gruppe("Für alle Stufen der Strecke").getByRole("menuitem", { name: "Schützen" })).toBeEnabled();
    await userEvent.click(screen.getByRole("menuitem", { name: "Regel entfernen (Stufe)" }));
    expect(onAktion).toHaveBeenCalledWith({ art: "entfernen", ebene: "stufe" });
  });

  it("Regel der Strecke; „Ersetzen durch“ als aktive Regel", async () => {
    const { onAktion } = menue(
      zeile("config.php", { regel: regel("strecke", "ersetzen", "config.server.php"), regel_entfernbar: true }),
    );
    expect(gruppe("Für alle Stufen der Strecke").getByRole("menuitem", { name: "✓Ersetzen durch …" })).toBeEnabled();
    await userEvent.click(screen.getByRole("menuitem", { name: "Regel entfernen (Strecke)" }));
    expect(onAktion).toHaveBeenCalledWith({ art: "entfernen", ebene: "strecke" });
  });

  it("allgemeine Regel (nicht genau dieser Pfad): weder abgehakt noch entfernbar", () => {
    menue(zeile("config.php", { regel: regel("stufe", "schuetzen"), regel_entfernbar: false }));
    expect(gruppe("Für diese Stufe").getByRole("menuitem", { name: "Schützen" })).toBeEnabled();
    expect(screen.queryByText(/Regel entfernen/)).toBeNull();
  });

  it("schließt mit Escape, Klick daneben oder Rechtsklick daneben – nicht bei Klick hinein", async () => {
    const { onSchliessen } = menue(zeile("a.txt"));
    await userEvent.click(screen.getByText(/Regel für/));
    expect(onSchliessen).not.toHaveBeenCalled();
    await userEvent.keyboard("{Escape}");
    expect(onSchliessen).toHaveBeenCalledTimes(1);
    await userEvent.keyboard("a");
    expect(onSchliessen).toHaveBeenCalledTimes(1);
    const hintergrund = document.querySelector(".menue-hintergrund")!;
    await userEvent.click(hintergrund);
    expect(onSchliessen).toHaveBeenCalledTimes(2);
    expect(fireEvent.contextMenu(hintergrund)).toBe(false); // Systemmenü unterdrückt
    expect(onSchliessen).toHaveBeenCalledTimes(3);
  });

  it("bleibt im Fenster", () => {
    menue(zeile("a.txt"), 5000, 5000);
    const m = screen.getByRole("menu");
    expect(m.style.left).toBe(`${window.innerWidth - 280}px`);
    expect(m.style.top).toBe(`${window.innerHeight - 340}px`);
  });
});

describe("Dialog „Ersetzen durch …“", () => {
  function dialog(z: Zeile, vorschlaege: string[], ebene: "stufe" | "strecke" = "stufe") {
    const onUebernehmen = vi.fn();
    const onAbbrechen = vi.fn();
    render(<ErsatzDialog z={z} ebene={ebene} vorschlaege={vorschlaege} onUebernehmen={onUebernehmen} onAbbrechen={onAbbrechen} />);
    return { onUebernehmen, onAbbrechen, feld: screen.getByLabelText("Ersatzdatei (Pfad in der Quelle)") as HTMLInputElement };
  }

  it("schlägt die erste Datei des Ordners vor und übernimmt die Eingabe", async () => {
    const { onUebernehmen, feld } = dialog(zeile("config.php"), ["config.server.php", "index.php"]);
    expect(screen.getByRole("dialog", { name: "Ersetzen durch" })).toHaveTextContent("(nur in dieser Stufe)");
    expect(feld.value).toBe("config.server.php");
    expect([...document.querySelectorAll("#ersatz-vorschlaege option")].map((o) => o.getAttribute("value"))).toEqual([
      "config.server.php",
      "index.php",
    ]);
    await userEvent.clear(feld);
    await userEvent.type(feld, "  cfg/live.php ");
    await userEvent.click(screen.getByRole("button", { name: "Übernehmen" }));
    expect(onUebernehmen).toHaveBeenCalledWith("cfg/live.php");
  });

  it("vorhandene Ersatzdatei wird vorbelegt; leer oder dieselbe Datei geht nicht", async () => {
    const { feld } = dialog(zeile("config.php", { regel: regel("strecke", "ersetzen", "alt.php") }), ["neu.php"], "strecke");
    expect(screen.getByRole("dialog")).toHaveTextContent("(in allen Stufen der Strecke)");
    expect(feld.value).toBe("alt.php");
    await userEvent.clear(feld);
    expect(screen.getByRole("button", { name: "Übernehmen" })).toBeDisabled();
    await userEvent.type(feld, "config.php");
    expect(screen.getByRole("button", { name: "Übernehmen" })).toBeDisabled();
  });

  it("ohne Vorschläge leer; Abbrechen per Knopf oder Klick daneben", async () => {
    const { onAbbrechen, feld } = dialog(zeile("a.txt"), []);
    expect(feld.value).toBe("");
    await userEvent.click(screen.getByRole("heading", { name: "Ersetzen durch …" }));
    expect(onAbbrechen).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Abbrechen" }));
    await userEvent.click(document.querySelector(".dialog-hintergrund")!);
    expect(onAbbrechen).toHaveBeenCalledTimes(2);
  });
});
