# cerbera

**Kernel-level guardian for your credential files on Linux.**

cerbera uses Linux `fanotify` permission events to block unauthorized process access to browser profiles, password-manager databases, SSH keys, and other credential stores — synchronously, before the attacker's `read()` returns.

```
$ sudo cerbera run --config /etc/cerbera/cerbera.toml
INFO  cerbera: watching name=chromium-profile path=/home/alice/.config/chromium/Default
INFO  cerbera: watching name=ssh-keys path=/home/alice/.ssh
WARN  cerbera: MODE: enforce — unauthorized access will be blocked (FAN_DENY)
WARN  cerbera::event: BLOCKED unauthorized access exe=Some("/usr/bin/node") pid=4121 path=…/Login Data
```

## The attack takes 200 milliseconds

A developer joins an online meeting. They're asked to clone a repo and run `npm install`. A post-install script silently reads `~/.config/chromium/Default/Login Data`, Firefox's `key4.db`, and `~/.ssh/id_ed25519` — and exfiltrates them before the meeting ends.

A warning popup is useless here. The credentials are already gone.

cerbera stops the read before it happens. The attacker's process receives `EPERM` — no byte leaves the file.

## Design

The traditional UNIX permission model trusts *users* as the boundary: any process running as `alice` can read any file owned by `alice`. That made sense in 1970s multi-user mainframes. It is too coarse for a 2020s developer workstation.

cerbera shifts the trust boundary to the *file side*: **a credential file may only be read by the processes that legitimately own it**. Firefox's profile is for Firefox. Your SSH keys are for `ssh`, `git`, and `ssh-agent`. A `node` process spawned by `npm install` has no legitimate reason to touch either — regardless of which UID it runs as.

This yields a single rule: **default deny, explicit allow**. Any process not named in the allow-list for a path receives `FAN_DENY`.

### Process identity

cerbera resolves `/proc/PID/exe` to get the **full executable path** of the accessing process and compares it against the allow-list. A binary named `firefox` at `/home/user/src/evil/firefox` is rejected even if the legitimate `/usr/bin/firefox` is allowed. Process names (`comm`) are 16-byte-truncated and trivially fakeable; `exe` paths are not.

### Integration with existing tools

`FAN_DENY` makes the blocked `open()` fail with `EPERM` — indistinguishable from `chmod 000`. Every UNIX tool that correctly handles "Permission denied" already knows what to do: skip and continue. No tool needs cerbera-specific code.

## Getting started

### Requirements

- Linux kernel 5.1+
- Rust 1.85+ (edition 2024)
- `CAP_SYS_ADMIN` or root (required by fanotify)

### Build

```bash
cargo build --release
sudo cp target/release/cerbera /usr/local/bin/cerbera
```

### 1. Write a base config

```toml
# /etc/cerbera/base.toml

[[watch]]
name = "chromium-profile"
path = "/home/alice/.config/chromium/Default"

[[watch]]
name = "ssh-keys"
path = "/home/alice/.ssh"
```

Paths must be absolute. `~` is not expanded (cerbera runs as root and has no single home directory).

### 2. Learn your access patterns

Run `cerbera learn` while using your system normally. It observes which processes access the watched paths and emits a config with `allow_processes` filled in.

```bash
sudo cerbera learn --config /etc/cerbera/base.toml --duration 300 --output /etc/cerbera/cerbera.toml
```

Review the output before proceeding. Remove any entry you don't recognize.

### 3. Run in enforce mode

```bash
sudo cerbera run --config /etc/cerbera/cerbera.toml
```

Unauthorized processes now receive `EPERM`. Use `--watch-only` to log violations without blocking while you're validating the config.

## Config reference

```toml
[[watch]]
name   = "chromium-profile"                          # label shown in logs
path   = "/home/alice/.config/chromium/Default"      # absolute path; file or directory
allow_processes = [                                  # full /proc/PID/exe paths allowed
  "/usr/bin/chromium",
  "/usr/bin/google-chrome",
]
recursive = true                                     # mark subdirectories (default: false)
```

`allow_processes` contains full executable paths. An empty list means **no process** may access the path in enforce mode. Use `readlink /proc/$$/exe` or `which <cmd>` to find the correct path on your system.

If a `path` does not exist at startup, cerbera logs a warning and skips that entry — it does not abort. This lets you load example or shared configs that include paths not present on every machine.

## Commands

| Command | Description |
|---|---|
| `cerbera run -c <config> [-c <config>...]` | Start enforcing. Blocks unauthorized access by default. |
| `cerbera run -c <config> --watch-only` | Log violations only — do not block. |
| `cerbera learn -c <config> [-c <config>...] [-d <secs>] [-o <file>]` | Observe accesses and generate an allow-list config. |

## Composing multiple config files

`--config` (`-c`) can be passed multiple times. cerbera merges all files before starting:
- Watch entries with the **same path** are merged: `allow_processes` lists are unioned and deduplicated, `recursive = true` in any file wins.
- Watch entries with **different paths** are kept as separate watches.

This lets you separate OS-level presets from personal paths:

```
/etc/cerbera/
  debian.toml     ← system paths, maintained by you or shared
  home.toml       ← personal credential paths, portable across machines
```

```toml
# debian.toml — which binaries are the legitimate owners on this distro
[[watch]]
name = "ssh-keys"
path = "/home/alice/.ssh"
allow_processes = ["/usr/bin/ssh", "/usr/bin/ssh-agent", "/usr/bin/git", "/usr/bin/scp"]

# home.toml — what paths to protect (reusable on any machine)
[[watch]]
name = "ssh-keys"
path = "/home/alice/.ssh"
```

```bash
sudo cerbera run \
  --config /etc/cerbera/debian.toml \
  --config /home/alice/.config/cerbera/home.toml
```

The two `ssh-keys` entries are merged: `home.toml` declares the path to watch, `debian.toml` contributes the allowed processes. On a different distro (e.g. NixOS), swap `debian.toml` for `nixos.toml` — `home.toml` stays unchanged.

## Example configs

The [`examples/`](examples/) directory contains ready-to-use presets:

| File | Description |
|---|---|
| [`debian.toml`](examples/debian.toml) | Debian / Ubuntu — system paths (`/etc/shadow`, `sudoers`, SSH host keys, TLS private keys) |
| [`fedora.toml`](examples/fedora.toml) | Fedora / RHEL / AlmaLinux / Rocky — same coverage, RHEL-specific paths (`/etc/pki/`) |
| [`arch.toml`](examples/arch.toml) | Arch Linux — all binaries under `/usr/bin/`; no sbin distinction |
| [`nixos.toml`](examples/nixos.toml) | NixOS — template with hash-qualified path placeholders; see note below |
| [`home.toml`](examples/home.toml) | Per-user paths: SSH keys, browser profiles, password managers, cloud credentials |

**Typical setup** — load an OS preset alongside your personal config:

```bash
sudo cerbera run \
  --config /etc/cerbera/debian.toml \
  --config /home/alice/.config/cerbera/home.toml
```

`allow_processes` entries from both files are merged per path, so OS-specific binary paths and personal path declarations stay in separate files.

> **NixOS note:** executable paths in `/nix/store/` include a content hash that changes on every `nixos-rebuild switch`. Glob support in `allow_processes` is not yet implemented — run `cerbera learn` after each rebuild to regenerate the allow-list, or update the paths manually. See [`examples/nixos.toml`](examples/nixos.toml) for the path template.

## Scope

**What cerbera protects against:**

- Post-install scripts (`npm`, `pip`, `cargo`, etc.) reading credential files
- Third-party CLI tools or scripts run from the terminal
- Untrusted binaries downloaded and executed locally
- IDE plugins or editor extensions accessing credential stores

**What cerbera does not protect against:**

- An attacker who has gained `CAP_SYS_ADMIN` or root — they can disable fanotify
- Process injection or `ptrace`-based theft from a legitimate process (see `yama` LSM / `ptrace_scope`)
- Browser extensions, phishing, or network-layer attacks — cerbera guards the filesystem boundary only

## Status

| Version | Feature | |
|---|---|---|
| v0.1 | Watch-only monitoring | ✓ released |
| v0.2 | `cerbera learn` — allow-list generation | ✓ released |
| v0.3 | Enforce mode (`FAN_DENY`) by default | ✓ released |
| v1.0 | Systemd integration, hardening, presets | planned |

> **cerbera is pre-v1.0 software.** Misconfiguration can block legitimate processes. Always validate with `--watch-only` before enabling enforce mode.

## Name

*Cerbera* is a genus named after Cerberus, the three-headed dog of Greek mythology that guards the underworld — ensuring nothing that belongs inside leaks out. cerbera does the same for your credential files.

## License

[MIT](LICENSE)
