use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn scratch(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("joeira-ratchet-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn ratchet(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ratchet"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn judge_exits_zero_when_held_or_shrunk() {
    let b = scratch("held.txt", "entry: a 5\nentry: b 3\n");
    let out = ratchet(
        &["judge", "--baseline", b.to_str().unwrap()],
        "entry: a 5\nentry: b 1\n",
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(
        text(&out.stdout).ends_with("green: 2 held, 0 grew, 0 unrecorded, 1 shrunk, 0 orphans\n")
    );
}

#[test]
fn judge_exits_one_on_growth() {
    let b = scratch("grew.txt", "entry: a 5\n");
    let out = ratchet(
        &["judge", "--baseline", b.to_str().unwrap()],
        "entry: a 6\n",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stdout).starts_with("grew: entry: a measures 6, past its recorded 5 by 1\n"));
}

#[test]
fn judge_exits_one_on_an_unrecorded_scope_and_reads_a_measurements_file() {
    let b = scratch("unrec.txt", "entry: a 5\n");
    let m = scratch("unrec-m.txt", "entry: a 5\nentry: new 1\n");
    let out = ratchet(
        &[
            "judge",
            "--baseline",
            b.to_str().unwrap(),
            "--measurements",
            m.to_str().unwrap(),
        ],
        "",
    );
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn judge_stays_green_on_an_orphan() {
    let b = scratch("orphan.txt", "entry: a 5\nentry: gone 2\n");
    let out = ratchet(
        &["judge", "--baseline", b.to_str().unwrap()],
        "entry: a 5\n",
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out.stdout).contains("orphan: entry: gone recorded 2 and no longer measured\n"));
}

#[test]
fn bank_lowers_shrinks_drops_orphans_and_never_records_without_asking() {
    let b = scratch(
        "bank.txt",
        "# note\nentry: z 9\nentry: a 5\nentry: gone 2\n",
    );
    let out = ratchet(
        &["bank", "--baseline", b.to_str().unwrap()],
        "entry: a 3\nentry: z 10\nentry: new 4\n",
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        std::fs::read_to_string(&b).unwrap(),
        "entry: a 3\nentry: z 9\n"
    );
    let out = ratchet(
        &["bank", "--baseline", b.to_str().unwrap(), "--record"],
        "entry: a 3\nentry: z 9\nentry: new 4\n",
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        std::fs::read_to_string(&b).unwrap(),
        "entry: a 3\nentry: new 4\nentry: z 9\n"
    );
}

#[test]
fn render_prints_the_canonical_form() {
    let out = ratchet(&["render"], "rust: zebra 1\n# c\nrust: alpha 2\n");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out.stdout), "rust: alpha 2\nrust: zebra 1\n");
}

#[test]
fn a_usage_or_read_fault_exits_two() {
    assert_eq!(ratchet(&[], "").status.code(), Some(2));
    assert_eq!(ratchet(&["judge"], "").status.code(), Some(2));
    assert_eq!(
        ratchet(&["bank", "--baseline", "-"], "").status.code(),
        Some(2)
    );
    assert_eq!(ratchet(&["render", "--record"], "").status.code(), Some(2));
    let out = ratchet(&["judge", "--baseline", "/nonexistent/joeira-ratchet"], "");
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).starts_with("ratchet: cannot read /nonexistent/joeira-ratchet"));
}
