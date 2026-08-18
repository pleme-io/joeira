//! joeira's typed border — the closed predicate algebra, the derived severity
//! projection, and the mockable git-environment seam.
//!
//! Design: `theory/JOEIRA.md`.
//!
//! # What is pure here, and what is not
//!
//! An earlier revision of this comment claimed the crate "holds no I/O, spawns
//! no process, and reads no file". That was true when only the algebra and the
//! mock existed, and it stopped being true the moment [`shell`] and [`journal`]
//! landed. Correcting the claim rather than the code, because the split that
//! matters is not crate-shaped:
//!
//! - **The engine is pure.** [`Predicado`], [`Severidade`], [`Regra`],
//!   [`avalia`] and [`prova`] touch nothing. Every fact about a repository
//!   arrives through the [`AmbienteGit`] trait, which is why the whole rule
//!   corpus is testable mock-green with zero real side effects — and why the
//!   engine tests spawn no process and write no file. (The journal tests do
//!   write, to a temp path, never to okiba's real State dir — a test must not
//!   append to the operator's own history.)
//! - **Two modules do I/O, both behind that seam.** [`shell`] implements
//!   `AmbienteGit` over `git` subprocesses; [`journal`] appends the invocation
//!   record. Neither is reachable from the engine — the dependency runs the
//!   other way.
//!
//! They live here rather than in the binary because a *library* consumer needs
//! them: the oracle and the future hook entrypoint are separate programs that
//! must both read a real repository through the same proven invocations. Putting
//! them in the binary would force the second consumer to re-derive them, which
//! is the duplication this whole design exists to remove.
//!
//! # The one invariant this crate exists to enforce
//!
//! **[`Predicado`] has no text-bearing variant.** There is no `Shell(String)`,
//! no `Comando(String)`, no `Script(PathBuf)` — so "a hook grew a shell body"
//! has *no representation*, and that is a compile error rather than a lint.
//! TIER: **truly-unrepresentable** for predicates expressed in this algebra.
//! Read the honest limit with it: a rule could still be written in Rust that
//! shells out on its own, and *that* is only-mitigated by review. What is
//! removed is the ability to express shell **as a rule**.
//!
//! A regex is data, not a program, so [`Padrao`] carrying a pattern is not a
//! text-bearing variant in the sense above. The distinction is executability:
//! a pattern is matched, a shell body is *run*. Keeping regex out would not buy
//! unrepresentability of anything — it would only push the same matching into a
//! worse place.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════
// Errors
// ═══════════════════════════════════════════════════════════════════

/// Every way this crate refuses. Deliberately small: most illegal states are
/// unconstructible rather than reported.
#[derive(Debug, thiserror::Error)]
pub enum JoeiraError {
    /// A pattern in a rule did not compile. Reported at construction time, so a
    /// `Regra` that exists always holds compilable patterns.
    #[error("rule `{regra}`: pattern {padrao:?} does not compile: {fonte}")]
    PadraoInvalido {
        regra: String,
        padrao: String,
        #[source]
        fonte: Box<regex::Error>,
    },

    /// The environment could not answer. Distinct from "the answer is no" — see
    /// [`Veredito::Cego`].
    #[error("git environment could not answer: {0}")]
    AmbienteCego(String),
}

// ═══════════════════════════════════════════════════════════════════
// Ponto — where a rule mounts
// ═══════════════════════════════════════════════════════════════════

/// A git lifecycle point. **Closed** on purpose: an unknown `:ponto` in the
/// authoring surface is rejected at parse time rather than ignored, so a rule
/// cannot be silently mounted nowhere.
///
/// git exposes ~28 hooks; these are the ones with a stated job here. The
/// omissions are deliberate, not an oversight — notably `reference-transaction`,
/// which fires 5× per commit and twice on operations carrying no staged content
/// at all (`git branch`, `stash`), so mounting a blob-reading rule there routes
/// the costliest predicate class onto the highest-frequency invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ponto {
    /// Before a commit object is written. Reads the index.
    PreCommit,
    /// After the message is composed, before the commit. Reads (and may rewrite) the message.
    CommitMsg,
    /// Before refs are sent to a remote. The only legal mount for a cost-class-5 rule.
    PrePush,
}

impl Ponto {
    /// Every variant, so a census can assert coverage without a wildcard match.
    #[must_use]
    pub const fn todos() -> &'static [Self] {
        &[Self::PreCommit, Self::CommitMsg, Self::PrePush]
    }

    /// The hook filename git looks for.
    #[must_use]
    pub const fn arquivo(self) -> &'static str {
        match self {
            Self::PreCommit => "pre-commit",
            Self::CommitMsg => "commit-msg",
            Self::PrePush => "pre-push",
        }
    }

    /// What this point can read. Wildcard-free so adding a `Ponto` without
    /// deciding its reads is a compile error.
    #[must_use]
    pub const fn leitura(self) -> Leitura {
        match self {
            Self::CommitMsg => Leitura::Mensagem,
            Self::PreCommit | Self::PrePush => Leitura::Indice,
        }
    }
}

/// What a rule reads — the §7 family key, because it determines cost, mount
/// point and seam requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Leitura {
    /// The commit message only. Cost class 0 — no git object access.
    Mensagem,
    /// The staged index: paths, and the blobs they name.
    Indice,
}

// ═══════════════════════════════════════════════════════════════════
// The severity projection — DERIVED, never authored
// ═══════════════════════════════════════════════════════════════════

/// How recoverable the thing a rule prevents is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reversibilidade {
    /// Cannot be withdrawn once it lands. A credential in a commit is the type
    /// case: rewriting history reaches no existing clone, so the remedy is
    /// always rotation at the provider.
    Irreversivel,
    /// Recoverable, but the recovery is expensive or lands far from the author.
    Custoso,
    /// A later commit fixes it.
    Recuperavel,
}

/// How a rule fails when it is wrong — the axis that decides whether it may
/// block. This is the one that gets skipped, and skipping it is how a gate
/// earns a reputation for crying wolf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClasseFalsoPositivo {
    /// Structurally cannot false-positive, because it re-runs the consumer's own
    /// arithmetic (a hash tie) or matches an exact structural pair.
    ZeroEstrutural,
    /// Matches an exact token with a length or prefix anchor.
    TokenExato,
    /// A threshold or budget. Needs a baseline ratchet, never a hard fail.
    Limiar,
    /// Matches prose. The FP-generating class.
    Prosa,
}

/// What a rule does when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severidade {
    /// Printed, never gating. The birth state of every rule.
    Consultivo,
    /// Printed and counted against a baseline that may not grow.
    AvisaComCatraca,
    /// Printed loudly, exit 0.
    Avisa,
    /// Exit non-zero.
    Bloqueia,
}

impl Severidade {
    /// **The projection an author cannot route around.** Severity is a total
    /// function of `(reversibility × false-positive-class)`; there is no
    /// constructor that takes a severity directly, so "an unjustified block"
    /// has no path into a `Regra`.
    ///
    /// This is the **ceiling**, not the birth value. Every rule is born
    /// [`Severidade::Consultivo`] and its floor is raised as a separate,
    /// evidenced change — see [`Regra::com_piso`].
    ///
    /// The `Irreversivel × Prosa` cell is the interesting one and it is
    /// deliberately **not** `Bloqueia`: a prose matcher cannot defend an
    /// irreversible class without also refusing work that carries no
    /// credential, which is measurable — the fleet's own prose
    /// `password`/`secret` matcher accounts for ~13 of 37 recorded
    /// `--no-verify` bypasses. It gets `AvisaComCatraca`, and that demotion is
    /// named here rather than discovered in production.
    #[must_use]
    pub const fn tecto(rev: Reversibilidade, fp: ClasseFalsoPositivo) -> Self {
        use ClasseFalsoPositivo as F;
        use Reversibilidade as R;
        match (rev, fp) {
            (R::Irreversivel, F::ZeroEstrutural | F::TokenExato) => Self::Bloqueia,
            (R::Irreversivel, F::Limiar | F::Prosa) => Self::AvisaComCatraca,
            (R::Custoso, F::ZeroEstrutural) => Self::Bloqueia,
            (R::Custoso, F::TokenExato) => Self::Avisa,
            (R::Custoso, F::Limiar) => Self::AvisaComCatraca,
            (R::Custoso, F::Prosa) => Self::Consultivo,
            (R::Recuperavel, F::ZeroEstrutural | F::TokenExato) => Self::Avisa,
            (R::Recuperavel, F::Limiar) => Self::Consultivo,
            (R::Recuperavel, F::Prosa) => Self::Consultivo,
        }
    }

    /// Whether this severity fails the hook.
    #[must_use]
    pub const fn gateia(self) -> bool {
        matches!(self, Self::Bloqueia)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Padrao — a compiled pattern
// ═══════════════════════════════════════════════════════════════════

/// A compiled pattern. Holds the source text so a rule can be round-tripped and
/// printed, and the compiled `Regex` so matching never re-compiles.
///
/// Compiled at construction: a `Padrao` that exists always matches. That moves
/// "this rule's regex is broken" from a runtime surprise to a construction-time
/// refusal.
#[derive(Debug, Clone)]
pub struct Padrao {
    fonte: String,
    re: regex::Regex,
}

impl Padrao {
    /// Compile a pattern, naming the owning rule so the error is actionable.
    pub fn novo(regra: &str, fonte: impl Into<String>) -> Result<Self, JoeiraError> {
        let fonte = fonte.into();
        let re = regex::Regex::new(&fonte).map_err(|e| JoeiraError::PadraoInvalido {
            regra: regra.to_owned(),
            padrao: fonte.clone(),
            fonte: Box::new(e),
        })?;
        Ok(Self { fonte, re })
    }

    #[must_use]
    pub fn fonte(&self) -> &str {
        &self.fonte
    }

    #[must_use]
    pub fn casa(&self, texto: &str) -> bool {
        self.re.is_match(texto)
    }
}

impl PartialEq for Padrao {
    fn eq(&self, other: &Self) -> bool {
        self.fonte == other.fonte
    }
}
impl Eq for Padrao {}

// ═══════════════════════════════════════════════════════════════════
// Predicado — the closed algebra
// ═══════════════════════════════════════════════════════════════════

/// What a rule asserts. **Closed, and with no text-bearing variant** — see the
/// crate docs. Every arm is a structural question about the message or the
/// index; none carries a program.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Predicado {
    /// The whole trimmed subject, lowercased, is in this set.
    ///
    /// Set membership, not prefix or substring, and that is load-bearing:
    /// `init` is refused while `Revert "init"`, `Merge branch …` and
    /// `fixup! init` all pass — which is what keeps `git revert`, `git merge`
    /// and `rebase --autosquash` working. A guard that breaks recovery paths
    /// gets switched off.
    AssuntoNaLista { lista: Vec<String> },

    /// The subject's tail after its last colon is in this set. Catches the
    /// placeholder wearing a scope prefix (`updates: init`) while leaving the
    /// accepted `init: <what changed>` form alone.
    AssuntoCaudaNaLista { lista: Vec<String> },

    /// Some message line contains one of these trailer markers.
    MensagemTemTrailer { marcadores: Vec<String> },

    /// A line ADDED by this commit matches one of these patterns.
    ///
    /// Added lines, never the whole blob: scanning the blob re-accuses every
    /// matching line already in history on every later edit, which is the
    /// false-positive engine rather than a finding.
    LinhaAdicionadaCasa { padroes: Vec<Padrao> },

    /// The staged blob carries BOTH of these line prefixes. The pair is
    /// required: a lone `=======` is a markdown rule and a lone `<<<<<<<`
    /// appears in prose about merges.
    BlobTemParDeLinhas { abre: String, fecha: String },

    /// `sha256(arquivo)` equals the hex value at `campo` in `sidecar`, judged
    /// over the blobs this commit will contain. Structurally FP-free because it
    /// re-runs the consumer's own arithmetic. Absent sidecar ⇒ skip, which is
    /// what makes it safe across heterogeneous checkouts.
    AmarraDeHash {
        arquivo: String,
        sidecar: String,
        campo: String,
    },

    /// All must hold.
    Todos(Vec<Predicado>),
    /// Any may hold.
    Algum(Vec<Predicado>),
    /// Negation.
    Nao(Box<Predicado>),
}

// ═══════════════════════════════════════════════════════════════════
// AmbienteGit — the mockable seam
// ═══════════════════════════════════════════════════════════════════

/// Everything the engine may learn about a repository. A trait so the whole
/// engine runs mock-green with zero real side effects, and so the shell-backed
/// and in-process implementations are interchangeable.
///
/// Each method returns `Result`, and a failure is [`Veredito::Cego`] rather than
/// a pass: a gate that could not look is not a gate that found nothing.
pub trait AmbienteGit {
    /// The commit message as it will be committed.
    fn mensagem(&self) -> Result<String, JoeiraError>;
    /// Paths added/copied/modified/renamed in the index.
    fn caminhos_em_stage(&self) -> Result<Vec<String>, JoeiraError>;
    /// The staged blob at `caminho`.
    fn blob_em_stage(&self, caminho: &str) -> Result<String, JoeiraError>;
    /// HEAD's copy of `caminho`; `None` when the path is new or renamed-into.
    fn blob_em_head(&self, caminho: &str) -> Result<Option<String>, JoeiraError>;
    /// sha256 hex of the staged blob at `caminho`.
    fn sha256_em_stage(&self, caminho: &str) -> Result<String, JoeiraError>;
}

/// A fully in-memory environment. The engine's test seam.
#[derive(Debug, Default, Clone)]
pub struct AmbienteMock {
    pub mensagem: String,
    /// path → staged content
    pub stage: BTreeMap<String, String>,
    /// path → HEAD content
    pub head: BTreeMap<String, String>,
    /// path → sha256 hex, when a test wants to drive the hash tie directly
    pub shas: BTreeMap<String, String>,
    /// Force every read to fail, to exercise the `Cego` arm.
    pub cego: bool,
}

impl AmbienteMock {
    #[must_use]
    pub fn com_mensagem(msg: &str) -> Self {
        Self {
            mensagem: msg.to_owned(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn com_stage(mut self, caminho: &str, conteudo: &str) -> Self {
        self.stage.insert(caminho.to_owned(), conteudo.to_owned());
        self
    }

    #[must_use]
    pub fn com_head(mut self, caminho: &str, conteudo: &str) -> Self {
        self.head.insert(caminho.to_owned(), conteudo.to_owned());
        self
    }

    #[must_use]
    pub fn com_sha(mut self, caminho: &str, sha: &str) -> Self {
        self.shas.insert(caminho.to_owned(), sha.to_owned());
        self
    }
}

impl AmbienteGit for AmbienteMock {
    fn mensagem(&self) -> Result<String, JoeiraError> {
        if self.cego {
            return Err(JoeiraError::AmbienteCego("mock is blind".into()));
        }
        Ok(self.mensagem.clone())
    }

    fn caminhos_em_stage(&self) -> Result<Vec<String>, JoeiraError> {
        if self.cego {
            return Err(JoeiraError::AmbienteCego("mock is blind".into()));
        }
        Ok(self.stage.keys().cloned().collect())
    }

    fn blob_em_stage(&self, caminho: &str) -> Result<String, JoeiraError> {
        if self.cego {
            return Err(JoeiraError::AmbienteCego("mock is blind".into()));
        }
        Ok(self.stage.get(caminho).cloned().unwrap_or_default())
    }

    fn blob_em_head(&self, caminho: &str) -> Result<Option<String>, JoeiraError> {
        if self.cego {
            return Err(JoeiraError::AmbienteCego("mock is blind".into()));
        }
        Ok(self.head.get(caminho).cloned())
    }

    fn sha256_em_stage(&self, caminho: &str) -> Result<String, JoeiraError> {
        if self.cego {
            return Err(JoeiraError::AmbienteCego("mock is blind".into()));
        }
        Ok(self.shas.get(caminho).cloned().unwrap_or_default())
    }
}

// ═══════════════════════════════════════════════════════════════════
// Regra — a rule, with its proof obligation
// ═══════════════════════════════════════════════════════════════════

/// One rule. Both test vectors are **non-`Option`**, so a rule with no red-run
/// pair cannot be constructed — the proof obligation is structural rather than a
/// convention. TIER: parse-time-rejected for *existence* of the pair; whether
/// the pair is *useful* is CI-caught by [`prova`].
#[derive(Debug, Clone)]
pub struct Regra {
    nome: String,
    ponto: Ponto,
    predicado: Predicado,
    reversibilidade: Reversibilidade,
    fp: ClasseFalsoPositivo,
    /// Raised deliberately, never above [`Severidade::tecto`].
    piso: Severidade,
    mensagem: String,
    /// An environment this rule MUST refuse.
    prova_bloqueia: AmbienteMock,
    /// An environment this rule MUST pass.
    prova_passa: AmbienteMock,
}

impl Regra {
    /// Build a rule. Severity is not a parameter — it is derived.
    #[must_use]
    pub fn nova(
        nome: impl Into<String>,
        ponto: Ponto,
        predicado: Predicado,
        reversibilidade: Reversibilidade,
        fp: ClasseFalsoPositivo,
        mensagem: impl Into<String>,
        prova_bloqueia: AmbienteMock,
        prova_passa: AmbienteMock,
    ) -> Self {
        Self {
            nome: nome.into(),
            ponto,
            predicado,
            reversibilidade,
            fp,
            // Birth state: advisory, deliberately, including for irreversible
            // rules. The floor is raised as a separate evidenced change.
            piso: Severidade::Consultivo,
            mensagem: mensagem.into(),
            prova_bloqueia,
            prova_passa,
        }
    }

    /// Raise this rule's floor. **Clamped to the derived ceiling**, so a floor
    /// above what the evidence supports is not refused — it is not expressible.
    #[must_use]
    pub fn com_piso(mut self, piso: Severidade) -> Self {
        let tecto = self.tecto();
        self.piso = if piso > tecto { tecto } else { piso };
        self
    }

    #[must_use]
    pub fn nome(&self) -> &str {
        &self.nome
    }
    #[must_use]
    pub const fn ponto(&self) -> Ponto {
        self.ponto
    }
    #[must_use]
    pub fn mensagem(&self) -> &str {
        &self.mensagem
    }
    #[must_use]
    pub const fn predicado(&self) -> &Predicado {
        &self.predicado
    }
    /// The severity this rule's evidence permits.
    #[must_use]
    pub const fn tecto(&self) -> Severidade {
        Severidade::tecto(self.reversibilidade, self.fp)
    }
    /// The severity it actually acts at.
    #[must_use]
    pub const fn severidade(&self) -> Severidade {
        self.piso
    }
}

// ═══════════════════════════════════════════════════════════════════
// Veredito — what an answer says
// ═══════════════════════════════════════════════════════════════════

/// The four things an evaluation can mean, none of them rendering the same as
/// another. `Limpo` is a finding; `Cego` is not a pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Veredito {
    /// The predicate did not hold. Nothing to report.
    Limpo,
    /// The predicate held, at this severity.
    Achado {
        regra: String,
        severidade: Severidade,
        mensagem: String,
    },
    /// The rule did not apply — an absent artifact, a point mismatch. Distinct
    /// from `Limpo` so "skipped" can never be counted as "checked".
    NaoSeAplica { regra: String, porque: String },
    /// The environment could not answer. **Never** collapsed into `Limpo`.
    Cego { regra: String, porque: String },
}

impl Veredito {
    /// Whether this verdict should fail the hook.
    #[must_use]
    pub const fn gateia(&self) -> bool {
        match self {
            Self::Achado { severidade, .. } => severidade.gateia(),
            // A blind gate does not fail the commit — it fails loudly on stderr
            // and exits 0. Failing closed here would make an unreadable repo
            // uncommittable, which is how the whole plane gets disabled.
            Self::Limpo | Self::NaoSeAplica { .. } | Self::Cego { .. } => false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// The evaluator
// ═══════════════════════════════════════════════════════════════════

/// Evaluate one rule against one environment.
pub fn avalia<E: AmbienteGit + ?Sized>(regra: &Regra, amb: &E) -> Veredito {
    match sustenta(&regra.predicado, amb) {
        Ok(true) => Veredito::Achado {
            regra: regra.nome.clone(),
            severidade: regra.piso,
            mensagem: regra.mensagem.clone(),
        },
        Ok(false) => Veredito::Limpo,
        Err(e) => Veredito::Cego {
            regra: regra.nome.clone(),
            porque: e.to_string(),
        },
    }
}

/// The lines this commit ADDS to `caminho` — staged lines absent from HEAD's
/// copy of the same path.
///
/// Exact line comparison, deliberately. A move, a reorder or a duplication
/// re-adds an already-committed line, and re-accusing it is the false-positive
/// engine: the fleet measured ~7.3 `--no-verify` bypasses per day driven mostly
/// by exactly that. A REINDENTED line still reads as added — named as a
/// remaining case rather than papered over with a trim, because a normalisation
/// that collapses two different lines onto one key trades a false block for a
/// possible missed one, and that is the wrong direction on a credential gate.
fn linhas_adicionadas<E: AmbienteGit + ?Sized>(
    amb: &E,
    caminho: &str,
) -> Result<Vec<String>, JoeiraError> {
    let stage = amb.blob_em_stage(caminho)?;
    let head = amb.blob_em_head(caminho)?.unwrap_or_default();
    let antigas: std::collections::HashSet<&str> = head.lines().collect();
    Ok(stage
        .lines()
        .filter(|l| !antigas.contains(*l))
        .map(str::to_owned)
        .collect())
}

fn assunto_de(msg: &str) -> String {
    msg.lines().next().unwrap_or_default().trim().to_lowercase()
}

fn cauda_de(assunto: &str) -> String {
    assunto
        .rsplit_once(':')
        .map_or(assunto, |(_, cauda)| cauda)
        .trim()
        .to_owned()
}

/// Does this predicate hold?
fn sustenta<E: AmbienteGit + ?Sized>(p: &Predicado, amb: &E) -> Result<bool, JoeiraError> {
    match p {
        Predicado::AssuntoNaLista { lista } => {
            let a = assunto_de(&amb.mensagem()?);
            Ok(lista.iter().any(|w| w.to_lowercase() == a))
        }
        Predicado::AssuntoCaudaNaLista { lista } => {
            let cauda = cauda_de(&assunto_de(&amb.mensagem()?));
            Ok(lista.iter().any(|w| w.to_lowercase() == cauda))
        }
        Predicado::MensagemTemTrailer { marcadores } => {
            let msg = amb.mensagem()?;
            Ok(msg
                .lines()
                .any(|l| marcadores.iter().any(|m| l.contains(m.as_str()))))
        }
        Predicado::LinhaAdicionadaCasa { padroes } => {
            for caminho in amb.caminhos_em_stage()? {
                let adicionadas = linhas_adicionadas(amb, &caminho)?;
                let alvo = adicionadas.join("\n");
                if !adicionadas.is_empty() && padroes.iter().any(|pd| pd.casa(&alvo)) {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Predicado::BlobTemParDeLinhas { abre, fecha } => {
            for caminho in amb.caminhos_em_stage()? {
                let blob = amb.blob_em_stage(&caminho)?;
                let tem_abre = blob.lines().any(|l| l.starts_with(abre.as_str()));
                let tem_fecha = blob.lines().any(|l| l.starts_with(fecha.as_str()));
                if tem_abre && tem_fecha {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Predicado::AmarraDeHash {
            arquivo,
            sidecar,
            campo,
        } => {
            let caminhos = amb.caminhos_em_stage()?;
            // Absent sidecar ⇒ the rule does not apply. This is the evaluator
            // default that makes the freshness family FP-free across
            // heterogeneous checkouts, and it is deliberately NOT a pass:
            // `NaoSeAplica` is the caller's business, so here it is "predicate
            // does not hold".
            if !caminhos.iter().any(|c| c == sidecar) {
                return Ok(false);
            }
            let doc = amb.blob_em_stage(sidecar)?;
            let Some(gravado) = campo_hex(&doc, campo) else {
                // The sidecar carries no such field — an unhashed spec, a
                // different state from STALE and not this rule's business.
                return Ok(false);
            };
            let atual = amb.sha256_em_stage(arquivo)?;
            Ok(!atual.is_empty() && atual != gravado)
        }
        Predicado::Todos(ps) => {
            for p in ps {
                if !sustenta(p, amb)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Predicado::Algum(ps) => {
            for p in ps {
                if sustenta(p, amb)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Predicado::Nao(p) => Ok(!sustenta(p, amb)?),
    }
}

/// Pull `"<campo>": "<hex>"` out of a JSON-ish document without a JSON parser.
/// One fixed field, so a split is enough and the crate keeps zero JSON deps.
fn campo_hex(doc: &str, campo: &str) -> Option<String> {
    let chave = format!("\"{campo}\"");
    let depois = doc.split_once(&chave)?.1;
    let depois = depois.split_once(':')?.1;
    let aberto = depois.split_once('"')?.1;
    let (valor, _) = aberto.split_once('"')?;
    Some(valor.to_owned())
}

// ═══════════════════════════════════════════════════════════════════
// prova — the anti-vacuity gate
// ═══════════════════════════════════════════════════════════════════

/// The result of proving one rule in both directions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prova {
    pub regra: String,
    /// The refusing witness was refused.
    pub bloqueia_ok: bool,
    /// The passing witness passed.
    pub passa_ok: bool,
}

impl Prova {
    #[must_use]
    pub const fn verde(&self) -> bool {
        self.bloqueia_ok && self.passa_ok
    }
}

/// Prove every rule in both directions against its own witnesses.
///
/// Returns the count alongside the rows so a caller can refuse an EMPTY corpus:
/// a suite that proves nothing reports the same green as one that proves
/// everything, and that is the vacuity shape this fleet keeps rediscovering.
#[must_use]
pub fn prova(regras: &[Regra]) -> (usize, Vec<Prova>) {
    let rows = regras
        .iter()
        .map(|r| {
            // The witness must be refused *by the predicate*, independent of the
            // floor — otherwise every advisory-born rule would look unproven.
            let bloqueia_ok = matches!(
                avalia(&r.clone().com_piso(Severidade::Bloqueia), &r.prova_bloqueia),
                Veredito::Achado { .. }
            );
            let passa_ok = matches!(
                avalia(&r.clone().com_piso(Severidade::Bloqueia), &r.prova_passa),
                Veredito::Limpo
            );
            Prova {
                regra: r.nome.clone(),
                bloqueia_ok,
                passa_ok,
            }
        })
        .collect::<Vec<_>>();
    (regras.len(), rows)
}

pub mod journal;
pub mod shell;

#[cfg(test)]
mod tests;
