{
  description = "Monitor for duplicate links shared in Discord channels";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";
    docker-utils.url = "github:collinarnett/docker-utils";
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
    docker-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};

      craneLib = crane.mkLib pkgs;

      commonArgs = {
        src = craneLib.cleanCargoSource ./.;
        strictDeps = true;

        buildInputs = with pkgs;
          [
            openssl
            pkg-config
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
          ];
      };

      my-crate = craneLib.buildPackage (commonArgs
        // {
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        });

      dockerImage = pkgs.dockerTools.buildImage {
        name = "git.gmem.ca/arch/duplikate";
        tag = "latest-${system}";
        config = {
          Cmd = ["${my-crate}/bin/duplikate"];
        };
      };
    in {
      checks = {
        inherit my-crate;
      };

      packages.default = my-crate;
      packages.docker = dockerImage;

      apps.default = flake-utils.lib.mkApp {
        drv = my-crate;
      };

      apps.concatDocker = {
        type = "app";
        program = toString (pkgs.writers.writeBash "concatDocker" ''
          amd64=git.gmem.ca/arch/duplikate:latest-x86_64-linux
          arm64=git.gmem.ca/arch/duplikate:latest-aarch64-linux
          docker load -i ${self.packages.x86_64-linux.docker}
          docker load -i ${self.packages.aarch64-linux.docker}
          docker push $amd64
          docker push $arm64
          docker manifest create --amend git.gmem.ca/arch/duplikate:latest $arm64 $amd64
          docker manifest push git.gmem.ca/arch/duplikate:latest --purge
        '');
      };

      devShells.default = craneLib.devShell {
        checks = self.checks.${system};

        packages = with pkgs; [
          rust-analyzer
        ];
      };
    });
}
