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

use clap::{Parser, Subcommand};
use joeira_core::{
    AmbienteMock, ClasseFalsoPositivo, Ponto, Predicado, Regra, Reversibilidade, Severidade,
    Veredito, avalia, prova,
};

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
    Prova,
    /// Print the mount points and what each may read.
    Pontos,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().comando {
        Comando::Eval => cmd_eval(),
        Comando::Prova => cmd_prova(),
        Comando::Pontos => {
            for p in Ponto::todos() {
                println!("  {:<12} reads {:?}", p.arquivo(), p.leitura());
            }
            Ok(())
        }
    }
}

fn cmd_eval() -> anyhow::Result<()> {
    let regras = corpus();
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

fn cmd_prova() -> anyhow::Result<()> {
    let regras = corpus();
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
/// The four incumbent concerns, ported as data. Kept in the binary for P0 so the
/// oracle has something to compare; it moves to the `(defjoeira …)` corpus with
/// `joeira-lisp`'s derive.
fn corpus() -> Vec<Regra> {
    let placeholders: Vec<String> = ["init", "update", "updates", "wip", "fix", "test"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();

    vec![
        Regra::nova(
            "msg-placeholder-subject",
            Ponto::CommitMsg,
            Predicado::Algum(vec![
                Predicado::AssuntoNaLista {
                    lista: placeholders.clone(),
                },
                Predicado::AssuntoCaudaNaLista {
                    lista: placeholders,
                },
            ]),
            Reversibilidade::Custoso,
            ClasseFalsoPositivo::TokenExato,
            "placeholder subject — say what changed and why",
            AmbienteMock::com_mensagem("init"),
            AmbienteMock::com_mensagem("joeira: port the incumbent concerns"),
        ),
        Regra::nova(
            "msg-ai-attribution-trailer",
            Ponto::CommitMsg,
            Predicado::MensagemTemTrailer {
                marcadores: vec!["Co-Authored-By:".into(), "Claude-Session:".into()],
            },
            Reversibilidade::Custoso,
            ClasseFalsoPositivo::TokenExato,
            "AI-attribution trailer present",
            AmbienteMock::com_mensagem("feat: x\n\nClaude-Session: abc"),
            AmbienteMock::com_mensagem("feat: x\n\nplain body"),
        ),
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
            AmbienteMock::default().com_stage("flake.lock", "<<<<<<< HEAD\nx\n>>>>>>> them\n"),
            AmbienteMock::default().com_stage("README.md", "# T\n\n=======\n"),
        )
        // Structurally FP-free and costly-to-recover, so its ceiling is Bloqueia
        // and raising the floor there is evidenced rather than asserted.
        .com_piso(Severidade::Bloqueia),
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
                .com_stage("Cargo.lock", "l")
                .com_stage("Cargo.gen.lock", r#"{"cargo_lock_sha256": "deadbeef"}"#)
                .com_sha("Cargo.lock", "cafebabe"),
            AmbienteMock::default()
                .com_stage("Cargo.lock", "l")
                .com_stage("Cargo.gen.lock", r#"{"cargo_lock_sha256": "cafebabe"}"#)
                .com_sha("Cargo.lock", "cafebabe"),
        )
        .com_piso(Severidade::Bloqueia),
    ]
}
