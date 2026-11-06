import { describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { aufrufeVon, mitQuery, rustAttrappe } from "../test/hilfen";
import { Ueber } from "./Ueber";

function oeffnen(version: () => unknown = () => "0.1.0") {
  const a = rustAttrappe({ "plugin:app|version": version, "plugin:opener|open_url": () => null });
  const zu = vi.fn();
  mitQuery(<Ueber onSchliessen={zu} />);
  return { a, zu };
}

describe("Über Stellwerk", () => {
  it("zeigt Icon, Name, Version, Urheber, Kontakt und Webseite", async () => {
    oeffnen();
    const d = screen.getByRole("dialog", { name: "Über Stellwerk" });
    expect(d.querySelector("img")).toHaveAttribute("src", "/app-icon.png");
    expect(screen.getByRole("heading", { name: "Stellwerk" })).toBeInTheDocument();
    expect(await screen.findByText("Version 0.1.0")).toBeInTheDocument();
    expect(screen.getByText("© 2026 Kim Sieber, Stuttgart")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Stellwerk@Sieber-BW.de" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "https://stellwerk.sieber-bw.de" })).toBeInTheDocument();
  });

  it("Version „–“, solange unbekannt", () => {
    oeffnen(() => new Promise(() => {}));
    expect(screen.getByText("Version –")).toBeInTheDocument();
  });

  it("Kontakt und Webseite öffnen im Standardprogramm", async () => {
    const { a } = oeffnen();
    await userEvent.click(screen.getByRole("button", { name: "Stellwerk@Sieber-BW.de" }));
    await userEvent.click(screen.getByRole("button", { name: "https://stellwerk.sieber-bw.de" }));
    await waitFor(() => expect(aufrufeVon(a, "plugin:opener|open_url")).toHaveLength(2));
    const urls = aufrufeVon(a, "plugin:opener|open_url").map((x) => (x.args as { url: string }).url);
    expect(urls).toEqual(["mailto:Stellwerk@Sieber-BW.de", "https://stellwerk.sieber-bw.de"]);
  });

  it("schließt per Knopf, Escape und Klick daneben – nicht bei Klick hinein", async () => {
    const { zu } = oeffnen();
    await userEvent.click(screen.getByRole("heading", { name: "Stellwerk" }));
    fireEvent.keyDown(window, { key: "Enter" });
    expect(zu).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "Schließen" }));
    fireEvent.keyDown(window, { key: "Escape" });
    await userEvent.click(screen.getByRole("dialog").parentElement!);
    expect(zu).toHaveBeenCalledTimes(3);
  });
});
