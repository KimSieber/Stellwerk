// Reine Hilfsfunktionen des Strecken-Editors (ohne Oberfläche, voll testbar).

import type { Endpunkt, Problem, Quelle, Regel, RegelTyp, Stufe, Strecke } from "../api";

/** Anzeigenamen der Regeltypen. */
export const REGEL_TYPEN: [RegelTyp, string][] = [
  ["ignorieren", "Ignorieren"],
  ["schuetzen", "Schützen"],
  ["nur_anlegen", "Nur anlegen"],
  ["ersetzen", "Ersetzen durch"],
];

/** Kennung aus einem Namen: klein, Umlaute umschrieben, nur a-z, 0-9 und „-“. */
export function kennungAus(name: string): string {
  return name
    .toLowerCase()
    .replace(/ä/g, "ae")
    .replace(/ö/g, "oe")
    .replace(/ü/g, "ue")
    .replace(/ß/g, "ss")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64)
    .replace(/-+$/, "");
}

/** Interner Name des Schlüsselbund-Eintrags einer Stufe – vergibt Stellwerk selbst. */
export function kontoRef(streckeId: string, stufeId: string): string {
  return `stellwerk:${streckeId}:${stufeId}`;
}

/** Erste freie Stufenkennung „stufe-N“. */
export function freieStufenId(strecke: Strecke): string {
  const vergeben = new Set(strecke.stufen.map((s) => s.id));
  let n = strecke.stufen.length + 1;
  while (vergeben.has(`stufe-${n}`)) n += 1;
  return `stufe-${n}`;
}

export function neueStufe(strecke: Strecke): Stufe {
  const id = freieStufenId(strecke);
  const vorige = strecke.stufen[strecke.stufen.length - 1];
  return {
    id,
    name: `Stufe ${strecke.stufen.length + 1}`,
    modus: "direkt",
    // Ab der zweiten Stufe wird der Zielordner der Vorstufe als Quellordner
    // vorgeschlagen (als normaler Pfad, frei änderbar – kein Verweis).
    quelle: { typ: "lokal", pfad: vorige?.ziel.typ === "lokal" ? vorige.ziel.pfad : "" },
    ziel: { typ: "lokal", pfad: "" },
    nur_online: "herunterladen",
  };
}

export function neueStrecke(): Strecke {
  const leer: Strecke = { id: "", name: "", version: 1, stufen: [] };
  return { ...leer, stufen: [neueStufe(leer)] };
}

/** Leerer Endpunkt des gewünschten Typs. */
export function endpunktVom(typ: "lokal" | "ftps", konto: string): Endpunkt {
  return typ === "lokal"
    ? { typ: "lokal", pfad: "" }
    : { typ: "ftps", host: "", port: 21, pfad: "/", benutzer: "", tls: "explizit", konto_ref: konto };
}

/** Leere Quelle des gewünschten Typs; `stufe` verweist auf die Vorstufe. FTPS ist nur Ziel. */
export function quelleVom(typ: "lokal" | "stufe", vorstufe?: string): Quelle {
  return typ === "stufe" ? { typ: "stufe", stufe: vorstufe ?? "" } : { typ: "lokal", pfad: "" };
}

/** Setzt bei allen FTPS-Zielen den internen Schlüsselbund-Namen passend zu Strecke und Stufe. */
export function kontenAngleichen(s: Strecke): Strecke {
  return {
    ...s,
    stufen: s.stufen.map((st) =>
      st.ziel.typ === "ftps" ? { ...st, ziel: { ...st.ziel, konto_ref: kontoRef(s.id, st.id) } } : st,
    ),
  };
}

export function neueRegel(): Regel {
  return { muster: "", typ: "ignorieren" };
}

/** Probleme, deren Ort genau `ort` ist. */
export function problemeAn(probleme: Problem[], ort: string): Problem[] {
  return probleme.filter((p) => p.ort === ort);
}

export function hatFehler(probleme: Problem[]): boolean {
  return probleme.some((p) => p.schwere === "fehler");
}

/** Setzt den Ordner am Ort `stufen[i].quelle.pfad` bzw. `stufen[i].ziel.pfad`
 *  (Ziel einer Ablage per Drag-and-drop). Andere Orte oder Nicht-Ordner: unverändert. */
export function pfadSetzen(s: Strecke, ort: string, pfad: string): Strecke {
  const m = /^stufen\[(\d+)\]\.(quelle|ziel)\.pfad$/.exec(ort);
  const i = m ? Number(m[1]) : -1;
  const seite = m?.[2] as "quelle" | "ziel" | undefined;
  const st = s.stufen[i];
  if (!st || !seite || st[seite].typ !== "lokal") return s;
  return { ...s, stufen: s.stufen.map((x, k) => (k === i ? { ...x, [seite]: { typ: "lokal", pfad } } : x)) };
}

const ORT_NAMEN: Record<string, string> = {
  name: "Name",
  id: "Kennung",
  version: "Formatversion",
  stufen: "Stufe",
  regeln: "Regeln",
  eigene: "Regel",
  standard_aus: "Abschaltung",
  quelle: "Quelle",
  ziel: "Ziel",
  pfad: "Pfad",
  host: "Server",
  port: "Port",
  benutzer: "Benutzer",
  konto_ref: "Passwort",
  passwort: "Passwort",
  typ: "Art",
  stufe: "Vorstufe",
  muster: "Muster",
  rechte: "Dateirechte",
  ordner: "Ordner",
  dateien: "Dateien",
  modus: "Modus",
  nur_online: "„nur online“",
  global: "Global",
};

/** Ort einer Meldung für Menschen: `stufen[1].ziel.pfad` → `Stufe 2 „Server“ › Ziel › Pfad`. */
export function ortLesbar(ort: string, s?: Strecke): string {
  const teile = ort.split(".").map((teil) => {
    const m = /^(\w+)\[(\d+)\]$/.exec(teil);
    const name = m ? m[1] : teil;
    const text = ORT_NAMEN[name] ?? name;
    if (!m) return text;
    const nr = Number(m[2]);
    const stufenName = name === "stufen" ? s?.stufen[nr]?.name : undefined;
    return stufenName ? `${text} ${nr + 1} „${stufenName}“` : `${text} ${nr + 1}`;
  });
  // Regeln auf oberster Ebene gehören zur Strecke
  if (ort.startsWith("regeln.")) teile[0] = "Regeln der Strecke";
  // „Pfad“ heißt am Feld „Ordner“ (lokal) bzw. „Serverpfad“ (FTPS)
  const m = /^stufen\[(\d+)\]\.(quelle|ziel)\.pfad$/.exec(ort);
  const endpunkt = m ? s?.stufen[Number(m[1])]?.[m[2] as "quelle" | "ziel"] : undefined;
  if (endpunkt) teile[2] = endpunkt.typ === "ftps" ? "Serverpfad" : "Ordner";
  return teile.join(" › ");
}

/** Regel einer übergeordneten Ebene, die auf der aktuellen Ebene abgeschaltet werden kann. */
export type ObereRegel = { muster: string; typ: RegelTyp; herkunft: "global" | "strecke" };

const wirksam = (r: Regel) => r.aktiv !== false && r.muster.trim() !== "";

/** Was auf Ebene „Strecke“ abschaltbar ist: die wirksamen globalen Regeln. */
export function regelnFuerStrecke(global: Regel[]): ObereRegel[] {
  return global.filter(wirksam).map((r) => ({ muster: r.muster, typ: r.typ, herkunft: "global" }));
}

/** Was auf Ebene „Stufe“ abschaltbar ist: globale Regeln, die die Strecke nicht abschaltet,
 *  und die wirksamen eigenen Regeln der Strecke. */
export function regelnFuerStufe(global: Regel[], s: Strecke): ObereRegel[] {
  const aus = new Set(s.regeln?.standard_aus ?? []);
  return [
    ...regelnFuerStrecke(global).filter((r) => !aus.has(r.muster)),
    ...(s.regeln?.eigene ?? []).filter(wirksam).map((r) => ({ muster: r.muster, typ: r.typ, herkunft: "strecke" as const })),
  ];
}

/** Schaltet eine übergeordnete Regel auf dieser Ebene an (aus Liste entfernen) oder aus. */
export function ausUmschalten(aus: string[], muster: string, an: boolean): string[] {
  const ohne = aus.filter((m) => m !== muster);
  return an ? ohne : [...ohne, muster];
}

/** Muster lesbar machen – z. B. das Steuerzeichen im macOS-Muster „Icon[\r]“. */
export function musterAnzeige(muster: string): string {
  return muster.replace(/\r/g, "\\r");
}
