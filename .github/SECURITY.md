# Security Policy

cc-monitor is a **read-only** desktop renderer of Claude Code CLI output, with an
optional SSH remote mode. We take security seriously despite the limited threat surface.

## Reporting a vulnerability

Please report security issues **privately** — do **not** open a public issue until a
fix is available:

- Open a [GitHub security advisory](https://github.com/bo0Zeng/cc-monitor/security/advisories/new) (preferred), **or**
- Email the maintainer (see the address in the git commit history / `git log`).

We aim to acknowledge reports within a few days.

## Scope notes

- **Zero-intrusion** (INVARIANT §1): the app never modifies Claude Code files, except
  two user-initiated remote writes — deploying/uninstalling the remote daemon, and
  explicit history deletion.
- **SSH host keys** use trust-on-first-use (TOFU) by default; the first connection is
  MITM-capable. The UI warns loudly and offers one-click fingerprint pinning — pre-share
  or pin a `SHA256:` fingerprint for sensitive setups.
- **Untrusted content** (CLI / model output) is sanitized with DOMPurify before any
  `innerHTML` rendering.
- **Releases are unsigned.** Verify the published `SHA256SUMS.txt` against your download.
- Dependency advisories: CI gates `npm audit` on **production** deps (`--omit=dev`). For
  Rust, run `cargo audit` locally. Known **accepted residuals** (no actionable upstream fix,
  low risk for this app):
  - `rsa` **RUSTSEC-2023-0071** (Marvin Attack — RSA decryption timing sidechannel): pulled
    transitively by `russh`; no patched `rsa` release exists upstream. Only used for SSH
    host-key / auth against **user-controlled** hosts in a read-only monitor → minimal exposure.
  - **gtk-rs** crates (`atk` / `gdk` / `gtk` …) flagged *unmaintained*: these are Tauri's
    **Linux** GUI bindings — present in `Cargo.lock` but **not compiled or shipped** on this
    Windows-only build.
