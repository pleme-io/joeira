//! Mock-green: every test runs against `AmbienteMock`, so the suite touches no
//! repository, spawns no process and writes no file.
//!
//! The four incumbent concerns are ported here as rules, because a port that is
//! not asserted against the behaviour it replaces is a rewrite, not a port.
//!
//! SYNTHETIC-FIXTURE — every credential-shaped string below is invented test
//! data, never a real secret. `correcthorsebattery` is the canonical example
//! passphrase; the tokens are shape-only. The marker is deliberate and is the
//! fleet's own sanctioned escape for a repo that TESTS credential handling:
//! `blockSecrets` refused this very file on first commit, and the correct answer
//! is this marker rather than `--no-verify`, which would have disarmed the D2
//! tie and the conflict-marker gate along with it.

use super::*;

// ═══════════════════════════════════════════════════════════════════
// The severity projection
// ═══════════════════════════════════════════════════════════════════

#[test]
fn tecto_is_total_over_both_axes() {
    // Every cell is reachable and none panics — the projection is total, so a
    // new (reversibility, fp) pair cannot silently fall through.
    let revs = [
        Reversibilidade::Irreversivel,
        Reversibilidade::Custoso,
        Reversibilidade::Recuperavel,
    ];
    let fps = [
        ClasseFalsoPositivo::ZeroEstrutural,
        ClasseFalsoPositivo::TokenExato,
        ClasseFalsoPositivo::Limiar,
        ClasseFalsoPositivo::Prosa,
    ];
    let mut seen = 0;
    for r in revs {
        for f in fps {
            let _ = Severidade::tecto(r, f);
            seen += 1;
        }
    }
    assert_eq!(seen, 12, "12 cells, all evaluated");
}

/// The cell the doctrine argues about, pinned: an irreversible class defended by
/// a PROSE matcher may not block.
#[test]
fn irreversible_prose_does_not_reach_block() {
    let t = Severidade::tecto(Reversibilidade::Irreversivel, ClasseFalsoPositivo::Prosa);
    assert_eq!(t, Severidade::AvisaComCatraca);
    assert!(!t.gateia());
}

/// The other end: an irreversible class with a structurally-FP-free matcher is
/// exactly what blocking is for.
#[test]
fn irreversible_structural_reaches_block() {
    assert_eq!(
        Severidade::tecto(
            Reversibilidade::Irreversivel,
            ClasseFalsoPositivo::ZeroEstrutural
        ),
        Severidade::Bloqueia
    );
}

/// A floor above the derived ceiling is not refused — it is inexpressible. The
/// clamp is what makes "an unjustified block" have no representation.
#[test]
fn floor_is_clamped_to_the_derived_ceiling() {
    let r = regra_assunto_placeholder().com_piso(Severidade::Bloqueia);
    // TokenExato × Custoso ⇒ ceiling is Avisa, so the Bloqueia floor clamps.
    assert_eq!(r.tecto(), Severidade::Avisa);
    assert_eq!(r.severidade(), Severidade::Avisa);
    assert!(!r.severidade().gateia());
}

#[test]
fn rules_are_born_advisory_even_when_the_ceiling_is_block() {
    let r = regra_marcadores_de_conflito();
    assert_eq!(r.tecto(), Severidade::Bloqueia);
    assert_eq!(
        r.severidade(),
        Severidade::Consultivo,
        "birth state is advisory, deliberately, including for a blockable rule"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Ponto
// ═══════════════════════════════════════════════════════════════════

#[test]
fn every_ponto_has_a_file_and_a_reading() {
    // Wildcard-free: adding a Ponto without deciding these is a compile error,
    // and this asserts the census stays in step with the enum.
    assert_eq!(Ponto::todos().len(), 3);
    for p in Ponto::todos() {
        assert!(!p.arquivo().is_empty());
        let _ = p.leitura();
    }
}

#[test]
fn commit_msg_reads_the_message_and_pre_commit_reads_the_index() {
    assert_eq!(Ponto::CommitMsg.leitura(), Leitura::Mensagem);
    assert_eq!(Ponto::PreCommit.leitura(), Leitura::Indice);
    assert_eq!(Ponto::CommitMsg.arquivo(), "commit-msg");
}

// ═══════════════════════════════════════════════════════════════════
// The four incumbent concerns, ported
// ═══════════════════════════════════════════════════════════════════

/// Concern 1 — the placeholder-subject guard. 77 words live; three here.
fn regra_assunto_placeholder() -> Regra {
    Regra::nova(
        "msg-placeholder-subject",
        Ponto::CommitMsg,
        Predicado::Algum(vec![
            Predicado::AssuntoNaLista {
                lista: vec!["init".into(), "update".into(), "wip".into()],
            },
            Predicado::AssuntoCaudaNaLista {
                lista: vec!["init".into(), "update".into(), "wip".into()],
            },
        ]),
        Reversibilidade::Custoso,
        ClasseFalsoPositivo::TokenExato,
        "refusing a placeholder subject — say what changed and why",
        AmbienteMock::com_mensagem("init"),
        AmbienteMock::com_mensagem("joeira: port the incumbent concerns"),
    )
}

#[test]
fn placeholder_subject_refuses_the_bare_word() {
    let r = regra_assunto_placeholder();
    assert!(matches!(
        avalia(&r, &AmbienteMock::com_mensagem("init")),
        Veredito::Achado { .. }
    ));
    assert!(matches!(
        avalia(&r, &AmbienteMock::com_mensagem("  INIT  ")),
        Veredito::Achado { .. }
    ));
}

/// The scope-prefix hole the incumbent measured: `updates: init` is the
/// placeholder wearing a prefix, and it landed on main once.
#[test]
fn placeholder_subject_catches_the_scope_prefix_form() {
    let r = regra_assunto_placeholder();
    assert!(matches!(
        avalia(&r, &AmbienteMock::com_mensagem("updates: init")),
        Veredito::Achado { .. }
    ));
    assert!(matches!(
        avalia(&r, &AmbienteMock::com_mensagem("feat(scope): wip")),
        Veredito::Achado { .. }
    ));
}

/// The asymmetry that keeps recovery paths working. Whole-subject SET
/// membership, never prefix — so revert/merge/fixup and the accepted
/// `init: <what changed>` form all pass.
#[test]
fn placeholder_subject_leaves_recovery_paths_alone() {
    let r = regra_assunto_placeholder();
    for aceito in [
        "init: wire the loader",
        r#"Revert "init""#,
        "Merge branch 'main' into topic",
        "fixup! init",
        "update the parser to handle nested forms",
    ] {
        assert!(
            matches!(
                avalia(&r, &AmbienteMock::com_mensagem(aceito)),
                Veredito::Limpo
            ),
            "must pass: {aceito}"
        );
    }
}

/// Concern 2 — the AI-attribution trailer strip. Both forms: catching one
/// member of a pair reads exactly like catching the pair.
fn regra_trailer_ia() -> Regra {
    Regra::nova(
        "msg-ai-attribution-trailer",
        Ponto::CommitMsg,
        Predicado::MensagemTemTrailer {
            marcadores: vec!["Co-Authored-By:".into(), "Claude-Session:".into()],
        },
        Reversibilidade::Custoso,
        ClasseFalsoPositivo::TokenExato,
        "AI-attribution trailer present",
        AmbienteMock::com_mensagem("feat: x\n\nCo-Authored-By: someone"),
        AmbienteMock::com_mensagem("feat: x\n\nplain body"),
    )
}

#[test]
fn trailer_rule_catches_both_forms_not_just_the_famous_one() {
    let r = regra_trailer_ia();
    for m in [
        "feat: x\n\nCo-Authored-By: someone",
        "feat: x\n\nClaude-Session: abc123",
        "feat: x\n\n  Claude-Session: indented",
    ] {
        assert!(
            matches!(
                avalia(&r, &AmbienteMock::com_mensagem(m)),
                Veredito::Achado { .. }
            ),
            "must catch: {m}"
        );
    }
    assert!(matches!(
        avalia(&r, &AmbienteMock::com_mensagem("feat: x\n\nplain body")),
        Veredito::Limpo
    ));
}

/// Concern 3 — unresolved merge-conflict markers in the staged blob. The PAIR is
/// required.
fn regra_marcadores_de_conflito() -> Regra {
    Regra::nova(
        "vcs-conflict-markers",
        Ponto::PreCommit,
        Predicado::BlobTemParDeLinhas {
            abre: "<<<<<<< ".into(),
            fecha: ">>>>>>> ".into(),
        },
        Reversibilidade::Custoso,
        ClasseFalsoPositivo::ZeroEstrutural,
        "unresolved merge-conflict markers in the staged blob",
        AmbienteMock::default().com_stage("flake.lock", "a\n<<<<<<< HEAD\nb\n>>>>>>> other\nc\n"),
        AmbienteMock::default().com_stage("README.md", "# Title\n\n=======\n\nprose\n"),
    )
}

#[test]
fn conflict_markers_need_the_pair() {
    let r = regra_marcadores_de_conflito();
    // Both markers → caught.
    assert!(matches!(
        avalia(
            &r,
            &AmbienteMock::default().com_stage("f", "<<<<<<< HEAD\nx\n>>>>>>> them\n")
        ),
        Veredito::Achado { .. }
    ));
    // A lone `=======` is an ordinary markdown rule — must pass, or documentation
    // becomes uncommittable.
    assert!(matches!(
        avalia(
            &r,
            &AmbienteMock::default().com_stage("f", "# T\n\n=======\n")
        ),
        Veredito::Limpo
    ));
    // A lone opener appears in prose about merges.
    assert!(matches!(
        avalia(
            &r,
            &AmbienteMock::default().com_stage("f", "we saw <<<<<<< in the file\n")
        ),
        Veredito::Limpo
    ));
}

/// Concern 4 — the D2 lockfile tie. Structurally FP-free: it re-runs the
/// consumer's own arithmetic.
fn regra_amarra_d2() -> Regra {
    Regra::nova(
        "fresh-cargo-gen-lock",
        Ponto::PreCommit,
        Predicado::AmarraDeHash {
            arquivo: "Cargo.lock".into(),
            sidecar: "Cargo.gen.lock".into(),
            campo: "cargo_lock_sha256".into(),
        },
        Reversibilidade::Custoso,
        ClasseFalsoPositivo::ZeroEstrutural,
        "Cargo.gen.lock records a different Cargo.lock than this commit contains",
        AmbienteMock::default()
            .com_stage("Cargo.lock", "lock v2")
            .com_stage("Cargo.gen.lock", r#"{"cargo_lock_sha256": "deadbeef"}"#)
            .com_sha("Cargo.lock", "cafebabe"),
        AmbienteMock::default()
            .com_stage("Cargo.lock", "lock v2")
            .com_stage("Cargo.gen.lock", r#"{"cargo_lock_sha256": "cafebabe"}"#)
            .com_sha("Cargo.lock", "cafebabe"),
    )
}

#[test]
fn d2_tie_fires_only_on_a_real_mismatch() {
    let r = regra_amarra_d2();
    assert!(matches!(
        avalia(&r, &r.prova_bloqueia),
        Veredito::Achado { .. }
    ));
    assert!(matches!(avalia(&r, &r.prova_passa), Veredito::Limpo));
}

/// The evaluator default that makes the freshness family safe across
/// heterogeneous checkouts: absent sidecar ⇒ the rule does not fire.
#[test]
fn d2_tie_skips_a_workspace_that_never_onboarded() {
    let r = regra_amarra_d2();
    let sem_sidecar = AmbienteMock::default()
        .com_stage("Cargo.lock", "lock v2")
        .com_sha("Cargo.lock", "cafebabe");
    assert!(matches!(avalia(&r, &sem_sidecar), Veredito::Limpo));
}

/// gen's `unhashed-spec` is a DIFFERENT state from STALE and is none of this
/// rule's business.
#[test]
fn d2_tie_skips_a_sidecar_carrying_no_hash_field() {
    let r = regra_amarra_d2();
    let sem_campo = AmbienteMock::default()
        .com_stage("Cargo.lock", "lock v2")
        .com_stage("Cargo.gen.lock", r#"{"other": "field"}"#)
        .com_sha("Cargo.lock", "cafebabe");
    assert!(matches!(avalia(&r, &sem_campo), Veredito::Limpo));
}

// ═══════════════════════════════════════════════════════════════════
// The novelty gate — added lines, not the whole blob
// ═══════════════════════════════════════════════════════════════════

fn regra_credencial_prosa() -> Regra {
    Regra::nova(
        "sec-plaintext-password",
        Ponto::PreCommit,
        Predicado::LinhaAdicionadaCasa {
            padroes: vec![
                Padrao::novo(
                    "sec-plaintext-password",
                    r"(?i)pass(word|phrase)\s*[:=]\s*[^:/\s<{$][^\s<{$]{7,}",
                )
                .expect("pattern compiles"),
            ],
        },
        Reversibilidade::Irreversivel,
        ClasseFalsoPositivo::Prosa,
        "plaintext password assignment",
        AmbienteMock::default()
            .com_stage("app.conf", "listen = 0\npassword: correcthorsebattery\n"),
        AmbienteMock::default().com_stage("app.conf", "listen = 0\npassword: <redacted>\n"),
    )
}

/// THE measured false-positive class: a line already in HEAD, re-added by a
/// reorder or a duplication, introduces nothing.
#[test]
fn a_line_already_in_head_is_not_an_addition() {
    let r = regra_credencial_prosa();
    let reordenado = AmbienteMock::default()
        .com_head(
            "v.yaml",
            "alpha: 1\npassword: correcthorsebattery\nbeta: 2\n",
        )
        .com_stage(
            "v.yaml",
            "beta: 2\nalpha: 1\npassword: correcthorsebattery\n",
        );
    assert!(
        matches!(avalia(&r, &reordenado), Veredito::Limpo),
        "a reorder re-adds an already-committed line; re-accusing it is the FP engine"
    );
}

#[test]
fn a_genuinely_new_credential_line_is_caught() {
    let r = regra_credencial_prosa();
    let novo = AmbienteMock::default()
        .com_head("v.yaml", "alpha: 1\n")
        .com_stage("v.yaml", "alpha: 1\npassword: correcthorsebattery\n");
    assert!(matches!(avalia(&r, &novo), Veredito::Achado { .. }));
}

/// A brand-new file has no HEAD copy, so every line is novel. The correct
/// reading — nothing about this path is in history yet.
#[test]
fn a_new_file_has_every_line_novel() {
    let r = regra_credencial_prosa();
    let novo_arquivo =
        AmbienteMock::default().com_stage("secrets.conf", "password: correcthorsebattery\n");
    assert!(matches!(avalia(&r, &novo_arquivo), Veredito::Achado { .. }));
}

/// The remaining case, asserted so it is a KNOWN limit rather than a surprise:
/// a reindented credential line still reads as added.
#[test]
fn reindentation_is_a_named_remaining_false_positive() {
    let r = regra_credencial_prosa();
    let reindentado = AmbienteMock::default()
        .com_head("v.yaml", "password: correcthorsebattery\n")
        .com_stage("v.yaml", "  password: correcthorsebattery\n");
    assert!(
        matches!(avalia(&r, &reindentado), Veredito::Achado { .. }),
        "documented limit: exact line comparison cannot see a reindent as the same line"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Veredito — four states, none rendering as another
// ═══════════════════════════════════════════════════════════════════

/// A blind environment is NOT a pass. This is the anti-vacuity core: a gate that
/// could not look must not report that it found nothing.
#[test]
fn a_blind_environment_yields_cego_never_limpo() {
    let r = regra_assunto_placeholder();
    let cego = AmbienteMock {
        cego: true,
        ..AmbienteMock::default()
    };
    let v = avalia(&r, &cego);
    assert!(matches!(v, Veredito::Cego { .. }), "got {v:?}");
    assert_ne!(v, Veredito::Limpo);
}

/// …and a blind gate does not fail the commit either. Failing closed would make
/// an unreadable repo uncommittable, which is how the plane gets disabled.
#[test]
fn cego_is_loud_but_does_not_gate() {
    let r = regra_assunto_placeholder();
    let cego = AmbienteMock {
        cego: true,
        ..AmbienteMock::default()
    };
    assert!(!avalia(&r, &cego).gateia());
}

#[test]
fn only_a_block_severity_gates() {
    let r = regra_marcadores_de_conflito();
    let sujo = AmbienteMock::default().com_stage("f", "<<<<<<< a\nx\n>>>>>>> b\n");

    // Born advisory → finds, does not gate.
    assert!(!avalia(&r, &sujo).gateia());
    // Floor raised to its ceiling → gates.
    assert!(avalia(&r.clone().com_piso(Severidade::Bloqueia), &sujo).gateia());
}

// ═══════════════════════════════════════════════════════════════════
// prova — the both-directions obligation
// ═══════════════════════════════════════════════════════════════════

fn corpus() -> Vec<Regra> {
    vec![
        regra_assunto_placeholder(),
        regra_trailer_ia(),
        regra_marcadores_de_conflito(),
        regra_amarra_d2(),
        regra_credencial_prosa(),
    ]
}

#[test]
fn every_rule_proves_in_both_directions() {
    let (n, rows) = prova(&corpus());
    assert_eq!(n, 5, "denominator, printed with the verdict");
    for row in &rows {
        assert!(
            row.bloqueia_ok,
            "{} did not refuse its own witness",
            row.regra
        );
        assert!(
            row.passa_ok,
            "{} refused its own passing witness",
            row.regra
        );
        assert!(row.verde());
    }
}

/// The vacuity refusal: an EMPTY corpus must be distinguishable from a proven
/// one. `prova` returns the count precisely so a caller can refuse zero.
#[test]
fn an_empty_corpus_reports_zero_rather_than_green() {
    let (n, rows) = prova(&[]);
    assert_eq!(n, 0);
    assert!(rows.is_empty());
    assert!(
        rows.iter().all(Prova::verde),
        "vacuously true over an empty set — which is exactly why the COUNT is returned"
    );
}

/// A red run for the proof machinery itself: a rule whose witnesses are swapped
/// must fail both directions. Without this, `prova` reporting green would prove
/// nothing about `prova`.
#[test]
fn prova_goes_red_when_the_witnesses_are_swapped() {
    let bom = regra_marcadores_de_conflito();
    let invertida = Regra::nova(
        "vcs-conflict-markers-INVERTED",
        Ponto::PreCommit,
        Predicado::BlobTemParDeLinhas {
            abre: "<<<<<<< ".into(),
            fecha: ">>>>>>> ".into(),
        },
        Reversibilidade::Custoso,
        ClasseFalsoPositivo::ZeroEstrutural,
        "inverted on purpose",
        // Witnesses deliberately the wrong way round.
        bom.prova_passa.clone(),
        bom.prova_bloqueia.clone(),
    );
    let (n, rows) = prova(&[invertida]);
    assert_eq!(n, 1);
    assert!(!rows[0].bloqueia_ok);
    assert!(!rows[0].passa_ok);
    assert!(!rows[0].verde());
}

// ═══════════════════════════════════════════════════════════════════
// Padrao
// ═══════════════════════════════════════════════════════════════════

#[test]
fn an_uncompilable_pattern_is_refused_at_construction() {
    let e = Padrao::novo("r", "a(").expect_err("unbalanced paren must not compile");
    assert!(matches!(e, JoeiraError::PadraoInvalido { .. }));
    // The error names the owning rule, so it is actionable without a stack.
    assert!(e.to_string().contains("rule `r`"));
}

#[test]
fn padrao_keeps_its_source_for_round_tripping() {
    let p = Padrao::novo("r", r"\d+").expect("compiles");
    assert_eq!(p.fonte(), r"\d+");
    assert!(p.casa("abc 123"));
    assert!(!p.casa("abc"));
}

// ═══════════════════════════════════════════════════════════════════
// Composition
// ═══════════════════════════════════════════════════════════════════

#[test]
fn todos_algum_nao_compose() {
    let na_lista = |w: &str| Predicado::AssuntoNaLista {
        lista: vec![w.to_owned()],
    };
    let amb = AmbienteMock::com_mensagem("init");

    let r = |p: Predicado| {
        Regra::nova(
            "t",
            Ponto::CommitMsg,
            p,
            Reversibilidade::Recuperavel,
            ClasseFalsoPositivo::TokenExato,
            "m",
            AmbienteMock::com_mensagem("init"),
            AmbienteMock::com_mensagem("real subject here"),
        )
    };

    assert!(matches!(
        avalia(
            &r(Predicado::Algum(vec![na_lista("nope"), na_lista("init")])),
            &amb
        ),
        Veredito::Achado { .. }
    ));
    assert!(matches!(
        avalia(
            &r(Predicado::Todos(vec![na_lista("nope"), na_lista("init")])),
            &amb
        ),
        Veredito::Limpo
    ));
    assert!(matches!(
        avalia(&r(Predicado::Nao(Box::new(na_lista("nope")))), &amb),
        Veredito::Achado { .. }
    ));
}
