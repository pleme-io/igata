# Igata (鋳型) — Nix module for activation-time template rendering.
#
# Declares templates with variable sources. At activation, the igata binary
# reads a JSON manifest and renders all templates.
{ config, lib, pkgs, ... }:
let
  inherit (lib) mkOption mkEnableOption types mkIf;
  cfg = config.igata;
  igataLib = import ./lib.nix { inherit lib; };

  # Resolve template content: file takes precedence over inline content.
  effectiveContent = tmpl:
    if tmpl.file != null then builtins.readFile tmpl.file else tmpl.content;

  # Convert a template entry to its manifest JSON representation.
  templateToManifest = name: tmpl:
    let
      # Write template content to the Nix store so igata can read it at runtime.
      source = pkgs.writeText "igata-template-${name}" (effectiveContent tmpl);
    in
    {
      inherit source;
      target = tmpl.target;
      mode = tmpl.mode;
      owner = tmpl.owner;
      group = tmpl.group;
      context.variables = tmpl.variables;
    };

  # Build the complete manifest.
  manifest = {
    syntax = {
      variable = cfg.syntax.variable;
      block = cfg.syntax.block;
      comment = cfg.syntax.comment;
    };
    templates = lib.mapAttrs templateToManifest cfg.templates;
  };

  manifestFile = pkgs.writeText "igata-manifest.json" (builtins.toJSON manifest);
in
{
  options.igata = {
    enable = mkEnableOption "igata template rendering";

    package = mkOption {
      type = types.package;
      description = "The igata package to use.";
    };

    syntax = mkOption {
      type = igataLib.syntaxSubmodule;
      default = { };
      description = "Custom template syntax (delimiters).";
    };

    templates = mkOption {
      type = types.attrsOf igataLib.templateSubmodule;
      default = { };
      description = "Template entries to render at activation time.";
    };
  };

  config = mkIf cfg.enable {
    # Run igata at activation time.
    home.activation.igata-render = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
      ${cfg.package}/bin/igata render --manifest ${manifestFile}
    '';
  };
}
