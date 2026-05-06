# hop

**Fuzzy directory jumper for your terminal.**

Type a fragment, hit enter, go anywhere. `hop` learns your habits and gets smarter over time.

```
~ $ h dl
~/Downloads

~/Downloads $ h proj
~/code/work/project
```

No more `cd ../../../long/path`. No more memorizing aliases.

## Install

```bash
curl -fsSL https://codeberg.org/dioxus/hop/raw/branch/main/install.sh | bash
```

Then add shell integration:

| Shell    | Add this to your config file              |
|----------|-------------------------------------------|
| Bash     | `eval "$(hop init bash)"`                 |
| Zsh      | `eval "$(hop init zsh)"`                  |
| Fish     | `hop init fish \| source`                 |
| Nushell  | `hop init nushell \| save ~/.local/bin/hop.nu` then `use ~/.local/bin/hop.nu` in config.nu |
| Elvish   | `hop init elvish > ~/.config/hop/hop.elv` then `use ~/.config/hop/hop.elv` in rc.elv |

Restart your shell. That's it — `h` is now a function that jumps to directories.

## Quick usage

```
h proj          → jump to best match from history
hop /tmp        → literal path works too
hop             → open the interactive picker
hop book w ~/code/work   → bookmark a directory
h w             → jump to bookmark
```

## Documentation

All the details are in the wiki:

- [Installation](https://codeberg.org/dioxus/hop.wiki/wiki/Installation) — one-liner, from source, self-update, uninstall
- [Shell Setup](https://codeberg.org/dioxus/hop.wiki/wiki/Shell-Setup) — bash, zsh, fish, nushell, elvish
- [Usage](https://codeberg.org/dioxus/hop.wiki/wiki/Usage) — all commands with examples
- [Configuration](https://codeberg.org/dioxus/hop.wiki/wiki/Configuration) — config.toml reference
- [Importing](https://codeberg.org/dioxus/hop.wiki/wiki/Importing) — migrate from zsh/fasd/autojump/zoxide
- [Troubleshooting](https://codeberg.org/dioxus/hop.wiki/wiki/Troubleshooting) — common issues and fixes
- [Changelog](https://codeberg.org/dioxus/hop.wiki/wiki/Changelog) — release notes

## Verify

```bash
hop doctor
```

## Data

- **macOS**: `~/Library/Application Support/hop/hop.db`
- **Linux**: `~/.local/share/hop/hop.db`

## License

MIT
