{
  description = "ESP32 controlled stereo volume control";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    esp-rs-nix.url = "github:leighleighleigh/esp-rs-nix";
  };

  outputs = {
    self,
    nixpkgs,
    esp-rs-nix,
  }: let
    system = "x86_64-linux";
    pkgs = import nixpkgs {inherit system;};
  in {
    devShells.${system}.default =
      esp-rs-nix.devShells.${system}.default;
  };
}
