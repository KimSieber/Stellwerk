# Stellwerk – Regeln: Ignorieren, Schützen, Nur anlegen

Kurzanleitung für Anwender (Stand v0.6).

Regeln legen fest, wie Stellwerk einzelne Dateien oder Ordner behandelt, statt sie einfach von der Quelle zum Ziel abzugleichen. Ohne Regel gilt: neu → anlegen, geändert → überschreiben, nur am Ziel → löschen.

## Auf einen Blick

| | **Ignorieren** | **Schützen** | **Nur anlegen** |
|---|---|---|---|
| Wird eingelesen? | nein (Ordner wird nicht durchlaufen) | ja | ja |
| In der Vergleichsansicht | eine graue Zeile „ignoriert“, bei Ordnern ohne Inhalt | jede Datei grau „geschützt“ | „neu“, wenn am Ziel fehlend, sonst „geschützt“ |
| Fehlt am Ziel | wird **nicht** angelegt | wird **nicht** angelegt | wird **angelegt** |
| Ist am Ziel anders | wird **nicht** überschrieben | wird **nicht** überschrieben | wird **nicht** überschrieben |
| Gibt es nur am Ziel | wird **nicht** gelöscht | wird **nicht** gelöscht | wird **nicht** gelöscht |
| Gedacht für | Dinge, die nirgends hingehören | Dinge, die **am Ziel** gepflegt werden | Startdateien, die das Ziel danach selbst verändert |

Gemeinsam ist allen drei: **Am Ziel wird nie etwas überschrieben oder gelöscht.** Der Unterschied liegt darin, ob Stellwerk die Datei überhaupt ansieht und ob sie einmalig angelegt wird.

## Ignorieren – „gibt es für Stellwerk nicht“

Die Datei oder der Ordner wird weder verglichen noch übertragen, auf keiner Seite. Ein ignorierter Ordner wird gar nicht erst durchlaufen – das spart bei großen Ordnern viel Zeit.

**Beispiele**
- `.DS_Store`, `Thumbs.db`, `.git/` – Systemdateien (sind schon als Standardregel gesetzt)
- `node_modules/` – zehntausende Dateien, die auf dem Server nichts verloren haben
- `Webseite/entwurf/` – Arbeitsordner, der nur lokal existiert

## Schützen – „gehört dem Ziel, Finger weg“

Die Datei wird eingelesen und in der Vergleichsansicht grau angezeigt, aber am Ziel **nie angelegt, überschrieben oder gelöscht**. Du siehst also, was da ist, aber Stellwerk ändert nichts daran.

**Beispiele**
- `logs/` auf dem Server – Protokolle, die der Server schreibt; ohne Regel würde Stellwerk sie als „löschen“ vorschlagen, weil sie in der Quelle fehlen
- `uploads/` – Bilder, die Besucher hochladen
- `config.php` im Spiegel oder auf dem Server – dort gepflegt, mit Zugangsdaten, die nicht in der Quelle stehen sollen

## Nur anlegen – „einmal hinbringen, dann in Ruhe lassen“

Fehlt die Datei am Ziel, wird sie übertragen. Ist sie schon da, gilt sie als geschützt – auch wenn sie inzwischen anders aussieht.

**Beispiele**
- `daten/zaehler.txt` – Startwert 0; danach zählt der Server selbst hoch
- `config.php` als Vorlage für eine neue Installation: beim ersten Mal hinkopieren, danach am Server anpassen
- `daten/.htaccess` – einmal anlegen, wenn der Ordner neu entsteht

## Ignorieren oder Schützen? Die Faustregel

- **Gehört die Datei überhaupt zum Projekt?** Nein → **Ignorieren**.
- **Gehört sie zum Ziel, wird aber dort gepflegt?** → **Schützen**.
- **Soll sie nur beim ersten Mal hin?** → **Nur anlegen**.

Ein Unterschied in der Praxis: Eine Datei, die nur am Ziel liegt (z. B. `fehler.log`), bleibt bei beiden Regeln erhalten. Mit „Schützen“ siehst du sie aber weiterhin in der Liste, mit „Ignorieren“ verschwindet sie aus dem Blick.

## Was sonst noch gut zu wissen ist

- **Ordner bleiben stehen:** Enthält ein Ordner am Ziel etwas Ignoriertes oder Geschütztes, wird der Ordner selbst nicht gelöscht – auch wenn er in der Quelle fehlt (Hinweis „Ordner bleibt, weil darin etwas bestehen bleibt“).
- **Muster** schreibst du wie in einer `.gitignore`: `*.log`, `logs/` (Ordner mit Inhalt), `/config.php` (nur im obersten Ordner).
- **Ebenen:** Standardregeln → Regeln der Strecke → Regeln der Stufe. Passen mehrere, gewinnt die zuletzt genannte, also die der Stufe. Standardregeln lassen sich je Strecke oder Stufe abschalten.
- **Schnell setzen:** Rechtsklick auf eine Zeile der Vergleichsansicht → „Regel setzen“, für diese Stufe oder für alle Stufen der Strecke. Allgemeinere Muster setzt du im Editor.
- **Die vierte Regel, „Ersetzen durch“:** Am Ziel landet der Inhalt einer anderen Quelldatei, z. B. `config.server.php` → `config.php`.
