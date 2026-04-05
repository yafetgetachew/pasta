<div align="center">

# 🍝 Pasta

**The clipboard manager for devs and devops.**

A blazing-fast, Spotlight-style clipboard launcher built with Rust and [GPUI](https://gpui.rs).  
Paste smarter — search, transform, parametrize, and organize everything you copy.

[![macOS](https://img.shields.io/badge/macOS-only-000?logo=apple&logoColor=white)](#)
[![Built with Rust](https://img.shields.io/badge/Built_with-Rust-dea584?logo=rust)](#)
[![License: MIT](https://img.shields.io/badge/License-MIT-22d3ee.svg)](LICENSE)

</div>

<br>

<p align="center">
  <img src="docs/screenshots/main.png" width="720" alt="Pasta — clipboard launcher overview" />
</p>

<br>

## Features

### Instant Search
Type to fuzzy-search your entire clipboard history — text, commands, JSON, YAML, URLs, certs, and more. Results with smart content-type badges.

### Neural Search
Pasta ships with an on-device embedding model. Search by *meaning*, not just keywords. Type `"k8s restart command"` and find that `kubectl rollout restart` you copied last week.

### Syntax Highlighting
Code snippets are automatically highlighted with full syntax coloring for Bash, JSON, YAML, TOML, Python, Rust, Go, SQL, and [more](https://github.com/sublimehq/Packages).

<p align="center">
  <img src="docs/screenshots/syntax-highlighting.png" width="720" alt="Syntax highlighting and parametrization" />
</p>

### ⚡ Transforms
One-key transforms on any clipboard item. Open the transform menu with `Tab`, then press a shortcut:

| Key | Transform | Description |
|:---:|-----------|-------------|
| `s` | Shell quote | Wraps in single quotes, escapes inner quotes |
| `j` / `J` | JSON encode / decode | String-escapes or unescapes JSON |
| `f` / `F` | JSON pretty / minify | Pretty-print or compact JSON |
| `u` / `U` | URL encode / decode | Percent-encodes or decodes URLs |
| `b` / `B` | Base64 encode / decode | Standard + URL-safe auto-detection |
| `t` | JWT decode | Decodes header & payload, shows expiry status |
| `e` | Epoch decode | Unix timestamp ↔ human date (seconds, ms, ISO) |
| `h` | SHA256 hash | Computes the SHA-256 hex digest |
| `c` | Count stats | Lines, words, chars, bytes |
| `p` | Cert info | Parses PEM/DER certificates via OpenSSL |

<p align="center">
  <img src="docs/screenshots/transforms.png" width="720" alt="Transforms menu" />
</p>

### Parametrize Snippets
Turn any copied command into a reusable template:
- Select a snippet → `Cmd+P` → click values to mark as `{{parameters}}`
- **Smart sub-split**: `Cmd+click` a token like `deployment/checkout-api` to expand it into `deployment` and `checkout-api`, so you can parametrize just the part you need
- Fill parameters on paste — Pasta prompts you for each value

### Pasta Bowls
Organize snippets into **bowls** (tagged collections). Export and import bowls as YAML files to share with your team.

### Secrets
Mark any item as a secret. Secrets are encrypted at rest with AES-256-GCM, stored in the macOS Keychain, and masked in the UI until revealed.

### Native macOS
- Global hotkey: **`Option + Space`**
- Glassmorphic UI with dark/light mode auto-detection
- System tray with menu
- Launch at login via LaunchAgent
- Zero Electron, zero web views — pure native rendering - ZED TEAM FTW!

<br>

## Getting Started

### Prerequisites

- updated macOS
- [Rust](https://rustup.rs/) (stable)
- Xcode Command Line Tools (`xcode-select --install`)

### Run from Source

```bash
git clone https://github.com/yafetgetachew/pasta.git
cd pasta
cargo run
```

### Install as App

```bash
./scripts/install-macos-app.sh
```

This builds a release binary, creates `Pasta.app`, and installs it into `/Applications` (or `~/Applications` if not writable).

You could also just download the bindary from the releases (left panel of GitHub)

<br>

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Option + Space` | Toggle Pasta |
| `Enter` | Copy selected item to clipboard |
| `Tab` | Open transforms menu |
| `Cmd + P` | Parametrize snippet |
| `Cmd + B` | Assign bowl |
| `Cmd + S` | Mark as secret |
| `Cmd + I` | Edit description |
| `Cmd + H` | Toggle command help |
| `Cmd + D` | Delete item |
| `↑` / `↓` | Navigate items |
| `Esc` | Close Pasta |

<br>

## 🏗 Architecture

```
src/
├── main.rs            # App entry, hotkey registration, event loop
├── app/
│   ├── actions.rs     # All user actions and state mutations
│   ├── view.rs        # GPUI rendering and layout
│   ├── query_input.rs # Text input handling and IME support
│   └── state.rs       # Application state definitions
├── storage.rs         # SQLite persistence and neural search
├── transforms/
│   └── mod.rs         # All transform implementations
└── platform/
    └── macos/         # macOS-specific: tray, hotkey, file dialogs
```

**Key dependencies:**
- [GPUI](https://gpui.rs) — GPU-accelerated native UI framework
- [rusqlite](https://github.com/rusqlite/rusqlite) — SQLite for clipboard storage
- [fastembed](https://github.com/anush008/fastembed-rs) — On-device neural embeddings
- [syntect](https://github.com/trishume/syntect) — Syntax highlighting
- [global-hotkey](https://github.com/nicegui-dev/global-hotkey) — System-wide hotkey capture

<br>

## 🤝 Contributing

Contributions are welcome! Here's how:

1. **Fork** the repository
2. **Create a branch** for your feature or fix
   ```bash
   git checkout -b feature/your-feature
   ```
3. **Make your changes** — try to follow the existing code style (Rust 2024 edition, `#[cfg(target_os = "macos")]` guards on platform code)
4. **Test manually** using the [smoke test checklist](SMOKE_TEST_CHECKLIST.md)
5. **Submit a pull request** with a clear description

### Ideas for contributions
- 🐧 Linux support (Wayland/X11 clipboard, tray, hotkey) I might create a separate repo
- 🪟 Windows support - Not planning to do it now, but if you do let me know!
- 📋 Image clipboard support - I'm not sure if it's necessary, but if you think it's a good idea and if the UI/UX is bearable, please go ahead!
- 🔗 More transforms (hex encode/decode, regex extract, markdown strip and any other cool ideas)
- 🧪 Automated test suite
- 🌍 i18n / localization

<br>

## 📄 License

MIT License — free to use, modify, and distribute. Just keep the attribution (Yafet Getachew - mailofyafet@gmail.com - @YafetGetch on X *formerly Twitter).

See [LICENSE](LICENSE) for the full text.

**Made by [Yafet Getachew](https://github.com/yafetgetachew)**

<br>

<div align="center">
  <sub>Built with 🦀 Rust + ❤️ for the terminal-loving crowd</sub>
</div>
