// No console window next to the game in release builds on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod network;
mod server;

use std::sync::Mutex;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, Submenu};
use tauri::{AppHandle, Manager, RunEvent, Url, Wry};

use server::{Mode, Paths, ServerManager};

/// Solo games prefer a fixed port: the game client keeps its sign-in per
/// origin (host and port), so a stable port keeps the player signed in
/// between launches. Any free port is used if it is taken.
const SOLO_PORT: u16 = 47823;
const MAIN_WINDOW: &str = "main";
const APP_TITLE: &str = "Dungerra";
const MENU_BACK: &str = "back-to-launcher";
const MENU_FULLSCREEN: &str = "toggle-fullscreen";

#[derive(Default)]
struct AppState {
    server: ServerManager,
    /// The launcher page's own URL, to return to it from a game page.
    launcher_url: Mutex<Option<Url>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Started {
    /// Where this computer opens the game.
    url: String,
    /// Addresses other players use to join (hosting only).
    share_urls: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Probed {
    url: String,
    game_version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    default_host_port: u16,
}

fn server_paths(app: &AppHandle) -> Result<Paths, String> {
    // Tauri places the sidecar next to the app executable, without the
    // target-triple suffix it has in src-tauri/binaries
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let binary = exe
        .parent()
        .ok_or("The app executable has no folder")?
        .join(format!("dungerra-server{}", std::env::consts::EXE_SUFFIX));
    let paths = app.path();
    Ok(Paths {
        binary,
        data_dir: paths.app_data_dir().map_err(|e| e.to_string())?,
        web_dir: paths.resource_dir().map_err(|e| e.to_string())?.join("web"),
    })
}

fn set_title(app: &AppHandle, title: &str) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.set_title(title);
    }
}

fn start_server_blocking(
    app: &AppHandle,
    mode: &str,
    port: Option<u16>,
) -> Result<Started, String> {
    let paths = server_paths(app)?;
    let manager = &app.state::<AppState>().server;
    let (mode, port) = match mode {
        "solo" => {
            let port = match manager.start(&paths, Mode::Solo, SOLO_PORT) {
                Ok(port) => port,
                Err(e) if e.port_in_use => manager
                    .start(&paths, Mode::Solo, 0)
                    .map_err(|e| e.message)?,
                Err(e) => return Err(e.message),
            };
            (Mode::Solo, port)
        }
        "host" => {
            let port = port.unwrap_or(network::DEFAULT_LAN_PORT);
            if port < 1024 {
                return Err("Pick a port between 1024 and 65535".into());
            }
            let port = manager.start(&paths, Mode::Host, port).map_err(|e| {
                if e.port_in_use {
                    format!("Port {port} is already in use on this computer. Pick another port.")
                } else {
                    e.message
                }
            })?;
            (Mode::Host, port)
        }
        other => return Err(format!("Unknown mode {other:?}")),
    };

    let share_urls: Vec<String> = match mode {
        Mode::Solo => Vec::new(),
        Mode::Host => network::lan_addresses()
            .into_iter()
            .map(|ip| format!("http://{ip}:{port}/"))
            .collect(),
    };
    match (mode, share_urls.first()) {
        (Mode::Host, Some(url)) => set_title(
            app,
            &format!("{APP_TITLE} — hosting at {}", url.trim_end_matches('/')),
        ),
        (Mode::Host, None) => set_title(app, &format!("{APP_TITLE} — hosting on port {port}")),
        (Mode::Solo, _) => set_title(app, APP_TITLE),
    }
    Ok(Started {
        url: format!("http://127.0.0.1:{port}/"),
        share_urls,
    })
}

/// Starts the local game server: "solo" (this computer only) or "host" (LAN).
#[tauri::command]
async fn start_server(app: AppHandle, mode: String, port: Option<u16>) -> Result<Started, String> {
    tauri::async_runtime::spawn_blocking(move || start_server_blocking(&app, &mode, port))
        .await
        .map_err(|e| e.to_string())?
}

/// Stops the local game server, saving its sessions.
#[tauri::command]
async fn stop_server(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().server.stop();
        set_title(&app, APP_TITLE);
    })
    .await
    .map_err(|e| e.to_string())
}

/// Checks a LAN host before joining it.
#[tauri::command]
async fn probe_host(address: String) -> Result<Probed, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let url = network::parse_host_address(&address)?;
        let game_version = network::probe(&url)?;
        Ok(Probed {
            url: url.to_string(),
            game_version,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens the game client served at `url` (a local server or a LAN host).
#[tauri::command]
fn open_game(app: AppHandle, url: String) -> Result<(), String> {
    let url = Url::parse(&url).map_err(|e| e.to_string())?;
    if url.scheme() != "http" {
        return Err("Games are served over http".into());
    }
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or("No main window")?;
    window.navigate(url).map_err(|e| e.to_string())
}

#[tauri::command]
fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        default_host_port: network::DEFAULT_LAN_PORT,
    }
}

/// Stops the local server (if any) and shows the launcher again. Game pages
/// come from a game server and have no access to the app, so this is a menu
/// item rather than a button in the game.
fn back_to_launcher(app: AppHandle) {
    std::thread::spawn(move || {
        app.state::<AppState>().server.stop();
        set_title(&app, APP_TITLE);
        let launcher = app.state::<AppState>().launcher_url.lock().unwrap().clone();
        if let (Some(window), Some(url)) = (app.get_webview_window(MAIN_WINDOW), launcher) {
            let _ = window.navigate(url);
        }
    });
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    // The default menu keeps the platform basics (on macOS, Edit is what
    // makes copy and paste work in text fields)
    let menu = Menu::default(app)?;
    let back = MenuItem::with_id(
        app,
        MENU_BACK,
        "Back to Launcher",
        true,
        Some("CmdOrCtrl+Shift+L"),
    )?;
    let fullscreen = MenuItem::with_id(
        app,
        MENU_FULLSCREEN,
        "Toggle Full Screen",
        true,
        Some("F11"),
    )?;
    let game = Submenu::with_items(app, "Game", true, &[&back, &fullscreen])?;
    menu.append(&game)?;
    Ok(menu)
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            let menu = build_menu(app.handle())?;
            app.set_menu(menu)?;
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                *app.state::<AppState>().launcher_url.lock().unwrap() = Some(window.url()?);
            }
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_BACK => back_to_launcher(app.clone()),
            MENU_FULLSCREEN => {
                if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                    let fullscreen = window.is_fullscreen().unwrap_or(false);
                    let _ = window.set_fullscreen(!fullscreen);
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            probe_host,
            open_game,
            app_info
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the Dungerra app")
        .run(|app, event| {
            // Save and stop the local server however the app is closed
            if let RunEvent::Exit = event {
                app.state::<AppState>().server.stop();
            }
        });
}
