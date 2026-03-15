# Shared option types for igata templates.
{ lib }:
let
  inherit (lib) mkOption mkEnableOption types;
in
{
  # Syntax configuration submodule.
  syntaxSubmodule = types.submodule {
    options = {
      variable = mkOption {
        type = types.listOf types.str;
        default = [ "[=" "=]" ];
        description = "Variable delimiter pair.";
      };
      block = mkOption {
        type = types.listOf types.str;
        default = [ "[%" "%]" ];
        description = "Block delimiter pair.";
      };
      comment = mkOption {
        type = types.listOf types.str;
        default = [ "[#" "#]" ];
        description = "Comment delimiter pair.";
      };
    };
  };

  # Variable source submodule — tagged union matching Rust's Source enum.
  sourceSubmodule = types.submodule {
    options = {
      type = mkOption {
        type = types.enum [ "literal" "file" "env" ];
        description = "Variable source type.";
      };
      # For literal:
      value = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Literal value (when type = literal).";
      };
      # For file:
      path = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "File path to read (when type = file).";
      };
      # For env:
      name = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Environment variable name (when type = env).";
      };
    };
  };

  # Template entry submodule.
  templateSubmodule = types.submodule {
    options = {
      content = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Inline template content. Mutually exclusive with 'file'.";
      };
      file = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to a template file. Takes precedence over 'content'.";
      };
      target = mkOption {
        type = types.str;
        description = "Target path for the rendered output.";
      };
      mode = mkOption {
        type = types.str;
        default = "0600";
        description = "File permissions (octal string).";
      };
      owner = mkOption {
        type = types.str;
        default = "";
        description = "File owner.";
      };
      group = mkOption {
        type = types.str;
        default = "";
        description = "File group.";
      };
      variables = mkOption {
        type = types.attrsOf sourceSubmodule;
        default = { };
        description = "Variable sources for this template.";
      };
    };
  };

  # Helper: create a literal variable source.
  literal = value: {
    type = "literal";
    inherit value;
  };

  # Helper: create a file variable source.
  fromFile = path: {
    type = "file";
    inherit path;
  };

  # Helper: create an env variable source.
  fromEnv = name: {
    type = "env";
    inherit name;
  };
}
