# anyrun-plugins

Custom anyrun plugins:

- `hyprland-focus`: search and focus Hyprland windows by title or application class.
- `multi-web`: search the web or open bookmarks using independently configured prefixes.

## Building

From the repository directory, test and build all plugins:

```sh
cargo test --locked --workspace
cargo build --locked --release --workspace
```

To select one plugin, replace `--workspace` with `-p <package-name>`. Libraries are written to
`target/release/lib<crate_name>.so`, with hyphens replaced by underscores.

Build a plugin with Nix:

```sh
nix build '.#<plugin-name>' -L
```

For example, `nix build .#hyprland-focus -L` produces `result/lib/libhyprland_focus.so`. The flake currently supports
`x86_64-linux`.

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
  inputs.anyrun-plugins.packages.${system}.multi-web
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

Set `prefix: ""` to search windows without a prefix. Open anyrun inside Hyprland and type `:w ` followed by a window
title or application class. Select a result to focus that window.

## Configuring multi-web

Configure engines in `multi-web.ron`. No engines are enabled by default.

```nix
programs.anyrun.extraConfigFiles."multi-web.ron".text = ''
  Config(
      command: ["xdg-open"],
      engines: [
          (
              name: "Kagi",
              prefix: "?k",
              url: "https://kagi.com/search?q={}",
          ),
          (
              name: "Glean",
              prefix: "?g",
              url: "https://app.glean.com/search?q={}",
          ),
          (
              name: "GitHub",
              prefix: "gh",
              url: "https://github.com",
          )
      ],
  )
'';
```

If the URL contains `{}`, it is replaced with the URL-encoded query. Without `{}`, the entry is a bookmark: typing its
prefix offers the site, and any text after the prefix is ignored. The optional `command` is an executable followed by
arguments; it defaults to `["xdg-open"]`. For example, `command: ["firefox", "--new-tab"]` opens destinations in
Firefox. The destination is appended as one literal argument, with no shell expansion. The executable must be on
anyrun's PATH or specified by absolute path.
