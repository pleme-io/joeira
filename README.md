# joeira

> joeira (Brazilian-Portuguese joeira, the winnowing basket — the tray that separates grain from chaff by shaking, i.e. sifts a CHANGE before it is allowed through) — the typed git-hook control plane. The Sieve 篩 family, beside crivo and furui. Every git lifecycle gate is a typed declaration in one closed predicate algebra with NO text-bearing variant, so 'a hook grew a shell body' has no representation; one registry from which every enforcement artifact on every node in both arms is a readOnly projection; a default library whose rows each trace to a dated incident or a named doctrine, with severity DERIVED from (reversibility x false-positive-class) rather than authored; and a per-repo edge form that spends a fleet-granted capability rather than holding a switch, so a repo tunes one rule under one path prefix instead of reaching for --no-verify and disarming the credential gate with it. Three crates: joeira-core the engine and typed border, joeira-lisp the (defjoeira ...) authoring surface, joeira the binary. A SIBLING of guardrail consuming the shared hayai matching primitives, deliberately not an extension of it: a git hook's input is not one flat string, guardrail's resolve() is concatenate-then-filter with no representation for an override, and guardrail rebuilds its RegexSet on every Bash call. Honest ceiling: a local hook is capped at only-mitigated permanently by four bypasses, the fourth of which is not a deliberate act (HOME divergence). Design: theory/JOEIRA.md

## Status — P1 entrypoints, every rule advises

joeira has hook entrypoints and they never gate: every finding prints to stderr
and the exit is 0, including on unreadable input or outside a repository.
Nothing here installs itself as a git hook; the fleet's `pleme.gitHooks` module
decides that.

| crate | what it is |
|---|---|
| `joeira-core` | the typed border: the closed predicate algebra, the derived severity projection, the mockable `AmbienteGit` seam (index, message, pre-push ref-triples, repo-local tools as typed argv), the evaluator |
| `joeira-lisp` | the `(defjoeira …)` reader, with `:nivel` (sistema, usuario, classe-de-repo) and `:gatilho` (the marker path a row needs) |
| `joeira` | `pre-commit` · `commit-msg <file>` · `pre-push <remote> <url>` · `explain --staged` · `status` · `eval` · `prova` · `pontos` · `oraculo` |
| `joeira-ratchet` | the fleet's one ratchet (theory/JOEIRA.md §6.2), zero deps: `judge`, `shrunk`, `bank` (`BankMode::{Shrink, Record}`), `render`, `Summary`, multi-kind `Baseline`; conformance-tied to blue's `hadome`. Binary `ratchet judge\|bank\|render` (`nix run github:pleme-io/joeira#ratchet`): judge exits 1 only on growth or an unrecorded scope, bank rewrites the baseline down |

`JOEIRA_OFF=1` disarms every entrypoint and says so. A row whose tool is not on
`PATH` reports `not checked: <tool> absent`; a row whose marker is absent is
`not applicable`. Neither reads as clean.

The catalog holds 17 rows: the five incumbent concerns plus the §6.1 placement
rows (flake tracked, `nix flake check --no-build`, flake.lock age, `cargo
metadata`, `cargo fetch --locked`, `cargo fmt --check`, `blue bluefile
--confirm`, `go mod tidy -diff`, `go vet`, actionlint, new Dockerfile, shell past
glue). `joeira prova --all` proves each in both directions.

### Layers (P2)

Floors fold system → user → repository edge. `/etc/joeira/sistema.json` and
`~/.config/joeira/usuario.json` (`JOEIRA_SISTEMA` / `JOEIRA_USUARIO` override
the paths) are `{"nivel": …, "pisos": {<rule>: consultivo|avisa-com-catraca|avisa|bloqueia}}`;
only the system file may carry `"concessoes": {<name>: {"prefixo", "verbos", "regras"}}`.
A layer raises floors only and is clamped to each rule's ceiling; an unknown
rule is refused by name while its siblings apply. The edge is a
`(defjoeira-borda (extend "r" avisa) (relax "r" consultivo) (exempt-paths "r" "docs/") (subtract "r"))`
form in `joeira.lisp` or `caixa.lisp` at the worktree root. `extend` is always
honoured; the other verbs need a grant whose `prefixo` contains
`dirname(git rev-parse --git-common-dir)`, so worktrees inherit their repo's
grant. A malformed layer is dropped and named on every invocation; it never
blocks. `JOEIRA_SEM_BORDA=1` ignores the edge. `joeira explain --staged` prints
each rule's effective floor and the layer and path that set it.

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
