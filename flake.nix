{
  description = "Store secrets in your OS keyring and load them into your shell env on startup";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" "aarch64-linux" ];
      forEachSystem = nixpkgs.lib.genAttrs systems;

      reliquaryFor = pkgs: pkgs.callPackage ./package.nix { src = self; };
    in
    {
      packages = forEachSystem (system:
        let reliquary = reliquaryFor nixpkgs.legacyPackages.${system};
        in { inherit reliquary; default = reliquary; });

      overlays.default = final: _prev: { reliquary = reliquaryFor final; };
    };
}
