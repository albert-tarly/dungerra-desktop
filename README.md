# Dungerra Desktop

The desktop app for [Dungerra](https://github.com/albert-tarly/dungerra): play solo with no internet connection, open your game to players on your local network, or join theirs. It is built for Windows (x64) and macOS (universal: Apple Silicon and Intel).

This repo contains only the app shell. The game comes from two other repos, built at release time:

- [`dungerra-api`](https://github.com/albert-tarly/dungerra-api): the game server, run in desktop mode
- [`dungerra`](https://github.com/albert-tarly/dungerra): the web client, built with `npm run build:desktop`

Players: see the [Desktop App guide](https://github.com/albert-tarly/dungerra-api/blob/main/docs/user/desktop-app.md) for installing, opening a game to LAN and joining.

The design and its trade-offs are in [`dungerra-api/docs/decisions/desktop-version.md`](https://github.com/albert-tarly/dungerra-api/blob/main/docs/decisions/desktop-version.md).

## How it works

```
Dungerra app (Tauri v2)
 ├─ start page (launcher/): starts the server, then opens the game
 ├─ dungerra-server: the bundled game server, on 127.0.0.1 only
 │     serves the API, the WebSocket and the web client from one address
 │     "Open to LAN" (game menu): serves one game on a second port, on the network
 └─ the window shows the game:
       own games      → http://127.0.0.1:47823/
       Join LAN Game  → http://<host-ip>:<port>/  (the host's server and client)
```

- **Everything after start happens in the game client.** Solo play is your own game list. **Open to LAN** in the game menu opens the game you are in. **Join LAN Game** in the game list joins someone else's.
- **Open to LAN is per game.** Guests can sign in and play only that game. Leaving it (or **Stop Hosting**) closes the port and disconnects them. The server implements this (`-allow-lan`, `/api/v1/lan`).
- **Joining players load the client from the host's server.** Client and server versions therefore always match, and a player can also join from any web browser at the host's address.
- **Sign-in is a name or Google** on whichever server you play on. Each server keeps its own characters and leaderboard.
  - **A name** (`NAME_AUTH_ENABLED`) has no password. It is refused while someone plays with it. Joining from the app pre-fills it on the host.
  - **Google** runs in the system browser. The shell opens `accounts.google.com` there instead of in the window, and the local server receives Google's redirect on `127.0.0.1` (`GOOGLE_OAUTH_FLOW=loopback`).
- **Google sign-in only:** a host can require Google for a LAN game. Joining from the app then signs in with Google, carrying the host's one-time nonce, and hands the ID token to the host.
- **The local server uses port 47823** when it is free. The client stores its sign-in per address, so a fixed port keeps you signed in between launches.
- **Going back:** a guest who joined from the app gets **Main Menu** buttons back to their own server (the client passes `?home=<own origin>` to the host). **Game → Main Menu** (Ctrl/Cmd+Shift+L) opens the own game list from anywhere. Game pages have no access to the app itself.
- **Fonts:** the start page bundles the client's fonts (`launcher/fonts/`, SIL Open Font License) so it matches the game offline.
- **Shutdown:** closing the app closes the server's stdin, and the server disconnects LAN guests and saves every session before it exits.
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
| `launcher/` | Start page: plain HTML/CSS/JS, served from the app bundle |
| `src-tauri/src/main.rs` | Commands for the start page, the menu, and stopping the server on exit |
| `src-tauri/src/server.rs` | Starting and stopping the game server (the sidecar contract) |
| `src-tauri/tauri.conf.json` | Window, bundle, sidecar and resources |
| `src-tauri/Info.plist` | macOS local-network permission text and ATS exception for LAN http |
| `scripts/` | Server and client build scripts |

### Google sign-in (OAuth client)

Google sign-in needs a Google OAuth client of type **Desktop app**. It is baked into the app at build time from two environment variables. Without them, the app offers name sign-in only.

1. In [Google Cloud Console](https://console.cloud.google.com/) → **APIs & Services → Credentials**, choose **Create credentials → OAuth client ID → Desktop app**. Desktop clients accept loopback redirects on any port, so there are no redirect URIs to enter. The OAuth consent screen must exist; while it is in *Testing*, only listed test users can sign in.
2. **For CI:** add the ID and secret as repository secrets `DUNGERRA_GOOGLE_CLIENT_ID` and `DUNGERRA_GOOGLE_CLIENT_SECRET`.
3. **For local builds:** set the same names as environment variables before `npm run dev` / `npm run build`.

Google does not treat a desktop client's secret as confidential, because it ships inside every installer. Don't reuse the web version's client.

## Releases

`.github/workflows/build.yml` builds on every push and pull request:

- the Windows NSIS installer
- the macOS universal `.dmg`

By default, CI builds the client from `dungerra` `main` and the server at the commit pinned in the client's `package-lock.json`, so their protocol types match. CI fails if that server commit predates `-allow-lan`. Run the workflow manually to pick other refs.

**One version for everything.** The server, the web client and the app share one version, and dungerra-api's `package.json` holds it. `scripts/tauri.mjs` (used by `npm run dev`/`build` and CI) gives the app the version of the dungerra-api it bundles. The version fields in this repo stay `0.0.0`.

To release:
1. In dungerra-api, bump `version` in `package.json` and `GameVersion` in `internal/config/config.go` (a test keeps them equal), and date the `CHANGELOG.md` section. Push.
2. In dungerra, set the same `version` in `package.json`, run `npm update dungerra-api`, and push.
3. Here, push the tag `v<version>`. CI fails if the tag doesn't match the bundled dungerra-api version, and otherwise attaches the installers to a draft GitHub Release.

The builds are **not code-signed** yet, so players see a warning the first time they open the app:

- **Windows (SmartScreen):** click **More info → Run anyway**.
- **macOS (Gatekeeper):** right-click the app and choose **Open**, or allow it under **System Settings → Privacy & Security**. If macOS says the app is damaged, run `xattr -dr com.apple.quarantine /Applications/Dungerra.app` in Terminal.

## Opening a game to LAN: things players hit

- **Windows Firewall:** the first time a game is opened to LAN, Windows asks whether `dungerra-server` may accept connections. Allow it. On a network marked **Public**, nobody can join until you allow it. If you dismissed the prompt, allow the app under **Windows Security → Firewall → Allow an app through firewall**.
- **Third-party firewalls** (ESET, Norton, Kaspersky, Bitdefender…) block guests on their own, even when Windows allows them. Trust the home network in them, or allow `dungerra-server`.
- **macOS:** asks for permission to find devices on your local network. Allow it, or nobody can join.
- **Same network only:** joining works only on the same network. Guest Wi-Fi often isolates devices from each other. Internet play needs port forwarding (plain http, see the player guide) or a virtual LAN such as Tailscale.
