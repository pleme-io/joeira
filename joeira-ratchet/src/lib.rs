//! The baseline ratchet — one mechanism, shared by every gate that ships onto a
//! corpus already in violation.
//!
//! A gate that goes red on every run is a gate that gets skipped, so a gate has
//! to be adoptable on day one. The shape that makes it adoptable is not an
//! allowlist: known debt is recorded WITH THE MEASUREMENT IT HAD when recorded,
//! and exactly three things fail —
//!
//! - a violation the baseline does not name;
//! - a baselined item whose measurement has GROWN past the recorded value;
//! - (advisory) a baselined item whose subject no longer exists.
//!
//! That second rule is the whole seal. A pure allowlist lets a baselined 1,400 B
//! entry drift to 8,000 B in silence, which is precisely the failure mode being
//! sealed. Debt may shrink or hold; it may never grow.
//!
//! # Provenance — this is a LIFT, not a fourth copy
//!
//! Lifted verbatim from `skill-lint/src/ratchet.rs`, whose own doc comment
//! exists to forbid a further re-implementation:
//!
//! > a ratchet re-implemented per gate is a ratchet that will drift per gate —
//! > one of them would quietly grow a `>=` where the other has `>`, and the
//! > looser gate would be the one nobody noticed.
//!
//! It lifted cleanly because it had **zero dependencies** — `std` and one
//! `BTreeMap`. `BTreeMap` rather than `HashMap` is load-bearing: deterministic
//! iteration is what makes a rendered baseline byte-stable, and byte-stability is
//! what makes a baseline reviewable as a diff.
//!
//! ## What this crate does NOT retire, stated so the claim stays honest
//!
//! The class has **more than ten** implementations in this fleet, not the three
//! the design doc counted: ten baseline files under `pleme-io/actions`, one as a
//! Nix attrset (`blackmatter/fleet-checks-baseline.nix`), and two as Nix-eval
//! counting invariants (`nix/parts/fleet.nix`, `nix/parts/module-shape.nix`).
//!
//! **A Rust crate cannot retire any of those.** `nix/parts/fleet.nix` already
//! reached this conclusion in place, and its comment is the precedent:
//!
//! > Reuse check, since the fleet already owns a ratchet: skill-lint's
//! > `src/ratchet.rs` is SHIPPED and its doc comment forbids a fourth copy — but
//! > it ratchets ITS OWN baseline file from Rust, and this is a Nix eval-time
//! > invariant in a different repo.
//!
//! Nor is `actions/tlisp-lint`'s key-only baseline a degraded copy to absorb: it
//! chose to key on FILE rather than count *deliberately*, because "a count-based
//! ratchet lets a file swap one shell site for another and stay green." That is
//! [`Verdict::Unrecorded`] / [`Verdict::Held`] with [`Verdict::Grew`]
//! intentionally absent — a different, correct instrument for a different
//! subject.
//!
//! So the honest claim is narrow: **the three Rust consumers converge on one
//! crate** (`skill-lint`'s two gates plus joeira's `warn+ratchet` rows), and this
//! becomes the canonical grammar the tlisp and Nix copies cite rather than
//! reinvent.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Known debt for one kind of measured thing: key → measurement when recorded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ratchet {
    recorded: BTreeMap<String, usize>,
}

/// What the ratchet says about one measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The baseline does not name this key — a NEW violation.
    Unrecorded,
    /// Recorded, and the measurement has not exceeded the recorded value.
    /// Known debt, holding or shrinking.
    Held,
    /// Recorded, and the measurement has grown past the recorded value.
    Grew {
        /// The measurement when the baseline was written.
        recorded: usize,
        /// How far past it the current measurement sits.
        grew: usize,
    },
}

impl Ratchet {
    /// The recorded measurement for `key`, if the baseline names it.
    #[must_use]
    pub fn recorded(&self, key: &str) -> Option<usize> {
        self.recorded.get(key).copied()
    }

    /// Record `key` at `size`.
    pub fn insert(&mut self, key: impl Into<String>, size: usize) {
        self.recorded.insert(key.into(), size);
    }

    /// How many items the baseline names.
    #[must_use]
    pub fn len(&self) -> usize {
        self.recorded.len()
    }

    /// Does the baseline name nothing?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.recorded.is_empty()
    }

    /// Judge one measurement against the recorded debt.
    #[must_use]
    pub fn judge(&self, key: &str, measured: usize) -> Verdict {
        match self.recorded.get(key) {
            None => Verdict::Unrecorded,
            Some(&recorded) if measured > recorded => Verdict::Grew {
                recorded,
                grew: measured - recorded,
            },
            Some(_) => Verdict::Held,
        }
    }

    // ── The orphan direction — the one thing the lift adds ──────────────
    //
    // `judge` is keyed on measurements THE CORPUS PRODUCES, so a baseline row
    // whose subject no longer exists is in the one blind spot it structurally
    // cannot reach: `judge` is never called for that key, so nothing ever
    // reports it.
    //
    // Two reasons that matters, and the second is the sharp one:
    //
    //   1. Debt was PAID and the row is dead weight — a baseline that only ever
    //      grows stops being a ratchet and becomes scenery.
    //   2. A stale row is a LATENT AMNESTY. If the key ever returns — a revert,
    //      a restored heading, a renamed file moving back — the old ceiling
    //      silently re-applies at a value nobody re-measured. That is
    //      "debt may shrink or hold; it may never grow" failing on a
    //      technicality, which is the exact class the seal exists to close.
    //
    // Before this, `recorded()` + `len()` let a caller COUNT orphans
    // (`len()` minus the number of live keys the baseline names) but never NAME
    // them, because there was no key iterator. That is the whole gap.

    /// Every key the baseline names, in deterministic order.
    ///
    /// `BTreeMap` ordering is deliberate — see the crate docs.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.recorded.keys().map(String::as_str)
    }

    /// Rows whose subject the corpus no longer produces.
    ///
    /// `live` is the set of keys the current run actually measured. Callers
    /// already have it: both `skill-lint` gates iterate their scan before
    /// consulting the ratchet.
    ///
    /// **This is ADVISORY and must not fail a gate.** Both existing precedents
    /// print rather than fail — `actions/tlisp-lint/run.tlisp` reports
    /// "N now clean (delete them from the baseline)" without touching its exit
    /// code, and `blackmatter/fleet-checks-baseline.nix` records that a row
    /// naming a check absent on the building system "is not an error … but IS
    /// printed in the receipt, so it can never go invisible."
    /// `nix/parts/fleet.nix` states the reason plainly: *a gate that punishes
    /// correct work gets deleted.* Paying debt is correct work, and failing the
    /// person who paid it is how the ratchet gets ripped out.
    #[must_use]
    pub fn orphans<'a>(&'a self, live: &BTreeSet<String>) -> Vec<&'a str> {
        self.keys().filter(|k| !live.contains(*k)).collect()
    }

    pub fn rows(&self) -> impl Iterator<Item = (&str, usize)> {
        self.recorded.iter().map(|(k, &v)| (k.as_str(), v))
    }

    #[must_use]
    pub fn shrunk(&self, key: &str, measured: usize) -> Option<Shrink> {
        match self.recorded.get(key) {
            Some(&recorded) if measured < recorded => Some(Shrink {
                recorded,
                now: measured,
            }),
            _ => None,
        }
    }

    #[must_use]
    pub fn bank<K: AsRef<str>>(
        &self,
        measured: impl IntoIterator<Item = (K, usize)>,
        mode: BankMode,
    ) -> Ratchet {
        let recorded = latest(measured)
            .into_iter()
            .filter_map(|(key, now)| match self.recorded.get(&key) {
                Some(&was) => Some((key, was.min(now))),
                None if mode == BankMode::Record => Some((key, now)),
                None => None,
            })
            .collect();
        Ratchet { recorded }
    }

    #[must_use]
    pub fn render(&self, kind: &str) -> String {
        self.rows()
            .map(|(key, size)| format!("{kind}: {key} {size}\n"))
            .collect()
    }

    #[must_use]
    pub fn summarize<K: AsRef<str>>(
        &self,
        kind: &str,
        measured: impl IntoIterator<Item = (K, usize)>,
    ) -> Summary {
        let measured = latest(measured);
        let mut summary = Summary::default();
        for (key, size) in &measured {
            let verdict = self.judge(key, *size);
            if let Some(shrink) = self.shrunk(key, *size) {
                summary.shrinks.push(Shrunk {
                    kind: kind.to_owned(),
                    key: key.clone(),
                    shrink,
                });
            }
            summary.judged.push(Judged {
                kind: kind.to_owned(),
                key: key.clone(),
                measured: *size,
                verdict,
            });
        }
        summary.orphans = self
            .rows()
            .filter(|(key, _)| !measured.contains_key(*key))
            .map(|(key, recorded)| Orphan {
                kind: kind.to_owned(),
                key: key.to_owned(),
                recorded,
            })
            .collect();
        summary
    }
}

fn latest<K: AsRef<str>>(rows: impl IntoIterator<Item = (K, usize)>) -> BTreeMap<String, usize> {
    rows.into_iter()
        .map(|(k, v)| (k.as_ref().to_owned(), v))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shrink {
    pub recorded: usize,
    pub now: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BankMode {
    #[default]
    Shrink,
    Record,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judged {
    pub kind: String,
    pub key: String,
    pub measured: usize,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shrunk {
    pub kind: String,
    pub key: String,
    pub shrink: Shrink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub kind: String,
    pub key: String,
    pub recorded: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub judged: Vec<Judged>,
    pub shrinks: Vec<Shrunk>,
    pub orphans: Vec<Orphan>,
}

impl Summary {
    #[must_use]
    pub fn is_red(&self) -> bool {
        self.failures().next().is_some()
    }

    pub fn failures(&self) -> impl Iterator<Item = &Judged> {
        self.judged.iter().filter(|j| j.verdict != Verdict::Held)
    }

    fn count(&self, pick: fn(&Verdict) -> bool) -> usize {
        self.judged.iter().filter(|j| pick(&j.verdict)).count()
    }

    fn merge(&mut self, other: Summary) {
        self.judged.extend(other.judged);
        self.shrinks.extend(other.shrinks);
        self.orphans.extend(other.orphans);
    }
}

impl fmt::Display for Judged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Judged {
            kind,
            key,
            measured,
            verdict,
        } = self;
        match verdict {
            Verdict::Unrecorded => write!(
                f,
                "{kind}: {key} measures {measured} and the baseline has no row for it"
            ),
            Verdict::Grew { recorded, grew } => write!(
                f,
                "{kind}: {key} measures {measured}, past its recorded {recorded} by {grew}"
            ),
            Verdict::Held => write!(f, "{kind}: {key} measures {measured} and holds"),
        }
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for j in self.failures() {
            let word = match j.verdict {
                Verdict::Unrecorded => "unrecorded",
                _ => "grew",
            };
            writeln!(f, "{word}: {j}")?;
        }
        for s in &self.shrinks {
            writeln!(
                f,
                "shrunk: {}: {} measures {}, below its recorded {}; `ratchet bank` banks it",
                s.kind, s.key, s.shrink.now, s.shrink.recorded
            )?;
        }
        for o in &self.orphans {
            writeln!(
                f,
                "orphan: {}: {} recorded {} and no longer measured",
                o.kind, o.key, o.recorded
            )?;
        }
        writeln!(
            f,
            "{}: {} held, {} grew, {} unrecorded, {} shrunk, {} orphans",
            if self.is_red() { "red" } else { "green" },
            self.count(|v| *v == Verdict::Held),
            self.count(|v| matches!(v, Verdict::Grew { .. })),
            self.count(|v| *v == Verdict::Unrecorded),
            self.shrinks.len(),
            self.orphans.len()
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Baseline {
    kinds: BTreeMap<String, Ratchet>,
}

impl Baseline {
    #[must_use]
    pub fn parse(text: &str) -> Baseline {
        let mut b = Baseline::default();
        for (kind, key, size) in parse_lines(text) {
            b.insert(kind, key, size);
        }
        b
    }

    pub fn insert(&mut self, kind: impl Into<String>, key: impl Into<String>, size: usize) {
        self.kinds.entry(kind.into()).or_default().insert(key, size);
    }

    #[must_use]
    pub fn kind(&self, kind: &str) -> Option<&Ratchet> {
        self.kinds.get(kind)
    }

    pub fn kinds(&self) -> impl Iterator<Item = (&str, &Ratchet)> {
        self.kinds.iter().map(|(k, r)| (k.as_str(), r))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.kinds.values().map(Ratchet::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn judge(&self, kind: &str, key: &str, measured: usize) -> Verdict {
        self.kinds
            .get(kind)
            .map_or(Verdict::Unrecorded, |r| r.judge(key, measured))
    }

    #[must_use]
    pub fn summarize(&self, measured: &Baseline) -> Summary {
        let empty = Ratchet::default();
        let mut summary = Summary::default();
        for kind in self.union(measured) {
            let recorded = self.kinds.get(kind).unwrap_or(&empty);
            let now = measured.kinds.get(kind).unwrap_or(&empty);
            summary.merge(recorded.summarize(kind, now.rows()));
        }
        summary
    }

    #[must_use]
    pub fn bank(&self, measured: &Baseline, mode: BankMode) -> Baseline {
        let empty = Ratchet::default();
        let kinds = self
            .union(measured)
            .into_iter()
            .map(|kind| {
                let recorded = self.kinds.get(kind).unwrap_or(&empty);
                let now = measured.kinds.get(kind).unwrap_or(&empty);
                (kind.to_owned(), recorded.bank(now.rows(), mode))
            })
            .filter(|(_, r)| !r.is_empty())
            .collect();
        Baseline { kinds }
    }

    #[must_use]
    pub fn render(&self) -> String {
        self.kinds().map(|(kind, r)| r.render(kind)).collect()
    }

    fn union<'a>(&'a self, other: &'a Baseline) -> BTreeSet<&'a str> {
        self.kinds
            .keys()
            .chain(other.kinds.keys())
            .map(String::as_str)
            .collect()
    }
}

/// Parse baseline lines into `(kind, key, size)` triples.
///
/// The grammar is `<kind>: <key> <size>`. The size is the LAST
/// whitespace-separated token because a key contains spaces — parsing from the
/// left would truncate every key at its first space and silently cover nothing.
///
/// Blank lines and `#` comments are ignored, and so is anything else
/// unparseable: a malformed baseline must not be able to WEAKEN a gate by
/// accident. An unrecognized line covers nothing, which fails loudly on the
/// next run rather than passing quietly forever.
#[must_use]
pub fn parse_lines(text: &str) -> Vec<(String, String, usize)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((kind, rest)) = line.split_once(':') else {
            continue;
        };
        let Some((key, size)) = rest.trim().rsplit_once(char::is_whitespace) else {
            continue;
        };
        let Ok(size) = size.trim().parse::<usize>() else {
            continue;
        };
        out.push((kind.trim().to_owned(), key.trim().to_owned(), size));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline() -> Ratchet {
        let mut r = Ratchet::default();
        r.insert("docs/CLAUDE.md::The Tendril Method", 4139);
        r.insert("wf/image-push.yml::push/Build and push", 60);
        r
    }

    // ── The four lifted tests, unchanged in substance ───────────────────

    #[test]
    fn an_unrecorded_key_is_a_new_violation() {
        assert_eq!(baseline().judge("nope", 1), Verdict::Unrecorded);
    }

    #[test]
    fn recorded_debt_may_hold_or_shrink() {
        let r = baseline();
        assert_eq!(
            r.judge("wf/image-push.yml::push/Build and push", 60),
            Verdict::Held
        );
        assert_eq!(
            r.judge("wf/image-push.yml::push/Build and push", 12),
            Verdict::Held
        );
    }

    /// The seal. Without this, a baseline is an amnesty.
    #[test]
    fn recorded_debt_that_grows_by_one_fails() {
        assert_eq!(
            baseline().judge("wf/image-push.yml::push/Build and push", 61),
            Verdict::Grew {
                recorded: 60,
                grew: 1
            }
        );
    }

    #[test]
    fn parse_keeps_keys_containing_spaces() {
        let rows = parse_lines("entry: docs/CLAUDE.md::The Tendril Method 4139\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "entry");
        assert_eq!(rows[0].1, "docs/CLAUDE.md::The Tendril Method");
        assert_eq!(rows[0].2, 4139);
    }

    #[test]
    fn a_malformed_line_covers_nothing_rather_than_weakening_the_gate() {
        // No size, no colon, a bare comment, a non-numeric size — none may
        // become a recorded row, because a row is an exemption.
        let rows =
            parse_lines("# a comment\n\nentry: no-size-here\nnocolon 12\nentry: bad-size xyz\n");
        assert!(rows.is_empty(), "got {rows:?}");
    }

    // ── The orphan direction ────────────────────────────────────────────

    #[test]
    fn a_row_whose_subject_is_gone_is_an_orphan() {
        let r = baseline();
        let live: BTreeSet<String> = ["docs/CLAUDE.md::The Tendril Method".to_owned()]
            .into_iter()
            .collect();
        assert_eq!(
            r.orphans(&live),
            vec!["wf/image-push.yml::push/Build and push"]
        );
    }

    #[test]
    fn nothing_is_an_orphan_when_every_row_is_still_live() {
        let r = baseline();
        let live: BTreeSet<String> = r.keys().map(str::to_owned).collect();
        assert!(r.orphans(&live).is_empty());
    }

    /// The negative control. A predicate that returned every row would satisfy
    /// the test above while telling the operator nothing — the calibration
    /// discipline `sui`'s parity ledger records.
    #[test]
    fn orphans_does_not_simply_return_everything() {
        let r = baseline();
        let live: BTreeSet<String> = BTreeSet::new();
        // With NOTHING live, every row is an orphan — so the only way to tell a
        // correct implementation from a constant-true one is the pair of tests:
        // this asserts the all-orphan case, the one above asserts the none case.
        assert_eq!(r.orphans(&live).len(), r.len());
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn keys_are_deterministic_which_is_what_makes_a_baseline_diffable() {
        let mut a = Ratchet::default();
        a.insert("zebra", 1);
        a.insert("alpha", 2);
        let mut b = Ratchet::default();
        b.insert("alpha", 2);
        b.insert("zebra", 1);
        assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
        assert_eq!(a.keys().collect::<Vec<_>>(), vec!["alpha", "zebra"]);
    }

    /// The orphan count was computable from the OLD public API; naming them was
    /// not. This pins that the new method agrees with the old arithmetic, so the
    /// addition is an extension rather than a change.
    #[test]
    fn orphan_count_agrees_with_what_len_and_recorded_already_implied() {
        let r = baseline();
        let live: BTreeSet<String> = ["docs/CLAUDE.md::The Tendril Method".to_owned()]
            .into_iter()
            .collect();
        let countable_before = r.len() - live.iter().filter(|k| r.recorded(k).is_some()).count();
        assert_eq!(r.orphans(&live).len(), countable_before);
    }

    const PUSH: &str = "wf/image-push.yml::push/Build and push";
    const TENDRIL: &str = "docs/CLAUDE.md::The Tendril Method";

    #[test]
    fn shrunk_names_only_a_measurement_below_its_row() {
        let r = baseline();
        assert_eq!(
            r.shrunk(PUSH, 12),
            Some(Shrink {
                recorded: 60,
                now: 12
            })
        );
        assert_eq!(r.shrunk(PUSH, 60), None);
        assert_eq!(r.shrunk(PUSH, 61), None);
        assert_eq!(r.shrunk("nope", 0), None);
        assert_eq!(r.judge(PUSH, 12), Verdict::Held);
    }

    #[test]
    fn banking_lowers_a_shrunk_row_and_keeps_a_held_one() {
        let banked = baseline().bank([(PUSH, 12), (TENDRIL, 4139)], BankMode::Shrink);
        assert_eq!(banked.recorded(PUSH), Some(12));
        assert_eq!(banked.recorded(TENDRIL), Some(4139));
    }

    #[test]
    fn banking_never_raises_a_grown_row() {
        let banked = baseline().bank([(PUSH, 61), (TENDRIL, 9000)], BankMode::Record);
        assert_eq!(banked.recorded(PUSH), Some(60));
        assert_eq!(banked.recorded(TENDRIL), Some(4139));
        assert_eq!(
            banked.judge(PUSH, 61),
            Verdict::Grew {
                recorded: 60,
                grew: 1
            }
        );
    }

    #[test]
    fn banking_drops_an_orphan_row() {
        let banked = baseline().bank([(TENDRIL, 4000)], BankMode::Shrink);
        assert_eq!(banked.recorded(PUSH), None);
        assert_eq!(banked.len(), 1);
    }

    #[test]
    fn banking_records_a_new_scope_only_when_asked() {
        let shrink = baseline().bank([("new", 7), (PUSH, 60)], BankMode::Shrink);
        assert_eq!(shrink.recorded("new"), None);
        assert_eq!(shrink.judge("new", 7), Verdict::Unrecorded);
        let record = baseline().bank([("new", 7), (PUSH, 60)], BankMode::Record);
        assert_eq!(record.recorded("new"), Some(7));
        assert_eq!(record.judge("new", 7), Verdict::Held);
    }

    #[test]
    fn banking_is_a_fixed_point_once_banked() {
        let ms = [(PUSH, 12), (TENDRIL, 4000), ("new", 3)];
        let once = baseline().bank(ms, BankMode::Record);
        assert_eq!(once.bank(ms, BankMode::Record), once);
        assert!(
            !Baseline::parse(&once.render("entry"))
                .summarize(&Baseline::parse(&once.render("entry")))
                .is_red()
        );
    }

    #[test]
    fn a_summary_is_red_only_for_growth_or_an_unrecorded_scope() {
        let r = baseline();
        let green = r.summarize("entry", [(PUSH, 12)]);
        assert!(!green.is_red());
        assert_eq!(green.shrinks.len(), 1);
        assert_eq!(green.orphans.len(), 1);
        assert!(r.summarize("entry", [(PUSH, 61)]).is_red());
        assert!(r.summarize("entry", [("nope", 0)]).is_red());
        assert!(!r.summarize("entry", Vec::<(&str, usize)>::new()).is_red());
    }

    #[test]
    fn a_summary_prints_one_stable_report() {
        let s = baseline().summarize("entry", [(PUSH, 61), ("nope", 3), (TENDRIL, 4000)]);
        assert_eq!(
            s.to_string(),
            "unrecorded: entry: nope measures 3 and the baseline has no row for it\n\
             grew: entry: wf/image-push.yml::push/Build and push measures 61, past its recorded 60 by 1\n\
             shrunk: entry: docs/CLAUDE.md::The Tendril Method measures 4000, below its recorded 4139; `ratchet bank` banks it\n\
             red: 1 held, 1 grew, 1 unrecorded, 1 shrunk, 0 orphans\n"
        );
    }

    #[test]
    fn a_multi_kind_baseline_judges_banks_and_renders_per_kind() {
        let b = Baseline::parse("rust: b 5\nnix: a 3\nrust: a 9\n");
        assert_eq!(b.render(), "nix: a 3\nrust: a 9\nrust: b 5\n");
        assert_eq!(b.judge("nix", "b", 1), Verdict::Unrecorded);
        let m = Baseline::parse("rust: a 4\nrust: b 6\nnix: z 1\n");
        let s = b.summarize(&m);
        assert!(s.is_red());
        assert_eq!(s.orphans.len(), 1);
        assert_eq!(
            b.bank(&m, BankMode::Shrink).render(),
            "rust: a 4\nrust: b 5\n"
        );
        assert_eq!(
            b.bank(&m, BankMode::Record).render(),
            "nix: z 1\nrust: a 4\nrust: b 5\n"
        );
    }

    #[test]
    fn render_parses_back_to_the_same_baseline() {
        let b = Baseline::parse("entry: docs/CLAUDE.md::The Tendril Method 4139\nx: k:v 0\n");
        assert_eq!(Baseline::parse(&b.render()), b);
    }
}
