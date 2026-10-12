use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Nivel, Regra, Severidade};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Camada {
    Sistema,
    Usuario,
    Borda,
}

impl Camada {
    #[must_use]
    pub const fn todos() -> &'static [Self] {
        &[Self::Sistema, Self::Usuario, Self::Borda]
    }

    #[must_use]
    pub const fn simbolo(self) -> &'static str {
        match self {
            Self::Sistema => "sistema",
            Self::Usuario => "usuario",
            Self::Borda => "borda",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verbo {
    Extend,
    Relax,
    ExemptPaths,
    Subtract,
}

impl Verbo {
    #[must_use]
    pub const fn todos() -> &'static [Self] {
        &[Self::Extend, Self::Relax, Self::ExemptPaths, Self::Subtract]
    }

    #[must_use]
    pub const fn simbolo(self) -> &'static str {
        match self {
            Self::Extend => "extend",
            Self::Relax => "relax",
            Self::ExemptPaths => "exempt-paths",
            Self::Subtract => "subtract",
        }
    }

    #[must_use]
    pub const fn precisa_de_concessao(self) -> bool {
        !matches!(self, Self::Extend)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Concessao {
    pub prefixo: PathBuf,
    pub verbos: BTreeSet<Verbo>,
    pub regras: BTreeSet<String>,
}

impl Concessao {
    #[must_use]
    pub fn cobre(&self, autoridade: &Path, verbo: Verbo, regra: &str) -> bool {
        self.prefixo.is_absolute()
            && autoridade.starts_with(&self.prefixo)
            && self.verbos.contains(&verbo)
            && self.regras.contains(regra)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArquivoDeCamada {
    pub nivel: Nivel,
    #[serde(default)]
    pub pisos: BTreeMap<String, Severidade>,
    #[serde(default)]
    pub concessoes: BTreeMap<String, Concessao>,
    #[serde(default)]
    pub orcamentos: BTreeMap<String, u64>,
}

pub const ORCAMENTO_MAXIMO_S: u64 = 600;

pub fn le_arquivo_de_camada(texto: &str) -> Result<ArquivoDeCamada, String> {
    serde_json::from_str(texto).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clausula {
    Estende {
        regra: String,
        piso: Severidade,
    },
    Relaxa {
        regra: String,
        piso: Severidade,
    },
    IsentaCaminhos {
        regra: String,
        caminhos: Vec<String>,
    },
    Subtrai {
        regra: String,
    },
}

impl Clausula {
    #[must_use]
    pub const fn verbo(&self) -> Verbo {
        match self {
            Self::Estende { .. } => Verbo::Extend,
            Self::Relaxa { .. } => Verbo::Relax,
            Self::IsentaCaminhos { .. } => Verbo::ExemptPaths,
            Self::Subtrai { .. } => Verbo::Subtract,
        }
    }

    #[must_use]
    pub fn regra(&self) -> &str {
        match self {
            Self::Estende { regra, .. }
            | Self::Relaxa { regra, .. }
            | Self::IsentaCaminhos { regra, .. }
            | Self::Subtrai { regra } => regra,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Borda {
    pub clausulas: Vec<Clausula>,
}

pub type Lida<T> = Option<(String, Result<T, String>)>;

#[derive(Debug, Clone, Default)]
pub struct Fontes {
    pub sistema: Lida<ArquivoDeCamada>,
    pub usuario: Lida<ArquivoDeCamada>,
    pub borda: Lida<Borda>,
    pub autoridade: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origem {
    pub camada: Option<Camada>,
    pub fonte: String,
}

impl Origem {
    #[must_use]
    pub fn rotulo(&self) -> String {
        match self.camada {
            Some(c) => format!("layer {} ({})", c.simbolo(), self.fonte),
            None => format!("catalog ({})", self.fonte),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recusa {
    pub camada: Camada,
    pub fonte: String,
    pub porque: String,
}

impl std::fmt::Display for Recusa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "layer {} ({}): {}",
            self.camada.simbolo(),
            self.fonte,
            self.porque
        )
    }
}

#[derive(Debug, Clone)]
pub struct Resolucao {
    pub regras: Vec<Regra>,
    pub origens: BTreeMap<String, Origem>,
    pub subtraidas: BTreeMap<String, Origem>,
    pub recusas: Vec<Recusa>,
}

impl Resolucao {
    #[must_use]
    pub fn origem(&self, regra: &str) -> Option<&Origem> {
        self.origens.get(regra)
    }
}

pub const FONTE_CATALOGO: &str = "joeira-lisp/catalogo/joeira.tlisp";

struct Dobra {
    regras: Vec<Regra>,
    origens: BTreeMap<String, Origem>,
    subtraidas: BTreeMap<String, Origem>,
    recusas: Vec<Recusa>,
}

impl Dobra {
    fn recusa(&mut self, camada: Camada, fonte: &str, porque: String) {
        self.recusas.push(Recusa {
            camada,
            fonte: fonte.to_owned(),
            porque,
        });
    }

    fn posicao(&self, nome: &str) -> Option<usize> {
        self.regras.iter().position(|r| r.nome() == nome)
    }

    fn eleva(&mut self, i: usize, piso: Severidade, camada: Camada, fonte: &str) {
        let tecto = self.regras[i].tecto();
        let nome = self.regras[i].nome().to_owned();
        if piso > tecto {
            self.recusa(
                camada,
                fonte,
                format!(
                    "rule `{nome}`: floor {} clamped to its ceiling {}",
                    piso.simbolo(),
                    tecto.simbolo()
                ),
            );
        }
        let alvo = piso.min(tecto);
        if alvo > self.regras[i].severidade() {
            self.regras[i] = self.regras[i].clone().com_piso(alvo);
            self.origens.insert(
                nome,
                Origem {
                    camada: Some(camada),
                    fonte: fonte.to_owned(),
                },
            );
        }
    }

    fn camada_de_arquivo(
        &mut self,
        camada: Camada,
        nivel: Nivel,
        lida: &Lida<ArquivoDeCamada>,
    ) -> Option<ArquivoDeCamada> {
        let (fonte, r) = lida.as_ref()?;
        let a = match r {
            Ok(a) => a,
            Err(e) => {
                self.recusa(camada, fonte, format!("layer dropped, unreadable: {e}"));
                return None;
            }
        };
        if a.nivel != nivel {
            self.recusa(
                camada,
                fonte,
                format!(
                    "layer dropped: declares nivel {} where {} was expected",
                    a.nivel.simbolo(),
                    nivel.simbolo()
                ),
            );
            return None;
        }
        for (nome, segundos) in &a.orcamentos {
            match self.posicao(nome) {
                None => self.recusa(
                    camada,
                    fonte,
                    format!("budget for unknown rule `{nome}` refused"),
                ),
                Some(_) if *segundos == 0 || *segundos > ORCAMENTO_MAXIMO_S => self.recusa(
                    camada,
                    fonte,
                    format!(
                        "budget {segundos}s for `{nome}` refused: must be 1..={ORCAMENTO_MAXIMO_S}"
                    ),
                ),
                Some(i) => {
                    self.regras[i] = self.regras[i]
                        .clone()
                        .com_orcamento(std::time::Duration::from_secs(*segundos));
                }
            }
        }
        for (nome, piso) in &a.pisos {
            match self.posicao(nome) {
                Some(i) => self.eleva(i, *piso, camada, fonte),
                None => self.recusa(camada, fonte, format!("unknown rule `{nome}` refused")),
            }
        }
        Some(a.clone())
    }
}

#[must_use]
pub fn resolve(base: Vec<Regra>, fontes: &Fontes) -> Resolucao {
    let origens = base
        .iter()
        .map(|r| {
            (
                r.nome().to_owned(),
                Origem {
                    camada: None,
                    fonte: FONTE_CATALOGO.to_owned(),
                },
            )
        })
        .collect();
    let mut d = Dobra {
        regras: base,
        origens,
        subtraidas: BTreeMap::new(),
        recusas: Vec::new(),
    };

    let sistema = d.camada_de_arquivo(Camada::Sistema, Nivel::Sistema, &fontes.sistema);
    if let Some((fonte, Ok(u))) = &fontes.usuario
        && !u.concessoes.is_empty()
    {
        d.recusa(
            Camada::Usuario,
            fonte,
            "grants are read from the system layer only; these were ignored".to_owned(),
        );
    }
    d.camada_de_arquivo(Camada::Usuario, Nivel::Usuario, &fontes.usuario);

    let concessoes = sistema.map(|s| s.concessoes).unwrap_or_default();
    let fonte_concessoes = fontes
        .sistema
        .as_ref()
        .map_or_else(|| "no system layer".to_owned(), |(f, _)| f.clone());

    match &fontes.borda {
        None => {}
        Some((fonte, Err(e))) => d.recusa(
            Camada::Borda,
            fonte,
            format!("edge dropped, fleet rules still apply: {e}"),
        ),
        Some((fonte, Ok(b))) => {
            for c in &b.clausulas {
                aplica(&mut d, c, fonte, &concessoes, &fonte_concessoes, fontes);
            }
        }
    }

    Resolucao {
        regras: d.regras,
        origens: d.origens,
        subtraidas: d.subtraidas,
        recusas: d.recusas,
    }
}

fn aplica(
    d: &mut Dobra,
    c: &Clausula,
    fonte: &str,
    concessoes: &BTreeMap<String, Concessao>,
    fonte_concessoes: &str,
    fontes: &Fontes,
) {
    let nome = c.regra();
    let verbo = c.verbo();
    let Some(i) = d.posicao(nome) else {
        d.recusa(
            Camada::Borda,
            fonte,
            format!("{} on unknown rule `{nome}` refused", verbo.simbolo()),
        );
        return;
    };
    if verbo.precisa_de_concessao() {
        let Some(autoridade) = &fontes.autoridade else {
            d.recusa(
                Camada::Borda,
                fonte,
                format!(
                    "{} on `{nome}` refused: the repository's authority path could not be resolved",
                    verbo.simbolo()
                ),
            );
            return;
        };
        if !concessoes
            .values()
            .any(|g| g.cobre(autoridade, verbo, nome))
        {
            d.recusa(
                Camada::Borda,
                fonte,
                format!(
                    "{} on `{nome}` refused: no grant in {fonte_concessoes} covers {} for verb `{}` on rule `{nome}`",
                    verbo.simbolo(),
                    autoridade.display(),
                    verbo.simbolo()
                ),
            );
            return;
        }
    }
    let origem = Origem {
        camada: Some(Camada::Borda),
        fonte: fonte.to_owned(),
    };
    match c {
        Clausula::Estende { piso, .. } => d.eleva(i, *piso, Camada::Borda, fonte),
        Clausula::Relaxa { piso, .. } => {
            let tecto = d.regras[i].tecto();
            d.regras[i] = d.regras[i].clone().com_piso((*piso).min(tecto));
            d.origens.insert(nome.to_owned(), origem);
        }
        Clausula::IsentaCaminhos { caminhos, .. } => {
            let mut isentos = d.regras[i].isencoes().to_vec();
            isentos.extend(caminhos.iter().cloned());
            d.regras[i] = d.regras[i].clone().com_isencoes(isentos);
            d.origens.insert(nome.to_owned(), origem);
        }
        Clausula::Subtrai { .. } => {
            d.regras.remove(i);
            d.origens.remove(nome);
            d.subtraidas.insert(nome.to_owned(), origem);
        }
    }
}

pub const CHAVE_SISTEMA: &str = "JOEIRA_SISTEMA";
pub const CHAVE_USUARIO: &str = "JOEIRA_USUARIO";

#[must_use]
pub fn caminho_sistema() -> PathBuf {
    std::env::var_os(CHAVE_SISTEMA)
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from("/etc/joeira/sistema.json"), PathBuf::from)
}

#[must_use]
pub fn caminho_usuario() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os(CHAVE_USUARIO).filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(v));
    }
    okiba::Okiba::for_app("joeira")
        .try_path(okiba::Tier::Config, "usuario.json")
        .ok()
        .map(okiba::AbsPath::into_path_buf)
}

#[must_use]
pub fn le_camada(caminho: &Path) -> Lida<ArquivoDeCamada> {
    let fonte = caminho.display().to_string();
    match std::fs::read_to_string(caminho) {
        Ok(t) => Some((fonte, le_arquivo_de_camada(&t))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => Some((fonte, Err(e.to_string()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AmbienteMock, ClasseFalsoPositivo, Ponto, Predicado, Reversibilidade};

    #[test]
    fn the_system_file_as_the_flake_renders_it_reads() {
        let a = le_arquivo_de_camada(
            r#"{"nivel":"sistema","pisos":{"msg-placeholder-subject":"avisa","vcs-conflict-markers":"bloqueia"}}"#,
        )
        .expect("reads");
        assert_eq!(a.nivel, Nivel::Sistema);
        assert_eq!(a.pisos["vcs-conflict-markers"], Severidade::Bloqueia);
        assert_eq!(a.pisos["msg-placeholder-subject"], Severidade::Avisa);
    }

    #[test]
    fn the_user_file_as_the_flake_renders_it_reads() {
        let a = le_arquivo_de_camada(r#"{"nivel":"usuario","pisos":{}}"#).expect("reads");
        assert_eq!(a.nivel, Nivel::Usuario);
        assert!(a.pisos.is_empty());
    }

    #[test]
    fn grants_as_the_flake_renders_them_read() {
        let a = le_arquivo_de_camada(
            r#"{"concessoes":{"tend":{"prefixo":"/code/tend","regras":["msg-placeholder-subject"],"verbos":["exempt-paths","subtract"]}},"nivel":"sistema","pisos":{}}"#,
        )
        .expect("reads");
        let g = &a.concessoes["tend"];
        assert!(g.verbos.contains(&Verbo::Subtract));
        assert!(g.cobre(
            Path::new("/code/tend"),
            Verbo::ExemptPaths,
            "msg-placeholder-subject"
        ));
    }

    #[test]
    fn the_english_names_the_trio_once_rendered_still_read() {
        let a = le_arquivo_de_camada(
            r#"{"nivel":"usuario","pisos":{"a":"advise","b":"warn-with-ratchet","c":"warn","d":"block"}}"#,
        )
        .expect("reads");
        let quer = [
            Severidade::Consultivo,
            Severidade::AvisaComCatraca,
            Severidade::Avisa,
            Severidade::Bloqueia,
        ];
        assert_eq!(a.pisos.values().copied().collect::<Vec<_>>(), quer);
    }

    #[test]
    fn every_symbol_round_trips_through_serde() {
        for s in Severidade::todos() {
            let j = serde_json::to_string(s).expect("serialises");
            assert_eq!(j, format!("\"{}\"", s.simbolo()));
        }
        for v in Verbo::todos() {
            let j = serde_json::to_string(v).expect("serialises");
            assert_eq!(j, format!("\"{}\"", v.simbolo()));
        }
    }

    #[test]
    fn an_unknown_severity_is_refused() {
        assert!(le_arquivo_de_camada(r#"{"nivel":"usuario","pisos":{"a":"loud"}}"#).is_err());
    }

    fn regra(nome: &str, rev: Reversibilidade, fp: ClasseFalsoPositivo) -> Regra {
        Regra::nova(
            nome,
            Ponto::PreCommit,
            Predicado::CaminhoEmStageSob {
                prefixo: "x".into(),
            },
            rev,
            fp,
            "m",
            AmbienteMock::default(),
            AmbienteMock::default(),
        )
    }

    fn base() -> Vec<Regra> {
        vec![
            regra(
                "sec-plaintext-credential",
                Reversibilidade::Irreversivel,
                ClasseFalsoPositivo::Prosa,
            ),
            regra(
                "vcs-conflict-markers",
                Reversibilidade::Custoso,
                ClasseFalsoPositivo::ZeroEstrutural,
            ),
            regra(
                "msg-placeholder-subject",
                Reversibilidade::Custoso,
                ClasseFalsoPositivo::TokenExato,
            ),
        ]
    }

    fn arquivo(nivel: Nivel, pisos: &[(&str, Severidade)]) -> ArquivoDeCamada {
        ArquivoDeCamada {
            nivel,
            pisos: pisos.iter().map(|(n, s)| ((*n).to_owned(), *s)).collect(),
            concessoes: BTreeMap::new(),
            orcamentos: BTreeMap::new(),
        }
    }

    fn piso(r: &Resolucao, nome: &str) -> Severidade {
        r.regras
            .iter()
            .find(|x| x.nome() == nome)
            .expect("present")
            .severidade()
    }

    fn concessao(prefixo: &str, verbos: &[Verbo], regras: &[&str]) -> Concessao {
        Concessao {
            prefixo: prefixo.into(),
            verbos: verbos.iter().copied().collect(),
            regras: regras.iter().map(|r| (*r).to_owned()).collect(),
        }
    }

    fn com_concessao(g: Concessao) -> Lida<ArquivoDeCamada> {
        let mut a = arquivo(Nivel::Sistema, &[]);
        a.concessoes.insert("g".into(), g);
        Some(("/etc/joeira/sistema.json".into(), Ok(a)))
    }

    fn borda_hostil() -> Lida<Borda> {
        Some((
            "/clone/joeira.lisp".into(),
            Ok(Borda {
                clausulas: vec![
                    Clausula::Relaxa {
                        regra: "sec-plaintext-credential".into(),
                        piso: Severidade::Consultivo,
                    },
                    Clausula::Subtrai {
                        regra: "sec-plaintext-credential".into(),
                    },
                    Clausula::Estende {
                        regra: "msg-placeholder-subject".into(),
                        piso: Severidade::Avisa,
                    },
                ],
            }),
        ))
    }

    #[test]
    fn a_floor_only_rises_across_layers() {
        let f = Fontes {
            sistema: Some((
                "/etc/s".into(),
                Ok(arquivo(
                    Nivel::Sistema,
                    &[("vcs-conflict-markers", Severidade::Bloqueia)],
                )),
            )),
            usuario: Some((
                "/u".into(),
                Ok(arquivo(
                    Nivel::Usuario,
                    &[("vcs-conflict-markers", Severidade::Consultivo)],
                )),
            )),
            ..Fontes::default()
        };
        let r = resolve(base(), &f);
        assert_eq!(piso(&r, "vcs-conflict-markers"), Severidade::Bloqueia);
        let o = r.origem("vcs-conflict-markers").expect("provenance");
        assert_eq!(o.camada, Some(Camada::Sistema));
        assert_eq!(o.fonte, "/etc/s");
        assert_eq!(r.origem("msg-placeholder-subject").expect("p").camada, None);
    }

    #[test]
    fn a_floor_above_the_ceiling_clamps_and_says_so() {
        let f = Fontes {
            usuario: Some((
                "/u".into(),
                Ok(arquivo(
                    Nivel::Usuario,
                    &[("sec-plaintext-credential", Severidade::Bloqueia)],
                )),
            )),
            ..Fontes::default()
        };
        let r = resolve(base(), &f);
        assert_eq!(
            piso(&r, "sec-plaintext-credential"),
            Severidade::AvisaComCatraca
        );
        assert!(r.recusas.iter().any(|x| x.porque.contains("clamped")));
    }

    #[test]
    fn an_unknown_rule_is_refused_and_its_siblings_apply() {
        let f = Fontes {
            sistema: Some((
                "/etc/s".into(),
                Ok(arquivo(
                    Nivel::Sistema,
                    &[
                        ("no-such-rule", Severidade::Bloqueia),
                        ("msg-placeholder-subject", Severidade::Avisa),
                    ],
                )),
            )),
            ..Fontes::default()
        };
        let r = resolve(base(), &f);
        assert!(r.recusas.iter().any(|x| x.porque.contains("no-such-rule")));
        assert_eq!(piso(&r, "msg-placeholder-subject"), Severidade::Avisa);
    }

    #[test]
    fn a_hostile_clone_outside_every_grant_is_refused_and_its_extend_honored() {
        let f = Fontes {
            sistema: com_concessao(concessao(
                "/code/tend",
                &[Verbo::Relax, Verbo::Subtract],
                &["sec-plaintext-credential"],
            )),
            borda: borda_hostil(),
            autoridade: Some("/tmp/evil".into()),
            ..Fontes::default()
        };
        let r = resolve(base(), &f);
        assert!(
            r.regras
                .iter()
                .any(|x| x.nome() == "sec-plaintext-credential")
        );
        let recusadas: Vec<_> = r
            .recusas
            .iter()
            .filter(|x| x.porque.contains("no grant"))
            .collect();
        assert_eq!(recusadas.len(), 2, "{recusadas:?}");
        assert!(recusadas[0].porque.contains("verb `relax`"));
        assert!(recusadas[0].porque.contains("/tmp/evil"));
        assert_eq!(piso(&r, "msg-placeholder-subject"), Severidade::Avisa);
        assert_eq!(
            r.origem("msg-placeholder-subject").expect("p").camada,
            Some(Camada::Borda)
        );
    }

    #[test]
    fn the_same_edge_inside_a_grant_is_honored() {
        let f = Fontes {
            sistema: com_concessao(concessao(
                "/code/tend",
                &[Verbo::Relax, Verbo::Subtract],
                &["sec-plaintext-credential"],
            )),
            borda: borda_hostil(),
            autoridade: Some("/code/tend".into()),
            ..Fontes::default()
        };
        let r = resolve(base(), &f);
        assert!(
            r.regras
                .iter()
                .all(|x| x.nome() != "sec-plaintext-credential")
        );
        assert!(r.subtraidas.contains_key("sec-plaintext-credential"));
        assert!(r.recusas.is_empty(), "{:?}", r.recusas);
    }

    #[test]
    fn a_grant_is_a_path_prefix_not_a_string_prefix() {
        let g = concessao("/code/tend", &[Verbo::Relax], &["r"]);
        assert!(g.cobre(Path::new("/code/tend/sub"), Verbo::Relax, "r"));
        assert!(!g.cobre(Path::new("/code/tend-evil"), Verbo::Relax, "r"));
        assert!(!g.cobre(Path::new("/code/tend"), Verbo::Subtract, "r"));
        assert!(!g.cobre(Path::new("/code/tend"), Verbo::Relax, "other"));
    }

    #[test]
    fn grants_in_the_user_layer_are_ignored_loudly() {
        let mut u = arquivo(Nivel::Usuario, &[]);
        u.concessoes.insert(
            "x".into(),
            concessao("/tmp", &[Verbo::Subtract], &["sec-plaintext-credential"]),
        );
        let f = Fontes {
            usuario: Some(("/u".into(), Ok(u))),
            borda: borda_hostil(),
            autoridade: Some("/tmp/evil".into()),
            ..Fontes::default()
        };
        let r = resolve(base(), &f);
        assert!(
            r.regras
                .iter()
                .any(|x| x.nome() == "sec-plaintext-credential")
        );
        assert!(
            r.recusas
                .iter()
                .any(|x| x.porque.contains("system layer only"))
        );
    }

    #[test]
    fn a_malformed_layer_is_dropped_with_a_reason_and_the_rest_apply() {
        let f = Fontes {
            sistema: Some(("/etc/s".into(), Err("expected value".into()))),
            usuario: Some((
                "/u".into(),
                Ok(arquivo(
                    Nivel::Usuario,
                    &[("msg-placeholder-subject", Severidade::Avisa)],
                )),
            )),
            borda: Some(("/r/joeira.lisp".into(), Err("unknown verb `nuke`".into()))),
            ..Fontes::default()
        };
        let r = resolve(base(), &f);
        assert_eq!(r.regras.len(), 3);
        assert_eq!(piso(&r, "msg-placeholder-subject"), Severidade::Avisa);
        assert_eq!(r.recusas.len(), 2);
        assert!(r.recusas[1].to_string().contains("fleet rules still apply"));
    }

    #[test]
    fn budgets_are_set_per_rule_later_layer_wins_and_bad_ones_are_refused() {
        let mut s = arquivo(Nivel::Sistema, &[]);
        s.orcamentos.insert("vcs-conflict-markers".into(), 9);
        s.orcamentos.insert("nope".into(), 9);
        let mut u = arquivo(Nivel::Usuario, &[]);
        u.orcamentos.insert("vcs-conflict-markers".into(), 4);
        u.orcamentos.insert("msg-placeholder-subject".into(), 0);
        let r = resolve(
            base(),
            &Fontes {
                sistema: Some(("/etc/s".into(), Ok(s))),
                usuario: Some(("/u".into(), Ok(u))),
                ..Fontes::default()
            },
        );
        let o = |n: &str| {
            r.regras
                .iter()
                .find(|x| x.nome() == n)
                .expect("present")
                .orcamento()
        };
        assert_eq!(o("vcs-conflict-markers").as_secs(), 4);
        assert_eq!(o("msg-placeholder-subject").as_secs(), 3);
        assert_eq!(r.recusas.len(), 2, "{:?}", r.recusas);
    }

    #[test]
    fn a_layer_with_the_wrong_nivel_is_dropped() {
        let f = Fontes {
            sistema: Some((
                "/etc/s".into(),
                Ok(arquivo(
                    Nivel::Usuario,
                    &[("vcs-conflict-markers", Severidade::Bloqueia)],
                )),
            )),
            ..Fontes::default()
        };
        let r = resolve(base(), &f);
        assert_eq!(piso(&r, "vcs-conflict-markers"), Severidade::Consultivo);
        assert_eq!(r.recusas.len(), 1);
    }

    #[test]
    fn exempt_paths_inside_a_grant_hide_only_those_paths() {
        let f = Fontes {
            sistema: com_concessao(concessao(
                "/r",
                &[Verbo::ExemptPaths],
                &["vcs-conflict-markers"],
            )),
            borda: Some((
                "/r/joeira.lisp".into(),
                Ok(Borda {
                    clausulas: vec![Clausula::IsentaCaminhos {
                        regra: "vcs-conflict-markers".into(),
                        caminhos: vec!["docs/".into()],
                    }],
                }),
            )),
            autoridade: Some("/r".into()),
            ..Fontes::default()
        };
        let r = resolve(
            vec![Regra::nova(
                "vcs-conflict-markers",
                Ponto::PreCommit,
                Predicado::BlobTemParDeLinhas {
                    abre: "<<<<<<< ".into(),
                    fecha: ">>>>>>> ".into(),
                },
                Reversibilidade::Custoso,
                ClasseFalsoPositivo::ZeroEstrutural,
                "m",
                AmbienteMock::default(),
                AmbienteMock::default(),
            )],
            &f,
        );
        let marcas = "<<<<<<< a\n>>>>>>> b\n";
        let so_docs = AmbienteMock::default().com_stage("docs/merge.md", marcas);
        let fora = AmbienteMock::default().com_stage("src/x.rs", marcas);
        assert_eq!(
            crate::avalia(&r.regras[0], &so_docs),
            crate::Veredito::Limpo
        );
        assert!(matches!(
            crate::avalia(&r.regras[0], &fora),
            crate::Veredito::Achado { .. }
        ));
    }
}
