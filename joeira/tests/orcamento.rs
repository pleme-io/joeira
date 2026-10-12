mod comum;

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use comum::{Repo, texto};

fn nix_lento(r: &Repo) -> std::path::PathBuf {
    let bin = r.base.join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir");
    let nix = bin.join("nix");
    std::fs::write(&nix, "#!/bin/sh\nexec sleep 30\n").expect("write");
    std::fs::set_permissions(&nix, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    bin
}

fn triplo(r: &Repo, remoto: &str) -> String {
    let local = r.git(&["rev-parse", "HEAD"]);
    format!("refs/heads/main {local} refs/heads/main {remoto}\n")
}

#[test]
fn a_slow_nix_eval_row_is_killed_at_its_budget_and_reported_not_checked() {
    let r = Repo::novo("orcamento");
    let base = r.commit("flake.nix", "{ outputs = _: { }; }\n", "seed: flake");
    r.commit("lib.nix", "{ }\n", "feat: lib");
    let s = r.camada(
        "sistema.json",
        r#"{"nivel":"sistema","orcamentos":{"nix-flake-check":1}}"#,
    );
    let bin = nix_lento(&r);
    let inicio = Instant::now();
    let o = r.joeira_com(
        &["pre-push", "origin", "x"],
        Some(&s),
        &triplo(&r, &base),
        Some(&bin),
    );
    let (_, err) = texto(&o);
    assert!(inicio.elapsed() < Duration::from_secs(15), "{err}");
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        err.contains("NOT CHECKED nix-flake-check: `nix flake check --no-build` exceeded its 1s budget and was killed"),
        "{err}"
    );
}

#[test]
fn the_nix_eval_row_does_not_run_when_the_push_touches_no_nix() {
    let r = Repo::novo("orcamento-toca");
    let base = r.commit("flake.nix", "{ outputs = _: { }; }\n", "seed: flake");
    r.commit("README.md", "x\n", "docs: readme");
    let bin = nix_lento(&r);
    let inicio = Instant::now();
    let o = r.joeira_com(
        &["pre-push", "origin", "x"],
        None,
        &triplo(&r, &base),
        Some(&bin),
    );
    let (_, err) = texto(&o);
    assert!(inicio.elapsed() < Duration::from_secs(10), "{err}");
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(!err.contains("nix-flake-check"), "{err}");
    let (out, _) = texto(&r.joeira(&["explain", "--staged"], None));
    assert!(
        out.contains("budget 20s; runs only when the push touches .nix or flake.lock"),
        "{out}"
    );
}
