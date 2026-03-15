{
  description = "Igata (鋳型) — general-purpose template engine for Nix activation-time rendering";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      substrate,
      ...
    }:
    let
      system = "aarch64-darwin";
      pkgs = import nixpkgs { inherit system; };
      package = pkgs.rustPlatform.buildRustPackage {
        pname = "igata";
        version = "0.1.0";
        src = ./.;
        useFetchCargoVendor = true;
        cargoHash = "";
        meta = {
          description = "General-purpose template engine for Nix activation-time rendering";
          homepage = "https://github.com/pleme-io/igata";
          license = pkgs.lib.licenses.mit;
        };
      };
    in
    {
      packages.${system}.default = package;

      overlays.default = final: prev: {
        igata = self.packages.${final.system}.default;
      };

      homeManagerModules.default = import ./module {
        inherit (nixpkgs) lib;
        inherit pkgs;
      };

      devShells.${system}.default = pkgs.mkShellNoCC {
        buildInputs = [
          pkgs.rustc
          pkgs.cargo
          pkgs.clippy
          pkgs.rustfmt
        ];
      };

      formatter.${system} = pkgs.nixfmt-tree;
    };
}
