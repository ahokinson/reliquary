{ rustPlatform, src }:

rustPlatform.buildRustPackage {
  pname = "reliquary";
  version = "0.1.4";

  inherit src;

  cargoHash = "sha256-1FybYm3F3WkGkxwMPdyVv2/2uL8Nqv7KpLTWsX634z0=";

  meta = {
    description = "Store secrets in your OS keyring and load them into your shell env on startup";
    homepage = "https://github.com/ahokinson/reliquary";
    mainProgram = "reliquary";
  };
}
