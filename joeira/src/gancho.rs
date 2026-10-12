use std::io::Read;
use std::path::{Path, PathBuf};

use joeira_core::camadas::{
    Borda, Fontes, Lida, Resolucao, caminho_sistema, caminho_usuario, le_camada, resolve,
};
use joeira_core::journal::{Diario, Registro, TagVeredito};
use joeira_core::shell::{AmbienteShell, autoridade_de, raiz_de};
use joeira_core::{
    AmbienteGit, Nivel, Ponto, Regra, Severidade, Veredito, Verificador, avalia, le_envios,
    necessidades,
};

pub const CHAVE_DESLIGA: &str = "JOEIRA_OFF";
pub const CHAVE_SEM_BORDA: &str = "JOEIRA_SEM_BORDA";
pub const ARQUIVOS_DE_BORDA: &[&str] = &["joeira.lisp", "caixa.lisp"];

pub fn le_borda(raiz: &Path) -> Lida<Borda> {
    for nome in ARQUIVOS_DE_BORDA {
        let caminho = raiz.join(nome);
        let fonte = caminho.display().to_string();
        let texto = match std::fs::read_to_string(&caminho) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Some((fonte, Err(e.to_string()))),
        };
        match joeira_lisp::ler_borda(&texto) {
            Ok(Some(b)) => return Some((fonte, Ok(b))),
            Ok(None) => {}
            Err(e) => return Some((fonte, Err(e.to_string()))),
        }
    }
    None
}

pub fn em_camadas(regras: &[Regra], raiz: &Path) -> Resolucao {
    let sem_borda = std::env::var(CHAVE_SEM_BORDA).is_ok_and(|v| v == "1");
    let fontes = Fontes {
        sistema: le_camada(&caminho_sistema()),
        usuario: caminho_usuario().and_then(|p| le_camada(&p)),
        borda: if sem_borda { None } else { le_borda(raiz) },
        autoridade: autoridade_de(raiz).ok(),
    };
    if sem_borda {
        eprintln!("joeira: the repository edge layer is ignored ({CHAVE_SEM_BORDA}=1)");
    }
    resolve(regras.to_vec(), &fontes)
}

pub fn relata_recusas(onde: &str, r: &Resolucao) {
    for x in &r.recusas {
        eprintln!("joeira {onde}: REFUSED {x}");
    }
}

fn julga_em_camadas(ponto: Ponto, regras: &[Regra], amb: AmbienteShell, repo: &Path) {
    let res = em_camadas(regras, repo);
    relata_recusas(ponto.arquivo(), &res);
    let ativas: Vec<Regra> = res
        .regras
        .iter()
        .filter(|r| r.ponto() == ponto)
        .cloned()
        .collect();
    let amb = amb.com_necessidades(necessidades(&ativas));
    julga(ponto, &ativas, &amb, repo, Diario::padrao().as_ref());
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Balanco {
    pub regras: usize,
    pub achados: usize,
    pub cegos: usize,
    pub fora: usize,
    pub arquivos_nao_verificados: usize,
}

pub fn desligado() -> bool {
    std::env::var(CHAVE_DESLIGA).is_ok_and(|v| v == "1")
}

fn protegido(ponto: Ponto, corpo: impl FnOnce() -> anyhow::Result<()>) {
    if desligado() {
        eprintln!(
            "joeira {}: DISABLED by {CHAVE_DESLIGA}=1 — no rule was evaluated",
            ponto.arquivo()
        );
        return;
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(corpo)) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!(
            "joeira {}: DEGRADED, nothing was judged: {e:#} (exit 0, advisory at P1)",
            ponto.arquivo()
        ),
        Err(_) => eprintln!(
            "joeira {}: DEGRADED, the engine panicked (exit 0, advisory at P1)",
            ponto.arquivo()
        ),
    }
}

fn raiz() -> anyhow::Result<PathBuf> {
    let aqui = std::env::current_dir()?;
    Ok(raiz_de(&aqui)?)
}

pub fn le_mensagem(bruta: &str) -> String {
    let mut linhas = Vec::new();
    for l in bruta.lines() {
        if l.starts_with("# ------------------------ >8 ------------------------") {
            break;
        }
        if !l.starts_with('#') {
            linhas.push(l);
        }
    }
    let mut m = linhas.join("\n");
    m.truncate(m.trim_end().len());
    m
}

pub fn julga<E: AmbienteGit + ?Sized>(
    ponto: Ponto,
    regras: &[Regra],
    amb: &E,
    repo: &Path,
    diario: Option<&Diario>,
) -> Balanco {
    let mut b = Balanco::default();
    for r in regras.iter().filter(|r| r.ponto() == ponto) {
        b.regras += 1;
        let v = avalia(r, amb);
        match &v {
            Veredito::Achado {
                regra, mensagem, ..
            } => {
                b.achados += 1;
                eprintln!(
                    "joeira {}: WARN {regra} [{}]: {mensagem}",
                    ponto.arquivo(),
                    r.nivel().simbolo()
                );
            }
            Veredito::Cego { regra, porque } => {
                b.cegos += 1;
                eprintln!("joeira {}: NOT CHECKED {regra}: {porque}", ponto.arquivo());
            }
            Veredito::NaoSeAplica { .. } => b.fora += 1,
            Veredito::Limpo => {}
        }
        if let Some(d) = diario {
            let reg = Registro {
                regra: r.nome().to_owned(),
                veredito: TagVeredito::from(&v),
                repo: repo.display().to_string(),
                quando: joeira_core::journal::agora(),
                severidade: matches!(v, Veredito::Achado { .. }).then_some(Severidade::Avisa),
            };
            if let Err(e) = d.registrar(&reg) {
                eprintln!("joeira {}: journal not written: {e}", ponto.arquivo());
            }
        }
    }
    for (caminho, motivo) in amb.nao_verificados() {
        b.arquivos_nao_verificados += 1;
        eprintln!(
            "joeira {}: not checked: {caminho} {motivo}",
            ponto.arquivo()
        );
    }
    eprintln!(
        "joeira {}: {} rules, {} findings, {} not checked, {} not applicable — advisory (P1), exit 0",
        ponto.arquivo(),
        b.regras,
        b.achados,
        b.cegos,
        b.fora
    );
    b
}

pub fn pre_commit(regras: &[Regra]) {
    protegido(Ponto::PreCommit, || {
        let repo = raiz()?;
        let amb = AmbienteShell::novo(&repo);
        julga_em_camadas(Ponto::PreCommit, regras, amb, &repo);
        Ok(())
    });
}

pub fn commit_msg(regras: &[Regra], arquivo: &Path) {
    protegido(Ponto::CommitMsg, || {
        let bruta = std::fs::read(arquivo)
            .map_err(|e| anyhow::anyhow!("cannot read message file {}: {e}", arquivo.display()))?;
        let msg = le_mensagem(&String::from_utf8_lossy(&bruta));
        if msg.is_empty() {
            eprintln!(
                "joeira commit-msg: the message in {} is EMPTY — judging it as empty",
                arquivo.display()
            );
        }
        let repo = raiz()?;
        let amb = AmbienteShell::novo(&repo).com_mensagem(&msg);
        julga_em_camadas(Ponto::CommitMsg, regras, amb, &repo);
        Ok(())
    });
}

pub fn pre_push(regras: &[Regra], remoto: &str, url: &str) {
    protegido(Ponto::PrePush, || {
        let mut entrada = String::new();
        std::io::stdin()
            .read_to_string(&mut entrada)
            .map_err(|e| anyhow::anyhow!("cannot read pre-push stdin: {e}"))?;
        let envios = le_envios(&entrada)?;
        let vivos: Vec<_> = envios.into_iter().filter(|e| !e.eh_remocao()).collect();
        if vivos.is_empty() {
            eprintln!(
                "joeira pre-push: no ref-triples to judge on stdin for {remoto} ({url}) — nothing was checked"
            );
            return Ok(());
        }
        let repo = raiz()?;
        let amb = AmbienteShell::novo(&repo).com_envios(vivos);
        julga_em_camadas(Ponto::PrePush, regras, amb, &repo);
        Ok(())
    });
}

pub fn explain(regras: &[Regra], staged: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        staged,
        "explain needs --staged; it judges the index as pre-commit would"
    );
    let repo = raiz()?;
    let amb = AmbienteShell::novo(&repo);
    let res = em_camadas(regras, &repo);
    println!("joeira explain --staged in {}", repo.display());
    println!(
        "source: joeira-lisp/catalogo/joeira.tlisp (compiled in), joeira {}",
        env!("CARGO_PKG_VERSION")
    );
    for x in &res.recusas {
        println!("  REFUSED {x}");
    }
    for (nome, o) in &res.subtraidas {
        println!("  -      {nome}\n         subtracted by {}", o.rotulo());
    }
    for r in &res.regras {
        let origem = res.origem(r.nome()).map_or_else(
            || "unknown".to_owned(),
            joeira_core::camadas::Origem::rotulo,
        );
        let isentos = if r.isencoes().is_empty() {
            String::new()
        } else {
            format!("; exempt under {}", r.isencoes().join(", "))
        };
        let cabeca = format!(
            "{} @{} layer={} trigger={}",
            r.nome(),
            r.ponto().arquivo(),
            r.nivel().simbolo(),
            r.gatilho().unwrap_or("-")
        );
        let porque = format!(
            "axes {} x {} => ceiling {}; effective floor {} set by {origem}{isentos}; P1 advisory: acts at most avisa, exit 0",
            r.reversibilidade().simbolo(),
            r.fp().simbolo(),
            r.tecto().simbolo(),
            r.severidade().simbolo()
        );
        if r.ponto() != Ponto::PreCommit {
            println!(
                "  -      {cabeca}\n         not evaluated: mounts at {}\n         {porque}",
                r.ponto().arquivo()
            );
            continue;
        }
        let (marca, detalhe) = match avalia(r, &amb) {
            Veredito::Achado { mensagem, .. } => ("WARN ", mensagem),
            Veredito::Limpo => ("clean", "predicate does not hold on the index".to_owned()),
            Veredito::NaoSeAplica { porque, .. } => ("n/a  ", porque),
            Veredito::Cego { porque, .. } => ("BLIND", porque),
        };
        println!("  {marca}  {cabeca}\n         {detalhe}\n         {porque}");
    }
    for (caminho, motivo) in amb.nao_verificados() {
        println!("  not checked: {caminho} {motivo}");
    }
    Ok(())
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BalancoCi {
    pub commits: usize,
    pub achados: usize,
    pub bloqueios: usize,
    pub cegos: usize,
}

pub fn julga_commit<E: AmbienteGit + ?Sized>(
    id: &str,
    regras: &[Regra],
    amb: &E,
    b: &mut BalancoCi,
) {
    b.commits += 1;
    for r in regras
        .iter()
        .filter(|r| matches!(r.ponto(), Ponto::PreCommit | Ponto::CommitMsg))
    {
        match avalia(r, amb) {
            Veredito::Achado {
                regra,
                severidade,
                mensagem,
            } => {
                b.achados += 1;
                let bloqueia = severidade.gateia();
                if bloqueia {
                    b.bloqueios += 1;
                }
                eprintln!(
                    "joeira ci {id}: {} {regra} [{}]: {mensagem}",
                    if bloqueia { "BLOCK" } else { "WARN" },
                    severidade.simbolo()
                );
            }
            Veredito::Cego { regra, porque } => {
                b.cegos += 1;
                eprintln!("joeira ci {id}: NOT CHECKED {regra}: {porque}");
            }
            Veredito::Limpo | Veredito::NaoSeAplica { .. } => {}
        }
    }
    for (caminho, motivo) in amb.nao_verificados() {
        eprintln!("joeira ci {id}: not checked: {caminho} {motivo}");
    }
}

pub fn ci(regras: &[Regra], faixa: &str) -> i32 {
    match ci_em(regras, faixa) {
        Ok(b) => {
            eprintln!(
                "joeira ci {faixa}: {} commits, {} findings, {} blocking, {} not checked — exit {}",
                b.commits,
                b.achados,
                b.bloqueios,
                b.cegos,
                i32::from(b.bloqueios > 0)
            );
            i32::from(b.bloqueios > 0)
        }
        Err(e) => {
            eprintln!("joeira ci: cannot judge range {faixa}: {e:#} (exit 2)");
            2
        }
    }
}

fn ci_em(regras: &[Regra], faixa: &str) -> anyhow::Result<BalancoCi> {
    anyhow::ensure!(
        faixa.contains("..") && !faixa.contains("..."),
        "expected <base>..<head>"
    );
    let repo = raiz()?;
    let out = std::process::Command::new("git")
        .args(["rev-list", "--reverse", faixa, "--"])
        .current_dir(&repo)
        .output()?;
    anyhow::ensure!(
        out.status.success(),
        "`git rev-list {faixa}` exited {}: {}",
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    let shas: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect();
    let res = em_camadas(regras, &repo);
    relata_recusas("ci", &res);
    let ativas: Vec<Regra> = res
        .regras
        .into_iter()
        .filter(|r| matches!(r.ponto(), Ponto::PreCommit | Ponto::CommitMsg))
        .collect();
    let n = necessidades(&ativas);
    let mut b = BalancoCi::default();
    for sha in &shas {
        let amb = AmbienteShell::novo(&repo)
            .no_commit(sha)?
            .com_necessidades(n.clone());
        julga_commit(&sha[..sha.len().min(12)], &ativas, &amb, &mut b);
    }
    Ok(b)
}

pub fn no_path(programa: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(programa))
        .find(|c| c.is_file())
}

pub fn status(regras: &[Regra]) {
    println!(
        "joeira {} — P1, every rule advises",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "kill switch: {CHAVE_DESLIGA}={}",
        if desligado() {
            "1 (hooks DISABLED)"
        } else {
            "unset (hooks armed)"
        }
    );
    println!("corpus: {} rules", regras.len());
    for p in Ponto::todos() {
        let n = regras.iter().filter(|r| r.ponto() == *p).count();
        println!("  {:<11} {n}", p.arquivo());
    }
    for nv in Nivel::todos() {
        let n = regras.iter().filter(|r| r.nivel() == *nv).count();
        println!("  layer {:<15} {n}", nv.simbolo());
    }
    println!("tools:");
    let mut vistos = Vec::new();
    for v in Verificador::todos() {
        let prog = v.programa();
        if vistos.contains(&prog) {
            continue;
        }
        vistos.push(prog);
        match no_path(prog) {
            Some(c) => println!("  {prog:<11} {}", c.display()),
            None => println!("  {prog:<11} ABSENT — its rows report `not checked`"),
        }
    }
    match Diario::padrao() {
        Some(d) => match d.registros() {
            Ok(r) => println!(
                "journal: {} ({} records)",
                d.caminho().as_path().display(),
                r.len()
            ),
            Err(e) => println!(
                "journal: {} (unreadable: {e})",
                d.caminho().as_path().display()
            ),
        },
        None => println!("journal: no state directory resolved"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use joeira_core::{AmbienteMock, ClasseFalsoPositivo, Predicado, Reversibilidade};

    #[test]
    fn the_message_file_drops_comments_and_the_scissors_tail() {
        let m = le_mensagem(
            "feat: x\n\nbody\n# Please enter\n# ------------------------ >8 ------------------------\ndiff\n",
        );
        assert_eq!(m, "feat: x\n\nbody");
    }

    #[test]
    fn an_empty_message_file_reads_empty() {
        assert_eq!(le_mensagem(""), "");
    }

    fn regra_externa() -> Regra {
        Regra::nova(
            "x",
            Ponto::PrePush,
            Predicado::Externo {
                verificador: Verificador::GoVet,
            },
            Reversibilidade::Recuperavel,
            ClasseFalsoPositivo::Limiar,
            "m",
            AmbienteMock::default(),
            AmbienteMock::default(),
        )
        .com_origem(Nivel::ClasseDeRepo, Some("go.mod".into()))
    }

    #[test]
    fn julga_counts_only_its_point_and_never_gates() {
        let r = regra_externa();
        let amb = AmbienteMock::default()
            .com_arvore("go.mod")
            .com_execucao(Verificador::GoVet, 2);
        let b = julga(
            Ponto::PrePush,
            std::slice::from_ref(&r),
            &amb,
            Path::new("/x"),
            None,
        );
        assert_eq!(
            b,
            Balanco {
                regras: 1,
                achados: 1,
                cegos: 0,
                fora: 0,
                arquivos_nao_verificados: 0
            }
        );
        let b = julga(Ponto::PreCommit, &[r], &amb, Path::new("/x"), None);
        assert_eq!(b.regras, 0);
    }

    fn regra_estrutural() -> Regra {
        Regra::nova(
            "estrutural",
            Ponto::PreCommit,
            Predicado::CaminhoEmStageSob {
                prefixo: "x/".into(),
            },
            Reversibilidade::Custoso,
            ClasseFalsoPositivo::ZeroEstrutural,
            "m",
            AmbienteMock::default(),
            AmbienteMock::default(),
        )
    }

    #[test]
    fn ci_blocks_only_when_a_layer_raised_the_floor_to_bloqueia() {
        let amb = AmbienteMock::default().com_stage("x/a", "a");
        let mut b = BalancoCi::default();
        julga_commit("c", &[regra_estrutural()], &amb, &mut b);
        assert_eq!((b.achados, b.bloqueios), (1, 0));

        let mut a = joeira_core::camadas::ArquivoDeCamada {
            nivel: Nivel::Sistema,
            pisos: Default::default(),
            concessoes: Default::default(),
        };
        a.pisos.insert("estrutural".into(), Severidade::Bloqueia);
        let res = resolve(
            vec![regra_estrutural()],
            &Fontes {
                sistema: Some(("/etc/s".into(), Ok(a))),
                ..Fontes::default()
            },
        );
        let mut b = BalancoCi::default();
        julga_commit("c", &res.regras, &amb, &mut b);
        assert_eq!((b.achados, b.bloqueios), (1, 1));
    }

    #[test]
    fn an_absent_tool_is_counted_not_checked() {
        let amb = AmbienteMock::default().com_arvore("go.mod");
        let b = julga(
            Ponto::PrePush,
            &[regra_externa()],
            &amb,
            Path::new("/x"),
            None,
        );
        assert_eq!(b.cegos, 1);
        assert_eq!(b.achados, 0);
    }

    #[test]
    fn an_absent_trigger_is_not_applicable() {
        let b = julga(
            Ponto::PrePush,
            &[regra_externa()],
            &AmbienteMock::default(),
            Path::new("/x"),
            None,
        );
        assert_eq!(b.fora, 1);
    }
}
