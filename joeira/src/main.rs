//! `joeira` — the typed git-hook control plane.
//!
//! **TIER: P0. This binary reads and PROVES; it does not yet gate a commit.**
//! There is deliberately no `pre-commit` subcommand: P0's done-predicate is the
//! oracle and the gate, "enforcing nothing new", and shipping a gating
//! entrypoint before the differential against the incumbent is green would
//! invert that order.
//!
//! The honest ceiling, restated because no phase of this plan may claim
//! otherwise: a local git hook cannot guarantee anything. There are four
//! bypasses and the fourth is not a deliberate act — `HOME` divergence, where a
//! missing `core.hooksPath` directory is not an error, which is the default
//! state of every container, sandbox, CI runner and daemon.

mod oraculo;

use clap::{Parser, Subcommand};
use joeira_core::{AmbienteMock, Ponto, Predicado, Regra, Severidade, Veredito, avalia, prova};

#[derive(Parser)]
#[command(name = "joeira", about = "The typed git-hook control plane")]
struct Cli {
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Subcommand)]
enum Comando {
    /// Evaluate the built-in corpus against a synthetic environment and print
    /// every verdict with its severity and ceiling.
    Eval,
    /// Prove every rule in both directions against its own witnesses. Exits
    /// non-zero on a red row OR on an empty corpus.
    Prova {
        /// Prove the WHOLE catalog and print its denominator, refusing to
        /// report green if any catalog form was not reached.
        ///
        /// It exists because "every rule passed" and "every rule I happened to
        /// build passed" print identically. `--all` makes the second one an
        /// error by comparing the corpus against the catalog it came from.
        #[arg(long)]
        all: bool,
    },
    /// Print the mount points and what each may read.
    Pontos,
    /// Differential against the DEPLOYED incumbent hooks over real history.
    ///
    /// Exits non-zero on an un-triaged disagreement, on an empty denominator,
    /// or if the deployed hooks are absent — never reports agreement against a
    /// hook that does not run.
    Oraculo {
        /// Repository to walk. Read-only; the index written is a scratch
        /// clone's own.
        #[arg(long, default_value = ".")]
        repo: std::path::PathBuf,
        /// How many non-merge commits.
        #[arg(long, default_value_t = 500)]
        n: usize,
    },
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().comando {
        Comando::Eval => cmd_eval(),
        Comando::Prova { all } => cmd_prova(all),
        Comando::Oraculo { repo, n } => {
            let regras = corpus()?;
            let linhas = oraculo::corre(&repo, n, &regras)?;
            oraculo::relata(&linhas)
        }
        Comando::Pontos => {
            for p in Ponto::todos() {
                println!("  {:<12} reads {:?}", p.arquivo(), p.leitura());
            }
            Ok(())
        }
    }
}

fn cmd_eval() -> anyhow::Result<()> {
    let regras = corpus()?;
    // A synthetic index carrying one of everything, so `eval` demonstrates each
    // arm rather than needing a repository.
    let amb = AmbienteMock::com_mensagem("init")
        .com_stage("flake.lock", "a\n<<<<<<< HEAD\nb\n>>>>>>> other\n")
        .com_head("app.conf", "listen = 0\n")
        .com_stage("app.conf", "listen = 0\npassword: correcthorsebattery\n");

    println!("corpus: {} rules", regras.len());
    for r in &regras {
        let v = avalia(r, &amb);
        let marca = match &v {
            Veredito::Achado { .. } => "FOUND ",
            Veredito::Limpo => "clean ",
            Veredito::NaoSeAplica { .. } => "n/a   ",
            Veredito::Cego { .. } => "BLIND ",
        };
        println!(
            "  {marca} {:<28} floor={:?} ceiling={:?}{}",
            r.nome(),
            r.severidade(),
            r.tecto(),
            if v.gateia() { "  [would gate]" } else { "" }
        );
    }
    Ok(())
}

fn cmd_prova(all: bool) -> anyhow::Result<()> {
    let regras = corpus()?;

    if all {
        // The denominator check, and it is the reason `--all` exists. `corpus`
        // already errors on an unwitnessed form, so this is belt-and-braces
        // against a future change that made the join lossy instead of fatal.
        let no_catalogo = joeira_lisp::ler_catalogo(joeira_lisp::CATALOGO)
            .map_err(|e| anyhow::anyhow!("catalog: {e}"))?
            .len();
        println!(
            "catalog: {no_catalogo} forms, corpus: {} rules",
            regras.len()
        );
        anyhow::ensure!(
            regras.len() == no_catalogo,
            "corpus has {} rules but the catalog declares {no_catalogo} —              refusing to report green over a subset",
            regras.len()
        );
    }

    let (n, rows) = prova(&regras);

    // An empty corpus reports the same green as a proven one unless it is
    // refused explicitly. This is the vacuity shape the fleet keeps
    // rediscovering, so the count is asserted before the rows are read.
    anyhow::ensure!(n > 0, "corpus is EMPTY — refusing to report green");

    let vermelhas = rows.iter().filter(|r| !r.verde()).count();
    for r in &rows {
        println!(
            "  {} {:<28} refuses-its-block-witness={} passes-its-pass-witness={}",
            if r.verde() { "ok  " } else { "FAIL" },
            r.regra,
            r.bloqueia_ok,
            r.passa_ok
        );
    }
    // Count AND denominator, always — a bare "ok" hides how much was examined.
    println!(
        "prova: {}/{n} rules green in both directions",
        n - vermelhas
    );
    anyhow::ensure!(vermelhas == 0, "{vermelhas} of {n} rules are red");
    Ok(())
}

/// SYNTHETIC-FIXTURE — the witnesses below are invented test data, never real
/// secrets. See `joeira-core/src/tests.rs` for why the marker is here.
///
/// ── THE SPLIT, AND WHY IT IS WHERE IT IS ─────────────────────────────────
///
/// The rule's IDENTITY comes from `joeira-lisp`'s catalog — name, mount point,
/// both severity axes, the operator message, and the 77-word placeholder list.
/// Its WITNESSES come from here, because a witness is a mock `AmbienteGit` and
/// tatara-lisp has no way to spell one; inventing a lisp surface for mock
/// filesystems would be a richer authoring language than the derive will ever
/// generate, which is the trap `joeira-lisp`'s own header warns about.
///
/// The two halves are joined BY NAME, and the join is asserted total in both
/// directions (`the_catalog_and_the_witnesses_are_in_bijection`). That matters
/// more than it looks: without the reverse check, a catalog form with no
/// witnesses would be silently skipped and `prova` would report green over a
/// corpus smaller than the catalog it claims to prove.
///
/// The FLOOR is also here rather than in the catalog. A floor is a deployment
/// decision — what this installation does today — not part of the rule's
/// identity, and the fleet's authority for it is `pleme.gitHooks.rules.<n>.floor`
/// in the nix repo, where it is clamped to the derived ceiling. Nothing here may
/// exceed a ceiling either: `Regra::com_piso` clamps, so a floor written above
/// the axes' ceiling is corrected rather than honoured.
fn corpus() -> anyhow::Result<Vec<Regra>> {
    let formas = joeira_lisp::ler_catalogo(joeira_lisp::CATALOGO)
        .map_err(|e| anyhow::anyhow!("the compiled-in catalog does not parse: {e}"))?;
    anyhow::ensure!(
        !formas.is_empty(),
        "the catalog is EMPTY — refusing to build a corpus that proves nothing"
    );

    let mut regras = Vec::with_capacity(formas.len());
    for f in &formas {
        let t = testemunhas(f).ok_or_else(|| {
            anyhow::anyhow!(
                "catalog form {:?} has no witnesses in this binary — a rule that \
                 cannot be proven in both directions must not ship silently",
                f.nome
            )
        })?;
        let rev = f
            .reversibilidade
            .ok_or_else(|| anyhow::anyhow!("catalog form {:?} omits :reversibilidade", f.nome))?;
        let fp =
            f.fp.ok_or_else(|| anyhow::anyhow!("catalog form {:?} omits :fp", f.nome))?;

        let mut r = Regra::nova(
            &f.nome,
            f.ponto,
            t.predicado,
            rev,
            fp,
            &f.mensagem,
            t.bloqueia,
            t.passa,
        );
        if let Some(piso) = t.piso {
            r = r.com_piso(piso);
        }
        regras.push(r);
    }
    Ok(regras)
}

/// What the catalog cannot carry: the predicate, its two witnesses, and the
/// floor this installation acts at.
struct Testemunhas {
    predicado: Predicado,
    bloqueia: AmbienteMock,
    passa: AmbienteMock,
    piso: Option<Severidade>,
}

/// Every name the catalog may use. `None` for an unknown name rather than a
/// panic, so `corpus` can report WHICH form is unwitnessed.
fn testemunhas(f: &joeira_lisp::FormaLida) -> Option<Testemunhas> {
    Some(match f.nome.as_str() {
        "msg-placeholder-subject" => Testemunhas {
            // BOTH arms, and neither is optional. The whole lowercased-trimmed
            // subject catches `init`; the tail after the last colon catches
            // `chore: wip`. Dropping either makes the corpus disagree with the
            // incumbent on a class rather than on a word, which would read as a
            // behavioural difference when it is a missing arm.
            predicado: Predicado::Algum(vec![
                Predicado::AssuntoNaLista {
                    lista: f.lista.clone(),
                },
                Predicado::AssuntoCaudaNaLista {
                    lista: f.lista.clone(),
                },
            ]),
            bloqueia: AmbienteMock::com_mensagem("init"),
            passa: AmbienteMock::com_mensagem("joeira: port the incumbent concerns"),
            piso: None,
        },
        "msg-ai-attribution-trailer" => Testemunhas {
            predicado: Predicado::MensagemTemTrailer {
                marcadores: vec!["Co-Authored-By:".into(), "Claude-Session:".into()],
            },
            bloqueia: AmbienteMock::com_mensagem("feat: x\n\nClaude-Session: abc"),
            passa: AmbienteMock::com_mensagem("feat: x\n\nplain body"),
            piso: None,
        },
        "vcs-conflict-markers" => Testemunhas {
            predicado: Predicado::BlobTemParDeLinhas {
                abre: "<<<<<<< ".into(),
                fecha: ">>>>>>> ".into(),
            },
            bloqueia: AmbienteMock::default()
                .com_stage("flake.lock", "<<<<<<< HEAD\nx\n>>>>>>> them\n"),
            // A LONE `=======` line, deliberately: it is the marker most likely
            // to appear in legitimate content (a reStructuredText heading rule,
            // an ASCII table), and a rule that tripped on it would be a prose
            // matcher wearing a structural label.
            passa: AmbienteMock::default().com_stage("README.md", "# T\n\n=======\n"),
            piso: Some(Severidade::Bloqueia),
        },
        "gen-lock-tie" => Testemunhas {
            predicado: Predicado::AmarraDeHash {
                arquivo: "Cargo.lock".into(),
                sidecar: "Cargo.gen.lock".into(),
                campo: "cargo_lock_sha256".into(),
            },
            bloqueia: AmbienteMock::default()
                .com_stage("Cargo.lock", "l")
                .com_stage("Cargo.gen.lock", r#"{"cargo_lock_sha256": "deadbeef"}"#)
                .com_sha("Cargo.lock", "cafebabe"),
            passa: AmbienteMock::default()
                .com_stage("Cargo.lock", "l")
                .com_stage("Cargo.gen.lock", r#"{"cargo_lock_sha256": "cafebabe"}"#)
                .com_sha("Cargo.lock", "cafebabe"),
            piso: Some(Severidade::Bloqueia),
        },
        // The prose matcher, at the ratchet tier its axes derive. Its witnesses
        // are the two shapes that matter: a real credential line, and the
        // `secret::` path expression that made the incumbent refuse a commit
        // containing no credential at all.
        "sec-plaintext-credential" => Testemunhas {
            // ADDED lines, not the whole blob — the same judgement the
            // incumbent makes. A rule reading the whole blob refuses a commit
            // for a credential that was already there, which punishes the
            // author who touched the file rather than the one who added it.
            predicado: Predicado::LinhaAdicionadaCasa {
                padroes: vec![
                    joeira_core::Padrao::novo(
                        "plaintext-credential",
                        // Every narrowing here is the incumbent's, measured
                        // against its false-positive corpus rather than
                        // invented. `(^|[^A-Za-z0-9])` is the whole-word
                        // lookbehind this engine does not have, so `mysecret:`
                        // and `SecretBearing` no longer match while
                        // `client_secret =` still does. `[^:\s]` after the
                        // separator is what stops a path expression
                        // (`std::secret::x`) reading as `secret:` followed by
                        // a long value — found by the gate refusing a commit
                        // that contained no credential. `\S{7,}` is what lets
                        // `password: <redacted>` through.
                        // TWO arms with DIFFERENT length floors, and the
                        // asymmetry is measured rather than stylistic. The
                        // incumbent uses 7 for `pass(word|wd|phrase)` and 15
                        // for `secret`, because `secret` is an ordinary word in
                        // English and in code while `password` is nearly always
                        // a credential key.
                        //
                        // Collapsing both to 7 was a joeira FALSE POSITIVE, and
                        // the oracle caught it on a real commit: the Nix line
                        // `lookupOf = name: secret: secret.key or name;` is
                        // function-argument syntax, and at 7 it reads as
                        // `secret:` followed by `ecret.key`. At 15 there are
                        // only 10 non-space characters available and it does
                        // not fire. Adopting the incumbent's floor rather than
                        // inventing one.
                        r"(?i)pass(word|wd|phrase)\s*[:=]\s*[^:/\s<{$][^\s<{$]{7,}",
                    )
                    .ok()?,
                    joeira_core::Padrao::novo(
                        "plaintext-secret-assignment",
                        r"(?i)(^|[^A-Za-z0-9])((client|api|app|consumer)[_-])?secret([_-](key|token|access[_-]key))?\s*[:=]\s*[^:/\s<{$][^\s<{$]{15,}",
                    )
                    .ok()?,
                ],
            },
            bloqueia: AmbienteMock::default()
                .com_stage("notes.txt", "password: correcthorsebattery\n"),
            passa: AmbienteMock::default().com_stage(
                "lib.rs",
                "use cofre_secret::Secret::new;\nlet x = std::secret::y;\n",
            ),
            piso: None,
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The join between the catalog and the witnesses, asserted in BOTH
    /// directions.
    ///
    /// The forward direction is already fatal in `corpus` — an unwitnessed form
    /// is an error, not a skip. This pins the reverse: a witness arm whose
    /// catalog form was deleted becomes dead code that no longer proves
    /// anything, and nothing else would say so. Counted rather than inspected,
    /// because a count is what `prova --all` compares.
    #[test]
    fn the_catalog_and_the_witnesses_are_in_bijection() {
        let formas = joeira_lisp::ler_catalogo(joeira_lisp::CATALOGO).expect("catalog parses");
        let regras = corpus().expect("corpus builds");
        assert_eq!(
            regras.len(),
            formas.len(),
            "every catalog form must reach the corpus"
        );

        // The reverse: name every arm `testemunhas` answers, and require each to
        // be a form. Kept as a literal list on purpose — deriving it from the
        // catalog would make the test compare the catalog to itself.
        let arms = [
            "msg-placeholder-subject",
            "msg-ai-attribution-trailer",
            "vcs-conflict-markers",
            "gen-lock-tie",
            "sec-plaintext-credential",
        ];
        for a in arms {
            assert!(
                formas.iter().any(|f| f.nome == a),
                "witness arm {a:?} has no catalog form — it proves nothing"
            );
        }
        assert_eq!(arms.len(), formas.len(), "no arm is unreachable");
    }

    /// The corpus carries the REAL list, not the six words the binary shipped
    /// with. Asserted here as well as in `joeira-lisp` because this is the
    /// surface the oracle will compare against the incumbent, and a size
    /// difference would read as a behavioural disagreement.
    #[test]
    fn the_placeholder_rule_carries_all_77_words() {
        let formas = joeira_lisp::ler_catalogo(joeira_lisp::CATALOGO).expect("parses");
        let msg = formas
            .iter()
            .find(|f| f.nome == "msg-placeholder-subject")
            .expect("present");
        assert_eq!(msg.lista.len(), 77);
        let t = testemunhas(msg).expect("witnessed");
        // Both arms, each carrying the full list — the reason this is checked
        // through the predicate rather than the form is that a bug cloning only
        // one arm would leave the form correct and the rule half-blind.
        match t.predicado {
            Predicado::Algum(ref arms) => {
                assert_eq!(arms.len(), 2, "whole-subject AND tail-after-colon");
                for a in arms {
                    match a {
                        Predicado::AssuntoNaLista { lista }
                        | Predicado::AssuntoCaudaNaLista { lista } => {
                            assert_eq!(lista.len(), 77, "both arms carry the full list");
                        }
                        outro => panic!("unexpected arm: {outro:?}"),
                    }
                }
            }
            ref outro => panic!("expected Algum, got {outro:?}"),
        }
    }
}
