# OpenFortiVpn Connect

A lightweight, open-source desktop GUI client for [openfortivpn](https://github.com/adrienverge/openfortivpn).

![OpenFortiVpn Connect](docs/screenshots/openfortivpn-connect.png)

## Why?

If you've ever used the official FortiClient on macOS, you know the pain: bloated, buggy, and unreliable. Constant crashes, broken updates, and features you never asked for getting in the way of the one thing you need — a VPN connection.

I got tired of depending on it. So I built **OpenFortiVpn Connect** — a minimal, native macOS GUI that wraps the excellent [openfortivpn](https://github.com/adrienverge/openfortivpn) CLI tool. No telemetry, no bloat, no surprises. Just a clean interface to manage your VPN profiles and connect.

The UI is inspired by [OpenVPN Connect](https://openvpn.net/client/) — simple, focused, and easy to use. Built with native macOS vibrancy for a modern frosted glass look that blends with your desktop.

## Features

- **Multiple VPN profiles** — save and switch between different VPN connections
- **SAML authentication** — full support for SSO/SAML login flows
- **Password auth** — credentials stored securely in the macOS Keychain
- **Certificate trust management** — pin server certificates by SHA256 digest
- **Native macOS look** — vibrancy blur, overlay title bar, system tray integration
- **System tray** — connect/disconnect and switch profiles without opening the app
- **Real-time logs** — monitor connection logs with optional debug mode
- **Lightweight** — small binary, minimal resource usage

## Platform Support

- `macOS`: original upstream target, with native Keychain, helper, and DNS integration
- `Linux/Fedora`: experimental port in progress, using `pkexec` and the system `openfortivpn`

## Installation

### Fedora

Install the runtime packages:

```bash
sudo dnf install openfortivpn polkit
```

For development and local builds, install the toolchain:

```bash
sudo dnf install gcc gcc-c++ make pkg-config openssl-devel webkit2gtk4.1-devel \
  javascriptcoregtk4.1-devel libsoup3-devel gtk3-devel libappindicator-gtk3-devel
```

The Linux port uses `pkexec` for privilege escalation when connecting and disconnecting. The app prefers the bundled `openfortivpn` binary shipped inside the package and falls back to `/usr/local/bin/openfortivpn` or `/usr/bin/openfortivpn` when running from the source tree.

### Homebrew

```bash
brew tap walcew/tap
brew install --cask openfortivpn-connect
```

This installs the macOS build and the openfortivpn dependency.

### Manual

Download the `.dmg` for your architecture from [Releases](https://github.com/walcew/openfortivpn-connect/releases):

- **Apple Silicon (M1/M2/M3/M4):** `OpenFortiVpn.Connect_x.x.x_aarch64.dmg`
- **Intel:** `OpenFortiVpn.Connect_x.x.x_x64.dmg`

You will also need to install openfortivpn manually:

```bash
brew install openfortivpn
```

## Prerequisites

- macOS 12 (Monterey) or later
- [openfortivpn](https://github.com/adrienverge/openfortivpn)

For the Linux/Fedora port:

- Fedora with a graphical Polkit agent available
- `openfortivpn` built in `src-tauri/openfortivpn/openfortivpn`, or installed at `/usr/local/bin/openfortivpn` or `/usr/bin/openfortivpn`
- `pkexec` available for privileged launch

## Building from Source

### Requirements

- [Rust](https://rustup.rs/) (1.77.2+)
- [Node.js](https://nodejs.org/) (18+)
- [Tauri CLI](https://tauri.app/)

### Build

```bash
npm install
npm run build:openfortivpn
cargo tauri build
```

The built `.app` will be in `src-tauri/target/release/bundle/macos/`.

### Fedora RPM

To generate an RPM bundle on Fedora:

```bash
npm install
npm run build:openfortivpn
cargo tauri build --bundles rpm
```

The RPM will be written under `src-tauri/target/release/bundle/rpm/`.

If you are packaging for installation outside the source tree, the bundle already includes:

- the embedded `openfortivpn` binary
- the Linux helper used for `pkexec` and DNS handling
- the Tauri desktop application

### Development

```bash
cargo tauri dev
```

## Tech Stack

- **Backend:** [Tauri v2](https://tauri.app/) + Rust
- **Frontend:** React 19 + TypeScript + Tailwind CSS v4
- **VPN engine:** [openfortivpn](https://github.com/adrienverge/openfortivpn) (CLI)
- **Security:** macOS Keychain on macOS, local config-backed storage on Linux
- **DNS:** Native macOS `scutil` integration on macOS, native `openfortivpn` handling on Linux

## Credits

- [openfortivpn](https://github.com/adrienverge/openfortivpn) by Adrien Vergé — the open-source CLI tool that makes this project possible
- [OpenVPN Connect](https://openvpn.net/client/) — UI inspiration for the clean, profile-based interface
- [Tauri](https://tauri.app/) — the framework powering this native desktop app

## Disclaimer

This project is **not affiliated with, endorsed by, or associated with Fortinet, FortiClient, FortiGate, or FortiVPN** in any way. These are registered trademarks of Fortinet, Inc. This is an independent, open-source project that provides a graphical interface for the community-driven [openfortivpn](https://github.com/adrienverge/openfortivpn) tool.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Author

**Wallacy Santos Ferreira** — [@walcew](https://github.com/walcew)
