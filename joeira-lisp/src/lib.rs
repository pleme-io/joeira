//! joeira's authoring surface — the `(defjoeira …)` form.
//!
//! **TIER: M0, hand-written reader. NOT yet a `#[derive(TataraDomain)]` domain.**
//! Stated plainly because the difference is load-bearing: the derive buys
//! registry dispatch and a keyword closed against `tatara-lisp`'s own catalog,
//! and this crate has neither yet. What it does have is the shape the derive
//! will replace, so consumers can be written against `ler` today and keep
//! working when the border moves underneath them.
//!
//! `pending-joeira: tatara-domain-border` — the derive lands with `joeira-core`
//! M1, alongside the `defjoeira-ponto` catalog.

use joeira_core::{ClasseFalsoPositivo, Ponto, Reversibilidade};

/// The fleet catalog, compiled in.
///
/// `include_str!` rather than a runtime path: the data is part of the library's
/// contract, so a consumer cannot be handed a joeira whose catalog is missing,
/// stale, or pointing at a file someone else can edit. It also means the
/// parse is exercised by this crate's own tests rather than only by whatever
/// binary happens to load it.
pub const CATALOGO: &str = include_str!("../catalogo/joeira.tlisp");

/// Why a form was refused. Every arm names the position, because a parse error
/// without one sends the reader to the wrong line.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ErroDeLeitura {
    #[error("expected a `(defjoeira …)` form, found {achado:?}")]
    NaoEhDefjoeira { achado: String },

    /// **The trap this exists to close.** A typo'd keyword in a plist reader
    /// yields an EMPTY value that reports as success — so an unknown keyword is
    /// a refusal here, never a silent default.
    #[error("unknown keyword `{chave}` in rule `{regra}` (recognised: {conhecidas})")]
    ChaveDesconhecida {
        regra: String,
        chave: String,
        conhecidas: String,
    },

    #[error("rule `{regra}` is missing required keyword `{chave}`")]
    ChaveAusente { regra: String, chave: String },

    #[error(
        "rule `{regra}`: `:ponto {achado}` is not a known mount point (recognised: {conhecidas})"
    )]
    PontoDesconhecido {
        regra: String,
        achado: String,
        conhecidas: String,
    },

    #[error("unbalanced parentheses")]
    ParentesesDesbalanceados,

    #[error("rule {regra:?}: {chave} got {achado:?}, expected one of: {conhecidas}")]
    SimboloDesconhecido {
        regra: String,
        chave: String,
        achado: String,
        conhecidas: String,
    },

    #[error("rule {regra:?}: :lista must be a parenthesised list, found {achado:?}")]
    ListaMalFormada { regra: String, achado: String },

    #[error("trailing content after the form: {achado:?} — use `ler_catalogo` for more than one")]
    ConteudoExtra { achado: String },
}

/// The keywords `(defjoeira …)` accepts. Closed, and echoed into every
/// `ChaveDesconhecida` so the error teaches the surface.
pub const CHAVES: &[&str] = &[
    ":nome",
    ":ponto",
    ":mensagem",
    ":reversibilidade",
    ":fp",
    ":lista",
];

/// A read form, before it becomes a `joeira_core::Regra`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormaLida {
    pub nome: String,
    pub ponto: Ponto,
    pub mensagem: String,
    /// Both axes are optional in the READER and mandatory in `Regra`. That is
    /// deliberate: a form omitting them is a well-formed form that cannot be
    /// lowered, so the failure names the missing axis at lowering time rather
    /// than being a parse error about a field the author may legitimately be
    /// mid-way through writing.
    pub reversibilidade: Option<Reversibilidade>,
    pub fp: Option<ClasseFalsoPositivo>,
    /// The word list for a subject-list predicate. Empty when absent — NOT
    /// `Option`, because "no list" and "an empty list" behave identically for
    /// every consumer, and two spellings of one state is the shape this repo
    /// exists to remove.
    pub lista: Vec<String>,
}

/// Resolve a `:ponto` symbol. Closed over `Ponto`, so an unknown mount point is
/// **parse-time-rejected** rather than mounted nowhere.
fn ponto_de(regra: &str, s: &str) -> Result<Ponto, ErroDeLeitura> {
    let conhecidas = Ponto::todos()
        .iter()
        .map(|p| p.arquivo())
        .collect::<Vec<_>>()
        .join(", ");
    Ponto::todos()
        .iter()
        .copied()
        .find(|p| p.arquivo() == s)
        .ok_or_else(|| ErroDeLeitura::PontoDesconhecido {
            regra: regra.to_owned(),
            achado: s.to_owned(),
            conhecidas,
        })
}

/// Resolve a `:reversibilidade` symbol against the closed roster in core.
///
/// The roster and the spelling both come from `Reversibilidade` itself, so this
/// function cannot drift from the type: adding a variant makes it resolvable
/// here with no edit, and there is no second table of names to forget.
fn reversibilidade_de(regra: &str, s: &str) -> Result<Reversibilidade, ErroDeLeitura> {
    Reversibilidade::todos()
        .iter()
        .copied()
        .find(|r| r.simbolo() == s)
        .ok_or_else(|| ErroDeLeitura::SimboloDesconhecido {
            regra: regra.to_owned(),
            chave: ":reversibilidade".to_owned(),
            achado: s.to_owned(),
            conhecidas: Reversibilidade::todos()
                .iter()
                .map(|r| r.simbolo())
                .collect::<Vec<_>>()
                .join(", "),
        })
}

/// Resolve a `:fp` symbol against the closed roster in core.
fn fp_de(regra: &str, s: &str) -> Result<ClasseFalsoPositivo, ErroDeLeitura> {
    ClasseFalsoPositivo::todos()
        .iter()
        .copied()
        .find(|c| c.simbolo() == s)
        .ok_or_else(|| ErroDeLeitura::SimboloDesconhecido {
            regra: regra.to_owned(),
            chave: ":fp".to_owned(),
            achado: s.to_owned(),
            conhecidas: ClasseFalsoPositivo::todos()
                .iter()
                .map(|c| c.simbolo())
                .collect::<Vec<_>>()
                .join(", "),
        })
}

/// Read one `(defjoeira :nome "x" :ponto pre-commit :mensagem "y" …)` form.
///
/// A deliberately small reader: it tokenises on whitespace outside strings and
/// walks a plist. `tatara-lisp` has no map and no vector — plists are the shape
/// — so this mirrors the surface the derive will generate rather than inventing
/// a richer one that would later have to be taken away.
///
/// Refuses trailing content, so `(defjoeira …) (defjoeira …)` handed to `ler`
/// is an error naming the second form rather than a silent read of the first.
/// Use [`ler_catalogo`] for more than one.
pub fn ler(fonte: &str) -> Result<FormaLida, ErroDeLeitura> {
    let toks = tokenise(fonte)?;
    let mut it = toks.iter().peekable();
    let forma = ler_forma(&mut it)?;
    if let Some(resto) = it.next() {
        return Err(ErroDeLeitura::ConteudoExtra {
            achado: resto.clone(),
        });
    }
    Ok(forma)
}

/// Read every `(defjoeira …)` form in `fonte`, in source order.
///
/// An EMPTY source is `Ok(vec![])` and not an error — a catalog with no forms
/// is a legitimate starting state. The consumer that needs a non-empty catalog
/// asserts its own floor, because only the consumer knows what its denominator
/// should be, and a reader that refused empty input would make an honest
/// "nothing declared yet" indistinguishable from a parse failure.
pub fn ler_catalogo(fonte: &str) -> Result<Vec<FormaLida>, ErroDeLeitura> {
    let toks = tokenise(fonte)?;
    let mut it = toks.iter().peekable();
    let mut formas = Vec::new();
    while it.peek().is_some() {
        formas.push(ler_forma(&mut it)?);
    }
    Ok(formas)
}

/// The shared reader both entry points drive.
///
/// Factored out rather than duplicated: a catalog reader that re-implemented
/// the plist walk would be a second place for the keyword set to drift, which
/// is the whole failure mode `CHAVES` being one constant is meant to prevent.
fn ler_forma<'a, I>(it: &mut std::iter::Peekable<I>) -> Result<FormaLida, ErroDeLeitura>
where
    I: Iterator<Item = &'a String>,
{
    match it.next().map(String::as_str) {
        Some("(") => {}
        outro => {
            return Err(ErroDeLeitura::NaoEhDefjoeira {
                achado: outro.unwrap_or("<empty>").to_owned(),
            });
        }
    }
    match it.next().map(String::as_str) {
        Some("defjoeira") => {}
        outro => {
            return Err(ErroDeLeitura::NaoEhDefjoeira {
                achado: outro.unwrap_or("<empty>").to_owned(),
            });
        }
    }

    let mut nome = None;
    let mut ponto = None;
    let mut mensagem = None;
    let mut reversibilidade = None;
    let mut fp = None;
    let mut lista = Vec::new();

    while let Some(t) = it.next() {
        if t == ")" {
            break;
        }
        if !t.starts_with(':') {
            continue;
        }
        let rotulo = nome.clone().unwrap_or_else(|| "<unnamed>".to_owned());

        // `:lista` takes a parenthesised list; every other key takes one value.
        // Peeked rather than assumed, so a `:lista` written without parens is a
        // named error instead of silently reading the next keyword as its value.
        if t == ":lista" {
            match it.next().map(String::as_str) {
                Some("(") => {}
                outro => {
                    return Err(ErroDeLeitura::ListaMalFormada {
                        regra: rotulo,
                        achado: outro.unwrap_or("<end of form>").to_owned(),
                    });
                }
            }
            for v in it.by_ref() {
                if v == ")" {
                    break;
                }
                lista.push(v.clone());
            }
            continue;
        }

        let valor = it.next().cloned().unwrap_or_default();
        match t.as_str() {
            ":nome" => nome = Some(valor),
            ":ponto" => ponto = Some(valor),
            ":mensagem" => mensagem = Some(valor),
            ":reversibilidade" => reversibilidade = Some(reversibilidade_de(&rotulo, &valor)?),
            ":fp" => fp = Some(fp_de(&rotulo, &valor)?),
            outra => {
                return Err(ErroDeLeitura::ChaveDesconhecida {
                    regra: rotulo,
                    chave: outra.to_owned(),
                    conhecidas: CHAVES.join(" "),
                });
            }
        }
    }

    let nome = nome.ok_or_else(|| ErroDeLeitura::ChaveAusente {
        regra: "<unnamed>".to_owned(),
        chave: ":nome".to_owned(),
    })?;
    let ponto_s = ponto.ok_or_else(|| ErroDeLeitura::ChaveAusente {
        regra: nome.clone(),
        chave: ":ponto".to_owned(),
    })?;
    let mensagem = mensagem.ok_or_else(|| ErroDeLeitura::ChaveAusente {
        regra: nome.clone(),
        chave: ":mensagem".to_owned(),
    })?;

    Ok(FormaLida {
        ponto: ponto_de(&nome, &ponto_s)?,
        nome,
        mensagem,
        reversibilidade,
        fp,
        lista,
    })
}

/// Tokenise, keeping quoted strings whole and stripping `;;` comments.
fn tokenise(fonte: &str) -> Result<Vec<String>, ErroDeLeitura> {
    let mut toks = Vec::new();
    let mut atual = String::new();
    let mut em_string = false;
    let mut profundidade: i32 = 0;

    for linha in fonte.lines() {
        // Comment stripping happens INSIDE the char walk, guarded on
        // `em_string`. Stripping per-line beforehand cannot know whether the
        // `;;` sits inside a string, so `:mensagem "see ;; below"` lost its
        // closing quote and the whole catalog failed as unbalanced — a false
        // rejection of a legitimate message. Safe direction, still wrong.
        let mut comentario = false;
        let mut anterior = '\0';
        for c in linha.chars() {
            if comentario {
                break;
            }
            if c == ';' && anterior == ';' && !em_string {
                // Un-push the first `;` if it was collected as an atom char.
                if atual.ends_with(';') {
                    atual.pop();
                }
                if !atual.is_empty() {
                    toks.push(std::mem::take(&mut atual));
                }
                comentario = true;
                continue;
            }
            anterior = c;
            match c {
                '"' => {
                    if em_string {
                        toks.push(std::mem::take(&mut atual));
                        em_string = false;
                    } else {
                        em_string = true;
                    }
                }
                _ if em_string => atual.push(c),
                '(' | ')' => {
                    if !atual.is_empty() {
                        toks.push(std::mem::take(&mut atual));
                    }
                    profundidade += if c == '(' { 1 } else { -1 };
                    toks.push(c.to_string());
                }
                c if c.is_whitespace() => {
                    if !atual.is_empty() {
                        toks.push(std::mem::take(&mut atual));
                    }
                }
                c => atual.push(c),
            }
        }
    }
    if !atual.is_empty() {
        toks.push(atual);
    }
    if profundidade != 0 || em_string {
        return Err(ErroDeLeitura::ParentesesDesbalanceados);
    }
    Ok(toks)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── The compiled-in catalog ──────────────────────────────────────────

    #[test]
    fn the_fleet_catalog_parses() {
        let formas = ler_catalogo(CATALOGO).expect("the compiled-in catalog must parse");
        assert_eq!(formas.len(), 5, "five concerns");
        let nomes: Vec<&str> = formas.iter().map(|f| f.nome.as_str()).collect();
        assert_eq!(
            nomes,
            vec![
                "msg-placeholder-subject",
                "vcs-conflict-markers",
                "gen-lock-tie",
                "sec-plaintext-credential",
                "msg-ai-attribution-trailer",
            ],
            "source order is preserved"
        );
    }

    /// The number is asserted, not merely read. A catalog that silently lost
    /// most of its words would still parse, still build a rule, and still pass
    /// every behavioural test — while refusing almost nothing. This is the
    /// denominator for every parity claim made against the incumbent.
    #[test]
    fn the_placeholder_list_is_the_real_77() {
        let formas = ler_catalogo(CATALOGO).expect("parses");
        let msg = formas
            .iter()
            .find(|f| f.nome == "msg-placeholder-subject")
            .expect("the commit-msg concern is present");
        assert_eq!(msg.lista.len(), 77, "the fleet list is 77 words");
        let unicas: std::collections::BTreeSet<&String> = msg.lista.iter().collect();
        assert_eq!(unicas.len(), 77, "no duplicates");
        for w in ["init", "wip", "refactor!", ".", "..", "hotfix"] {
            assert!(msg.lista.iter().any(|x| x == w), "{w} is in the list");
        }
    }

    #[test]
    fn every_catalog_form_carries_both_axes() {
        for f in ler_catalogo(CATALOGO).expect("parses") {
            assert!(
                f.reversibilidade.is_some(),
                "{}: :reversibilidade is what makes the ceiling derivable",
                f.nome
            );
            assert!(f.fp.is_some(), "{}: :fp likewise", f.nome);
        }
    }

    /// The ceiling the catalog's axes derive, checked against `Severidade::tecto`
    /// — so the catalog and the lattice cannot disagree about what a form means.
    #[test]
    fn the_catalog_axes_derive_the_expected_ceilings() {
        use joeira_core::Severidade;
        let esperado = [
            // custoso x token-exato — a loud warning, no ratchet.
            ("msg-placeholder-subject", Severidade::Avisa),
            // custoso x zero-estrutural — the only cell that reaches a refusal
            // without being irreversible.
            ("vcs-conflict-markers", Severidade::Bloqueia),
            ("gen-lock-tie", Severidade::Bloqueia),
            // irreversivel x prosa — the RATCHET tier, not a plain warning.
            // A prose matcher defending an irreversible class is exactly the
            // case that needs a baseline that may not grow, which is why
            // `joeira-ratchet` exists rather than being a nicety.
            ("sec-plaintext-credential", Severidade::AvisaComCatraca),
            ("msg-ai-attribution-trailer", Severidade::Avisa),
        ];
        for f in ler_catalogo(CATALOGO).expect("parses") {
            let (_, quer) = esperado
                .iter()
                .find(|(n, _)| *n == f.nome)
                .expect("every form is accounted for");
            let tecto = Severidade::tecto(
                f.reversibilidade.expect("axis present"),
                f.fp.expect("axis present"),
            );
            assert_eq!(tecto, *quer, "{}: derived ceiling", f.nome);
        }
    }

    /// `pode_recusar` and the lattice are two spellings of one fact, so they are
    /// compared rather than trusted. This is the invariant the nix gate's
    /// `githooks-blocks-are-structural-only` row asserts on the other side.
    #[test]
    fn pode_recusar_agrees_with_the_lattice() {
        use joeira_core::Severidade;
        for fp in ClasseFalsoPositivo::todos() {
            let alcanca_recusa = Reversibilidade::todos()
                .iter()
                .any(|r| Severidade::tecto(*r, *fp) == Severidade::Bloqueia);
            assert_eq!(
                fp.pode_recusar(),
                alcanca_recusa,
                "{}: pode_recusar must match whether ANY reversibilidade reaches Recusa",
                fp.simbolo()
            );
        }
    }

    // ── The reader's new surface ─────────────────────────────────────────

    #[test]
    fn an_unknown_axis_symbol_is_refused() {
        let e = ler(r#"(defjoeira :nome "r" :ponto commit-msg :mensagem "m" :fp vibes)"#)
            .expect_err("an unknown :fp must not be accepted");
        let texto = e.to_string();
        assert!(texto.contains("vibes"), "names what was found: {texto}");
        assert!(texto.contains("prosa"), "lists the roster: {texto}");
    }

    #[test]
    fn a_lista_without_parens_is_refused() {
        let e = ler(r#"(defjoeira :nome "r" :ponto commit-msg :mensagem "m" :lista "init")"#)
            .expect_err("a bare :lista value must not be read as a one-word list");
        assert!(matches!(e, ErroDeLeitura::ListaMalFormada { .. }), "{e}");
    }

    #[test]
    fn a_semicolon_pair_inside_a_string_survives() {
        // The bug this pins: comment stripping used to run per-line BEFORE
        // string tracking, so the closing quote was eaten and the whole form
        // failed as unbalanced.
        let f = ler(r#"(defjoeira :nome "r" :ponto commit-msg :mensagem "see ;; below")"#)
            .expect("a message may contain ;;");
        assert_eq!(f.mensagem, "see ;; below");
    }

    #[test]
    fn a_real_comment_is_still_stripped() {
        let f = ler("(defjoeira :nome \"r\" :ponto commit-msg ;; a note\n :mensagem \"m\")")
            .expect("parses");
        assert_eq!(f.nome, "r");
        assert_eq!(f.mensagem, "m");
    }

    #[test]
    fn ler_refuses_a_second_form() {
        let dois = r#"(defjoeira :nome "a" :ponto commit-msg :mensagem "m")
                      (defjoeira :nome "b" :ponto commit-msg :mensagem "m")"#;
        let e = ler(dois).expect_err("ler reads exactly one form");
        assert!(matches!(e, ErroDeLeitura::ConteudoExtra { .. }), "{e}");
        assert_eq!(ler_catalogo(dois).expect("catalogo reads both").len(), 2);
    }

    #[test]
    fn an_empty_catalog_is_not_an_error() {
        assert_eq!(ler_catalogo("").expect("empty is Ok"), vec![]);
        assert_eq!(
            ler_catalogo(";; only a comment\n").expect("comment-only is Ok"),
            vec![]
        );
    }

    const FORMA: &str = r#"
        ;; the placeholder guard, as authored
        (defjoeira
          :nome "msg-placeholder-subject"
          :ponto commit-msg
          :mensagem "refusing a placeholder subject")
    "#;

    #[test]
    fn reads_a_well_formed_form() {
        let f = ler(FORMA).expect("reads");
        assert_eq!(f.nome, "msg-placeholder-subject");
        assert_eq!(f.ponto, Ponto::CommitMsg);
        assert_eq!(f.mensagem, "refusing a placeholder subject");
    }

    /// THE trap: a typo'd keyword must REFUSE, not yield an empty value that
    /// reports as success.
    #[test]
    fn a_typod_keyword_is_refused_not_defaulted() {
        let e = ler(r#"(defjoeira :nome "r" :ponto commit-msg :mensagem "m" :pontoo commit-msg)"#)
            .expect_err("must refuse");
        assert!(matches!(e, ErroDeLeitura::ChaveDesconhecida { .. }));
        // The error teaches the surface rather than just naming the typo.
        assert!(e.to_string().contains(":ponto"));
    }

    /// An unknown mount point is parse-time-rejected, so a rule cannot be
    /// mounted nowhere.
    #[test]
    fn an_unknown_ponto_is_refused() {
        let e = ler(r#"(defjoeira :nome "r" :ponto post-receive :mensagem "m")"#)
            .expect_err("must refuse");
        match e {
            ErroDeLeitura::PontoDesconhecido { achado, .. } => assert_eq!(achado, "post-receive"),
            outro => panic!("wrong arm: {outro:?}"),
        }
    }

    #[test]
    fn a_missing_required_keyword_is_named() {
        let e = ler(r#"(defjoeira :nome "r" :ponto commit-msg)"#).expect_err("must refuse");
        match e {
            ErroDeLeitura::ChaveAusente { regra, chave } => {
                assert_eq!(regra, "r");
                assert_eq!(chave, ":mensagem");
            }
            outro => panic!("wrong arm: {outro:?}"),
        }
    }

    #[test]
    fn a_non_defjoeira_form_is_refused() {
        assert!(matches!(
            ler("(defmonitor :name \"x\")").expect_err("must refuse"),
            ErroDeLeitura::NaoEhDefjoeira { .. }
        ));
    }

    #[test]
    fn unbalanced_parens_are_refused() {
        assert!(matches!(
            ler(r#"(defjoeira :nome "r" :ponto commit-msg :mensagem "m""#)
                .expect_err("must refuse"),
            ErroDeLeitura::ParentesesDesbalanceados
        ));
    }

    #[test]
    fn comments_and_whitespace_do_not_change_the_reading() {
        let denso = r#"(defjoeira :nome "r" :ponto pre-commit :mensagem "m")"#;
        let esparso = "\n;; leading\n(defjoeira\n  :nome \"r\"  ;; trailing\n  :ponto pre-commit\n  :mensagem \"m\")\n";
        assert_eq!(ler(denso).unwrap(), ler(esparso).unwrap());
    }
}
