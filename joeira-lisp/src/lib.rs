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

use joeira_core::Ponto;

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
}

/// The keywords `(defjoeira …)` accepts. Closed, and echoed into every
/// `ChaveDesconhecida` so the error teaches the surface.
pub const CHAVES: &[&str] = &[":nome", ":ponto", ":mensagem", ":reversibilidade", ":fp"];

/// A read form, before it becomes a `joeira_core::Regra`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormaLida {
    pub nome: String,
    pub ponto: Ponto,
    pub mensagem: String,
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

/// Read one `(defjoeira :nome "x" :ponto pre-commit :mensagem "y" …)` form.
///
/// A deliberately small reader: it tokenises on whitespace outside strings and
/// walks a plist. `tatara-lisp` has no map and no vector — plists are the shape
/// — so this mirrors the surface the derive will generate rather than inventing
/// a richer one that would later have to be taken away.
pub fn ler(fonte: &str) -> Result<FormaLida, ErroDeLeitura> {
    let toks = tokenise(fonte)?;
    let mut it = toks.iter().peekable();

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

    while let Some(t) = it.next() {
        if t == ")" {
            break;
        }
        if !t.starts_with(':') {
            continue;
        }
        let valor = it.next().cloned().unwrap_or_default();
        let rotulo = nome.clone().unwrap_or_else(|| "<unnamed>".to_owned());
        match t.as_str() {
            ":nome" => nome = Some(valor),
            ":ponto" => ponto = Some(valor),
            ":mensagem" => mensagem = Some(valor),
            // Accepted and not yet consumed — the axes exist in core but the M0
            // reader does not build a full Regra. Named rather than dropped.
            ":reversibilidade" | ":fp" => {}
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
    })
}

/// Tokenise, keeping quoted strings whole and stripping `;;` comments.
fn tokenise(fonte: &str) -> Result<Vec<String>, ErroDeLeitura> {
    let mut toks = Vec::new();
    let mut atual = String::new();
    let mut em_string = false;
    let mut profundidade: i32 = 0;

    for linha in fonte.lines() {
        let linha = linha.split_once(";;").map_or(linha, |(antes, _)| antes);
        for c in linha.chars() {
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
