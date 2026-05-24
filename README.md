# cerbera

Kernel-level credential file protection for Linux.

Uses `fanotify` `FAN_OPEN_PERM` / `FAN_ACCESS_PERM` to block unauthorized process access to credential files — browser profiles, password manager databases, SSH keys, API keys — before any data is read.

**Status:** Early development (v0.1 — watch-only mode). Not yet recommended for production.

## Requirements

- Linux kernel 5.1+
- `CAP_SYS_ADMIN` or root
- Rust 1.85+

## Quick start

```bash
cargo install cerbera
sudo cerbera run --config ~/.config/cerbera/cerbera.toml
```

## Configuration

`~/.config/cerbera/cerbera.toml`:

```toml
[[watch]]
name = "ssh-keys"
path = "~/.ssh"
allow_processes = ["/usr/bin/ssh", "/usr/bin/ssh-agent", "/usr/bin/git"]
recursive = true

[[watch]]
name = "browser-profile"
path = "~/.config/chromium/Default"
allow_processes = ["/usr/bin/chromium"]
recursive = true
```

`allow_processes` entries are full executable paths matched against `/proc/PID/exe`.
Use `readlink /proc/$$/exe` or `which <cmd>` to find the correct path on your system.

## License

[MIT](LICENSE)