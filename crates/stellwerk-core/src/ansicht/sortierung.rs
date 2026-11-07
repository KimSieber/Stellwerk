//! Reihenfolge der Zeilen in der Vergleichsansicht – wie im Finder:
//! Baum (Eltern vor Inhalt), innerhalb eines Ordners erst Unterordner, dann Dateien;
//! Namen deutsch sortiert (Groß/Klein egal, Umlaute wie ihr Grundbuchstabe nach
//! DIN 5007-1, `ß` wie `ss`) und Zahlen nach ihrem Wert („Datei2“ vor „Datei10“).

use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

use crate::model::RelPath;

/// Vergleicht zwei Datei- oder Ordnernamen. Die Reihenfolge ist vollständig:
/// Nur gleiche Namen sind gleich.
pub fn namen_vergleichen(a: &str, b: &str) -> Ordering {
    natuerlich(&grundform(a), &grundform(b))
        .then_with(|| natuerlich(&a.to_lowercase(), &b.to_lowercase()))
        .then_with(|| a.cmp(b))
}

/// Vergleicht zwei Pfade in Baum-Reihenfolge. `*_ordner` sagt, ob der Eintrag
/// selbst ein Ordner ist (seine Vorfahren sind es immer).
pub fn pfade_vergleichen(a: &RelPath, a_ordner: bool, b: &RelPath, b_ordner: bool) -> Ordering {
    let ka: Vec<&str> = a.as_str().split('/').collect();
    let kb: Vec<&str> = b.as_str().split('/').collect();
    for (i, (x, y)) in ka.iter().zip(&kb).enumerate() {
        if x != y {
            let x_ordner = i + 1 < ka.len() || a_ordner;
            let y_ordner = i + 1 < kb.len() || b_ordner;
            // Ordner zuerst: `true` soll vor `false` stehen
            return y_ordner.cmp(&x_ordner).then_with(|| namen_vergleichen(x, y));
        }
    }
    // Ein Pfad ist Vorfahr des anderen (oder beide gleich): der kürzere zuerst
    ka.len().cmp(&kb.len())
}

/// Kleinbuchstaben ohne Akzente/Umlautpunkte, `ß` → `ss`.
fn grundform(s: &str) -> String {
    s.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .flat_map(|c| if c == 'ß' { vec!['s', 's'] } else { vec![c] })
        .collect()
}

/// Zeichenweiser Vergleich, Ziffernfolgen nach Zahlenwert.
fn natuerlich(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        let o = match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                zahlen_vergleichen(&ziffern(&mut x), &ziffern(&mut y))
            }
            (Some(c), Some(d)) => {
                x.next();
                y.next();
                c.cmp(&d)
            }
        };
        if o != Ordering::Equal {
            return o;
        }
    }
}

fn ziffern(it: &mut Peekable<Chars>) -> String {
    let mut s = String::new();
    while let Some(c) = it.next_if(char::is_ascii_digit) {
        s.push(c);
    }
    s
}

/// Ziffernfolgen beliebiger Länge nach Wert (führende Nullen zählen nicht).
fn zahlen_vergleichen(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sortiert(namen: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = namen.iter().map(|s| s.to_string()).collect();
        v.sort_by(|a, b| namen_vergleichen(a, b));
        v
    }

    #[test]
    fn gross_klein_egal() {
        assert_eq!(sortiert(&["Zebra", "apfel", "Birne"]), ["apfel", "Birne", "Zebra"]);
    }

    #[test]
    fn umlaute_wie_grundbuchstabe() {
        assert_eq!(sortiert(&["zeta", "Äpfel", "Ober", "Öl", "ab"]), ["ab", "Äpfel", "Ober", "Öl", "zeta"]);
        assert_eq!(sortiert(&["Strasse", "Straße", "Strand"]), ["Strand", "Strasse", "Straße"]);
        assert_eq!(sortiert(&["e", "é", "f"]), ["e", "é", "f"]);
        // der Umlaut zählt wie sein Grundbuchstabe, nicht als eigenes Zeichen
        assert_eq!(sortiert(&["Ac", "Äb", "ec", "éb"]), ["Äb", "Ac", "éb", "ec"]);
    }

    #[test]
    fn umlaut_in_nfd_wie_in_nfc() {
        let nfd = "A\u{0308}pfel"; // Ä zerlegt (macOS)
        assert_eq!(sortiert(&["Bar", nfd, "Ab"]), ["Ab", nfd, "Bar"]);
    }

    #[test]
    fn gleichstand_wird_eindeutig_aufgeloest() {
        // erst Grundform, dann Kleinschreibung mit Umlaut, dann Bytes
        assert_eq!(sortiert(&["bär", "Bar", "bar"]), ["Bar", "bar", "bär"]);
        assert_eq!(namen_vergleichen("x", "x"), Ordering::Equal);
    }

    #[test]
    fn zahlen_nach_wert() {
        assert_eq!(sortiert(&["Datei10", "Datei2", "Datei1"]), ["Datei1", "Datei2", "Datei10"]);
        assert_eq!(sortiert(&["v010", "v9", "v0011"]), ["v9", "v010", "v0011"]);
        assert_eq!(sortiert(&["7", "007"]), ["007", "7"], "gleicher Wert → Bytes entscheiden");
        assert_eq!(sortiert(&["a1b", "a1a"]), ["a1a", "a1b"], "nach der Zahl geht es weiter");
        assert_eq!(
            namen_vergleichen("99999999999999999999999", "100000000000000000000000"),
            Ordering::Less,
            "beliebig lange Zahlen"
        );
    }

    #[test]
    fn praefix_zuerst() {
        assert_eq!(sortiert(&["abc", "ab"]), ["ab", "abc"]);
        assert_eq!(namen_vergleichen("abc", "ab"), Ordering::Greater);
        assert_eq!(namen_vergleichen("ab", "abc"), Ordering::Less);
    }

    fn p(s: &str) -> RelPath {
        RelPath::new(s).unwrap()
    }

    fn baum(eintraege: &[(&str, bool)]) -> Vec<String> {
        let mut v: Vec<(RelPath, bool)> = eintraege.iter().map(|(s, o)| (p(s), *o)).collect();
        v.sort_by(|(a, ao), (b, bo)| pfade_vergleichen(a, *ao, b, *bo));
        v.into_iter().map(|(x, _)| x.as_str().to_string()).collect()
    }

    #[test]
    fn baum_ordner_zuerst_und_eltern_vor_inhalt() {
        let liste = baum(&[
            ("z.txt", false),
            ("ä", true),
            ("ä/b.txt", false),
            ("ä/a", true),
            ("ä/a/x", false),
            ("B.txt", false),
            ("c", true),
        ]);
        assert_eq!(liste, ["ä", "ä/a", "ä/a/x", "ä/b.txt", "c", "B.txt", "z.txt"]);
    }

    #[test]
    fn inhalt_eines_ordners_auch_ohne_ordnerzeile() {
        // Vorfahren gelten immer als Ordner, auch wenn sie selbst nicht in der Liste stehen
        assert_eq!(baum(&[("b.txt", false), ("a/x", false)]), ["a/x", "b.txt"]);
        assert_eq!(pfade_vergleichen(&p("a"), true, &p("a"), true), Ordering::Equal);
    }
}
