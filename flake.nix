{
  description = "joeira (Brazilian-Portuguese joeira, the winnowing basket — the tray that separates grain from chaff by shaking, i.e. sifts a CHANGE before it is allowed through) — the typed git-hook control plane. The Sieve 篩 family, beside crivo and furui. Every git lifecycle gate is a typed declaration in one closed predicate algebra with NO text-bearing variant, so 'a hook grew a shell body' has no representation; one registry from which every enforcement artifact on every node in both arms is a readOnly projection; a default library whose rows each trace to a dated incident or a named doctrine, with severity DERIVED from (reversibility x false-positive-class) rather than authored; and a per-repo edge form that spends a fleet-granted capability rather than holding a switch, so a repo tunes one rule under one path prefix instead of reaching for --no-verify and disarming the credential gate with it. Three crates: joeira-core the engine and typed border, joeira-lisp the (defjoeira ...) authoring surface, joeira the binary. A SIBLING of guardrail consuming the shared hayai matching primitives, deliberately not an extension of it: a git hook's input is not one flat string, guardrail's resolve() is concatenate-then-filter with no representation for an override, and guardrail rebuilds its RegexSet on every Bash call. Honest ceiling: a local hook is capped at only-mitigated permanently by four bypasses, the fourth of which is not a deliberate act (HOME divergence). Design: theory/JOEIRA.md";
  inputs = {
    nixpkgs = {
      follows = "substrate/nixpkgs";
    };
    crate2nix = {
      url = "github:nix-community/crate2nix";
    };
    flake-utils = {
      url = "github:numtide/flake-utils";
    };
    substrate = {
      url = "github:pleme-io/substrate";
    };
  };
  outputs = inputs @ { self, nixpkgs, crate2nix, flake-utils, substrate, ... }:
    (import "${substrate}/lib/rust-workspace-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils;
    }) {
      toolName = "joeira";
      packageName = "joeira";
      src = self;
      repo = "pleme-io/joeira";
    };
}
