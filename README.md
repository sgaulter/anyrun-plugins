# anyrun-plugins

Custom anyrun plugins. `hyprland-focus` searches and focuses Hyprland windows by title or application class.

## Building

From the repository directory, test and build all plugins:

```sh
cargo test --locked --workspace
cargo build --locked --release --workspace
```

To select one plugin, replace `--workspace` with `-p <package-name>`.
Libraries are written to `target/release/lib<crate_name>.so`, with hyphens replaced by underscores.

Build a plugin with Nix:

```sh
nix build '.#<plugin-name>' -L
```

For example, `nix build .#hyprland-focus -L` produces `result/lib/libhyprland_focus.so`.
The flake currently supports `x86_64-linux`.

## Installing with Home Manager

Add this repository as an input to your configuration's `flake.nix`:

```nix
inputs.anyrun-plugins = {
  url = "github:sgaulter/anyrun-plugins";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

Add the package to your anyrun plugin list:

```nix
programs.anyrun.config.plugins = [
  inputs.anyrun-plugins.packages.${system}.hyprland-focus
];
```

## Configuring hyprland-focus

Optional configuration (defaults shown):

```nix
programs.anyrun.extraConfigFiles."hyprland-focus.ron".text = ''
  Config(
      prefix: ":w ",
      max_entries: 10,
  )
'';
```

Set `prefix: ""` to search windows without a prefix.
Open anyrun inside Hyprland and type `:w ` followed by a window title or application class.
Select a result to focus that window.
