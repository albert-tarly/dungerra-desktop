// Launcher: picks solo, host or join, then hands the window to the game
// client served by a game server. Runs only inside the app (window.__TAURI__
// comes from "withGlobalTauri" in tauri.conf.json).
"use strict";

const invoke = window.__TAURI__.core.invoke;

const $ = (id) => document.getElementById(id);
const statusEl = $("status");
const buttons = Array.from(document.querySelectorAll("button"));

// Remembered per install for convenience; the launcher works without them
const STORAGE_HOST_PORT = "launcher.hostPort";
const STORAGE_JOIN_ADDRESS = "launcher.joinAddress";

function load(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function save(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage unavailable; nothing to remember
  }
}

function setStatus(message, isError = false) {
  statusEl.textContent = message;
  statusEl.classList.toggle("error", isError);
}

function setBusy(busy) {
  for (const button of buttons) {
    button.disabled = busy;
  }
}

// Runs one launcher action at a time and reports its error, if any
async function run(busyMessage, action) {
  setBusy(true);
  setStatus(busyMessage);
  try {
    await action();
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

$("solo").addEventListener("click", () =>
  run("Starting your world…", async () => {
    const started = await invoke("start_server", { mode: "solo" });
    await invoke("open_game", { url: started.url });
  }),
);

let hostUrl = null;

$("host-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const port = Number($("host-port").value);
  run(`Starting a LAN game on port ${port}…`, async () => {
    const started = await invoke("start_server", { mode: "host", port });
    save(STORAGE_HOST_PORT, String(port));
    hostUrl = started.url;

    const list = $("share-list");
    list.replaceChildren();
    const urls = started.shareUrls.length
      ? started.shareUrls
      : [`(no network found) port ${port}`];
    for (const url of urls) {
      const item = document.createElement("li");
      item.textContent = url.replace(/^http:\/\//, "").replace(/\/$/, "");
      list.append(item);
    }
    $("host-info").hidden = false;
    $("host-form").hidden = true;
    setStatus("");
  });
});

$("enter-host").addEventListener("click", () =>
  run("Opening the game…", () => invoke("open_game", { url: hostUrl })),
);

$("stop-host").addEventListener("click", () =>
  run("Stopping…", async () => {
    await invoke("stop_server");
    hostUrl = null;
    $("host-info").hidden = true;
    $("host-form").hidden = false;
    setStatus("Stopped hosting. Your world is saved.");
  }),
);

$("join-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const address = $("join-address").value;
  run("Looking for the host…", async () => {
    const host = await invoke("probe_host", { address });
    save(STORAGE_JOIN_ADDRESS, address.trim());
    // A joining player needs no local server
    await invoke("stop_server");
    setStatus(`Found a game (version ${host.gameVersion}). Joining…`);
    await invoke("open_game", { url: host.url });
  });
});

async function init() {
  const info = await invoke("app_info");
  $("version").textContent = `Dungerra desktop ${info.version}`;
  $("host-port").value = load(STORAGE_HOST_PORT) ?? String(info.defaultHostPort);
  $("join-address").value = load(STORAGE_JOIN_ADDRESS) ?? "";
  // Back from a game: the server was stopped, start fresh
  await invoke("stop_server");
}

init().catch((error) => setStatus(String(error), true));
