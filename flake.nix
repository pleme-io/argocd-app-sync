{
  description = "pleme-io/argocd-app-sync — trigger ArgoCD Application sync + wait for healthy";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    crate2nix = {
      url = "github:nix-community/crate2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ { self, nixpkgs, crate2nix, flake-utils, substrate, ... }:
    (import "${substrate}/lib/rust-action-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils;
    }) {
      toolName = "argocd-app-sync";
      src = self;
      repo = "pleme-io/argocd-app-sync";
      action = {
        description = "Trigger an ArgoCD Application sync (via annotation) and optionally wait for Synced/Healthy. Uses kubectl against the ArgoCD cluster — no argocd CLI auth setup required beyond the kubeconfig consumers already have.";
        inputs = [
          { name = "app-name"; description = "ArgoCD Application name"; required = true; }
          { name = "namespace"; description = "ArgoCD namespace"; default = "argocd"; }
          { name = "wait"; description = "Wait for Synced + Healthy"; default = "true"; }
          { name = "timeout-seconds"; description = "Wait timeout in seconds"; default = "600"; }
          { name = "kubectl-context"; description = "kubectl context"; }
          { name = "hard-refresh"; description = "Force argocd.argoproj.io/refresh=hard"; default = "false"; }
        ];
        outputs = [
          { name = "sync-status"; description = "Final sync status (Synced / OutOfSync / Unknown)"; }
          { name = "health-status"; description = "Final health status (Healthy / Degraded / Progressing / Missing / Suspended / Unknown)"; }
        ];
      };
    };
}
