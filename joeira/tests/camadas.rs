mod comum;

use comum::{Repo, texto};

const MARCAS: &str = "<<<<<<< ours\nx\n>>>>>>> theirs\n";

#[test]
fn a_malformed_edge_prints_on_every_run_exits_zero_and_fleet_rules_still_apply() {
    let r = Repo::novo("borda-ruim");
    r.commit("README.md", "x\n", "seed: readme");
    r.escreve(
        "joeira.lisp",
        "(defjoeira-borda (nuke \"vcs-conflict-markers\"))\n",
    );
    r.escreve("src/a.txt", MARCAS);
    r.git(&["add", "src/a.txt"]);
    for _ in 0..2 {
        let o = r.joeira(&["pre-commit"], None);
        let (_, err) = texto(&o);
        assert_eq!(o.status.code(), Some(0), "{err}");
        assert!(err.contains("REFUSED layer borda"), "{err}");
        assert!(err.contains("unknown verb `nuke`"), "{err}");
        assert!(err.contains("fleet rules still apply"), "{err}");
        assert!(err.contains("WARN vcs-conflict-markers"), "{err}");
    }
}

#[test]
fn explain_shows_the_layer_and_source_that_set_each_floor() {
    let r = Repo::novo("explain");
    r.commit("README.md", "x\n", "seed: readme");
    let s = r.camada(
        "sistema.json",
        r#"{"nivel":"sistema","pisos":{"sec-plaintext-credential":"avisa-com-catraca","no-such-rule":"avisa"}}"#,
    );
    let o = r.joeira(&["explain", "--staged"], Some(&s));
    let (out, err) = texto(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    let linha = out
        .split("  ")
        .find(|l| l.contains("sec-plaintext-credential @pre-commit"))
        .map(str::to_owned);
    assert!(linha.is_some(), "{out}");
    assert!(
        out.contains(&format!(
            "effective floor avisa-com-catraca set by layer sistema ({})",
            s.display()
        )),
        "{out}"
    );
    assert!(
        out.contains("set by catalog (joeira-lisp/catalogo/joeira.tlisp)"),
        "{out}"
    );
    assert!(out.contains("unknown rule `no-such-rule` refused"), "{out}");
    assert!(!out.contains("acts at Avisa (P1)"), "{out}");
}

#[test]
fn a_granted_edge_subtracts_and_an_ungranted_one_is_refused_by_name() {
    let r = Repo::novo("concessao");
    r.commit("README.md", "x\n", "seed: readme");
    r.escreve(
        "joeira.lisp",
        "(defjoeira-borda (subtract \"vcs-conflict-markers\") (extend \"shell-dockerfile-new\" avisa))\n",
    );
    let fora = r.camada(
        "fora.json",
        r#"{"nivel":"sistema","concessoes":{"x":{"prefixo":"/nowhere","verbos":["subtract"],"regras":["vcs-conflict-markers"]}}}"#,
    );
    let (out, _) = texto(&r.joeira(&["explain", "--staged"], Some(&fora)));
    assert!(out.contains("no grant in"), "{out}");
    assert!(out.contains("vcs-conflict-markers @pre-commit"), "{out}");
    assert!(
        out.contains("effective floor avisa set by layer borda"),
        "{out}"
    );

    let dentro = r.camada(
        "dentro.json",
        &format!(
            r#"{{"nivel":"sistema","concessoes":{{"x":{{"prefixo":"{}","verbos":["subtract"],"regras":["vcs-conflict-markers"]}}}}}}"#,
            r.base.canonicalize().expect("canon").display()
        ),
    );
    let (out, _) = texto(&r.joeira(&["explain", "--staged"], Some(&dentro)));
    assert!(out.contains("subtracted by layer borda"), "{out}");
    assert!(!out.contains("REFUSED"), "{out}");
}

#[test]
fn a_too_large_or_binary_file_is_named_not_checked() {
    let r = Repo::novo("limite");
    r.commit("README.md", "x\n", "seed: readme");
    r.escreve("big.txt", &"y".repeat(2 * 1024 * 1024));
    std::fs::write(r.dir.join("blob.bin"), b"a\0b\n").expect("w");
    r.git(&["add", "big.txt", "blob.bin"]);
    let o = r.joeira(&["pre-commit"], None);
    let (_, err) = texto(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(err.contains("not checked: big.txt too large"), "{err}");
    assert!(err.contains("not checked: blob.bin binary"), "{err}");
}
