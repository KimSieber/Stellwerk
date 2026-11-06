import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Regel } from "../api";
import { Zustand } from "../test/hilfen";
import { RegelListe } from "./RegelListe";

function aufbau(start: Regel[]) {
  const werte: Regel[][] = [];
  render(
    <Zustand
      start={start}
      beobachter={(w) => werte.push(w)}
      kind={(w, s) => <RegelListe titel="Regeln" regeln={w} ort="regeln.eigene" onChange={s} />}
    />,
  );
  return () => werte.at(-1);
}

describe("RegelListe", () => {
  it("leer, dann Regel hinzufügen und ausfüllen", async () => {
    const zuletzt = aufbau([]);
    expect(screen.getByText("Keine eigenen Regeln.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "+ Regel" }));
    expect(screen.queryByText("Keine eigenen Regeln.")).toBeNull();
    await userEvent.type(screen.getByLabelText("Muster"), "*.log");
    await userEvent.selectOptions(screen.getByLabelText("Regel"), "Schützen");
    expect(zuletzt()).toEqual([{ muster: "*.log", typ: "schuetzen", quelle: undefined }]);
    expect(screen.queryByLabelText("Ersatzdatei")).toBeNull();
  });

  it("„Ersetzen durch“ zeigt die Ersatzdatei; Wechsel weg entfernt sie", async () => {
    const zuletzt = aufbau([{ muster: "config.php", typ: "ignorieren" }]);
    await userEvent.selectOptions(screen.getByLabelText("Regel"), "Ersetzen durch");
    expect(screen.getByLabelText("Ersatzdatei")).toHaveValue("");
    await userEvent.type(screen.getByLabelText("Ersatzdatei"), "config.server.php");
    expect(zuletzt()).toEqual([{ muster: "config.php", typ: "ersetzen", quelle: "config.server.php" }]);
    await userEvent.selectOptions(screen.getByLabelText("Regel"), "Nur anlegen");
    expect(zuletzt()![0].quelle).toBeUndefined();
  });

  it("vorhandene Ersatzangabe bleibt beim Wechsel zu „Ersetzen durch“ erhalten", async () => {
    const zuletzt = aufbau([{ muster: "a", typ: "schuetzen", quelle: "b" }]);
    await userEvent.selectOptions(screen.getByLabelText("Regel"), "Ersetzen durch");
    expect(zuletzt()![0].quelle).toBe("b");
  });

  it("„Ersetzen durch“ ohne Angabe zeigt leeres Feld", () => {
    aufbau([{ muster: "a", typ: "ersetzen" }]);
    expect(screen.getByLabelText("Ersatzdatei")).toHaveValue("");
  });

  it("Schalter und Entfernen", async () => {
    const zuletzt = aufbau([
      { muster: "a", typ: "ignorieren" },
      { muster: "b", typ: "ignorieren", aktiv: false },
    ]);
    const [r0, r1] = [screen.getByTestId("regel-0"), screen.getByTestId("regel-1")];
    expect(within(r0).getByRole("checkbox")).toBeChecked();
    expect(within(r1).getByRole("checkbox")).not.toBeChecked();

    await userEvent.click(within(r0).getByRole("checkbox"));
    expect(zuletzt()![0].aktiv).toBe(false);
    await userEvent.click(within(r1).getByRole("checkbox"));
    expect(zuletzt()![1].aktiv).toBeUndefined(); // „an“ ist der Standard und wird nicht gespeichert

    await userEvent.click(within(r0).getByRole("button", { name: "Regel entfernen" }));
    expect(zuletzt()!.map((r) => r.muster)).toEqual(["b"]);
  });
});
