// LiveboxMix desktop shell.
//
// Deliberately thin. The operator UI is a web app served by the mixer daemon,
// so this is a native window pointed at it. Running the same UI locally and
// remotely means there is only one implementation to keep in step: a remote
// operator gets the identical page by changing the address.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to start the LiveboxMix desktop shell");
}
