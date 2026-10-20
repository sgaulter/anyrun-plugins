{
  description = "Custom anyrun plugins";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      hyprlandFocus = pkgs.callPackage ./nix/hyprland-focus.nix { };
    in {
      packages.${system} = {
        hyprland-focus = hyprlandFocus;
        default = hyprlandFocus;
      };
      checks.${system}.hyprland-focus = hyprlandFocus;
      devShells.${system}.default = pkgs.mkShell {
        packages = [ pkgs.cargo pkgs.rustc pkgs.rustfmt pkgs.clippy ];
      };
    };
}
