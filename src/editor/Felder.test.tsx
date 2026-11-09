import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Endpunkt, Quelle, Stufe } from "../api";
import { aufrufeVon, mitQuery, rustAttrappe, Zustand } from "../test/hilfen";
import { AblageKontext, AuswahlFeld, EndpunktFelder, PasswortFeld, PfadFeld, ProblemeKontext, QuelleFelder, TextFeld, ZielFelder } from "./Felder";
import { neueStufe } from "./modell";
import { system } from "../system";

describe("Feld und Meldungen", () => {
  it("zeigt nur die Meldungen zum eigenen Ort", () => {
    render(
      <ProblemeKontext.Provider
        value={[
          { schwere: "fehler", ort: "name", meldung: "Name fehlt" },
          { schwere: "warnung", ort: "name", meldung: "Hinweis" },
          { schwere: "fehler", ort: "id", meldung: "anderes Feld" },
        ]}
      >
        <TextFeld label="Name" ort="name" wert="" onChange={() => {}} />
      </ProblemeKontext.Provider>,
    );
    expect(screen.getByLabelText("Name")).toBeInTheDocument();
    expect(screen.getByText("Name fehlt")).toHaveClass("fehler");
    expect(screen.getByText("Hinweis")).toHaveClass("warnung");
    expect(screen.queryByText("anderes Feld")).toBeNull();
  });

  it("TextFeld: Eingabe und nur lesen", async () => {
    const f = vi.fn();
    render(
      <>
        <Zustand start="" beobachter={f} kind={(w, s) => <TextFeld label="A" ort="a" wert={w} onChange={s} platzhalter="leer" />} />
        <TextFeld label="B" ort="b" wert="fest" nurLesen onChange={() => {}} />
      </>,
    );
    await userEvent.type(screen.getByLabelText("A"), "xy");
    expect(f).toHaveBeenLastCalledWith("xy");
    expect(screen.getByLabelText("A")).toHaveAttribute("placeholder", "leer");
    expect(screen.getByLabelText("B")).toHaveAttribute("readonly");
  });

  it("AuswahlFeld", async () => {
    const f = vi.fn();
    render(
      <AuswahlFeld label="Modus" ort="m" wert="a" optionen={[["a", "Eins"], ["b", "Zwei"]]} onChange={f} />,
    );
    await userEvent.selectOptions(screen.getByLabelText("Modus"), "Zwei");
    expect(f).toHaveBeenCalledWith("b");
  });
});

describe("PfadFeld", () => {
  it("übernimmt den im Dialog gewählten Ordner", async () => {
    const a = rustAttrappe({ "plugin:dialog|open": () => "/Users/kim/Server" });
    const f = vi.fn();
    render(<PfadFeld label="Ordner" ort="p" wert="/Users/kim" onChange={f} />);
    await userEvent.click(screen.getByRole("button", { name: "Wählen …" }));
    await waitFor(() => expect(f).toHaveBeenCalledWith("/Users/kim/Server"));
    expect(aufrufeVon(a, "plugin:dialog|open")).toHaveLength(1);
  });

  it("Abbruch im Dialog ändert nichts", async () => {
    const a = rustAttrappe({ "plugin:dialog|open": () => null });
    const f = vi.fn();
    render(<PfadFeld label="Ordner" ort="p" wert="" onChange={f} />);
    await userEvent.click(screen.getByRole("button", { name: "Wählen …" }));
    await waitFor(() => expect(a).toHaveLength(1));
    expect(f).not.toHaveBeenCalled();
  });

  it("Drag-and-drop: Felder sind Ablageziele und werden hervorgehoben", () => {
    const rahmen = () => screen.getByLabelText("Ordner").parentElement!;
    const mit = (aktiv: string | null, wartet: boolean) => (
      <AblageKontext.Provider value={{ aktiv, wartet }}>
        <PfadFeld label="Ordner" ort="stufen[0].ziel.pfad" wert="/alt" onChange={() => {}} />
      </AblageKontext.Provider>
    );
    const { rerender } = render(mit(null, false));
    expect(rahmen()).toHaveAttribute("data-drop-target", "stufen[0].ziel.pfad");
    expect(rahmen().className).toBe("pfad");

    rerender(mit(null, true));
    expect(rahmen().className).toBe("pfad ablage-bereit");
    rerender(mit("stufen[1].ziel.pfad", true));
    expect(rahmen().className).toBe("pfad");
    rerender(mit("stufen[0].ziel.pfad", true));
    expect(rahmen().className).toBe("pfad ablage-aktiv");
    expect(screen.getByLabelText("Ordner")).toHaveAttribute("placeholder", "Ordner hier ablegen …");
    expect(screen.getByLabelText("Ordner")).toHaveValue("");
  });

  it("Pfad lässt sich auch tippen", async () => {
    const f = vi.fn();
    render(<Zustand start="" beobachter={f} kind={(w, s) => <PfadFeld label="Ordner" ort="p" wert={w} onChange={s} />} />);
    await userEvent.type(screen.getByLabelText("Ordner"), "/x");
    expect(f).toHaveBeenLastCalledWith("/x");
  });
});

describe("Endpunkte", () => {
  const ftps: Endpunkt = {
    typ: "ftps", host: "", port: 21, pfad: "/", benutzer: "", tls: "explizit", konto_ref: "stellwerk:s:z",
  };

  it("lokal zeigt das Ordnerfeld", async () => {
    let zuletzt: Endpunkt = { typ: "lokal", pfad: "/a" };
    render(<Zustand<Endpunkt> start={zuletzt} beobachter={(w) => (zuletzt = w)} kind={(w, s) => <EndpunktFelder wert={w} ort="z" onChange={s} />} />);
    expect(screen.getByLabelText("Ordner")).toHaveValue("/a");
    await userEvent.type(screen.getByLabelText("Ordner"), "b");
    expect(zuletzt).toEqual({ typ: "lokal", pfad: "/ab" });
  });

  it("FTPS: jedes Feld wirkt", async () => {
    let zuletzt: Endpunkt = ftps;
    render(<Zustand<Endpunkt> start={ftps} beobachter={(w) => (zuletzt = w)} kind={(w, s) => <EndpunktFelder wert={w} ort="z" onChange={s} />} />);
    // Leerzeichen am Rand (beim Kopieren mitgenommen) fallen weg
    await userEvent.type(screen.getByLabelText("Server"), " ftp.x.de ");
    await userEvent.clear(screen.getByLabelText("Port"));
    await userEvent.type(screen.getByLabelText("Port"), "990");
    await userEvent.type(screen.getByLabelText("Serverpfad"), "httpdocs");
    await userEvent.type(screen.getByLabelText("Benutzer"), "kim ");
    expect(zuletzt).toEqual({ ...ftps, host: "ftp.x.de", port: 990, pfad: "/httpdocs", benutzer: "kim" });
    expect(screen.getByText(/explizitem TLS/)).toBeInTheDocument();
    // Der interne Schlüsselbund-Name ist kein Eingabefeld
    expect(screen.queryByLabelText(/Schlüsselbund/)).toBeNull();
    expect(screen.queryByLabelText("Passwort")).toBeNull(); // ohne Passwort-Eingabe (Quelle)
  });

  it("FTPS: ungültiger Port wird 0 (die Prüfung meldet das)", async () => {
    let zuletzt: Endpunkt = ftps;
    render(<Zustand<Endpunkt> start={ftps} beobachter={(w) => (zuletzt = w)} kind={(w, s) => <EndpunktFelder wert={w} ort="z" onChange={s} />} />);
    await userEvent.clear(screen.getByLabelText("Port"));
    expect(zuletzt).toMatchObject({ port: 0 });
  });

  it("ZielFelder wechseln die Art", async () => {
    let zuletzt: Endpunkt = { typ: "lokal", pfad: "/a" };
    render(
      <Zustand<Endpunkt>
        start={zuletzt}
        beobachter={(w) => (zuletzt = w)}
        kind={(w, s) => <ZielFelder wert={w} ort="z" konto="stellwerk:s:z" onChange={s} />}
      />,
    );
    await userEvent.selectOptions(screen.getByLabelText("Art"), "FTPS-Server");
    expect(zuletzt).toEqual(ftps);
    expect(screen.getByLabelText("Server")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Art"), "Lokaler Ordner");
    expect(zuletzt).toEqual({ typ: "lokal", pfad: "" });
  });
});

describe("QuelleFelder", () => {
  const vor1 = { ...neueStufe({ id: "", name: "", version: 1, stufen: [] }), id: "spiegel", name: "Spiegel" };
  const vor2: Stufe = { ...vor1, id: "zwei", name: "" };

  function aufbau(start: Quelle, vorstufen: Stufe[]) {
    const werte: Quelle[] = [];
    render(
      <Zustand
        start={start}
        beobachter={(w) => werte.push(w)}
        kind={(w, s) => <QuelleFelder wert={w} ort="q" vorstufen={vorstufen} konto="stellwerk:s:x:quelle" onChange={s} />}
      />,
    );
    return werte;
  }

  it("erste Stufe: lokaler Ordner, FTPS-Server (v0.6.5), Samba-Freigabe (v0.7.1) oder SFTP-Server (v0.7.2)", () => {
    aufbau({ typ: "lokal", pfad: "" }, []);
    const optionen = Array.from(screen.getByLabelText<HTMLSelectElement>("Art").options).map((o) => o.text);
    expect(optionen).toEqual(["Lokaler Ordner", "FTPS-Server", "Samba-Freigabe", "SFTP-Server"]);
  });

  it("Samba-Freigabe als Quelle: eigenes Konto", async () => {
    const werte = aufbau({ typ: "lokal", pfad: "" }, []);
    await userEvent.selectOptions(screen.getByLabelText("Art"), "Samba-Freigabe");
    expect(werte.at(-1)).toEqual({
      typ: "smb", host: "", port: 445, freigabe: "", pfad: "/", benutzer: "", konto_ref: "stellwerk:s:x:quelle",
    });
  });

  it("FTPS-Server als Quelle: Felder wie beim Ziel, eigenes Konto; zurück auf lokal", async () => {
    const werte = aufbau({ typ: "lokal", pfad: "" }, []);
    await userEvent.selectOptions(screen.getByLabelText("Art"), "FTPS-Server");
    expect(werte.at(-1)).toEqual({ typ: "ftps", host: "", port: 21, pfad: "/", benutzer: "", tls: "explizit", konto_ref: "stellwerk:s:x:quelle" });
    await userEvent.type(screen.getByLabelText("Server"), "h");
    expect(werte.at(-1)).toMatchObject({ host: "h" });
    await userEvent.selectOptions(screen.getByLabelText("Art"), "Lokaler Ordner");
    expect(werte.at(-1)).toEqual({ typ: "lokal", pfad: "" });
  });

  it("mit Vorstufen: Ziel der letzten Vorstufe als Vorschlag", async () => {
    const werte = aufbau({ typ: "lokal", pfad: "" }, [vor2, vor1]);
    await userEvent.selectOptions(screen.getByLabelText("Art"), "Ziel einer Vorstufe");
    expect(werte.at(-1)).toEqual({ typ: "stufe", stufe: "spiegel" });
    // Vorstufe ohne Namen erscheint mit Kennung
    const vorstufen = Array.from(screen.getByLabelText<HTMLSelectElement>("Vorstufe").options).map((o) => o.text);
    expect(vorstufen).toEqual(["zwei", "Spiegel"]);
    await userEvent.selectOptions(screen.getByLabelText("Vorstufe"), "zwei");
    expect(werte.at(-1)).toEqual({ typ: "stufe", stufe: "zwei" });
    await userEvent.selectOptions(screen.getByLabelText("Art"), "Lokaler Ordner");
    expect(werte.at(-1)).toEqual({ typ: "lokal", pfad: "" });
  });

  it("Verweis ohne Vorstufe bleibt sichtbar (Prüfung meldet ihn)", () => {
    aufbau({ typ: "stufe", stufe: "weg" }, []);
    expect(screen.getByLabelText("Art")).toHaveValue("stufe");
    expect(screen.getByLabelText<HTMLSelectElement>("Vorstufe").options).toHaveLength(0);
  });
});

describe("Samba-Freigabe (v0.7.1)", () => {
  const smb: Endpunkt = { typ: "smb", host: "", port: 445, freigabe: "", pfad: "/", benutzer: "", konto_ref: "stellwerk:s:z" };

  it("Adresse wird beim Tippen zerlegt, Benutzer wirkt", async () => {
    let zuletzt: Endpunkt = smb;
    render(<Zustand<Endpunkt> start={smb} beobachter={(w) => (zuletzt = w)} kind={(w, s) => <EndpunktFelder wert={w} ort="z" onChange={s} />} />);
    const adresse = screen.getByLabelText("Adresse");
    expect(adresse).toHaveValue("");
    expect(adresse).toHaveAttribute("placeholder", "smb://192.168.1.11/freigabe");
    await userEvent.type(adresse, "smb://192.168.1.11/manitu/web ");
    expect(adresse).toHaveValue("smb://192.168.1.11/manitu/web ");
    await userEvent.type(screen.getByLabelText("Benutzer"), "manitu ");
    expect(zuletzt).toEqual({ ...smb, host: "192.168.1.11", freigabe: "manitu", pfad: "/web", benutzer: "manitu" });
    expect(screen.getByText(/ohne sie im Finder zu verbinden/)).toBeInTheDocument();
    expect(screen.queryByLabelText("Passwort")).toBeNull();
  });

  it("vorhandene Adresse wird angezeigt; Meldungen aller Teile stehen unter der Adresse", () => {
    const e: Endpunkt = { ...smb, host: "pi", port: 4445, freigabe: "f", pfad: "/a" };
    render(
      <ProblemeKontext.Provider
        value={[
          { schwere: "fehler", ort: "z.freigabe", meldung: "Freigabe fehlt" },
          { schwere: "fehler", ort: "z.pfad", meldung: "Pfad falsch" },
          { schwere: "fehler", ort: "anders.host", meldung: "nicht hier" },
        ]}
      >
        <EndpunktFelder wert={e} ort="z" onChange={() => {}} />
      </ProblemeKontext.Provider>,
    );
    expect(screen.getByLabelText("Adresse")).toHaveValue("smb://pi:4445/f/a");
    expect(screen.getByText("Freigabe fehlt")).toHaveClass("fehler");
    expect(screen.getByText("Pfad falsch")).toBeInTheDocument();
    expect(screen.queryByText("nicht hier")).toBeNull();
  });

  it("ZielFelder: Samba-Freigabe wählen, mit Passwort", async () => {
    rustAttrappe({ passwort_vorhanden: () => false });
    let zuletzt: Endpunkt = { typ: "lokal", pfad: "/a" };
    mitQuery(
      <Zustand<Endpunkt>
        start={zuletzt}
        beobachter={(w) => (zuletzt = w)}
        kind={(w, s) => <ZielFelder wert={w} ort="z" konto="stellwerk:s:z" onChange={s} passwort={{ wert: "", onChange: () => {} }} />}
      />,
    );
    await userEvent.selectOptions(screen.getByLabelText("Art"), "Samba-Freigabe");
    expect(zuletzt).toEqual(smb);
    expect(await screen.findByLabelText("Passwort")).toHaveAttribute("type", "password");
  });
});

describe("SFTP-Server (v0.7.2)", () => {
  const sftp: Endpunkt = { typ: "sftp", host: "", port: 22, pfad: "/", benutzer: "", konto_ref: "stellwerk:s:z" };
  const HK = `SHA256:${"A".repeat(43)}`;

  it("jedes Feld wirkt; Server oder Port ändern verwirft den bestätigten Host-Schlüssel", async () => {
    const start: Endpunkt = { ...sftp, hostschluessel: HK };
    let zuletzt: Endpunkt = start;
    render(<Zustand<Endpunkt> start={start} beobachter={(w) => (zuletzt = w)} kind={(w, s) => <EndpunktFelder wert={w} ort="z" onChange={s} />} />);
    expect(screen.getByText(/manitu den Port 23/)).toBeInTheDocument();
    expect(screen.getByText(/immer verschlüsselt \(SSH\)/)).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Serverpfad"), "web");
    await userEvent.type(screen.getByLabelText("Benutzer"), " kim");
    expect(zuletzt).toMatchObject({ pfad: "/web", benutzer: "kim", hostschluessel: HK });
    await userEvent.type(screen.getByLabelText("Server"), "h ");
    expect(zuletzt).toMatchObject({ host: "h", hostschluessel: undefined });
    zuletzt = { ...zuletzt, hostschluessel: HK };
    await userEvent.clear(screen.getByLabelText("Port"));
    await userEvent.type(screen.getByLabelText("Port"), "23");
    expect(zuletzt).toEqual({ ...sftp, host: "h", port: 23, pfad: "/web", benutzer: "kim", hostschluessel: undefined });
    expect(screen.queryByLabelText("Passwort")).toBeNull();
  });

  it("Anmeldung per SSH-Schlüssel (v0.7.2b): Pfad tippen oder wählen, zurück auf Passwort", async () => {
    const wahl = vi.spyOn(system, "schluesselWaehlen").mockResolvedValueOnce("/Users/kim/.ssh/stellwerk").mockResolvedValueOnce(null);
    rustAttrappe({ passwort_vorhanden: () => false });
    let zuletzt: Endpunkt = sftp;
    mitQuery(
      <Zustand<Endpunkt>
        start={sftp}
        beobachter={(w) => (zuletzt = w)}
        kind={(w, s) => <EndpunktFelder wert={w} ort="z" onChange={s} passwort={{ wert: "", onChange: () => {} }} />}
      />,
    );
    expect(await screen.findByLabelText("Passwort")).toBeInTheDocument();
    expect(screen.queryByLabelText("SSH-Schlüssel")).toBeNull();
    await userEvent.selectOptions(screen.getByLabelText("Anmeldung"), "SSH-Schlüssel");
    expect(zuletzt).toEqual({ ...sftp, schluessel: "" });
    expect(screen.getByLabelText("Passphrase")).toHaveAttribute("type", "password");
    expect(screen.getByText(/nur nötig, wenn der Schlüssel eine hat/)).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("SSH-Schlüssel"), " ~/.ssh/k");
    expect(zuletzt).toMatchObject({ schluessel: "~/.ssh/k" });
    await userEvent.click(screen.getByRole("button", { name: "Wählen …" }));
    await waitFor(() => expect(zuletzt).toMatchObject({ schluessel: "/Users/kim/.ssh/stellwerk" }));
    // Abbruch im Dialog ändert nichts
    await userEvent.click(screen.getByRole("button", { name: "Wählen …" }));
    expect(wahl).toHaveBeenCalledTimes(2);
    expect(zuletzt).toMatchObject({ schluessel: "/Users/kim/.ssh/stellwerk" });
    await userEvent.selectOptions(screen.getByLabelText("Anmeldung"), "Passwort");
    expect(zuletzt).toEqual({ ...sftp, schluessel: undefined });
    wahl.mockRestore();
  });

  it("ZielFelder: SFTP-Server wählen, mit Passwort", async () => {
    rustAttrappe({ passwort_vorhanden: () => false });
    let zuletzt: Endpunkt = { typ: "lokal", pfad: "/a" };
    mitQuery(
      <Zustand<Endpunkt>
        start={zuletzt}
        beobachter={(w) => (zuletzt = w)}
        kind={(w, s) => <ZielFelder wert={w} ort="z" konto="stellwerk:s:z" onChange={s} passwort={{ wert: "", onChange: () => {} }} />}
      />,
    );
    await userEvent.selectOptions(screen.getByLabelText("Art"), "SFTP-Server");
    expect(zuletzt).toEqual(sftp);
    expect(await screen.findByLabelText("Passwort")).toHaveAttribute("type", "password");
    expect(screen.getByRole("button", { name: "Verbindung testen" })).toBeDisabled();
  });
});

describe("Passwort", () => {
  const ftps: Endpunkt = {
    typ: "ftps", host: "h", port: 21, pfad: "/", benutzer: "b", tls: "explizit", konto_ref: "stellwerk:s:z",
  };

  function aufbau(vorhanden: boolean) {
    const a = rustAttrappe({ passwort_vorhanden: () => vorhanden });
    const eingaben: string[] = [];
    mitQuery(
      <Zustand
        start=""
        beobachter={(w) => eingaben.push(w)}
        kind={(w, s) => <ZielFelder wert={ftps} ort="z" konto="stellwerk:s:z" onChange={() => {}} passwort={{ wert: w, onChange: s }} />}
      />,
    );
    return { a, eingaben };
  }

  it("noch keines hinterlegt: verdecktes Eingabefeld", async () => {
    const { a, eingaben } = aufbau(false);
    const feld = await screen.findByLabelText("Passwort");
    expect(feld).toHaveAttribute("type", "password");
    expect(screen.getByText(/im macOS-Schlüsselbund abgelegt/)).toBeInTheDocument();
    await userEvent.type(feld, "geheim");
    expect(eingaben.at(-1)).toBe("geheim");
    expect(aufrufeVon(a, "passwort_vorhanden")[0].args).toEqual({ konto: "stellwerk:s:z" });
  });

  it("bereits hinterlegt: Hinweis und Ändern", async () => {
    aufbau(true);
    expect(await screen.findByText("Im Schlüsselbund hinterlegt ✓")).toBeInTheDocument();
    expect(screen.queryByLabelText("Passwort")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Ändern" }));
    expect(screen.getByLabelText("Passwort")).toHaveAttribute("placeholder", "Neues Passwort");
    expect(screen.getByText(/ersetzt beim Speichern das bisherige/)).toBeInTheDocument();
  });

  it("Passphrase eines SSH-Schlüssels (v0.7.2b): bereits hinterlegt", async () => {
    rustAttrappe({ passwort_vorhanden: () => true });
    mitQuery(<PasswortFeld konto="k" ort="z.passwort" name="Passphrase" wert="" onChange={() => {}} />);
    expect(await screen.findByText("Im Schlüsselbund hinterlegt ✓")).toBeInTheDocument();
    expect(screen.getByText("Passphrase")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Ändern" }));
    expect(screen.getByLabelText("Passphrase")).toHaveAttribute("placeholder", "Neue Passphrase");
    expect(screen.getByText("Die neue Passphrase ersetzt beim Speichern die bisherige.")).toBeInTheDocument();
  });
});
