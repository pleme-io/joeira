//! The invocation journal — `(rule, verdict, repo, timestamp)`, appended.
//!
//! This is not telemetry and not a nicety. Every promotion in this design is
//! gated on evidence: a rule's floor may only be raised on a recorded firing
//! count *with its denominator*. Without a journal that count does not exist, so
//! **no floor can ever be raised honestly** — which makes the journal a
//! dependency of the phase plan rather than a late addition to it.
//!
//! # Append-only JSONL, not a keyed map
//!
//! `(rule, verdict, repo, timestamp)` is an **event**, and no field or
//! combination of fields is unique: the same rule fires on the same repository
//! many times. So a keyed map has nowhere to put the key, and keying on
//! `rule` — or on `rule + repo` — makes each firing overwrite the last.
//!
//! That is not hypothetical. `guardrail`'s write-journal is a keyed map and has a
//! test named `journal_overwrite_replaces_entry` pinning exactly that behaviour.
//! It is *correct* there, because the question is "is this file dangerous right
//! now?". It is fatal here, because the question is "how many times, over what
//! window?" — **a counting journal built on a keyed map counts to 1.**
//!
//! Five more reasons the shape is forced:
//!
//! 1. **The denominator is a second event stream.** `Limpo` and `NaoSeAplica`
//!    exist as distinct verdicts precisely so "skipped" is never counted as
//!    "checked", and [`Veredito::Cego`] is neither. All four arms are journalled
//!    as first-class rows; aggregating at write time throws away the denominator
//!    the promotion predicate demands.
//! 2. **Concurrency.** Every commit on the machine invokes the hook, and
//!    concurrent agent sessions share one checkout. An `O_APPEND` write under
//!    `PIPE_BUF` is atomic; read-modify-write of a whole JSON map is a
//!    lost-update race — and it loses in the **under-counting** direction, which
//!    argues against a floor raise the evidence actually supported.
//! 3. **Cost on the hot path.** A keyed map is read-all + parse-all +
//!    serialize-all + write-all on *every* invocation, growing with history,
//!    inside a hook whose budget is p99 ≤ 150 ms. An append is O(1) in journal
//!    size.
//! 4. **Crash tolerance.** A torn whole-file JSON map fails to parse, and
//!    `guardrail`'s loader then `unwrap_or_default()`s it — the entire history
//!    silently becomes empty (its own test pins this). Acceptable for a
//!    five-minute TTL cache; catastrophic for the record justifying a promotion.
//!    JSONL loses exactly the torn last line.
//! 5. **A map cannot be windowed.** "Was this rule firing before the novelty
//!    gate landed?" is the question a floor raise turns on, and only a
//!    timestamped log can answer it.
//!
//! # Tier::State, and why not the other two
//!
//! Resolved through `okiba`, never by hand — see [`caminho_padrao`].
//!
//! - **`State` (chosen)** — durable, not the operator's to hand-edit, and it must
//!   survive a cache wipe.
//! - **`Runtime`** — okiba deliberately gives it no `$HOME`-relative default
//!   (`base` returns `NoSpecDefault`), and its contract is *must not survive the
//!   login session*. An accumulator that justifies a floor raise weeks later
//!   cannot live in a directory defined by being deleted at logout.
//! - **`Cache`** — "a system cleaner may delete it at any moment, and putting a
//!   ledger there means losing it is correct behaviour by somebody else's tool."
//!
//! # One deliberate divergence from `banken`'s ledger
//!
//! The append below is `banken`'s `glass.rs` almost verbatim, with one thing
//! removed: **no `sync_all()`**. banken fsyncs because its ledger must be
//! write-ahead of a live effect — the record must not be lost while the effect
//! persists. This journal is written *after* the verdict and is a statistical
//! accumulator: losing the last few rows to a power cut does not corrupt a
//! corpus of hundreds of thousands, and an fsync per invocation is a real cost
//! against the p99 budget on the commit path. Stated here so the omission reads
//! as a decision rather than as banken's discipline forgotten.
//!
//! And not `guardrail`'s `cache.rs`, which still resolves XDG by hand and yields
//! a *relative* path when `HOME` is unset.

use std::fs::OpenOptions;
use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::{JoeiraError, Veredito};

/// One journalled invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registro {
    /// The rule's name.
    pub regra: String,
    /// Which of the four verdicts, as a stable tag.
    pub veredito: TagVeredito,
    /// The repository the rule was evaluated against.
    pub repo: String,
    /// Unix seconds.
    pub quando: u64,
    /// The severity the rule acted at, so a later reader can tell an advisory
    /// firing from a gating one without re-deriving the lattice.
    pub severidade: Option<crate::Severidade>,
}

/// The four verdict classes, flattened for the wire.
///
/// A separate type from [`Veredito`] on purpose: the verdict carries a message
/// and a reason, and journalling those would put rule prose — and, for a
/// credential rule, potentially the matched line — into a file on disk. The
/// journal needs the *class*, never the content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TagVeredito {
    /// The predicate did not hold.
    Limpo,
    /// The predicate held.
    Achado,
    /// The rule did not apply.
    NaoSeAplica,
    /// The environment could not answer.
    Cego,
}

impl From<&Veredito> for TagVeredito {
    fn from(v: &Veredito) -> Self {
        match v {
            Veredito::Limpo => Self::Limpo,
            Veredito::Achado { .. } => Self::Achado,
            Veredito::NaoSeAplica { .. } => Self::NaoSeAplica,
            Veredito::Cego { .. } => Self::Cego,
        }
    }
}

/// Where the journal lives.
///
/// `okiba` rather than a hand-rolled `$XDG_STATE_HOME` read: okiba applies the
/// spec rule that a relative or empty override is *ignored* rather than joined,
/// which is the whole bug class here. A cwd-relative journal means every
/// invocation from a different directory reads and writes a different history —
/// and it fails toward permitting, because an empty journal looks like a clean
/// one.
///
/// Returns `None` when okiba cannot resolve a base. A journal that cannot be
/// placed is not a reason to fail a commit; see [`Diario::registrar`].
#[must_use]
pub fn caminho_padrao() -> Option<okiba::AbsPath> {
    okiba::Okiba::for_app("joeira")
        .try_path(okiba::Tier::State, "invocations.jsonl")
        .ok()
}

/// The journal.
pub struct Diario {
    caminho: okiba::AbsPath,
}

impl Diario {
    /// Open the journal at okiba's `State` path.
    #[must_use]
    pub fn padrao() -> Option<Self> {
        caminho_padrao().map(|caminho| Self { caminho })
    }

    /// Open a journal at an explicit path — the test seam, and the sweep seam
    /// for a run that must not touch the operator's own history.
    #[must_use]
    pub const fn em(caminho: okiba::AbsPath) -> Self {
        Self { caminho }
    }

    #[must_use]
    pub fn caminho(&self) -> &okiba::AbsPath {
        &self.caminho
    }

    /// Append one record.
    ///
    /// **Journalling never fails a commit.** A full disk, a read-only state dir
    /// or a missing parent must not turn into a refused commit: the journal
    /// exists to measure the gate, and a measurement apparatus that can block
    /// the thing it measures is worse than no measurement. The error is returned
    /// so a caller that *is* auditing the journal (the promotion path) can
    /// surface it, and dropped by the hook path.
    pub fn registrar(&self, r: &Registro) -> Result<(), JoeiraError> {
        if let Some(dir) = self.caminho.as_path().parent() {
            std::fs::create_dir_all(dir).map_err(|e| {
                JoeiraError::AmbienteCego(format!(
                    "cannot create the journal directory {}: {e}",
                    dir.display()
                ))
            })?;
        }
        let mut linha = serde_json::to_string(r).map_err(|e| {
            JoeiraError::AmbienteCego(format!("cannot serialize a journal record: {e}"))
        })?;
        linha.push('\n');

        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.caminho.as_path())
            .map_err(|e| {
                JoeiraError::AmbienteCego(format!(
                    "cannot open the journal at {}: {e}",
                    self.caminho.as_path().display()
                ))
            })?;
        f.write_all(linha.as_bytes()).map_err(|e| {
            JoeiraError::AmbienteCego(format!("cannot write a journal record: {e}"))
        })?;
        // No `sync_all()` — see the module docs for why this diverges from
        // banken's ledger deliberately.
        Ok(())
    }

    /// Every record the journal holds.
    ///
    /// A malformed line is **skipped, not fatal**: a truncated final line is what
    /// a crash mid-append looks like, and refusing to read the whole journal
    /// because of one would destroy its usefulness at the moment it matters most.
    /// An absent file is `Ok(vec![])` — no invocation has been journalled, which
    /// is a fact rather than an error.
    pub fn registros(&self) -> Result<Vec<Registro>, JoeiraError> {
        match std::fs::read_to_string(self.caminho.as_path()) {
            Ok(texto) => Ok(texto
                .lines()
                .filter(|l| !l.trim().is_empty())
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(JoeiraError::AmbienteCego(format!(
                "cannot read the journal at {}: {e}",
                self.caminho.as_path().display()
            ))),
        }
    }

    /// Per-rule counts **with the denominator**, which is the shape a promotion
    /// argument needs and the shape a bare hit count hides.
    ///
    /// Returns `(rule, achados, total)` sorted by rule. `total` counts every
    /// invocation of that rule regardless of verdict, so a reader can see
    /// "fired 3 times out of 40,000" rather than "fired 3 times".
    #[must_use]
    pub fn contagem(registros: &[Registro]) -> Vec<(String, usize, usize)> {
        let mut por_regra: std::collections::BTreeMap<&str, (usize, usize)> =
            std::collections::BTreeMap::new();
        for r in registros {
            let e = por_regra.entry(r.regra.as_str()).or_insert((0, 0));
            e.1 += 1;
            if r.veredito == TagVeredito::Achado {
                e.0 += 1;
            }
        }
        por_regra
            .into_iter()
            .map(|(k, (a, t))| (k.to_owned(), a, t))
            .collect()
    }
}

/// Unix seconds now. `0` if the clock is before the epoch, which is not a case
/// worth an error type.
#[must_use]
pub fn agora() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Severidade;

    fn diario_temporario(nome: &str) -> Diario {
        // A path under the process temp dir, never okiba's real State dir — a
        // test must not append to the operator's own history.
        let mut p = std::env::temp_dir();
        p.push(format!(
            "joeira-journal-test-{nome}-{}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&p);
        Diario::em(okiba::AbsPath::new(p).expect("temp_dir is absolute"))
    }

    fn registro(regra: &str, v: TagVeredito) -> Registro {
        Registro {
            regra: regra.to_owned(),
            veredito: v,
            repo: "pleme-io/joeira".to_owned(),
            quando: 1_755_000_000,
            severidade: Some(Severidade::Consultivo),
        }
    }

    #[test]
    fn an_absent_journal_reads_as_empty_not_as_an_error() {
        let d = diario_temporario("absent");
        assert_eq!(d.registros().expect("absent is not an error"), vec![]);
    }

    /// THE property a keyed map cannot have: the same rule firing twice on the
    /// same repo is two records, not one.
    #[test]
    fn the_same_rule_twice_is_two_records() {
        let d = diario_temporario("append");
        d.registrar(&registro("r", TagVeredito::Achado))
            .expect("write");
        d.registrar(&registro("r", TagVeredito::Achado))
            .expect("write");
        assert_eq!(d.registros().expect("read").len(), 2);
    }

    #[test]
    fn a_torn_final_line_loses_only_itself() {
        let d = diario_temporario("torn");
        d.registrar(&registro("r", TagVeredito::Achado))
            .expect("write");
        // Simulate a crash mid-append.
        let mut f = OpenOptions::new()
            .append(true)
            .open(d.caminho().as_path())
            .expect("open");
        f.write_all(b"{\"regra\":\"r\",\"vered")
            .expect("partial write");
        drop(f);
        let rows = d.registros().expect("a torn line must not be fatal");
        assert_eq!(rows.len(), 1, "the intact record survives");
    }

    /// The denominator is the point. A bare hit count of 1 reads the same
    /// whether the rule saw 1 commit or 40,000.
    #[test]
    fn counts_carry_their_denominator() {
        let d = diario_temporario("count");
        d.registrar(&registro("a", TagVeredito::Achado)).expect("w");
        d.registrar(&registro("a", TagVeredito::Limpo)).expect("w");
        d.registrar(&registro("a", TagVeredito::Limpo)).expect("w");
        d.registrar(&registro("b", TagVeredito::Limpo)).expect("w");
        let c = Diario::contagem(&d.registros().expect("read"));
        assert_eq!(c, vec![("a".into(), 1, 3), ("b".into(), 0, 1)]);
    }

    /// `NaoSeAplica` and `Cego` must never be counted as `Limpo` — the whole
    /// reason all four arms are journalled rather than a boolean.
    #[test]
    fn skipped_and_blind_are_not_counted_as_findings_nor_lost() {
        let d = diario_temporario("arms");
        d.registrar(&registro("r", TagVeredito::NaoSeAplica))
            .expect("w");
        d.registrar(&registro("r", TagVeredito::Cego)).expect("w");
        let rows = d.registros().expect("read");
        let c = Diario::contagem(&rows);
        // 0 findings, but the denominator still counts both invocations.
        assert_eq!(c, vec![("r".into(), 0, 2)]);
        // And the classes survive the round trip distinctly.
        assert_eq!(rows[0].veredito, TagVeredito::NaoSeAplica);
        assert_eq!(rows[1].veredito, TagVeredito::Cego);
    }

    #[test]
    fn the_verdict_tag_maps_every_arm() {
        assert_eq!(TagVeredito::from(&Veredito::Limpo), TagVeredito::Limpo);
        assert_eq!(
            TagVeredito::from(&Veredito::Achado {
                regra: "r".into(),
                severidade: Severidade::Bloqueia,
                mensagem: "m".into()
            }),
            TagVeredito::Achado
        );
        assert_eq!(
            TagVeredito::from(&Veredito::NaoSeAplica {
                regra: "r".into(),
                porque: "p".into()
            }),
            TagVeredito::NaoSeAplica
        );
        assert_eq!(
            TagVeredito::from(&Veredito::Cego {
                regra: "r".into(),
                porque: "p".into()
            }),
            TagVeredito::Cego
        );
    }

    /// The journal records the CLASS, never the content — a credential rule's
    /// matched line must not reach disk.
    #[test]
    fn a_record_carries_no_rule_prose() {
        let v = Veredito::Achado {
            regra: "sec-plaintext-password".into(),
            severidade: Severidade::Bloqueia,
            mensagem: "plaintext password assignment".into(),
        };
        let r = Registro {
            regra: "sec-plaintext-password".into(),
            veredito: TagVeredito::from(&v),
            repo: "x".into(),
            quando: 1,
            severidade: Some(Severidade::Bloqueia),
        };
        let wire = serde_json::to_string(&r).expect("serialize");
        assert!(!wire.contains("plaintext password assignment"));
    }

    #[test]
    fn okiba_resolves_a_state_path_that_is_absolute() {
        // Resolution only — nothing is written to it.
        if let Some(p) = caminho_padrao() {
            assert!(p.as_path().is_absolute());
            assert!(p.as_path().ends_with("invocations.jsonl"));
        }
    }
}
