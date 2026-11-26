//! Crochet abbreviations in German, US and UK English. UK stitch names are one step higher than
//! US ones: a UK double crochet is a US single crochet.

/// One term in the three languages. The first abbreviation is the usual one; German patterns vary
/// more, so the German side can list several. An abbreviation can be the word itself (e.g. "row").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Term {
    pub de: &'static [&'static str],
    pub de_name: &'static str,
    pub us: &'static str,
    pub us_name: &'static str,
    pub uk: &'static str,
    pub uk_name: &'static str,
}

impl Term {
    /// The UK abbreviation differs from the US one, so a pattern has to say which it uses.
    pub fn uk_differs(&self) -> bool {
        self.uk != self.us
    }

    /// How well the term matches a search (lower is better), `None` if it doesn't match.
    /// An exact abbreviation or name comes first, then words starting with the query, then the rest.
    fn rank(&self, query: &str) -> Option<u8> {
        let all = || {
            self.de.iter().chain([
                &self.us,
                &self.uk,
                &self.de_name,
                &self.us_name,
                &self.uk_name,
            ])
        };
        if all().any(|t| t.to_lowercase() == query) {
            Some(0)
        } else if all().any(|t| {
            t.to_lowercase()
                .split([' ', '-'])
                .any(|w| w.starts_with(query))
        }) {
            Some(1)
        } else if all().any(|t| t.to_lowercase().contains(query)) {
            Some(2)
        } else {
            None
        }
    }
}

/// The terms matching `query` (case-insensitive, anywhere in any language), best matches first.
/// An empty query returns all terms in table order.
pub fn search(query: &str) -> Vec<&'static Term> {
    let query = query.trim().to_lowercase();
    let mut found: Vec<(u8, &Term)> = TERMS
        .iter()
        .filter_map(|t| t.rank(&query).map(|r| (r, t)))
        .collect();
    found.sort_by_key(|(rank, _)| *rank);
    found.into_iter().map(|(_, t)| t).collect()
}

const fn term(
    de: &'static [&'static str],
    de_name: &'static str,
    us: (&'static str, &'static str),
    uk: (&'static str, &'static str),
) -> Term {
    Term {
        de,
        de_name,
        us: us.0,
        us_name: us.1,
        uk: uk.0,
        uk_name: uk.1,
    }
}

/// Basic stitches from short to tall, then special stitches, techniques and pattern terms.
pub const TERMS: &[Term] = &[
    term(&["Lm"], "Luftmasche", ("ch", "chain"), ("ch", "chain")),
    term(
        &["Km", "Kettm"],
        "Kettmasche",
        ("sl st", "slip stitch"),
        ("ss", "slip stitch"),
    ),
    term(
        &["fM", "fe M"],
        "feste Masche",
        ("sc", "single crochet"),
        ("dc", "double crochet"),
    ),
    term(
        &["hStb", "hStäb"],
        "halbes Stäbchen",
        ("hdc", "half double crochet"),
        ("htr", "half treble crochet"),
    ),
    term(
        &["Stb", "Stäb"],
        "Stäbchen",
        ("dc", "double crochet"),
        ("tr", "treble crochet"),
    ),
    term(
        &["DStb", "Dstb"],
        "Doppelstäbchen",
        ("tr", "treble crochet"),
        ("dtr", "double treble crochet"),
    ),
    term(
        &["3fStb", "DDStb"],
        "Dreifachstäbchen",
        ("dtr", "double treble crochet"),
        ("trtr", "triple treble crochet"),
    ),
    term(
        &["Krebsm"],
        "Krebsmasche",
        ("rev sc", "reverse single crochet"),
        ("rev dc", "reverse double crochet"),
    ),
    term(
        &["RStbv", "RStb v"],
        "Reliefstäbchen von vorne",
        ("FPdc", "front post double crochet"),
        ("FPtr", "front post treble crochet"),
    ),
    term(
        &["RStbh", "RStb h"],
        "Reliefstäbchen von hinten",
        ("BPdc", "back post double crochet"),
        ("BPtr", "back post treble crochet"),
    ),
    term(&[], "Büschelmasche", ("cl", "cluster"), ("cl", "cluster")),
    term(&[], "Noppe", ("bob", "bobble"), ("bob", "bobble")),
    term(&[], "Popcornmasche", ("pc", "popcorn"), ("pc", "popcorn")),
    term(&[], "Muschel", ("sh", "shell"), ("sh", "shell")),
    term(
        &["MR", "FR"],
        "Magic Ring, Fadenring",
        ("MR", "magic ring"),
        ("MR", "magic ring"),
    ),
    term(
        &["Zun"],
        "Zunahme",
        ("inc", "increase"),
        ("inc", "increase"),
    ),
    term(
        &["Abn"],
        "Abnahme",
        ("dec", "decrease"),
        ("dec", "decrease"),
    ),
    term(
        &[],
        "unsichtbare Abnahme",
        ("inv dec", "invisible decrease"),
        ("inv dec", "invisible decrease"),
    ),
    term(
        &["zus"],
        "zusammen abmaschen",
        ("tog", "together"),
        ("tog", "together"),
    ),
    term(
        &["U", "Umschl"],
        "Umschlag",
        ("yo", "yarn over"),
        ("yrh", "yarn round hook"),
    ),
    term(
        &["hMg"],
        "hinteres Maschenglied",
        ("BLO", "back loop only"),
        ("BLO", "back loop only"),
    ),
    term(
        &["vMg"],
        "vorderes Maschenglied",
        ("FLO", "front loop only"),
        ("FLO", "front loop only"),
    ),
    term(
        &["überspr"],
        "überspringen",
        ("sk", "skip"),
        ("miss", "miss"),
    ),
    term(
        &["Wdh"],
        "wiederholen",
        ("rep", "repeat"),
        ("rep", "repeat"),
    ),
    term(
        &["Lm-Bogen", "LmB"],
        "Luftmaschenbogen",
        ("ch-sp", "chain space"),
        ("ch-sp", "chain space"),
    ),
    term(
        &["WLm"],
        "Wendeluftmasche",
        ("tch", "turning chain"),
        ("tch", "turning chain"),
    ),
    term(
        &["M"],
        "Masche(n)",
        ("st(s)", "stitch(es)"),
        ("st(s)", "stitch(es)"),
    ),
    term(&["R"], "Reihe", ("row", "row"), ("row", "row")),
    term(&["Rd", "Rde"], "Runde", ("rnd", "round"), ("rnd", "round")),
    term(
        &["Hinr"],
        "Hinreihe",
        ("RS", "right side"),
        ("RS", "right side"),
    ),
    term(
        &["Rückr"],
        "Rückreihe",
        ("WS", "wrong side"),
        ("WS", "wrong side"),
    ),
    term(
        &["MM"],
        "Maschenmarkierer",
        ("sm", "stitch marker"),
        ("sm", "stitch marker"),
    ),
    term(
        &["Anf"],
        "Anfang",
        ("beg", "beginning"),
        ("beg", "beginning"),
    ),
    term(
        &["folg"],
        "folgende",
        ("foll", "following"),
        ("foll", "following"),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn by_us(us: &str) -> &'static Term {
        TERMS.iter().find(|t| t.us == us).unwrap()
    }

    #[test]
    fn uk_stitches_are_one_step_higher() {
        // US sc = UK dc, US dc = UK tr, …: each UK name is the US name of the next taller stitch.
        let heights = ["sc", "dc", "tr", "dtr"];
        for pair in heights.windows(2) {
            assert_eq!(by_us(pair[0]).uk, pair[1]);
        }
        assert_eq!(by_us("hdc").uk, "htr");
        assert_eq!(by_us("sc").de, ["fM", "fe M"]);
        assert!(by_us("sc").uk_differs());
        assert!(!by_us("ch").uk_differs());
    }

    #[test]
    fn no_duplicates() {
        for (i, a) in TERMS.iter().enumerate() {
            for b in &TERMS[i + 1..] {
                assert_ne!(a.de_name, b.de_name);
                assert_ne!(a.us, b.us, "{}", a.us);
            }
        }
    }

    #[test]
    fn empty_search_returns_everything_in_order() {
        assert_eq!(search(" ").len(), TERMS.len());
        assert_eq!(search("")[0].de_name, "Luftmasche");
    }

    #[test]
    fn search_puts_exact_abbreviations_first() {
        // "dc" is the US Stäbchen and the UK feste Masche; both come before hdc, FPdc, …
        let found = search("DC");
        let first: Vec<_> = found[..2].iter().map(|t| t.de_name).collect();
        assert!(first.contains(&"Stäbchen") && first.contains(&"feste Masche"));
        assert!(found.iter().any(|t| t.us == "hdc"));

        assert_eq!(search("stäbchen")[0].de_name, "Stäbchen");
        assert_eq!(search("tr")[0].us, "dc");
        assert_eq!(search("treble")[0].us, "hdc");
        assert!(
            search("Reliefst")
                .iter()
                .all(|t| t.de_name.starts_with("Relief"))
        );
        assert_eq!(search("xyz").len(), 0);
    }
}
