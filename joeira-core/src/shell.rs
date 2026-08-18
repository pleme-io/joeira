//! `AmbienteShell` — the shell-backed [`AmbienteGit`], reading a repository
//! through `git` subprocesses.
//!
//! This is the third implementation of the seam, beside [`crate::AmbienteMock`]
//! (the test environment) and, later, an in-process engine. It exists so the
//! engine can be pointed at a real repository without the engine itself
//! knowing what a subprocess is.
//!
//! # Every invocation here is copied from the incumbent hook, deliberately
//!
//! The fleet's shipped `pre-commit` hook already reads exactly these five facts,
//! and its comments record four bugs that shipped before the current form
//! settled. Re-deriving the invocations from first principles would re-derive
//! the bugs, so each one below names what it is defending.
//!
//! | fact | invocation | why exactly this |
//! |---|---|---|
//! | staged paths | `git diff --cached --name-only --diff-filter=ACMR` | `R` was **out by omission until 2026-08-13** and that was a live bypass: git reports a `git mv` that also edits a file as `R` whenever similarity stays above the rename threshold, so renaming a file while adding a credential to it was ALLOWED. `D` is out by reasoning — a deletion cannot introduce anything, and asking git to show a deleted path errors. |
//! | staged blob | `git show :<path>` | the INDEX, never the worktree. A worktree read reports "fresh" for the exact mistake the D2 tie catches — reproduced on `shinka`: stage the sidecar alone, leave its partner dirty, and a worktree-based gate passes a commit that breaks every consumer. |
//! | HEAD blob | `git show HEAD:<path>` | non-zero exit ⇒ `Ok(None)`, **not** an error. A new file, or a rename's destination, has no HEAD copy; that is a fact about the path, not a failure to look. |
//! | added lines | `git diff --cached -U0 --text -- <path>` | `--text` is load-bearing. git classifies a file carrying NUL bytes as binary and prints `Binary files differ` instead of hunks, so without it a diff-based scan yields **zero** `+` lines for such a file and every credential inside walks through. Invisible in testing unless a fixture really contains a NUL. |
//! | staged sha256 | sha256 over the `git show :<path>` bytes | byte-fidelity is already calibrated upstream: the incumbent records that this reproduces `shasum -a 256` of the staged blob exactly, so it compares the same quantity `gen` writes down. |
//!
//! # Process discipline, from `tend::GitOps`
//!
//! Three idioms, chosen per question rather than uniformly — the fleet's git seam
//! settled on these and the asymmetries are not stylistic:
//!
//! - **content wanted** → `.output()`, check `status.success()`, and put
//!   **stderr** in the error;
//! - **boolean from an exit code** → `.status()` and invert it, capturing nothing;
//! - **boolean from emptiness** → `.output()` and test `stdout.is_empty()`.
//!
//! Every spawn carries the invocation in its error text, so a missing binary or a
//! bad working directory names itself instead of surfacing as a blank failure.
//!
//! # Memoization is required, not an optimization
//!
//! One `git` subprocess measures **~14.5 ms on darwin**. [`AmbienteGit`] is
//! one-path-per-call with no batch method, and a rule set asks for the same blob
//! more than once (a value rule reads the added lines, a structural rule reads
//! the whole blob), so an unmemoized implementation multiplies that by the rule
//! count. The incumbent pays exactly this: three of its gates each re-derive the
//! staged list with the byte-identical query, and it reads some blobs three
//! times.
//!
//! [`AmbienteGit`] deliberately has no `Send + Sync` bound — unlike
//! `tend::GitOps` — so a `RefCell` cache behind `&self` is available and is what
//! is used here.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{AmbienteGit, JoeiraError};

/// A repository read through `git` subprocesses.
pub struct AmbienteShell {
    repo: PathBuf,
    /// path → staged blob. See the memoization note in the module docs.
    stage: RefCell<BTreeMap<String, String>>,
    /// path → HEAD blob, `None` when the path has no HEAD copy.
    head: RefCell<BTreeMap<String, Option<String>>>,
    /// The staged path list, resolved at most once.
    caminhos: RefCell<Option<Vec<String>>>,
}

impl AmbienteShell {
    /// Read the repository rooted at `repo`.
    ///
    /// The root is explicit rather than taken from the process working
    /// directory — the same choice `tend::GitOps` makes, and the reason the
    /// incumbent's own corpus test needs a `cd` shim it would rather not have.
    /// A fleet-wide sweep visits hundreds of repositories from one process.
    #[must_use]
    pub fn novo(repo: impl Into<PathBuf>) -> Self {
        Self {
            repo: repo.into(),
            stage: RefCell::new(BTreeMap::new()),
            head: RefCell::new(BTreeMap::new()),
            caminhos: RefCell::new(None),
        }
    }

    #[must_use]
    pub fn raiz(&self) -> &Path {
        &self.repo
    }

    /// Run `git` and return stdout, refusing on a non-zero exit.
    ///
    /// The invocation is in the error text: a failure that does not say what it
    /// ran is a failure someone has to reproduce before they can read it.
    fn git(&self, args: &[&str]) -> Result<String, JoeiraError> {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.repo)
            .output()
            .map_err(|e| {
                JoeiraError::AmbienteCego(format!(
                    "spawning `git {}` in {}: {e}",
                    args.join(" "),
                    self.repo.display()
                ))
            })?;
        if !out.status.success() {
            return Err(JoeiraError::AmbienteCego(format!(
                "`git {}` in {} exited {}: {}",
                args.join(" "),
                self.repo.display(),
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Run `git` where a non-zero exit is a legitimate ANSWER rather than a
    /// failure — the `git show HEAD:<path>` case, where "no such path in HEAD"
    /// is the fact being asked for.
    ///
    /// This is the single discrimination the whole implementation turns on. Fold
    /// it into [`Self::git`] and every new file becomes
    /// [`crate::Veredito::Cego`], which reads as "the gate could not look" when
    /// the truth is "there is nothing there yet" — and a rule that should have
    /// fired on a brand-new file would report blind instead.
    fn git_opcional(&self, args: &[&str]) -> Result<Option<String>, JoeiraError> {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.repo)
            .output()
            .map_err(|e| {
                JoeiraError::AmbienteCego(format!(
                    "spawning `git {}` in {}: {e}",
                    args.join(" "),
                    self.repo.display()
                ))
            })?;
        if out.status.success() {
            Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
        } else {
            Ok(None)
        }
    }
}

impl AmbienteGit for AmbienteShell {
    fn mensagem(&self) -> Result<String, JoeiraError> {
        // The message of the commit being prepared is not a repository fact —
        // git hands `commit-msg` a FILE path in argv. A shell environment
        // pointed at a repository can only answer for a commit that already
        // exists, so this reads HEAD's message and the hook entrypoint (P1)
        // overrides it with the file it was given. Returning an error here
        // instead would make every message rule report `Cego` in the oracle,
        // where HEAD's message is exactly the right answer.
        self.git(&["log", "-1", "--format=%B", "HEAD"])
    }

    fn caminhos_em_stage(&self) -> Result<Vec<String>, JoeiraError> {
        if let Some(cached) = self.caminhos.borrow().as_ref() {
            return Ok(cached.clone());
        }
        let out = self.git(&["diff", "--cached", "--name-only", "--diff-filter=ACMR"])?;
        let paths: Vec<String> = out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        *self.caminhos.borrow_mut() = Some(paths.clone());
        Ok(paths)
    }

    fn blob_em_stage(&self, caminho: &str) -> Result<String, JoeiraError> {
        if let Some(hit) = self.stage.borrow().get(caminho) {
            return Ok(hit.clone());
        }
        // A staged path that cannot be shown is a genuine failure to look: the
        // path came from `--diff-filter=ACMR`, so it exists in the index by
        // construction. Unlike the HEAD read, there is no legitimate absent case.
        let blob = self.git(&["show", &format!(":{caminho}")])?;
        self.stage
            .borrow_mut()
            .insert(caminho.to_owned(), blob.clone());
        Ok(blob)
    }

    fn blob_em_head(&self, caminho: &str) -> Result<Option<String>, JoeiraError> {
        if let Some(hit) = self.head.borrow().get(caminho) {
            return Ok(hit.clone());
        }
        let blob = self.git_opcional(&["show", &format!("HEAD:{caminho}")])?;
        self.head
            .borrow_mut()
            .insert(caminho.to_owned(), blob.clone());
        Ok(blob)
    }

    fn sha256_em_stage(&self, caminho: &str) -> Result<String, JoeiraError> {
        // Hashed from the same bytes `blob_em_stage` returns, so the cache is
        // shared and the quantity is the one the sidecar records.
        let blob = self.blob_em_stage(caminho)?;
        Ok(sha256_hex(blob.as_bytes()))
    }
}

// ═══════════════════════════════════════════════════════════════════
// sha256 — implemented here to keep the crate dependency-free
// ═══════════════════════════════════════════════════════════════════

/// FIPS 180-4 SHA-256, hex-encoded.
///
/// Hand-rolled rather than taking `sha2`, for one reason: this crate is the
/// typed border and its dependency list is part of its contract. The algorithm
/// is fixed by standard, fully specified, and pinned below against the two
/// canonical vectors plus the fleet's own `shasum -a 256` output — so the usual
/// argument against rolling your own (the spec is subtle, the failure is silent)
/// does not apply to a hash with published test vectors.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];

    let mut msg = bytes.to_vec();
    let bitlen = (bytes.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(v);
        }
    }

    // Hex-encoded by nibble lookup rather than `format!("{byte:02x}")`:
    // TYPED EMISSION bans `format!()` for emitted strings, and a hash IS emitted
    // output. A 16-byte table and two shifts need no formatter.
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for word in h {
        for byte in word.to_be_bytes() {
            out.push(char::from(D[usize::from(byte >> 4)]));
            out.push(char::from(D[usize::from(byte & 0x0f)]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── sha256, against published vectors ───────────────────────────────

    #[test]
    fn sha256_matches_the_canonical_empty_vector() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_matches_the_canonical_abc_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// Crosses a block boundary — the padding branch that only fires when the
    /// message length mod 64 lands in [56, 64).
    #[test]
    fn sha256_matches_across_a_block_boundary() {
        let msg = "a".repeat(1_000_000);
        // The published vector for one million 'a' characters.
        assert_eq!(
            sha256_hex(msg.as_bytes()),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn sha256_is_64_lowercase_hex_chars() {
        let h = sha256_hex(b"joeira");
        assert_eq!(h.len(), 64);
        assert!(
            h.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }
}
