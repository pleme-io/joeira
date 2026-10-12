use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Repo {
    raiz: PathBuf,
    estado: PathBuf,
    bin_git: PathBuf,
}

fn git_absoluto() -> PathBuf {
    let out = Command::new("git")
        .args(["--exec-path"])
        .output()
        .expect("git runs");
    let exec = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    let candidato = exec.join("git");
    assert!(candidato.is_file(), "git at {}", candidato.display());
    candidato
}

impl Repo {
    fn novo(nome: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "joeira-it-{nome}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let raiz = base.join("repo");
        let ganchos = base.join("hooks-empty");
        let estado = base.join("state");
        let bin_git = base.join("bin");
        for d in [&raiz, &ganchos, &estado, &bin_git] {
            std::fs::create_dir_all(d).expect("mkdir");
        }
        std::os::unix::fs::symlink(git_absoluto(), bin_git.join("git")).expect("symlink git");
        let r = Self {
            raiz,
            estado,
            bin_git,
        };
        r.git(&["init", "-q", "-b", "main"]);
        r.git(&["config", "core.hooksPath", ganchos.to_str().expect("utf8")]);
        r.git(&["config", "user.name", "t"]);
        r.git(&["config", "user.email", "t@example.invalid"]);
        r.git(&["config", "commit.gpgsign", "false"]);
        r
    }

    fn git(&self, args: &[&str]) {
        let st = Command::new("git")
            .args(args)
            .current_dir(&self.raiz)
            .status()
            .expect("git");
        assert!(st.success(), "git {args:?}");
    }

    fn escreve(&self, caminho: &str, conteudo: &str) {
        let p = self.raiz.join(caminho);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).expect("mkdir");
        }
        std::fs::write(p, conteudo).expect("write");
    }

    fn joeira(&self, args: &[&str], stdin: Stdio, extra: &[(&str, &str)]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_joeira"));
        c.args(args)
            .current_dir(&self.raiz)
            .stdin(stdin)
            .env("PATH", &self.bin_git)
            .env("XDG_STATE_HOME", &self.estado)
            .env("HOME", &self.estado)
            .env_remove("JOEIRA_OFF");
        for (k, v) in extra {
            c.env(k, v);
        }
        c.output().expect("joeira runs")
    }
}

fn erro(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn dev_null() -> Stdio {
    Stdio::from(std::fs::File::open("/dev/null").expect("/dev/null"))
}

#[test]
fn pre_commit_warns_and_exits_zero_on_a_staged_index() {
    let r = Repo::novo("pc");
    r.escreve("README.md", "x\n");
    r.git(&["add", "."]);
    r.git(&["commit", "-q", "-m", "seed: readme"]);
    r.escreve("svc/Dockerfile", "FROM x\n");
    r.escreve("run.sh", "#!/bin/sh\na\nb\nc\nd\n");
    r.escreve("lock.txt", "<<<<<<< HEAD\nx\n>>>>>>> o\n");
    r.escreve("Cargo.toml", "[package]\nname = \"x\"\n");
    r.git(&["add", "."]);
    let o = r.joeira(&["pre-commit"], dev_null(), &[]);
    let e = erro(&o);
    assert_eq!(o.status.code(), Some(0), "{e}");
    assert!(e.contains("WARN shell-dockerfile-new"), "{e}");
    assert!(e.contains("WARN shell-script-beyond-glue"), "{e}");
    assert!(e.contains("WARN vcs-conflict-markers"), "{e}");
    assert!(
        e.contains("NOT CHECKED rust-cargo-fmt: not checked: cargo absent"),
        "{e}"
    );
    assert!(e.contains("advisory (P1), exit 0"), "{e}");
}

#[test]
fn commit_msg_reads_the_file_and_never_gates() {
    let r = Repo::novo("cm");
    r.escreve("MSG", "init\n# comment\n");
    let o = r.joeira(&["commit-msg", "MSG"], dev_null(), &[]);
    let e = erro(&o);
    assert_eq!(o.status.code(), Some(0), "{e}");
    assert!(e.contains("WARN msg-placeholder-subject"), "{e}");
}

#[test]
fn pre_push_judges_the_worktree_for_the_pushed_refs() {
    let r = Repo::novo("pp");
    r.escreve("flake.nix", "{ outputs = _: {}; }\n");
    r.escreve("README.md", "x\n");
    r.escreve("lib.nix", "{ }\n");
    r.git(&["add", "README.md", "lib.nix"]);
    r.git(&["commit", "-q", "-m", "seed: readme"]);
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&r.raiz)
        .output()
        .expect("git");
    let head = String::from_utf8_lossy(&head.stdout).into_owned();
    let triple = format!(
        "refs/heads/main {} refs/heads/main 0000000000000000000000000000000000000000\n",
        head.trim()
    );
    let mut c = Command::new(env!("CARGO_BIN_EXE_joeira"));
    c.args(["pre-push", "origin", "git@example.invalid:x.git"])
        .current_dir(&r.raiz)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PATH", &r.bin_git)
        .env("XDG_STATE_HOME", &r.estado)
        .env("HOME", &r.estado)
        .env_remove("JOEIRA_OFF");
    let mut filho = c.spawn().expect("spawn");
    std::io::Write::write_all(&mut filho.stdin.take().expect("stdin"), triple.as_bytes())
        .expect("write");
    let o = filho.wait_with_output().expect("wait");
    let e = erro(&o);
    assert_eq!(o.status.code(), Some(0), "{e}");
    assert!(e.contains("WARN nix-flake-untracked"), "{e}");
    assert!(
        e.contains("NOT CHECKED nix-flake-check: not checked: nix absent"),
        "{e}"
    );
}

#[test]
fn dev_null_to_every_point_exits_zero_and_speaks() {
    let r = Repo::novo("dn");
    for args in [
        vec!["pre-commit"],
        vec!["commit-msg", "/dev/null"],
        vec!["pre-push", "origin", "u"],
    ] {
        let o = r.joeira(&args, dev_null(), &[]);
        let e = erro(&o);
        assert_eq!(o.status.code(), Some(0), "{args:?}: {e}");
        assert!(e.starts_with("joeira "), "{args:?} is silent: {e}");
    }
}

#[test]
fn outside_a_repository_every_point_degrades_to_exit_zero() {
    let r = Repo::novo("fora");
    let longe = r.estado.clone();
    let o = Command::new(env!("CARGO_BIN_EXE_joeira"))
        .arg("pre-commit")
        .current_dir(Path::new(&longe))
        .env("PATH", &r.bin_git)
        .env("GIT_CEILING_DIRECTORIES", longe.parent().expect("parent"))
        .env("XDG_STATE_HOME", &r.estado)
        .output()
        .expect("runs");
    assert_eq!(o.status.code(), Some(0));
    assert!(erro(&o).contains("DEGRADED"), "{}", erro(&o));
}

#[test]
fn the_kill_switch_disarms_every_point_and_says_so() {
    let r = Repo::novo("off");
    let o = r.joeira(&["pre-commit"], dev_null(), &[("JOEIRA_OFF", "1")]);
    assert_eq!(o.status.code(), Some(0));
    assert!(
        erro(&o).contains("DISABLED by JOEIRA_OFF=1"),
        "{}",
        erro(&o)
    );
}

#[test]
fn explain_staged_prints_provenance() {
    let r = Repo::novo("ex");
    r.escreve("svc/Dockerfile", "FROM x\n");
    r.git(&["add", "."]);
    let o = r.joeira(&["explain", "--staged"], dev_null(), &[]);
    let s = String::from_utf8_lossy(&o.stdout);
    assert_eq!(o.status.code(), Some(0), "{}", erro(&o));
    assert!(
        s.contains("shell-dockerfile-new @pre-commit layer=usuario"),
        "{s}"
    );
    assert!(s.contains("ceiling"), "{s}");
    assert!(s.contains("not evaluated: mounts at pre-push"), "{s}");
}

#[test]
fn status_reports_the_corpus_and_absent_tools() {
    let r = Repo::novo("st");
    let o = r.joeira(&["status"], dev_null(), &[]);
    let s = String::from_utf8_lossy(&o.stdout);
    assert_eq!(o.status.code(), Some(0));
    assert!(s.contains("corpus: 17 rules"), "{s}");
    assert!(s.contains("ABSENT"), "{s}");
}
