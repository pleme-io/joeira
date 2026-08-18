# joeira

> joeira (Brazilian-Portuguese joeira, the winnowing basket — the tray that separates grain from chaff by shaking, i.e. sifts a CHANGE before it is allowed through) — the typed git-hook control plane. The Sieve 篩 family, beside crivo and furui. Every git lifecycle gate is a typed declaration in one closed predicate algebra with NO text-bearing variant, so 'a hook grew a shell body' has no representation; one registry from which every enforcement artifact on every node in both arms is a readOnly projection; a default library whose rows each trace to a dated incident or a named doctrine, with severity DERIVED from (reversibility x false-positive-class) rather than authored; and a per-repo edge form that spends a fleet-granted capability rather than holding a switch, so a repo tunes one rule under one path prefix instead of reaching for --no-verify and disarming the credential gate with it. Three crates: joeira-core the engine and typed border, joeira-lisp the (defjoeira ...) authoring surface, joeira the binary. A SIBLING of guardrail consuming the shared hayai matching primitives, deliberately not an extension of it: a git hook's input is not one flat string, guardrail's resolve() is concatenate-then-filter with no representation for an override, and guardrail rebuilds its RegexSet on every Bash call. Honest ceiling: a local hook is capped at only-mitigated permanently by four bypasses, the fourth of which is not a deliberate act (HOME divergence). Design: theory/JOEIRA.md

## Status — P0, and it does not gate anything yet

**Read this before reaching for it.** joeira is at P0: it *reads* and it *proves*.
There is deliberately no `pre-commit` subcommand and nothing here is installed as
a git hook, because P0's job is the oracle and the gate "enforcing nothing new" —
shipping a gating entrypoint before the differential against the incumbent hooks
is green would invert that order.

What exists today:

| crate | what it is |
|---|---|
| `joeira-core` | the typed border: the closed predicate algebra, the derived severity projection, the mockable `AmbienteGit` seam, the evaluator |
| `joeira-lisp` | the `(defjoeira …)` reader. **M0 hand-written**, not yet a `#[derive(TataraDomain)]` domain |
| `joeira` | `eval` · `prova` · `pontos`. No hook entrypoint. |

35 tests, all mock-green — the suite touches no repository, spawns no process and
writes no file.

### The honest ceiling, which no later phase may claim away

**A local git hook cannot guarantee anything.** There are four bypasses and the
fourth is not a deliberate act:

1. `git commit --no-verify` — six characters, and it disables *every* hook at once
2. `git -c core.hooksPath=/dev/null` / `GIT_CONFIG_COUNT=…`
3. `HOME` divergence — a missing `hooksPath` directory is **not an error**, so the
   commit simply lands. That is the default state of every container, build
   sandbox, systemd unit, CI runner and daemon.
4. the hook only exists where it was installed at all

So what joeira buys is a **typed, complete, self-proving and gated library**, plus
fast local feedback on *the commit as the unit* — which CI structurally cannot see,
because CI sees a branch. It does **not** buy an unbypassable guard, and the
design doc says so in the same words.

### Two invariants worth knowing

- **`Predicado` has no text-bearing variant.** No `Shell(String)`, no
  `Comando(String)` — so "a hook grew a shell body" has no representation. That
  is a compile error, not a lint. (A regex is data, not a program; matching is
  not executing.)
- **Severity is derived, never authored.** It is a total function of
  `(reversibility × false-positive-class)`, and a rule's floor is *clamped* to
  that ceiling — so an unjustified block is not refused, it is inexpressible.
  Every rule is born advisory, including the irreversible ones.

## Building

```bash
nix run .#joeira -- --help
```

## License

MIT.
