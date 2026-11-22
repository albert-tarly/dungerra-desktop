fn main() {
    // The Google OAuth client is baked in with option_env! (src/server.rs)
    println!("cargo:rerun-if-env-changed=DUNGERRA_GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=DUNGERRA_GOOGLE_CLIENT_SECRET");
    tauri_build::build()
}
