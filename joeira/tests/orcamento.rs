mod comum;

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use comum::{Repo, texto};

const FOLGA: Duration = Duration::from_millis(1000);

fn ferramenta(r: &Repo, nome: &str, corpo: &str) -> std::path::PathBuf {
    let bin = r.base.join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir");
    let f = bin.join(nome);
    std::fs::write(&f, format!("#!/bin/sh\n{corpo}\n")).expect("write");
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    bin
}

fn triplo(r: &Repo, remoto: &str) -> String {
    let local = r.git(&["rev-parse", "HEAD"]);
    format!("refs/heads/main {local} refs/heads/main {remoto}\n")
}

fn flake_empurrado(r: &Repo) -> String {
    let base = r.commit("flake.nix", "{ outputs = _: { }; }\n", "seed: flake");
    r.commit("lib.nix", "{ }\n", "feat: lib");
    base
}

fn no_gancho(r: &Repo, prazo_ms: u64) -> std::path::PathBuf {
    r.camada(
        "sistema.json",
        &format!(
            r#"{{"nivel":"sistema","colocacoes":{{"nix-flake-check":"gancho"}},"prazos_ms":{{"pre-push":{prazo_ms}}}}}"#
        ),
    )
}

#[test]
fn a_verifier_sleeping_past_the_hook_budget_returns_within_it_and_is_not_checked() {
    let r = Repo::novo("orcamento-lento");
    let base = flake_empurrado(&r);
    let s = no_gancho(&r, 3000);
    let bin = ferramenta(&r, "nix", "exec sleep 30");
    let inicio = Instant::now();
    let o = r.joeira_com(
        &["pre-push", "origin", "x"],
        Some(&s),
        &triplo(&r, &base),
        Some(&bin),
    );
    let gasto = inicio.elapsed();
    let (_, err) = texto(&o);
    assert!(
        gasto < Duration::from_millis(3000) + FOLGA,
        "{gasto:?} {err}"
    );
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        err.contains("NOT CHECKED nix-flake-check: not checked: exceeded budget 3000ms, `nix flake check --no-build` was killed"),
        "{err}"
    );
    assert!(err.contains("1 not checked"), "{err}");
}

#[test]
fn a_verifier_whose_grandchild_holds_stdout_still_returns_and_the_grandchild_dies() {
    let r = Repo::novo("orcamento-neto");
    let base = flake_empurrado(&r);
    let s = no_gancho(&r, 3000);
    let pid = r.base.join("neto.pid");
    let bin = ferramenta(
        &r,
        "nix",
        &format!("sleep 30 &\necho $! > {}\nexit 1", pid.display()),
    );
    let inicio = Instant::now();
    let o = r.joeira_com(
        &["pre-push", "origin", "x"],
        Some(&s),
        &triplo(&r, &base),
        Some(&bin),
    );
    let gasto = inicio.elapsed();
    let (_, err) = texto(&o);
    assert!(
        gasto < Duration::from_millis(3000) + FOLGA,
        "{gasto:?} {err}"
    );
    assert!(
        err.contains("NOT CHECKED nix-flake-check: not checked: exceeded budget 3000ms, `nix flake check --no-build` was killed"),
        "{err}"
    );
    let neto = std::fs::read_to_string(&pid).expect("grandchild pid");
    let vivo = std::process::Command::new("kill")
        .args(["-0", neto.trim()])
        .status()
        .expect("kill -0");
    assert!(!vivo.success(), "grandchild {neto} survived the kill");
}

#[test]
fn an_eval_row_is_deferred_by_default_and_runs_under_joeira_deferred() {
    let r = Repo::novo("orcamento-adiado");
    let base = flake_empurrado(&r);
    let bin = ferramenta(&r, "nix", "exec sleep 30");
    let inicio = Instant::now();
    let o = r.joeira_com(
        &["pre-push", "origin", "x"],
        None,
        &triplo(&r, &base),
        Some(&bin),
    );
    let (_, err) = texto(&o);
    assert!(inicio.elapsed() < Duration::from_secs(2), "{err}");
    assert!(!err.contains("nix-flake-check"), "{err}");
    assert!(err.contains(", 4 deferred, 0 off; "), "{err}");

    let s = r.camada(
        "sistema.json",
        r#"{"nivel":"sistema","orcamentos":{"nix-flake-check":1}}"#,
    );
    let o = r.joeira_com(&["deferred", "pre-push"], Some(&s), "", Some(&bin));
    let (_, err) = texto(&o);
    assert!(
        err.contains("NOT CHECKED nix-flake-check: not checked: exceeded budget 1000ms"),
        "{err}"
    );
    let (out, _) = texto(&r.joeira(&["explain", "--staged"], None));
    assert!(
        out.contains("placement adiada: not run in the hook, run by `joeira deferred pre-push` with a 600000ms budget"),
        "{out}"
    );
}

#[test]
fn an_off_rule_is_counted_off_and_never_runs() {
    let r = Repo::novo("orcamento-desligado");
    let base = flake_empurrado(&r);
    let s = r.camada(
        "sistema.json",
        r#"{"nivel":"sistema","colocacoes":{"nix-flake-untracked":"off"}}"#,
    );
    let o = r.joeira_com(
        &["pre-push", "origin", "x"],
        Some(&s),
        &triplo(&r, &base),
        None,
    );
    let (_, err) = texto(&o);
    assert!(err.contains(", 4 deferred, 1 off; "), "{err}");
}

#[test]
fn the_nix_eval_row_does_not_run_when_the_push_touches_no_nix() {
    let r = Repo::novo("orcamento-toca");
    let base = r.commit("flake.nix", "{ outputs = _: { }; }\n", "seed: flake");
    r.commit("README.md", "x\n", "docs: readme");
    let s = no_gancho(&r, 800);
    let bin = ferramenta(&r, "nix", "exec sleep 30");
    let o = r.joeira_com(
        &["pre-push", "origin", "x"],
        Some(&s),
        &triplo(&r, &base),
        Some(&bin),
    );
    let (_, err) = texto(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(!err.contains("nix-flake-check"), "{err}");
}

#[test]
fn pre_push_on_a_repo_of_every_class_with_every_tool_hung_finishes_under_its_budget() {
    let r = Repo::novo("orcamento-bancada");
    let base = r.commit("flake.nix", "{ outputs = _: { }; }\n", "seed: flake");
    r.escreve("Cargo.toml", "[package]\nname = \"x\"\n");
    r.escreve("Cargo.lock", "version = 4\n");
    r.escreve("go.mod", "module x\n");
    r.escreve("flake.lock", "{\"nodes\":{\"root\":{}}}\n");
    r.git(&["add", "Cargo.toml", "Cargo.lock", "go.mod", "flake.lock"]);
    r.git(&["commit", "-q", "--no-verify", "-m", "feat: every class"]);
    let s = r.camada(
        "sistema.json",
        r#"{"nivel":"sistema","colocacoes":{"nix-flake-check":"gancho","rust-cargo-fetch-locked":"gancho","go-mod-tidy":"gancho","go-vet":"gancho"}}"#,
    );
    let mut bin = std::path::PathBuf::new();
    for t in ["nix", "cargo", "go"] {
        bin = ferramenta(&r, t, "sleep 30 &\nexec sleep 30");
    }
    let inicio = Instant::now();
    let o = r.joeira_com(
        &["pre-push", "origin", "x"],
        Some(&s),
        &triplo(&r, &base),
        Some(&bin),
    );
    let gasto = inicio.elapsed();
    let (_, err) = texto(&o);
    assert!(
        gasto < joeira_core::PRAZO_PRE_PUSH + FOLGA,
        "joeira pre-push took {gasto:?} against a {:?} budget: {err}",
        joeira_core::PRAZO_PRE_PUSH
    );
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(err.contains("of a 5000ms budget"), "{err}");
}
