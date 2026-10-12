//! SYNTHETIC-FIXTURE

mod comum;

use comum::{Repo, texto};

const MARCAS: &str = "<<<<<<< ours\nx\n>>>>>>> theirs\n";

fn faixa(r: &Repo) -> (String, String) {
    let base = r.commit("README.md", "x\n", "seed: readme");
    (base, String::new())
}

#[test]
fn a_conflict_marker_commit_in_the_range_exits_one() {
    let r = Repo::novo("ci-bloqueia");
    let (base, _) = faixa(&r);
    r.commit("ok.txt", "fine\n", "docs: fine");
    r.commit("src/a.txt", MARCAS, "feat: oops");
    let o = r.joeira(&["ci", "--range", &format!("{base}..HEAD")], None);
    let (_, err) = texto(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("BLOCK vcs-conflict-markers [bloqueia]"),
        "{err}"
    );
    assert!(err.contains("2 commits"), "{err}");
}

#[test]
fn findings_below_bloqueia_exit_zero_even_when_a_layer_asks_for_more() {
    let r = Repo::novo("ci-avisa");
    let (base, _) = faixa(&r);
    r.commit("notes.txt", "password: correcthorsebattery\n", "init");
    let s = r.camada(
        "sistema.json",
        r#"{"nivel":"sistema","pisos":{"msg-placeholder-subject":"bloqueia","sec-plaintext-credential":"bloqueia"}}"#,
    );
    let o = r.joeira(&["ci", "--range", &format!("{base}..HEAD")], Some(&s));
    let (_, err) = texto(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        err.contains("WARN msg-placeholder-subject [avisa]"),
        "{err}"
    );
    assert!(
        err.contains("WARN sec-plaintext-credential [avisa-com-catraca]"),
        "{err}"
    );
    assert!(err.contains("clamped to its ceiling"), "{err}");
}

#[test]
fn a_granted_edge_relaxing_the_marker_gate_exits_zero() {
    let r = Repo::novo("ci-relaxa");
    let (base, _) = faixa(&r);
    r.commit("src/a.txt", MARCAS, "feat: markers on purpose");
    r.escreve(
        "joeira.lisp",
        "(defjoeira-borda (relax \"vcs-conflict-markers\" avisa))\n",
    );
    let s = r.camada(
        "sistema.json",
        &format!(
            r#"{{"nivel":"sistema","concessoes":{{"t":{{"prefixo":"{}","verbos":["relax"],"regras":["vcs-conflict-markers"]}}}}}}"#,
            r.base.canonicalize().expect("canon").display()
        ),
    );
    let o = r.joeira(&["ci", "--range", &format!("{base}..HEAD")], Some(&s));
    let (_, err) = texto(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(err.contains("WARN vcs-conflict-markers [avisa]"), "{err}");
}

#[test]
fn an_unreadable_range_exits_two_with_a_reason() {
    let r = Repo::novo("ci-cego");
    faixa(&r);
    for range in ["nope..HEAD", "HEAD"] {
        let o = r.joeira(&["ci", "--range", range], None);
        let (_, err) = texto(&o);
        assert_eq!(o.status.code(), Some(2), "{range}: {err}");
        assert!(err.contains("cannot judge range"), "{err}");
    }
}
