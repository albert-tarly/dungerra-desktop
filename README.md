# Dungerra Desktop

The desktop app for [Dungerra](https://github.com/albert-tarly/dungerra): play solo with no internet connection, or host and join games on your local network. It is built for Windows (x64) and macOS (universal: Apple Silicon and Intel).

This repo contains only the app shell. The game comes from two other repos, built at release time:

- [`dungerra-api`](https://github.com/albert-tarly/dungerra-api): the game server, run in desktop mode
- [`dungerra`](https://github.com/albert-tarly/dungerra): the web client, built with `npm run build:desktop`

The design and its trade-offs are in [`dungerra-api/docs/decisions/desktop-version.md`](https://github.com/albert-tarly/dungerra-api/blob/main/docs/decisions/desktop-version.md).

## How it works

```
Dungerra app (Tauri v2)
 ├─ launcher page (launcher/): Solo | Host a LAN game | Join a LAN game
 ├─ dungerra-server: the bundled game server, started by the app
 │     serves the API, the WebSocket and the web client from one address
 └─ the window then opens the game from a server:
       Solo → http://127.0.0.1:47823/   (this computer only)
       Host → http://127.0.0.1:<port>/  (server open to the LAN)
       Join → http://<host-ip>:<port>/  (no local server)
```

- **Joining players load the client from the host's server.** Client and server versions therefore always match, and a player can also join from any web browser at the host's address.
- **Sign-in uses local accounts** (username and password) on whichever server you play on. Each server keeps its own accounts and leaderboard.
- **Solo play uses port 47823** when it is free. The client stores its sign-in per address, so a fixed port keeps you signed in between launches.
- **Game menu → Back to Launcher** (Ctrl/Cmd+Shift+L) stops the local server and returns to the launcher. Game pages have no access to the app.
- **Shutdown:** closing the app closes the server's stdin, and the server saves every session before it exits.
- **Your data** (database, secret key, `server.log` from the latest run) lives in the app data folder:
  - Windows: `%APPDATA%\io.github.albert-tarly.dungerra`
  - macOS: `~/Library/Application Support/io.github.albert-tarly.dungerra`

## Development

### Prerequisites

- **Node.js 26** (`.nvmrc`)
- **Go** (the version in `dungerra-api/go.mod`)
- **Rust (stable)**
- **Windows:** Visual Studio Build Tools with the C++ workload.
- **macOS:** Xcode Command Line Tools.
- **Sibling checkouts** of `dungerra` and `dungerra-api` (`../dungerra`, `../dungerra-api`). To use other locations, set `DUNGERRA_DIR` and `DUNGERRA_API_DIR`.

### Commands

```sh
npm install
npm run dev            # build server + client, then run the app in development mode (rebuilds on Rust changes)
npm run build          # build server + client, then the installer for this machine
npm run build:sidecar  # only the server  (-- --target <rust-triple>, e.g. universal-apple-darwin)
npm run build:web      # only the client  (-- --install to run npm ci in the client first)
npm run icons          # regenerate src-tauri/icons from the client's favicon
cd src-tauri && cargo test && cargo clippy --all-targets && cargo fmt --check
```

`build:sidecar` writes `src-tauri/binaries/dungerra-server-<triple>`, and `build:web` writes `src-tauri/resources/web/`. Both are generated, not committed. Run them again after changing the server or the client.

### Layout

| Path | What |
|---|---|
| `launcher/` | Launcher page: plain HTML/CSS/JS, served from the app bundle |
| `src-tauri/src/main.rs` | Commands for the launcher, menu, navigation, and stopping the server on exit |
| `src-tauri/src/server.rs` | Starting and stopping the game server (the sidecar contract) |
| `src-tauri/src/network.rs` | Parsing host addresses, probing a host, listing LAN addresses |
| `src-tauri/tauri.conf.json` | Window, bundle, sidecar and resources |
| `src-tauri/Info.plist` | macOS local-network permission text and ATS exception for LAN http |
| `scripts/` | Server and client build scripts |

## Releases

`.github/workflows/build.yml` builds on every push and pull request:

- the Windows NSIS installer
- the macOS universal `.dmg`

By default, CI builds the client from `dungerra` `main` and the server at the commit pinned in the client's `package-lock.json`, so their protocol types match. Run the workflow manually to pick other refs.

To release, bump `version` in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`, then push a `v*` tag. CI attaches the installers to a draft GitHub Release.

The builds are **not code-signed** yet, so players see a warning the first time they open the app:

- **Windows (SmartScreen):** click **More info → Run anyway**.
- **macOS (Gatekeeper):** right-click the app and choose **Open**, or allow it under **System Settings → Privacy & Security**. If macOS says the app is damaged, run `xattr -dr com.apple.quarantine /Applications/Dungerra.app` in Terminal.

## Hosting on a LAN: things players hit

- **Windows Firewall:** the first time you host, Windows asks whether `dungerra-server` may accept connections. Allow it. On a network marked **Public**, nobody can join until you allow it. If you dismissed the prompt, allow the app under **Windows Security → Firewall → Allow an app through firewall**.
- **macOS:** asks for permission to find devices on your local network. Allow it, or nobody can join.
- **Same network only:** joining works only on the same network. Guest Wi-Fi often isolates devices from each other.
