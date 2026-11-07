import { describe, expect, it, vi } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Stufe } from "../api";
import { beispielStrecke, mitQuery, rustAttrappe, Zustand } from "../test/hilfen";
import { StufeEditor } from "./StufeEditor";

function aufbau(start: Stufe, onEntfernen = vi.fn(), onPasswort = vi.fn()) {
  rustAttrappe({ passwort_vorhanden: () => false });
  const werte: Stufe[] = [];
  mitQuery(
    <Zustand
      start={start}
      beobachter={(w) => werte.push(w)}
      kind={(w, s) => (
        <StufeEditor
          stufe={w}
          index={1}
          vorstufen={[beispielStrecke().stufen[0]]}
          streckeId="mailwerk-web"
          onChange={s}
          onEntfernen={onEntfernen}
          passwort=""
          onPasswort={onPasswort}
          obereRegeln={[{ muster: ".DS_Store", typ: "ignorieren", herkunft: "global" }]}
        />
      )}
    />,
  );
  return () => werte.at(-1)!;
}

const server = () => beispielStrecke().stufen[1];

describe("StufeEditor", () => {
  it("zeigt Kopf, Felder, Quelle und Ziel", () => {
    aufbau(server());
    const st = screen.getByRole("region", { name: "Stufe 2" });
    expect(within(st).getByRole("heading", { name: "Spiegel → Server" })).toBeInTheDocument();
    expect(screen.getByLabelText("Name der Stufe")).toHaveValue("Spiegel → Server");
    expect(screen.getByLabelText("Kennung der Stufe")).toHaveValue("server");
    expect(screen.getByLabelText("Vorstufe")).toHaveValue("spiegel");
    expect(screen.getByLabelText("Server")).toHaveValue("ftp.example.de");
  });

  it("Name, Kennung, Modus und „nur online“ wirken", async () => {
    const zuletzt = aufbau({ ...server(), name: "" });
    expect(screen.getByRole("heading", { name: "Neue Stufe" })).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Name der Stufe"), "Live");
    await userEvent.type(screen.getByLabelText("Kennung der Stufe"), "-2");
    await userEvent.selectOptions(screen.getByLabelText("Modus"), "Direkt");
    await userEvent.selectOptions(screen.getByLabelText("Dateien „nur online“"), "Überspringen");
    expect(zuletzt()).toMatchObject({ name: "Live", id: "server-2", modus: "direkt", nur_online: "ueberspringen" });
  });

  it("Quelle und Ziel wirken", async () => {
    const zuletzt = aufbau(server());
    const [artQuelle, artZiel] = screen.getAllByLabelText("Art");
    await userEvent.selectOptions(artQuelle, "Lokaler Ordner");
    expect(zuletzt().quelle).toEqual({ typ: "lokal", pfad: "" });
    await userEvent.selectOptions(artZiel, "Lokaler Ordner");
    expect(zuletzt().ziel).toEqual({ typ: "lokal", pfad: "" });
    await userEvent.selectOptions(artZiel, "FTPS-Server");
    expect(zuletzt().ziel).toMatchObject({ konto_ref: "stellwerk:mailwerk-web:server" });
  });

  it("Dateirechte: fest mit Vorgaben, leeren, nicht setzen", async () => {
    const zuletzt = aufbau({ ...server(), rechte: undefined });
    expect(screen.getByLabelText("Rechte")).toHaveValue("nicht_setzen");
    expect(screen.queryByLabelText("Dateien")).toBeNull();

    await userEvent.selectOptions(screen.getByLabelText("Rechte"), "Feste Werte");
    expect(zuletzt().rechte).toEqual({ modus: "fest", ordner: "755", dateien: "644" });

    await userEvent.clear(screen.getByLabelText("Dateien"));
    await userEvent.type(screen.getByLabelText("Dateien"), "600");
    await userEvent.clear(screen.getByLabelText("Ordner"));
    expect(zuletzt().rechte).toEqual({ modus: "fest", ordner: undefined, dateien: "600" });
    await userEvent.clear(screen.getByLabelText("Dateien"));
    await userEvent.type(screen.getByLabelText("Ordner"), "7");
    expect(zuletzt().rechte).toEqual({ modus: "fest", ordner: "7", dateien: undefined });

    await userEvent.selectOptions(screen.getByLabelText("Rechte"), "Nicht setzen");
    expect(zuletzt().rechte).toBeUndefined();
  });

  it("Musterregeln aus der Datei werden angezeigt", () => {
    aufbau({ ...server(), rechte: { modus: "muster", regeln: [{ muster: "*.sh", modus: "755" }] } });
    expect(screen.getByLabelText("Rechte")).toHaveValue("muster");
    expect(screen.getByRole("option", { name: "Nach Muster (in der Datei gepflegt)" })).toBeInTheDocument();
  });

  it("Regeln der Stufe – auch wenn noch keine Regeln vorhanden sind", async () => {
    const zuletzt = aufbau({ ...server(), regeln: undefined });
    await userEvent.click(screen.getByRole("button", { name: "+ Regel" }));
    expect(zuletzt().regeln).toEqual({ eigene: [{ muster: "", typ: "ignorieren" }] });
  });

  it("Regeln der Stufe ergänzen bestehende", async () => {
    const zuletzt = aufbau(server());
    await userEvent.click(screen.getByRole("button", { name: "+ Regel" }));
    expect(zuletzt().regeln!.eigene).toHaveLength(2);
  });

  it("Passwort-Eingabe geht an den Strecken-Editor", async () => {
    const pw = vi.fn();
    aufbau(server(), vi.fn(), pw);
    await userEvent.type(await screen.findByLabelText("Passwort"), "x");
    expect(pw).toHaveBeenCalledWith("x");
    expect(screen.getByText("Quelle (wird nur gelesen)")).toBeInTheDocument();
    expect(screen.getByText("Ziel (wird beschrieben)")).toBeInTheDocument();
  });

  it("übernommene Regeln für diese Stufe abschalten", async () => {
    const zuletzt = aufbau({ ...server(), regeln: undefined });
    expect(screen.getByText("Übernommene Regeln (global und Strecke) – 1 von 1 wirksam")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("checkbox", { name: ".DS_Store" }));
    expect(zuletzt().regeln).toEqual({ standard_aus: [".DS_Store"] });
  });

  it("Entfernen", async () => {
    const weg = vi.fn();
    aufbau(server(), weg);
    await userEvent.click(screen.getByRole("button", { name: "Stufe entfernen" }));
    expect(weg).toHaveBeenCalledOnce();
  });
});
