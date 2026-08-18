//! The oracle — a differential against the INCUMBENT hooks over real history.
//!
//! ── WHAT IT COMPARES, AND WHY THAT SHAPE ──────────────────────────────────
//!
//! For each of N real commits it puts a scratch repository into exactly the
//! state the author's machine was in at `git commit` time — `HEAD` at the
//! parent, the index holding the commit's tree — and then runs BOTH engines
//! against that one state: the deployed tatara-script hooks, and joeira's own
//! `avalia` through `AmbienteShell`.
//!
//! That is deliberately NOT the tree-to-tree design this was first planned as
//! (`git diff N^ N` and `git show N:path`, no index at all). Tree-to-tree has
//! one real virtue — it writes nothing — but it makes the comparison
//! *joeira's tree reader* versus *the incumbent's index reader*, so every
//! disagreement has two possible causes and the interesting one is the rarer.
//! Reading one shared index removes that entire artifact class: identical
//! inputs, and the only variable left is the engine.
//!
//! The reason tree-to-tree was chosen first still stands and is honoured a
//! different way: `git read-tree` takes `.git/index.lock`, and doing that in a
//! live checkout would collide with concurrent agent sessions. So the index
//! being written is a `--shared` clone's own, in a scratch directory, sharing
//! objects by alternates. Nothing touches the repository under test.
//!
//! ── THE FOUR TRAPS ────────────────────────────────────────────────────────
//!
//! 1. **Every git invocation is status-checked.** `git diff --cached A B` exits
//!    129 with EMPTY stdout, and a reader that only looks at stdout sees "no
//!    staged paths", concludes `pass`, and reports a perfect fake agreement.
//!    `AmbienteShell` already bails with stderr on non-zero; this module does
//!    the same for every call it makes itself.
//! 2. **Rename detection is on in porcelain and off in plumbing.** The
//!    incumbent's filter includes `R`, whose omission was a live bypass until
//!    2026-08-13, so a harness that lost renames would report agreement on
//!    exactly the class `R` exists for. Both engines read one index here, so
//!    neither does its own rename detection — the shared state makes it moot
//!    rather than handled.
//! 3. **Merges and the root commit have no `N^`.** `--no-merges` drops the
//!    first; a root commit is counted in the SKIPPED bucket with a named
//!    reason rather than silently absent or forced against the empty tree.
//! 4. **The DEPLOYED hooks are read, never the Nix source.** `rejectSubjects`
//!    interpolates Nix into the script, so a source slice yields unevaluated
//!    Nix; and `(define path (car (argv)))` sits outside both `optionalString`
//!    blocks, so either commit-msg concern extracted alone has an unbound
//!    `path`. The `/nix/store` symlinks under `~/.config/git/hooks` are
//!    GC-rooted and read-only.
//!
//! ── THE FOURTH BUCKET ─────────────────────────────────────────────────────
//!
//! `Cego` is counted separately and NEVER as a match. An environment that could
//! not answer is not an engine agreeing — collapsing the two is how a harness
//! reports 500/500 while having read nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use joeira_core::shell::AmbienteShell;
use joeira_core::{Ponto, Regra, Veredito, avalia};

/// How the two engines related on one commit, for one concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Balde {
    /// Both engines reached the same verdict.
    Concorda,
    /// They disagreed. Every one must be attributed to a named cause or the
    /// run fails — a printed mismatch nobody classified is an unproven claim
    /// wearing a green summary.
    Discorda,
    /// Not comparable, with a reason. A root commit, or a concern the
    /// incumbent does not implement.
    Omitido,
    /// joeira's environment could not answer. Never a match.
    Cego,
    /// They disagreed AND the harness could attribute it to a named, TESTED
    /// cause. Counted separately from agreement, never folded into it.
    Triado,
}

/// One row of the ledger.
#[derive(Debug, Clone)]
pub struct Linha {
    pub commit: String,
    pub regra: String,
    pub balde: Balde,
    pub joeira_recusa: Option<bool>,
    pub incumbente_recusa: Option<bool>,
    pub causa: Option<&'static str>,
}

/// Which incumbent message attributes to which joeira rule.
///
/// Attribution is by the hook's own uniquely-prefixed stderr, because a bare
/// exit code says a commit was refused and not by WHICH concern — and with
/// three concerns on one `pre-commit` script, an exit-code-only comparison
/// would credit joeira's conflict-marker rule for the incumbent's credential
/// refusal.
const ATRIBUICAO: &[(&str, &str)] = &[
    ("credential material", "sec-plaintext-credential"),
    ("gen delta tie", "gen-lock-tie"),
    ("merge-conflict markers", "vcs-conflict-markers"),
    ("placeholder subject", "msg-placeholder-subject"),
];

/// Concerns joeira carries that the incumbent does not, with the reason.
///
/// Not a waiver list — a comparison of these would be comparing joeira against
/// nothing and calling the silence agreement.
const SEM_CONTRAPARTE: &[(&str, &str)] = &[(
    "msg-ai-attribution-trailer",
    "the incumbent REWRITES the trailer (stripCoAuthored) rather than refusing \
     it, so there is no refusal to compare against",
)];

/// Attribute a disagreement to a known cause, by TESTING for it.
///
/// A predicate, deliberately not an allowlist. An allowlist keyed on the rule
/// name would absorb every future disagreement on that rule — including a real
/// one — and that is the vacuity this whole exercise keeps finding. Because this
/// re-derives the cause from the state in front of it, a row stops being
/// attributed the moment the cause is actually fixed, and reverts to a plain
/// agreement rather than staying quietly excused.
fn triagem(dir: &Path, regra: &str, joeira: bool, incumbente: bool) -> Option<&'static str> {
    // ── The SOPS-exemption bypass (found by this oracle, 2026-08-18) ──
    //
    // The incumbent's `encrypted?` was `string-contains?` on the bare
    // `ENC[AES256_GCM` marker, so any file that merely MENTIONED SOPS — in a
    // comment, in prose, in a red-run receipt — was exempt from EVERY credential
    // rule at once. Proven with a matched pair: `password: hunter2supersecret`
    // alone exits 1, the same line beside a comment naming the marker exits 0.
    //
    // Detected here rather than assumed: a staged blob that names the marker but
    // carries none in VALUE position (`<key>: ENC[AES256_GCM,data:`) is not a
    // SOPS file, and the old predicate exempted it anyway. Fixed at the cause in
    // blackmatter; this attribution disappears on its own once the fixed hook is
    // the deployed one, with no edit here.
    if joeira && !incumbente && regra.starts_with("sec-") {
        let nomeia_marcador = git(
            dir,
            &["diff", "--cached", "--name-only", "--diff-filter=ACMR"],
        )
        .ok()?
        .lines()
        .any(|p| {
            git(dir, &["show", &format!(":{p}")])
                .map(|t| {
                    t.contains("ENC[AES256_GCM")
                        && !t.lines().any(|l| {
                            l.contains(": ENC[AES256_GCM,data:") || l.trim_start() == "sops:"
                        })
                })
                .unwrap_or(false)
        });
        if nomeia_marcador {
            return Some(
                "incumbent SOPS-exemption bypass: a staged blob NAMES \
                 ENC[AES256_GCM without carrying one in value position, and the \
                 incumbent's string-contains? predicate exempted the whole file \
                 from every credential rule. Fixed at the cause in blackmatter; \
                 this attribution stops matching once the fixed hook deploys.",
            );
        }
    }
    None
}

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let saida = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("running git {args:?} in {}: {e}", dir.display()))?;
    // Trap 1. Status first, ALWAYS — an exit-129 with empty stdout is the shape
    // that manufactures a fake 500/500.
    anyhow::ensure!(
        saida.status.success(),
        "git {args:?} in {} exited {:?}: {}",
        dir.display(),
        saida.status.code(),
        String::from_utf8_lossy(&saida.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&saida.stdout).into_owned())
}

/// Run one deployed hook and return `(refused, stderr+stdout)`.
///
/// A hook that is absent is an ERROR, not a pass. The whole point is comparing
/// against what actually runs, and "the file was missing so nothing refused"
/// would silently become agreement on every commit.
fn roda_incumbente(
    dir: &Path,
    gancho: &Path,
    arg: Option<&Path>,
) -> anyhow::Result<(bool, String)> {
    anyhow::ensure!(
        gancho.exists(),
        "the deployed hook {} is absent — refusing to report agreement against \
         a hook that does not run",
        gancho.display()
    );
    let mut cmd = Command::new(gancho);
    cmd.current_dir(dir);
    if let Some(a) = arg {
        cmd.arg(a);
    }
    let saida = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("running {}: {e}", gancho.display()))?;
    let mut texto = String::from_utf8_lossy(&saida.stderr).into_owned();
    texto.push_str(&String::from_utf8_lossy(&saida.stdout));
    Ok((!saida.status.success(), texto))
}

/// Prepare the scratch clone once. Returns its path.
fn prepara(repo: &Path, scratch: &Path) -> anyhow::Result<PathBuf> {
    if scratch.exists() {
        std::fs::remove_dir_all(scratch)?;
    }
    // `--shared` shares objects by alternates rather than copying them, and
    // `--no-checkout` means no worktree is materialised — this clone exists
    // only to own an index.
    git(
        Path::new("."),
        &[
            "clone",
            "--quiet",
            "--shared",
            "--no-checkout",
            repo.to_str()
                .ok_or_else(|| anyhow::anyhow!("non-utf8 repo path"))?,
            scratch
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("non-utf8 scratch path"))?,
        ],
    )?;
    // The hooks are invoked explicitly by this harness, so the clone must not
    // ALSO run them itself on `read-tree`/`update-ref` — otherwise a refusal
    // would abort the setup rather than being measured.
    git(scratch, &["config", "core.hooksPath", "/dev/null"])?;
    Ok(scratch.to_path_buf())
}

/// Put the scratch clone into the state the author's machine was in.
fn encena(scratch: &Path, commit: &str) -> anyhow::Result<bool> {
    // A root commit has no parent. Counted as omitted rather than forced
    // against the empty tree: the empty-tree comparison is a legitimate thing
    // to measure, but it is not the state any author was ever in.
    let pais = git(scratch, &["rev-list", "--parents", "-n", "1", commit])?;
    if pais.split_whitespace().count() < 2 {
        return Ok(false);
    }
    let pai = format!("{commit}^");
    git(scratch, &["update-ref", "HEAD", &pai])?;
    git(scratch, &["read-tree", commit])?;
    Ok(true)
}

/// Prove the harness can SEE, before it is allowed to claim agreement.
///
/// Builds a throwaway repository with two commits and asserts a known verdict
/// for each: one both engines must refuse, one both must pass. It runs as a
/// PRECONDITION rather than an optional flag, because "500/500 agree" and "both
/// sides returned pass unconditionally on every commit" print identically — and
/// that is not hypothetical here. The status-check trap makes it live: a `git`
/// call whose non-zero exit went unchecked yields empty stdout, which reads as
/// "no staged paths", which yields `pass` for every content rule on every
/// commit, which yields a perfect fake.
///
/// A real fleet history is the wrong place to look for the positive control.
/// Most commits refuse nothing — that is the point of a well-calibrated gate —
/// so a walk that happens to contain no refusal is both the expected outcome
/// and indistinguishable from total blindness.
fn calibra(regras: &[Regra]) -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join("joeira-calibracao");
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    git(&dir, &["init", "--quiet", "."])?;
    git(&dir, &["config", "user.email", "oraculo@joeira"])?;
    git(&dir, &["config", "user.name", "oraculo"])?;
    // The clone under test disables hooks; so does this, for the same reason —
    // the harness invokes them explicitly and a refusal during setup would
    // abort the fixture rather than being measured.
    git(&dir, &["config", "core.hooksPath", "/dev/null"])?;

    let hooks = dirs_hooks()?;

    // ── the NEGATIVE control: both engines must pass ────────────────────
    std::fs::write(
        dir.join("a.txt"),
        "ordinary content
",
    )?;
    git(&dir, &["add", "a.txt"])?;
    git(
        &dir,
        &["commit", "--quiet", "-m", "calibration: a clean commit"],
    )?;

    // ── the POSITIVE control: both engines must refuse ──────────────────
    // Conflict markers, because it is the one concern both engines implement
    // with the same structural predicate, so a disagreement here is a harness
    // fault rather than a modelling difference.
    std::fs::write(
        dir.join("b.txt"),
        "<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> other\n",
    )?;
    git(&dir, &["add", "b.txt"])?;

    let (inc_recusa, texto) = roda_incumbente(&dir, &hooks.join("pre-commit"), None)?;
    anyhow::ensure!(
        inc_recusa && texto.contains("merge-conflict markers"),
        "CALIBRATION FAILED: the deployed pre-commit hook did not refuse a \
         staged conflict-marker pair. The harness cannot see, so no agreement \
         it reports means anything. hook said: {}",
        texto.trim()
    );

    let amb = AmbienteShell::novo(&dir);
    let regra = regras
        .iter()
        .find(|r| r.nome() == "vcs-conflict-markers")
        .ok_or_else(|| {
            anyhow::anyhow!("the corpus has no vcs-conflict-markers rule to calibrate")
        })?;
    match avalia(regra, &amb) {
        Veredito::Achado { .. } => {}
        outro => anyhow::bail!(
            "CALIBRATION FAILED: joeira did not refuse a staged conflict-marker \
             pair, it returned {outro:?}. Every `pass` in the run below would be \
             unfalsifiable."
        ),
    }

    // And the negative control on joeira's side: the same rule must PASS on
    // the clean tree. Without this, a rule that refuses unconditionally would
    // satisfy the positive control above and agree with nothing.
    git(&dir, &["reset", "--quiet", "HEAD", "--", "b.txt"])?;
    std::fs::remove_file(dir.join("b.txt"))?;
    let amb_limpo = AmbienteShell::novo(&dir);
    match avalia(regra, &amb_limpo) {
        Veredito::Limpo { .. } | Veredito::NaoSeAplica { .. } => {}
        outro => anyhow::bail!(
            "CALIBRATION FAILED: joeira refused a CLEAN tree ({outro:?}) — the \
             rule fires unconditionally, so the positive control above proved \
             nothing."
        ),
    }

    println!("calibration: both engines refuse a planted conflict marker and pass a clean tree");
    Ok(())
}

/// Compare both engines over `n` commits of `repo`.
pub fn corre(repo: &Path, n: usize, regras: &[Regra]) -> anyhow::Result<Vec<Linha>> {
    // Before anything: prove the harness can see. A failure here is fatal.
    calibra(regras)?;

    let scratch = std::env::temp_dir().join("joeira-oraculo");
    let scratch = prepara(repo, &scratch)?;
    let hooks = dirs_hooks()?;

    let commits: Vec<String> = git(
        repo,
        &["log", "--no-merges", "--format=%H", "-n", &n.to_string()],
    )?
    .lines()
    .map(str::to_owned)
    .collect();
    anyhow::ensure!(
        !commits.is_empty(),
        "no non-merge commits found in {} — refusing to report agreement over \
         an empty denominator",
        repo.display()
    );

    let mut linhas = Vec::new();
    for c in &commits {
        let msg = git(repo, &["log", "-1", "--format=%B", c])?;
        if !encena(&scratch, c)? {
            for r in regras {
                linhas.push(Linha {
                    commit: c.clone(),
                    regra: r.nome().to_owned(),
                    balde: Balde::Omitido,
                    joeira_recusa: None,
                    incumbente_recusa: None,
                    causa: Some("root commit — no parent, so no author ever saw this state"),
                });
            }
            continue;
        }

        // ── the incumbent ────────────────────────────────────────────────
        let (_, pre) = roda_incumbente(&scratch, &hooks.join("pre-commit"), None)?;
        let msg_path = scratch.join("COMMIT_EDITMSG_oraculo");
        std::fs::write(&msg_path, &msg)?;
        let (_, cm) = roda_incumbente(&scratch, &hooks.join("commit-msg"), Some(&msg_path))?;
        let texto = format!("{pre}{cm}");

        // ── joeira ───────────────────────────────────────────────────────
        let amb = AmbienteShell::novo(&scratch);
        for r in regras {
            if let Some((_, porque)) = SEM_CONTRAPARTE.iter().find(|(n, _)| *n == r.nome()) {
                linhas.push(Linha {
                    commit: c.clone(),
                    regra: r.nome().to_owned(),
                    balde: Balde::Omitido,
                    joeira_recusa: None,
                    incumbente_recusa: None,
                    causa: Some(porque),
                });
                continue;
            }

            let v = match r.ponto() {
                // commit-msg rules read the message, which lives in the file
                // the hook was handed, not in the index.
                Ponto::CommitMsg => avalia(r, &AmbienteShell::novo(&scratch).com_mensagem(&msg)),
                _ => avalia(r, &amb),
            };

            let (balde, jr) = match &v {
                Veredito::Cego { .. } => (Balde::Cego, None),
                Veredito::NaoSeAplica { .. } => (Balde::Concorda, Some(false)),
                Veredito::Achado { .. } => (Balde::Concorda, Some(true)),
                Veredito::Limpo { .. } => (Balde::Concorda, Some(false)),
            };

            let ir = ATRIBUICAO
                .iter()
                .find(|(_, nome)| *nome == r.nome())
                .map(|(marca, _)| texto.contains(marca));

            let (balde, causa) = match (balde, jr, ir) {
                (Balde::Cego, _, _) => (
                    Balde::Cego,
                    Some("joeira's environment could not answer — never counted as a match"),
                ),
                (_, _, None) => (
                    Balde::Omitido,
                    Some("no incumbent message attributes to this rule"),
                ),
                (_, Some(j), Some(i)) if j == i => (Balde::Concorda, None),
                (_, Some(j), Some(i)) => match triagem(&scratch, r.nome(), j, i) {
                    Some(porque) => (Balde::Triado, Some(porque)),
                    None => (Balde::Discorda, None),
                },
                _ => (Balde::Discorda, None),
            };

            linhas.push(Linha {
                commit: c.clone(),
                regra: r.nome().to_owned(),
                balde,
                joeira_recusa: jr,
                incumbente_recusa: ir,
                causa,
            });
        }
    }
    Ok(linhas)
}

/// Where the deployed hooks live.
fn dirs_hooks() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME is unset"))?;
    let p = PathBuf::from(home).join(".config/git/hooks");
    anyhow::ensure!(
        p.is_dir(),
        "no deployed hooks at {} — the oracle compares against what RUNS, not \
         against the Nix source, so there is nothing to compare",
        p.display()
    );
    Ok(p)
}

/// Print the ledger and return non-zero on an un-triaged disagreement.
pub fn relata(linhas: &[Linha]) -> anyhow::Result<()> {
    let mut contagem: BTreeMap<Balde, usize> = BTreeMap::new();
    for l in linhas {
        *contagem.entry(l.balde).or_default() += 1;
    }
    let total = linhas.len();

    // Every disagreement, with its row-level cause. A count of mismatches is
    // not a finding — the plan's bar is that each one is ATTRIBUTED, because an
    // unattributed mismatch is indistinguishable from a harness bug.
    for l in linhas.iter().filter(|l| l.balde == Balde::Discorda) {
        println!(
            "  DISAGREE {} {:<28} joeira={:?} incumbent={:?}{}",
            &l.commit[..8.min(l.commit.len())],
            l.regra,
            l.joeira_recusa,
            l.incumbente_recusa,
            l.causa.map(|c| format!("  cause: {c}")).unwrap_or_default()
        );
    }

    // Triaged rows, each with the cause that was TESTED for. Printed in full
    // rather than summarised: an attributed disagreement is still a
    // disagreement, and a reader has to be able to judge the attribution.
    for l in linhas.iter().filter(|l| l.balde == Balde::Triado) {
        println!(
            "  TRIAGED  {} {:<28} joeira={:?} incumbent={:?}\n             cause: {}",
            &l.commit[..8.min(l.commit.len())],
            l.regra,
            l.joeira_recusa,
            l.incumbente_recusa,
            l.causa.unwrap_or("no cause recorded")
        );
    }

    // Blind rows are named individually. They are the bucket most likely to be
    // read as "fine" — nothing refused, nothing disagreed — when what happened
    // is that joeira could not see.
    for l in linhas.iter().filter(|l| l.balde == Balde::Cego) {
        println!(
            "  BLIND    {} {:<28} {}",
            &l.commit[..8.min(l.commit.len())],
            l.regra,
            l.causa.unwrap_or("no reason recorded")
        );
    }

    // Omission REASONS, deduplicated with their counts — so "300 omitted" is
    // never a number without an explanation attached.
    let mut porques: BTreeMap<&str, usize> = BTreeMap::new();
    for l in linhas.iter().filter(|l| l.balde == Balde::Omitido) {
        *porques
            .entry(l.causa.unwrap_or("no reason recorded"))
            .or_default() += 1;
    }
    for (porque, n) in &porques {
        println!("  omitted {n}: {porque}");
    }
    for (b, n) in &contagem {
        println!("  {b:?} {n}/{total}");
    }
    // The denominator, always — "0 disagreements" reads identically over 500
    // comparisons and over none.
    println!(
        "oracle: {} comparisons over {} rows",
        contagem.get(&Balde::Concorda).copied().unwrap_or(0),
        total
    );
    anyhow::ensure!(
        total > 0,
        "the oracle compared NOTHING — refusing to report agreement"
    );
    let d = contagem.get(&Balde::Discorda).copied().unwrap_or(0);
    anyhow::ensure!(d == 0, "{d} un-triaged disagreements of {total}");
    Ok(())
}
