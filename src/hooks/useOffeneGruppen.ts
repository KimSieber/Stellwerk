// Welche Gruppen der Startseite aufgeklappt sind – bleibt über einen Neustart erhalten (v0.8.1).
// Gespeichert im Browser-Speicher der App; geht er verloren, sind einfach alle Gruppen zu.

import { useState } from "react";

export const SPEICHER_SCHLUESSEL = "stellwerk.offeneGruppen";

function laden(): Set<string> {
  try {
    const wert: unknown = JSON.parse(localStorage.getItem(SPEICHER_SCHLUESSEL) ?? "[]");
    return new Set(Array.isArray(wert) ? wert.filter((g): g is string => typeof g === "string") : []);
  } catch {
    return new Set();
  }
}

function sichern(offen: Set<string>) {
  try {
    localStorage.setItem(SPEICHER_SCHLUESSEL, JSON.stringify([...offen]));
  } catch {
    // Speicher nicht verfügbar: Zustand gilt nur bis zum Schließen
  }
}

export function useOffeneGruppen() {
  const [offen, setOffen] = useState<Set<string>>(laden);

  function setzen(neu: Set<string>) {
    setOffen(neu);
    sichern(neu);
  }

  return {
    istOffen: (gruppe: string) => offen.has(gruppe),
    umschalten: (gruppe: string) => {
      const neu = new Set(offen);
      if (!neu.delete(gruppe)) neu.add(gruppe);
      setzen(neu);
    },
    /** Alle genannten Gruppen auf; mit leerer Liste: alle zu. */
    alleSetzen: (gruppen: string[]) => setzen(new Set(gruppen)),
  };
}
