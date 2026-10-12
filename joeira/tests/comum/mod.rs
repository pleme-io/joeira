#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub struct Repo {
    pub base: PathBuf,
    pub dir: PathBuf,
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

impl Repo {
    pub fn novo(nome: &str) -> Self {
        let base = std::env::temp_dir().join(format!("joeira-it-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("repo");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let r = Self { base, dir };
        r.git(&["init", "-q", "-b", "main"]);
        r
    }

    pub fn git(&self, args: &[&str]) -> String {
        let o = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(&self.dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("git runs");
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).trim().to_owned()
    }

    pub fn escreve(&self, caminho: &str, conteudo: &str) {
        let p = self.dir.join(caminho);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).expect("mkdir");
        }
        std::fs::write(p, conteudo).expect("write");
    }

    pub fn commit(&self, caminho: &str, conteudo: &str, msg: &str) -> String {
        self.escreve(caminho, conteudo);
        self.git(&["add", caminho]);
        self.git(&["commit", "-q", "--no-verify", "-m", msg]);
        self.git(&["rev-parse", "HEAD"])
    }

    pub fn camada(&self, nome: &str, conteudo: &str) -> PathBuf {
        let p = self.base.join(nome);
        std::fs::write(&p, conteudo).expect("write");
        p
    }

    pub fn joeira(&self, args: &[&str], sistema: Option<&Path>) -> Output {
        self.joeira_com(args, sistema, "", None)
    }

    pub fn joeira_com(
        &self,
        args: &[&str],
        sistema: Option<&Path>,
        entrada: &str,
        bin_extra: Option<&Path>,
    ) -> Output {
        use std::io::Write;
        let path = match bin_extra {
            Some(b) => format!(
                "{}:{}",
                b.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
            None => std::env::var("PATH").unwrap_or_default(),
        };
        let mut filho = Command::new(env!("CARGO_BIN_EXE_joeira"))
            .env("PATH", path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .args(args)
            .current_dir(&self.dir)
            .env(
                "JOEIRA_SISTEMA",
                sistema.unwrap_or(&self.base.join("absent-sistema.json")),
            )
            .env("JOEIRA_USUARIO", self.base.join("absent-usuario.json"))
            .env("XDG_STATE_HOME", self.base.join("state"))
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("JOEIRA_OFF")
            .env_remove("JOEIRA_SEM_BORDA")
            .spawn()
            .expect("joeira runs");
        filho
            .stdin
            .take()
            .expect("stdin")
            .write_all(entrada.as_bytes())
            .expect("stdin written");
        filho.wait_with_output().expect("joeira exits")
    }
}

pub fn texto(o: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}
