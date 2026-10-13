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

    #[error("not checked: {0} absent")]
    FerramentaAusente(String),

    #[error("not checked: exceeded budget {ms}ms, `{linha}` was killed")]
    OrcamentoEsgotado { linha: String, ms: u64 },

    #[error("not checked: exceeded budget {ms}ms, the hook spent it before `{linha}` could run")]
    PrazoGasto { linha: String, ms: u64 },

    #[error("not checked: {0}")]
    NaoVerificavel(String),
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

impl Reversibilidade {
    /// Every variant, in decreasing severity of consequence.
    ///
    /// Exhaustive by construction rather than by discipline: the array is
    /// matched against `Self` below, so adding a variant without adding it here
    /// is a non-exhaustive-match compile error, not a roster that silently
    /// drifts one short.
    #[must_use]
    pub const fn todos() -> &'static [Self] {
        &[Self::Irreversivel, Self::Custoso, Self::Recuperavel]
    }

    /// The authoring symbol, identical to the serde name.
    ///
    /// ONE source for the spelling. A second hand-written table — in the lisp
    /// reader, in a CLI parser, in an error message — is the duplication that
    /// lets `irreversivel` and `irreversible` both exist and mean the same
    /// thing until one of them silently stops matching.
    #[must_use]
    pub const fn simbolo(self) -> &'static str {
        match self {
            Self::Irreversivel => "irreversivel",
            Self::Custoso => "custoso",
            Self::Recuperavel => "recuperavel",
        }
    }
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

impl ClasseFalsoPositivo {
    /// Every variant, in increasing false-positive rate.
    #[must_use]
    pub const fn todos() -> &'static [Self] {
        &[
            Self::ZeroEstrutural,
            Self::TokenExato,
            Self::Limiar,
            Self::Prosa,
        ]
    }

    /// The authoring symbol, identical to the serde name.
    #[must_use]
    pub const fn simbolo(self) -> &'static str {
        match self {
            Self::ZeroEstrutural => "zero-estrutural",
            Self::TokenExato => "token-exato",
            Self::Limiar => "limiar",
            Self::Prosa => "prosa",
        }
    }

    /// Whether a rule on this predicate class may refuse a commit.
    ///
    /// Derived from the variant, never stored: a soft predicate's false
    /// positive refuses work that carries nothing wrong, so it may warn and
    /// never block. `Severidade::tecto` already encodes this through the
    /// lattice; this is the same fact addressable directly, which is what the
    /// nix gate's `githooks-blocks-are-structural-only` row compares against.
    #[must_use]
    pub const fn pode_recusar(self) -> bool {
        matches!(self, Self::ZeroEstrutural | Self::TokenExato)
    }
}

/// What a rule does when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severidade {
    /// Printed, never gating. The birth state of every rule.
    #[serde(alias = "advise")]
    Consultivo,
    /// Printed and counted against a baseline that may not grow.
    #[serde(alias = "warn-with-ratchet")]
    AvisaComCatraca,
    /// Printed loudly, exit 0.
    #[serde(alias = "warn")]
    Avisa,
    /// Exit non-zero.
    #[serde(alias = "block")]
    Bloqueia,
}

impl Severidade {
    #[must_use]
    pub const fn todos() -> &'static [Self] {
        &[
            Self::Consultivo,
            Self::AvisaComCatraca,
            Self::Avisa,
            Self::Bloqueia,
        ]
    }

    #[must_use]
    pub const fn simbolo(self) -> &'static str {
        match self {
            Self::Consultivo => "consultivo",
            Self::AvisaComCatraca => "avisa-com-catraca",
            Self::Avisa => "avisa",
            Self::Bloqueia => "bloqueia",
        }
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Nivel {
    Sistema,
    Usuario,
    ClasseDeRepo,
}

impl Nivel {
    #[must_use]
    pub const fn todos() -> &'static [Self] {
        &[Self::Sistema, Self::Usuario, Self::ClasseDeRepo]
    }

    #[must_use]
    pub const fn simbolo(self) -> &'static str {
        match self {
            Self::Sistema => "sistema",
            Self::Usuario => "usuario",
            Self::ClasseDeRepo => "classe-de-repo",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verificador {
    NixFlakeCheck,
    CargoMetadata,
    CargoFetchLocked,
    CargoFmtCheck,
    GoModTidyDiff,
    GoVet,
    Actionlint,
    BlueBluefileConfirm,
}

impl Verificador {
    #[must_use]
    pub const fn todos() -> &'static [Self] {
        &[
            Self::NixFlakeCheck,
            Self::CargoMetadata,
            Self::CargoFetchLocked,
            Self::CargoFmtCheck,
            Self::GoModTidyDiff,
            Self::GoVet,
            Self::Actionlint,
            Self::BlueBluefileConfirm,
        ]
    }

    #[must_use]
    pub const fn programa(self) -> &'static str {
        match self {
            Self::NixFlakeCheck => "nix",
            Self::CargoMetadata | Self::CargoFetchLocked | Self::CargoFmtCheck => "cargo",
            Self::GoModTidyDiff | Self::GoVet => "go",
            Self::Actionlint => "actionlint",
            Self::BlueBluefileConfirm => "blue",
        }
    }

    #[must_use]
    pub const fn argumentos(self) -> &'static [&'static str] {
        match self {
            Self::NixFlakeCheck => &["flake", "check", "--no-build"],
            Self::CargoMetadata => &["metadata", "--format-version", "1", "--no-deps"],
            Self::CargoFetchLocked => &["fetch", "--locked"],
            Self::CargoFmtCheck => &["fmt", "--all", "--check"],
            Self::GoModTidyDiff => &["mod", "tidy", "-diff"],
            Self::GoVet => &["vet", "./..."],
            Self::Actionlint => &[],
            Self::BlueBluefileConfirm => &["bluefile", "--confirm", "Bluefile"],
        }
    }

    #[must_use]
    pub fn nao_verificavel(self, erro: &str) -> Option<String> {
        match self {
            Self::NixFlakeCheck => {
                let ultimo = erro
                    .lines()
                    .rev()
                    .map(str::trim)
                    .find(|l| l.starts_with("error:"))?;
                let ifd = ultimo.contains("allow-import-from-derivation")
                    || (ultimo.starts_with("error: path '/nix/store/")
                        && ultimo.ends_with(".drv' is not valid"));
                ifd.then(|| {
                    format!(
                        "the flake needs import-from-derivation, which `{}` cannot evaluate",
                        self.linha()
                    )
                })
            }
            _ => None,
        }
    }

    #[must_use]
    pub fn linha(self) -> String {
        let mut l = self.programa().to_owned();
        for a in self.argumentos() {
            l.push(' ');
            l.push_str(a);
        }
        l
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Colocacao {
    #[default]
    #[serde(alias = "hook")]
    Gancho,
    #[serde(alias = "deferred")]
    Adiada,
    #[serde(alias = "off")]
    Desligada,
}

impl Colocacao {
    #[must_use]
    pub const fn todas() -> &'static [Self] {
        &[Self::Gancho, Self::Adiada, Self::Desligada]
    }

    #[must_use]
    pub const fn simbolo(self) -> &'static str {
        match self {
            Self::Gancho => "gancho",
            Self::Adiada => "adiada",
            Self::Desligada => "desligada",
        }
    }

    #[must_use]
    pub fn de(s: &str) -> Option<Self> {
        match s {
            "gancho" | "hook" => Some(Self::Gancho),
            "adiada" | "deferred" => Some(Self::Adiada),
            "desligada" | "off" => Some(Self::Desligada),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execucao {
    pub codigo: i32,
    pub saida: String,
    pub erro: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envio {
    pub ref_local: String,
    pub sha_local: String,
    pub ref_remota: String,
    pub sha_remota: String,
}

impl Envio {
    #[must_use]
    pub fn eh_remocao(&self) -> bool {
        !self.sha_local.is_empty() && self.sha_local.bytes().all(|b| b == b'0')
    }
}

pub fn le_envios(entrada: &str) -> Result<Vec<Envio>, JoeiraError> {
    let mut envios = Vec::new();
    for (i, linha) in entrada.lines().enumerate() {
        let linha = linha.trim();
        if linha.is_empty() {
            continue;
        }
        let campos: Vec<&str> = linha.split_whitespace().collect();
        let [ref_local, sha_local, ref_remota, sha_remota] = campos[..] else {
            return Err(JoeiraError::AmbienteCego(format!(
                "pre-push stdin line {}: expected `<local ref> <local sha> <remote ref> <remote sha>`, got {} fields",
                i + 1,
                campos.len()
            )));
        };
        if ![sha_local, sha_remota]
            .iter()
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(JoeiraError::AmbienteCego(format!(
                "pre-push stdin line {}: object names must be hex",
                i + 1
            )));
        }
        envios.push(Envio {
            ref_local: ref_local.to_owned(),
            sha_local: sha_local.to_owned(),
            ref_remota: ref_remota.to_owned(),
            sha_remota: sha_remota.to_owned(),
        });
    }
    Ok(envios)
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
    AssuntoNaLista {
        lista: Vec<String>,
    },

    /// The subject's tail after its last colon is in this set. Catches the
    /// placeholder wearing a scope prefix (`updates: init`) while leaving the
    /// accepted `init: <what changed>` form alone.
    AssuntoCaudaNaLista {
        lista: Vec<String>,
    },

    /// Some message line contains one of these trailer markers.
    MensagemTemTrailer {
        marcadores: Vec<String>,
    },

    /// A line ADDED by this commit matches one of these patterns.
    ///
    /// Added lines, never the whole blob: scanning the blob re-accuses every
    /// matching line already in history on every later edit, which is the
    /// false-positive engine rather than a finding.
    LinhaAdicionadaCasa {
        padroes: Vec<Padrao>,
    },

    /// The staged blob carries BOTH of these line prefixes. The pair is
    /// required: a lone `=======` is a markdown rule and a lone `<<<<<<<`
    /// appears in prose about merges.
    BlobTemParDeLinhas {
        abre: String,
        fecha: String,
    },

    /// `sha256(arquivo)` equals the hex value at `campo` in `sidecar`, judged
    /// over the blobs this commit will contain. Structurally FP-free because it
    /// re-runs the consumer's own arithmetic. Absent sidecar ⇒ skip, which is
    /// what makes it safe across heterogeneous checkouts.
    AmarraDeHash {
        arquivo: String,
        sidecar: String,
        campo: String,
    },

    Rastreado {
        caminho: String,
    },

    CaminhoEmStageSob {
        prefixo: String,
    },

    CaminhoAdicionadoCasa {
        padroes: Vec<Padrao>,
    },

    ScriptAlemDaCola {
        sufixo: String,
        linhas_cola: usize,
    },

    LockMaisVelhoQue {
        arquivo: String,
        dias: u64,
    },

    Externo {
        verificador: Verificador,
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
    fn caminhos_adicionados(&self) -> Result<Vec<String>, JoeiraError>;
    fn existe(&self, caminho: &str) -> Result<bool, JoeiraError>;
    fn rastreado(&self, caminho: &str) -> Result<bool, JoeiraError>;
    fn agora_unix(&self) -> Result<u64, JoeiraError>;
    fn envios(&self) -> Result<Vec<Envio>, JoeiraError>;
    fn executa(&self, v: Verificador) -> Result<Execucao, JoeiraError>;
    fn executa_em(
        &self,
        v: Verificador,
        _orcamento: std::time::Duration,
    ) -> Result<Execucao, JoeiraError> {
        self.executa(v)
    }
    fn caminhos_enviados(&self) -> Result<Vec<String>, JoeiraError> {
        Ok(Vec::new())
    }
    fn nao_verificado(&self, _caminho: &str) -> Option<String> {
        None
    }
    fn nao_verificados(&self) -> Vec<(String, String)> {
        Vec::new()
    }
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
    pub adicionados: std::collections::BTreeSet<String>,
    pub arvore: std::collections::BTreeSet<String>,
    pub rastreados: std::collections::BTreeSet<String>,
    pub agora: u64,
    pub envios: Vec<Envio>,
    pub execucoes: BTreeMap<Verificador, Execucao>,
    pub nao_verificados: BTreeMap<String, String>,
    pub enviados: std::collections::BTreeSet<String>,
    pub lentos: std::collections::BTreeSet<Verificador>,
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

    #[must_use]
    pub fn com_adicionado(mut self, caminho: &str, conteudo: &str) -> Self {
        self.adicionados.insert(caminho.to_owned());
        self.com_stage(caminho, conteudo)
    }

    #[must_use]
    pub fn com_arvore(mut self, caminho: &str) -> Self {
        self.arvore.insert(caminho.to_owned());
        self
    }

    #[must_use]
    pub fn com_rastreado(mut self, caminho: &str) -> Self {
        self.arvore.insert(caminho.to_owned());
        self.rastreados.insert(caminho.to_owned());
        self
    }

    #[must_use]
    pub const fn com_agora(mut self, agora: u64) -> Self {
        self.agora = agora;
        self
    }

    #[must_use]
    pub fn com_execucao(mut self, v: Verificador, codigo: i32) -> Self {
        self.execucoes.insert(
            v,
            Execucao {
                codigo,
                saida: String::new(),
                erro: String::new(),
            },
        );
        self
    }

    #[must_use]
    pub fn com_execucao_erro(mut self, v: Verificador, codigo: i32, erro: &str) -> Self {
        self.execucoes.insert(
            v,
            Execucao {
                codigo,
                saida: String::new(),
                erro: erro.to_owned(),
            },
        );
        self
    }

    fn olha(&self) -> Result<(), JoeiraError> {
        if self.cego {
            return Err(JoeiraError::AmbienteCego("mock is blind".into()));
        }
        Ok(())
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

    fn caminhos_adicionados(&self) -> Result<Vec<String>, JoeiraError> {
        self.olha()?;
        Ok(self.adicionados.iter().cloned().collect())
    }

    fn existe(&self, caminho: &str) -> Result<bool, JoeiraError> {
        self.olha()?;
        let sob = |c: &String| c == caminho || c.starts_with(caminho);
        Ok(self.arvore.iter().any(sob) || self.stage.keys().any(sob) || self.head.keys().any(sob))
    }

    fn rastreado(&self, caminho: &str) -> Result<bool, JoeiraError> {
        self.olha()?;
        Ok(self.rastreados.contains(caminho)
            || self.stage.contains_key(caminho)
            || self.head.contains_key(caminho))
    }

    fn agora_unix(&self) -> Result<u64, JoeiraError> {
        self.olha()?;
        Ok(self.agora)
    }

    fn envios(&self) -> Result<Vec<Envio>, JoeiraError> {
        self.olha()?;
        Ok(self.envios.clone())
    }

    fn executa(&self, v: Verificador) -> Result<Execucao, JoeiraError> {
        self.olha()?;
        self.execucoes
            .get(&v)
            .cloned()
            .ok_or_else(|| JoeiraError::FerramentaAusente(v.programa().to_owned()))
    }

    fn executa_em(
        &self,
        v: Verificador,
        orcamento: std::time::Duration,
    ) -> Result<Execucao, JoeiraError> {
        if self.lentos.contains(&v) {
            self.olha()?;
            return Err(JoeiraError::OrcamentoEsgotado {
                linha: v.linha(),
                ms: ms(orcamento),
            });
        }
        self.executa(v)
    }

    fn caminhos_enviados(&self) -> Result<Vec<String>, JoeiraError> {
        self.olha()?;
        Ok(self.enviados.iter().cloned().collect())
    }

    fn nao_verificado(&self, caminho: &str) -> Option<String> {
        self.nao_verificados.get(caminho).cloned()
    }

    fn nao_verificados(&self) -> Vec<(String, String)> {
        self.nao_verificados
            .iter()
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect()
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
    nivel: Nivel,
    gatilho: Option<String>,
    isencoes: Vec<String>,
    orcamento: Option<std::time::Duration>,
    toca: Vec<String>,
    colocacao: Colocacao,
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
            nivel: Nivel::Sistema,
            gatilho: None,
            isencoes: Vec::new(),
            orcamento: None,
            toca: Vec::new(),
            colocacao: Colocacao::Gancho,
        }
    }

    #[must_use]
    pub const fn com_colocacao(mut self, colocacao: Colocacao) -> Self {
        self.colocacao = colocacao;
        self
    }

    #[must_use]
    pub const fn colocacao(&self) -> Colocacao {
        self.colocacao
    }

    #[must_use]
    pub const fn com_orcamento(mut self, orcamento: std::time::Duration) -> Self {
        self.orcamento = Some(orcamento);
        self
    }

    #[must_use]
    pub const fn orcamento(&self) -> std::time::Duration {
        match self.orcamento {
            Some(o) => o,
            None => orcamento_padrao(self.ponto, self.colocacao),
        }
    }

    #[must_use]
    pub fn com_toca(mut self, toca: Vec<String>) -> Self {
        self.toca = toca;
        self
    }

    #[must_use]
    pub fn toca(&self) -> &[String] {
        &self.toca
    }

    #[must_use]
    pub fn roda_ferramenta(&self) -> bool {
        roda_ferramenta(&self.predicado)
    }

    #[must_use]
    pub fn com_isencoes(mut self, isencoes: Vec<String>) -> Self {
        self.isencoes = isencoes;
        self
    }

    #[must_use]
    pub fn isencoes(&self) -> &[String] {
        &self.isencoes
    }

    #[must_use]
    pub fn com_origem(mut self, nivel: Nivel, gatilho: Option<String>) -> Self {
        self.nivel = nivel;
        self.gatilho = gatilho;
        self
    }

    #[must_use]
    pub const fn nivel(&self) -> Nivel {
        self.nivel
    }

    #[must_use]
    pub fn gatilho(&self) -> Option<&str> {
        self.gatilho.as_deref()
    }

    #[must_use]
    pub const fn reversibilidade(&self) -> Reversibilidade {
        self.reversibilidade
    }

    #[must_use]
    pub const fn fp(&self) -> ClasseFalsoPositivo {
        self.fp
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

pub const PRAZO_PRE_PUSH: std::time::Duration = std::time::Duration::from_millis(5000);
pub const PRAZO_PRE_COMMIT: std::time::Duration = std::time::Duration::from_millis(1500);
pub const PRAZO_COMMIT_MSG: std::time::Duration = std::time::Duration::from_millis(1000);

#[must_use]
pub const fn prazo_padrao(ponto: Ponto) -> std::time::Duration {
    match ponto {
        Ponto::PrePush => PRAZO_PRE_PUSH,
        Ponto::PreCommit => PRAZO_PRE_COMMIT,
        Ponto::CommitMsg => PRAZO_COMMIT_MSG,
    }
}

pub const ORCAMENTO_ADIADO: std::time::Duration = std::time::Duration::from_secs(600);

#[must_use]
pub const fn orcamento_padrao(ponto: Ponto, colocacao: Colocacao) -> std::time::Duration {
    match colocacao {
        Colocacao::Gancho => prazo_padrao(ponto),
        Colocacao::Adiada | Colocacao::Desligada => ORCAMENTO_ADIADO,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prazo {
    pub fim: std::time::Instant,
    pub total: std::time::Duration,
}

impl Prazo {
    #[must_use]
    pub fn desde(inicio: std::time::Instant, total: std::time::Duration) -> Self {
        Self {
            fim: inicio + total,
            total,
        }
    }

    #[must_use]
    pub fn resta(&self) -> std::time::Duration {
        self.fim
            .saturating_duration_since(std::time::Instant::now())
    }
}

#[must_use]
pub fn ms(d: std::time::Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

fn primeira_ferramenta(p: &Predicado) -> Option<Verificador> {
    match p {
        Predicado::Externo { verificador } => Some(*verificador),
        Predicado::Todos(ps) | Predicado::Algum(ps) => ps.iter().find_map(primeira_ferramenta),
        Predicado::Nao(q) => primeira_ferramenta(q),
        _ => None,
    }
}

fn roda_ferramenta(p: &Predicado) -> bool {
    match p {
        Predicado::Externo { .. } => true,
        Predicado::Todos(ps) | Predicado::Algum(ps) => ps.iter().any(roda_ferramenta),
        Predicado::Nao(q) => roda_ferramenta(q),
        _ => false,
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

struct Isento<'a, E: ?Sized> {
    amb: &'a E,
    prefixos: &'a [String],
}

impl<E: AmbienteGit + ?Sized> Isento<'_, E> {
    fn fora(&self, caminhos: Vec<String>) -> Vec<String> {
        caminhos
            .into_iter()
            .filter(|c| !self.prefixos.iter().any(|p| c.starts_with(p.as_str())))
            .collect()
    }
}

impl<E: AmbienteGit + ?Sized> AmbienteGit for Isento<'_, E> {
    fn mensagem(&self) -> Result<String, JoeiraError> {
        self.amb.mensagem()
    }
    fn caminhos_em_stage(&self) -> Result<Vec<String>, JoeiraError> {
        Ok(self.fora(self.amb.caminhos_em_stage()?))
    }
    fn blob_em_stage(&self, caminho: &str) -> Result<String, JoeiraError> {
        self.amb.blob_em_stage(caminho)
    }
    fn blob_em_head(&self, caminho: &str) -> Result<Option<String>, JoeiraError> {
        self.amb.blob_em_head(caminho)
    }
    fn sha256_em_stage(&self, caminho: &str) -> Result<String, JoeiraError> {
        self.amb.sha256_em_stage(caminho)
    }
    fn caminhos_adicionados(&self) -> Result<Vec<String>, JoeiraError> {
        Ok(self.fora(self.amb.caminhos_adicionados()?))
    }
    fn existe(&self, caminho: &str) -> Result<bool, JoeiraError> {
        self.amb.existe(caminho)
    }
    fn rastreado(&self, caminho: &str) -> Result<bool, JoeiraError> {
        self.amb.rastreado(caminho)
    }
    fn agora_unix(&self) -> Result<u64, JoeiraError> {
        self.amb.agora_unix()
    }
    fn envios(&self) -> Result<Vec<Envio>, JoeiraError> {
        self.amb.envios()
    }
    fn executa(&self, v: Verificador) -> Result<Execucao, JoeiraError> {
        self.amb.executa(v)
    }
    fn executa_em(
        &self,
        v: Verificador,
        orcamento: std::time::Duration,
    ) -> Result<Execucao, JoeiraError> {
        self.amb.executa_em(v, orcamento)
    }
    fn caminhos_enviados(&self) -> Result<Vec<String>, JoeiraError> {
        self.amb.caminhos_enviados()
    }
    fn nao_verificado(&self, caminho: &str) -> Option<String> {
        self.amb.nao_verificado(caminho)
    }
    fn nao_verificados(&self) -> Vec<(String, String)> {
        self.amb.nao_verificados()
    }
}

/// Evaluate one rule against one environment.
pub fn avalia<E: AmbienteGit + ?Sized>(regra: &Regra, amb: &E) -> Veredito {
    avalia_ate(regra, amb, None)
}

pub fn avalia_ate<E: AmbienteGit + ?Sized>(
    regra: &Regra,
    amb: &E,
    prazo: Option<Prazo>,
) -> Veredito {
    if regra.isencoes.is_empty() {
        avalia_em(regra, amb, prazo)
    } else {
        avalia_em(
            regra,
            &Isento {
                amb,
                prefixos: &regra.isencoes,
            },
            prazo,
        )
    }
}

fn avalia_em<E: AmbienteGit + ?Sized>(regra: &Regra, amb: &E, prazo: Option<Prazo>) -> Veredito {
    if let Some(g) = &regra.gatilho {
        match amb.existe(g) {
            Ok(true) => {}
            Ok(false) => {
                return Veredito::NaoSeAplica {
                    regra: regra.nome.clone(),
                    porque: format!("trigger `{g}` absent"),
                };
            }
            Err(e) => {
                return Veredito::Cego {
                    regra: regra.nome.clone(),
                    porque: e.to_string(),
                };
            }
        }
    }
    if !regra.toca.is_empty() {
        match amb.caminhos_enviados() {
            Ok(cs) => {
                if !cs
                    .iter()
                    .any(|c| regra.toca.iter().any(|t| c.ends_with(t.as_str())))
                {
                    return Veredito::NaoSeAplica {
                        regra: regra.nome.clone(),
                        porque: format!("the pushed range touches no {}", regra.toca.join(" or ")),
                    };
                }
            }
            Err(e) => {
                return Veredito::Cego {
                    regra: regra.nome.clone(),
                    porque: e.to_string(),
                };
            }
        }
    }
    let orcamento = match prazo {
        None => regra.orcamento(),
        Some(p) => {
            let resta = p.resta();
            if resta.is_zero() {
                return Veredito::Cego {
                    regra: regra.nome.clone(),
                    porque: JoeiraError::PrazoGasto {
                        linha: primeira_ferramenta(&regra.predicado)
                            .map_or_else(|| regra.nome.clone(), Verificador::linha),
                        ms: ms(p.total),
                    }
                    .to_string(),
                };
            }
            regra.orcamento().min(resta)
        }
    };
    match sustenta(&regra.predicado, amb, orcamento) {
        Ok(true) => Veredito::Achado {
            regra: regra.nome.clone(),
            severidade: regra.piso,
            mensagem: regra.mensagem.clone(),
        },
        Ok(false) => Veredito::Limpo,
        Err(JoeiraError::OrcamentoEsgotado { linha, .. })
            if prazo.is_some_and(|p| orcamento < regra.orcamento() && orcamento <= p.total) =>
        {
            Veredito::Cego {
                regra: regra.nome.clone(),
                porque: JoeiraError::OrcamentoEsgotado {
                    linha,
                    ms: prazo.map_or(0, |p| ms(p.total)),
                }
                .to_string(),
            }
        }
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
fn sustenta<E: AmbienteGit + ?Sized>(
    p: &Predicado,
    amb: &E,
    orc: std::time::Duration,
) -> Result<bool, JoeiraError> {
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
                if amb.nao_verificado(&caminho).is_some() {
                    continue;
                }
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
                if amb.nao_verificado(&caminho).is_some() {
                    continue;
                }
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
        Predicado::Rastreado { caminho } => amb.rastreado(caminho),
        Predicado::CaminhoEmStageSob { prefixo } => Ok(amb
            .caminhos_em_stage()?
            .iter()
            .any(|c| c.starts_with(prefixo.as_str()))),
        Predicado::CaminhoAdicionadoCasa { padroes } => {
            Ok(amb.caminhos_adicionados()?.iter().any(|c| {
                let base = c.rsplit('/').next().unwrap_or(c);
                padroes.iter().any(|p| p.casa(base))
            }))
        }
        Predicado::ScriptAlemDaCola {
            sufixo,
            linhas_cola,
        } => {
            for caminho in amb.caminhos_em_stage()? {
                if !caminho.ends_with(sufixo.as_str()) || amb.nao_verificado(&caminho).is_some() {
                    continue;
                }
                let agora = conta_linhas(&amb.blob_em_stage(&caminho)?);
                let antes = amb.blob_em_head(&caminho)?.map(|b| conta_linhas(&b));
                if agora > *linhas_cola && antes.is_none_or(|a| agora > a) {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Predicado::LockMaisVelhoQue { arquivo, dias } => {
            let Some(doc) = amb.blob_em_head(arquivo)? else {
                return Ok(false);
            };
            let Some(mais_novo) = ultimo_lastmodified(&doc) else {
                return Ok(false);
            };
            let agora = amb.agora_unix()?;
            Ok(agora.saturating_sub(mais_novo) > dias.saturating_mul(86_400))
        }
        Predicado::Externo { verificador } => {
            let e = amb.executa_em(*verificador, orc)?;
            if e.codigo == 0 {
                return Ok(false);
            }
            match verificador.nao_verificavel(&e.erro) {
                Some(porque) => Err(JoeiraError::NaoVerificavel(porque)),
                None => Ok(true),
            }
        }
        Predicado::Todos(ps) => {
            for p in ps {
                if !sustenta(p, amb, orc)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Predicado::Algum(ps) => {
            for p in ps {
                if sustenta(p, amb, orc)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Predicado::Nao(p) => Ok(!sustenta(p, amb, orc)?),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filtro {
    pub todos: bool,
    pub caminhos: std::collections::BTreeSet<String>,
    pub sufixos: std::collections::BTreeSet<String>,
}

impl Filtro {
    #[must_use]
    pub fn quer(&self, caminho: &str) -> bool {
        self.todos
            || self.caminhos.contains(caminho)
            || self.sufixos.iter().any(|s| caminho.ends_with(s.as_str()))
    }

    #[must_use]
    pub fn vazio(&self) -> bool {
        !self.todos && self.caminhos.is_empty() && self.sufixos.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Necessidades {
    pub stage: Filtro,
    pub head: Filtro,
}

impl Necessidades {
    #[must_use]
    pub fn tudo() -> Self {
        let todos = Filtro {
            todos: true,
            ..Filtro::default()
        };
        Self {
            stage: todos.clone(),
            head: todos,
        }
    }
}

#[must_use]
pub fn necessidades(regras: &[Regra]) -> Necessidades {
    let mut n = Necessidades::default();
    for r in regras {
        acumula(&r.predicado, &mut n);
    }
    n
}

fn acumula(p: &Predicado, n: &mut Necessidades) {
    match p {
        Predicado::LinhaAdicionadaCasa { .. } => {
            n.stage.todos = true;
            n.head.todos = true;
        }
        Predicado::BlobTemParDeLinhas { .. } => n.stage.todos = true,
        Predicado::AmarraDeHash {
            arquivo, sidecar, ..
        } => {
            n.stage.caminhos.insert(arquivo.clone());
            n.stage.caminhos.insert(sidecar.clone());
        }
        Predicado::ScriptAlemDaCola { sufixo, .. } => {
            n.stage.sufixos.insert(sufixo.clone());
            n.head.sufixos.insert(sufixo.clone());
        }
        Predicado::LockMaisVelhoQue { arquivo, .. } => {
            n.head.caminhos.insert(arquivo.clone());
        }
        Predicado::Todos(ps) | Predicado::Algum(ps) => {
            for q in ps {
                acumula(q, n);
            }
        }
        Predicado::Nao(q) => acumula(q, n),
        Predicado::AssuntoNaLista { .. }
        | Predicado::AssuntoCaudaNaLista { .. }
        | Predicado::MensagemTemTrailer { .. }
        | Predicado::Rastreado { .. }
        | Predicado::CaminhoEmStageSob { .. }
        | Predicado::CaminhoAdicionadoCasa { .. }
        | Predicado::Externo { .. } => {}
    }
}

fn conta_linhas(blob: &str) -> usize {
    blob.lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        })
        .count()
}

fn ultimo_lastmodified(doc: &str) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(doc).ok()?;
    v.get("nodes")?
        .as_object()?
        .values()
        .filter_map(|n| n.get("locked")?.get("lastModified")?.as_u64())
        .max()
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

pub mod camadas;
pub mod journal;
pub mod shell;

#[cfg(test)]
mod tests;
