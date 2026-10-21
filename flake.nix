{
  description = "Custom anyrun plugins";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      hyprlandFocus = pkgs.callPackage ./nix/hyprland-focus.nix { };
      multiWeb = pkgs.callPackage ./nix/multi-web.nix { };
    in {
      packages.${system} = {
        hyprland-focus = hyprlandFocus;
        multi-web = multiWeb;
        default = hyprlandFocus;
      };
      checks.${system}.hyprland-focus = hyprlandFocus;
      checks.${system}.multi-web = multiWeb;
      devShells.${system}.default = pkgs.mkShell {
        packages = [ pkgs.cargo pkgs.rustc pkgs.rustfmt pkgs.clippy ];
      };
    };
}
