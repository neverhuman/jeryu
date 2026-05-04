# JeRyu Install Guide

`jeryu install` is a Rust-first guided installer for Linux and macOS. It installs the current `jeryu` binary into a user-space prefix by default and verifies the result before it exits.

## Local install

```bash
cargo run -p jeryu -- install --yes
```

What it does by default:

- installs to `~/.jeryu/bin/jeryu`
- creates the prefix if needed
- replaces the binary atomically
- verifies `jeryu --version`
- does not edit shell startup files
- does not require `sudo`

Useful flags:

- `--color auto|always|never`
- `--interactive auto|always|never`
- `--path-mode advise|update|skip`
- `--verbose`

PATH handling:

- `advise` prints a shell-specific snippet if the prefix is not already on `PATH`
- `update` appends a guarded block to the supported shell rc file
- `skip` leaves shell startup files untouched

Supported rc files:

- bash: `~/.bashrc`
- zsh: `~/.zshrc`
- fish: `~/.config/fish/config.fish`

The installer never duplicates the `jeryu` PATH block.

## Server bootstrap

```bash
cargo run -p jeryu -- install server --yes
```

Server mode verifies Docker before it runs `jeryu init`. On Linux, `--install-deps --allow-sudo` allows Docker package installation when Docker is missing. On macOS, the installer explains the missing prerequisite instead of trying to install Docker automatically.

## Remote install

```bash
cargo run -p jeryu -- remote install xbabe1 --setup-key --yes
```

Remote install:

- checks local `ssh` and `ssh-keygen`
- preflights the target host over SSH
- uploads the current `jeryu` binary
- verifies `--version` on the remote host
- saves `~/.jeryu/remotes/<alias>.toml` after verification

Remote service modes:

- `auto` uses a user systemd unit when available
- `user` requires user systemd
- `manual` skips unit installation and prints manual `serve` guidance

## Dry run

```bash
cargo run -p jeryu -- install --dry-run --json
cargo run -p jeryu -- remote install xbabe1 --dry-run --json
```

Dry runs emit the full plan without mutating the machine or the remote host.

## Troubleshooting

- Use `--verbose` to show the exact commands being run.
- If the binary is installed but not on `PATH`, use `--path-mode update` or copy the printed shell snippet manually.
- If remote install cannot enable a service automatically, run `jeryu remote status <alias>` and use the printed manual guidance.
