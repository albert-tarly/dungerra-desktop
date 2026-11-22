// No console window next to the game in release builds on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod server;

use std::sync::Mutex;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, Submenu};
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, RunEvent, Url, WebviewWindow, Wry};
use tauri_plugin_opener::OpenerExt;

use server::{Paths, ServerManager};

/// The local server prefers a fixed port: the game client keeps its sign-in
/// per origin (host and port), so a stable port keeps the player signed in
/// between launches. Any free port is used if it is taken.
const LOCAL_PORT: u16 = 47823;
const MAIN_WINDOW: &str = "main";
const MENU_MAIN: &str = "main-menu";
const MENU_FULLSCREEN: &str = "toggle-fullscreen";
/// Google refuses to sign in inside embedded browsers such as the game
/// window, so its sign-in page opens in the system browser instead.
const GOOGLE_SIGN_IN_HOST: &str = "accounts.google.com";

/// Opens Google's sign-in page in the system browser, where the game window
/// would navigate to it. The game page stays and polls its server, which
/// Google redirects the browser back to (dungerra-api loopback flow).
fn google_sign_in_in_browser() -> TauriPlugin<Wry> {
    tauri::plugin::Builder::new("google-sign-in")
        .on_navigation(|webview, url| {
            if url.scheme() == "https" && url.host_str() == Some(GOOGLE_SIGN_IN_HOST) {
                if let Err(e) = webview
                    .app_handle()
                    .opener()
                    .open_url(url.as_str(), None::<&str>)
                {
                    eprintln!("[desktop] cannot open the browser for Google sign-in: {e}");
                }
                return false;
            }
            true
        })
        .build()
}

#[derive(Default)]
struct AppState {
    server: ServerManager,
    /// The start page's own URL, shown again if the server is not running.
    start_url: Mutex<Option<Url>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
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

/// Where this computer opens the game served on `port`.
fn local_url(port: u16, path: &str) -> Url {
    Url::parse(&format!("http://127.0.0.1:{port}{path}")).expect("a valid local URL")
}

/// Starts the local server unless it runs already, and returns its port.
fn ensure_server(app: &AppHandle) -> Result<u16, String> {
    let manager = &app.state::<AppState>().server;
    if let Some(port) = manager.current() {
        return Ok(port);
    }
    let paths = server_paths(app)?;
    match manager.start(&paths, LOCAL_PORT) {
        Ok(port) => Ok(port),
        Err(e) if e.port_in_use => manager.start(&paths, 0).map_err(|e| e.message),
        Err(e) => Err(e.message),
    }
}

/// Starts the game server on this computer and opens the game in the window.
/// Joining other players, and opening a game to them, happens in the game.
#[tauri::command]
async fn start_game(app: AppHandle) -> Result<(), String> {
    let port = {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || ensure_server(&app))
            .await
            .map_err(|e| e.to_string())??
    };
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or("No main window")?;
    window
        .navigate(local_url(port, "/"))
        .map_err(|e| e.to_string())
}

/// Called by the start page when it loads. Only that page can call commands
/// (game pages get no app access), so the caller's URL is the start page.
/// The window's URL at startup is still about:blank, so it can't be taken
/// from there.
#[tauri::command]
fn app_info(app: AppHandle, window: WebviewWindow) -> AppInfo {
    if let Ok(url) = window.url() {
        *app.state::<AppState>().start_url.lock().unwrap() = Some(url);
    }
    AppInfo {
        version: app.package_info().version.to_string(),
    }
}

/// Game → Main Menu: the player's own game list, from anywhere (a game, or a
/// LAN host's pages). Leaving a game this way also closes it to LAN.
fn main_menu(app: &AppHandle) {
    let state = app.state::<AppState>();
    let target = match state.server.current() {
        Some(port) => Some(local_url(port, "/sessions")),
        None => state.start_url.lock().unwrap().clone(),
    };
    if let (Some(window), Some(url)) = (app.get_webview_window(MAIN_WINDOW), target) {
        let _ = window.navigate(url);
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    // The default menu keeps the platform basics (on macOS, Edit is what
    // makes copy and paste work in text fields)
    let menu = Menu::default(app)?;
    let main = MenuItem::with_id(app, MENU_MAIN, "Main Menu", true, Some("CmdOrCtrl+Shift+L"))?;
    let fullscreen = MenuItem::with_id(
        app,
        MENU_FULLSCREEN,
        "Toggle Full Screen",
        true,
        Some("F11"),
    )?;
    let game = Submenu::with_items(app, "Game", true, &[&main, &fullscreen])?;
    menu.append(&game)?;
    Ok(menu)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(google_sign_in_in_browser())
        .manage(AppState::default())
        .setup(|app| {
            let menu = build_menu(app.handle())?;
            app.set_menu(menu)?;
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_MAIN => main_menu(app),
            MENU_FULLSCREEN => {
                if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                    let fullscreen = window.is_fullscreen().unwrap_or(false);
                    let _ = window.set_fullscreen(!fullscreen);
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![start_game, app_info])
        .build(tauri::generate_context!())
        .expect("failed to build the Dungerra app")
        .run(|app, event| {
            // Save and stop the local server however the app is closed
            if let RunEvent::Exit = event {
                app.state::<AppState>().server.stop();
            }
        });
}
