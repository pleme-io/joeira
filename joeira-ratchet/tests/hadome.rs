use std::collections::BTreeSet;

use joeira_ratchet::{BankMode, Baseline, Shrink, Verdict};

const PUSH: &str = "wf/image-push.yml::push/Build and push";
const TENDRIL: &str = "docs/CLAUDE.md::The Tendril Method";

fn example_baseline() -> Baseline {
    let mut b = Baseline::default();
    b.insert("entry", TENDRIL, 4139);
    b.insert("entry", PUSH, 60);
    b
}

fn live(keys: &[&str]) -> BTreeSet<String> {
    keys.iter().map(|k| (*k).to_owned()).collect()
}

fn orphans(b: &Baseline, kind: &str, keys: &[&str]) -> Vec<String> {
    b.kind(kind)
        .map(|r| {
            r.orphans(&live(keys))
                .into_iter()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn measured(kind: &str, rows: &[(&str, usize)]) -> Baseline {
    let mut m = Baseline::default();
    for (key, size) in rows {
        m.insert(kind, *key, *size);
    }
    m
}

#[test]
fn an_unrecorded_key_is_a_new_violation() {
    assert_eq!(
        example_baseline().judge("entry", "nope", 1),
        Verdict::Unrecorded
    );
    assert_eq!(
        Baseline::default().judge("entry", "anything", 0),
        Verdict::Unrecorded
    );
}

#[test]
fn recorded_debt_may_hold_or_shrink() {
    assert_eq!(example_baseline().judge("entry", PUSH, 60), Verdict::Held);
    assert_eq!(example_baseline().judge("entry", PUSH, 12), Verdict::Held);
}

#[test]
fn recorded_debt_that_grows_by_one_fails() {
    assert_eq!(
        example_baseline().judge("entry", PUSH, 61),
        Verdict::Grew {
            recorded: 60,
            grew: 1
        }
    );
}

#[test]
fn a_row_covers_only_its_own_kind() {
    assert_eq!(
        example_baseline().judge("host", PUSH, 1),
        Verdict::Unrecorded
    );
}

#[test]
fn parse_keeps_keys_containing_spaces() {
    let mut want = Baseline::default();
    want.insert("entry", TENDRIL, 4139);
    assert_eq!(
        Baseline::parse("entry: docs/CLAUDE.md::The Tendril Method 4139\n"),
        want
    );
}

#[test]
fn a_malformed_line_covers_nothing_rather_than_weakening_the_gate() {
    assert!(
        Baseline::parse(
            "# a comment\n\nentry: no-size-here\nnocolon 12\nentry: bad-size xyz\nentry: negative -3\n"
        )
        .is_empty()
    );
}

#[test]
fn a_later_row_for_the_same_key_wins_as_a_map_insert_does() {
    assert_eq!(
        Baseline::parse("entry: k 5\nentry: k 9\n")
            .kind("entry")
            .and_then(|r| r.recorded("k")),
        Some(9)
    );
}

#[test]
fn a_row_whose_subject_is_gone_is_an_orphan() {
    let b = example_baseline();
    assert_eq!(orphans(&b, "entry", &[TENDRIL]), vec![PUSH.to_owned()]);
    assert!(orphans(&b, "entry", &[TENDRIL, PUSH]).is_empty());
}

#[test]
fn orphans_does_not_simply_return_everything() {
    assert_eq!(
        orphans(&example_baseline(), "entry", &[]),
        vec![TENDRIL.to_owned(), PUSH.to_owned()]
    );
    assert!(orphans(&Baseline::default(), "entry", &[]).is_empty());
}

#[test]
fn failures_name_every_unrecorded_and_grown_measurement_and_nothing_held() {
    let ms = measured("entry", &[(PUSH, 61), ("nope", 3), (TENDRIL, 4139)]);
    let s = example_baseline().summarize(&ms);
    let mut failed: Vec<(String, String)> = s
        .failures()
        .map(|j| {
            let word = match j.verdict {
                Verdict::Grew { .. } => "grew",
                _ => "unrecorded",
            };
            (word.to_owned(), j.to_string())
        })
        .collect();
    failed.sort();
    assert_eq!(
        failed,
        vec![
            (
                "grew".to_owned(),
                "entry: wf/image-push.yml::push/Build and push measures 61, past its recorded 60 by 1"
                    .to_owned()
            ),
            (
                "unrecorded".to_owned(),
                "entry: nope measures 3 and the baseline has no row for it".to_owned()
            ),
        ]
    );
    assert!(
        example_baseline()
            .summarize(&measured("entry", &[]))
            .failures()
            .next()
            .is_none()
    );
}

#[test]
fn shrunk_lists_only_measurements_below_their_row() {
    let ms = measured("entry", &[(PUSH, 12), (TENDRIL, 4139), ("nope", 1)]);
    let s = example_baseline().summarize(&ms);
    let shrunk: Vec<(&str, Shrink)> = s
        .shrinks
        .iter()
        .map(|x| (x.key.as_str(), x.shrink))
        .collect();
    assert_eq!(
        shrunk,
        vec![(
            PUSH,
            Shrink {
                recorded: 60,
                now: 12
            }
        )]
    );
}

#[test]
fn render_is_sorted_so_a_baseline_diffs_stably_and_parses_back() {
    let a = measured("rust", &[("zebra", 1), ("alpha", 2)]).render();
    let b = measured("rust", &[("alpha", 2), ("zebra", 1)]).render();
    assert_eq!(a, "rust: alpha 2\nrust: zebra 1\n");
    assert_eq!(a, b);
    assert_eq!(
        Baseline::parse(&a),
        measured("rust", &[("alpha", 2), ("zebra", 1)])
    );
    assert_eq!(measured("rust", &[]).render(), "");
}

#[test]
fn banking_what_hadome_reports_as_shrunk_lowers_exactly_those_rows() {
    let ms = measured("entry", &[(PUSH, 12), (TENDRIL, 4139)]);
    assert_eq!(
        example_baseline().bank(&ms, BankMode::Shrink).render(),
        "entry: docs/CLAUDE.md::The Tendril Method 4139\nentry: wf/image-push.yml::push/Build and push 12\n"
    );
}
