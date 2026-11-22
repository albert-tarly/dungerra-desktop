// Start page: starts the game server on this computer and hands the window
// to the game it serves. Solo play, opening a game to LAN and joining one all
// happen in the game. Runs only inside the app (window.__TAURI__ comes from
// "withGlobalTauri" in tauri.conf.json).
"use strict";

const invoke = window.__TAURI__.core.invoke;

const statusEl = document.getElementById("status");
const retryButton = document.getElementById("retry");

function setStatus(message, isError = false) {
  statusEl.textContent = message;
  statusEl.classList.toggle("error", isError);
}

async function start() {
  retryButton.hidden = true;
  setStatus("Starting your world…");
  try {
    await invoke("start_game");
  } catch (error) {
    setStatus(String(error), true);
    retryButton.hidden = false;
  }
}

retryButton.addEventListener("click", start);

async function init() {
  const info = await invoke("app_info");
  document.getElementById("version").textContent =
    `Dungerra desktop ${info.version}`;
  await start();
}

init().catch((error) => setStatus(String(error), true));
