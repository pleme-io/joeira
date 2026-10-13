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
    let
      inherit (nixpkgs.lib) mkOption types;
      canonical = {
        consultivo = "consultivo";
        avisa-com-catraca = "avisa-com-catraca";
        avisa = "avisa";
        bloqueia = "bloqueia";
        advise = "consultivo";
        warn-with-ratchet = "avisa-com-catraca";
        warn = "avisa";
        block = "bloqueia";
      };
      severity = types.enum (builtins.attrNames canonical);
      floors = description: mkOption {
        type = types.attrsOf severity;
        default = { };
        inherit description;
      };
      grant = types.submodule {
        options = {
          pathPrefix = mkOption {
            type = types.str;
            description = "Absolute path prefix matched against dirname(git rev-parse --git-common-dir), so every worktree of a granted repository inherits the grant.";
          };
          verbs = mkOption {
            type = types.listOf (types.enum [ "relax" "exempt-paths" "subtract" ]);
            description = "Edge verbs this grant lets the repository spend. extend needs no grant.";
          };
          rules = mkOption {
            type = types.listOf types.str;
            description = "Rule names the verbs may touch.";
          };
        };
      };
      budgets = description: mkOption {
        type = types.attrsOf (types.ints.between 1 600);
        default = { };
        inherit description;
      };
      placements = description: mkOption {
        type = types.attrsOf (types.enum [ "gancho" "adiada" "desligada" "hook" "deferred" "off" ]);
        default = { };
        inherit description;
      };
      hookBudgets = description: mkOption {
        type = types.attrsOf (types.ints.between 100 600000);
        default = { };
        inherit description;
      };
      pisosOf = rules: builtins.mapAttrs (_: s: canonical.${s}) rules;
      systemLayer = { cfg, ... }: {
        environment.etc."joeira/sistema.json".text = builtins.toJSON {
          nivel = "sistema";
          pisos = pisosOf cfg.rules;
          orcamentos = cfg.budgets;
          colocacoes = cfg.placements;
          prazos_ms = cfg.hookBudgets;
          concessoes = builtins.mapAttrs (_: g: {
            prefixo = g.pathPrefix;
            verbos = g.verbs;
            regras = g.rules;
          }) cfg.grants;
        };
      };
    in
    (import "${substrate}/lib/rust-workspace-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils;
    }) {
      toolName = "joeira";
      packageName = "joeira";
      src = self;
      repo = "pleme-io/joeira";
      module = {
        description = "joeira, the typed git-hook control plane";
        hmNamespace = "blackmatter.components";
        extraSystemOptions = {
          rules = floors "System-level rules and their floors (fleet rules, floors no lower layer relaxes), rendered to /etc/joeira/sistema.json.";
          budgets = budgets "Per-rule time budgets in seconds, rendered to sistema.json as orcamentos. A rule in the hook is also capped by its point's hook budget; a deferred rule defaults to 600 s. A row over budget has its process group killed and is reported not checked.";
          placements = placements "Per-rule placement, rendered to sistema.json as colocacoes: gancho runs in the hook, adiada only under `joeira deferred <point>`, desligada nowhere.";
          hookBudgets = hookBudgets "Total wall-clock budget per hook point in milliseconds (pre-commit, commit-msg, pre-push), rendered to sistema.json as prazos_ms. Defaults 1500, 1000, 5000.";
          grants = mkOption {
            type = types.attrsOf grant;
            default = { };
            description = "Capabilities a repository edge form (defjoeira-borda ...) may spend, rendered to sistema.json as concessoes. Only the system layer carries grants.";
          };
        };
        extraNixosConfigFn = systemLayer;
        extraDarwinConfigFn = systemLayer;
        extraHmOptions = {
          rules = floors "User-level rules and their floors, rendered to ~/.config/joeira/usuario.json.";
          budgets = budgets "Per-rule time budgets in seconds, rendered to usuario.json as orcamentos; they override the system layer's.";
          placements = placements "Per-rule placement, rendered to usuario.json as colocacoes; overrides the system layer's.";
          hookBudgets = hookBudgets "Hook budgets per point in milliseconds, rendered to usuario.json as prazos_ms; overrides the system layer's.";
          repoEdge.enable = mkOption {
            type = types.bool;
            default = true;
            description = "Read the repository's (defjoeira-borda ...) edge layer from joeira.lisp or caixa.lisp at the worktree root. false sets JOEIRA_SEM_BORDA=1 in the session, the rollback that makes every grant inert.";
          };
        };
        extraHmConfigFn = { cfg, lib, ... }: {
          xdg.configFile."joeira/usuario.json".text = builtins.toJSON {
            nivel = "usuario";
            pisos = pisosOf cfg.rules;
            orcamentos = cfg.budgets;
            colocacoes = cfg.placements;
            prazos_ms = cfg.hookBudgets;
          };
          home.sessionVariables = lib.mkIf (!cfg.repoEdge.enable) { JOEIRA_SEM_BORDA = "1"; };
        };
      };
    };
}
