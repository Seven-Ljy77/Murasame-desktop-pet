mod pet_window;
mod tray;

use std::time::{Duration, Instant};
use tauri::Manager;

/// Window moves arrive in bursts while dragging; at most one disk write per
/// interval keeps the state file fresh without hammering the disk.
const POSITION_SAVE_INTERVAL: Duration = Duration::from_millis(400);

static LAST_POSITION_SAVE: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

#[tauri::command]
fn begin_drag(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|error| error.to_string())
}

#[tauri::command]
fn reset_pet_position(app: tauri::AppHandle) -> Result<(), String> {
    pet_window::reset_position(&app)
}

fn position_save_due() -> bool {
    let Ok(mut last_save) = LAST_POSITION_SAVE.lock() else {
        return true;
    };
    let now = Instant::now();
    let due = match *last_save {
        Some(previous) => now.duration_since(previous) >= POSITION_SAVE_INTERVAL,
        None => true,
    };
    if due {
        *last_save = Some(now);
    }
    due
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![begin_drag, reset_pet_position])
        .setup(|app| {
            let handle = app.handle().clone();

            if let Some(window) = app.get_webview_window("pet") {
                pet_window::restore(&handle, &window)
                    .map_err(|error| format!("restore pet window: {error}"))?;

                let event_handle = handle.clone();
                window.on_window_event(move |event| match event {
                    tauri::WindowEvent::Moved(position) => {
                        if position_save_due() {
                            let _ = pet_window::save_position(
                                &event_handle,
                                position.x,
                                position.y,
                            );
                        }
                    }
                    // Closing the pet hides it; the tray menu owns quitting.
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        if let Some(window) = event_handle.get_webview_window("pet") {
                            let _ = window.hide();
                        }
                    }
                    _ => {}
                });
            }

            tray::create(&handle).map_err(|error| format!("create tray icon: {error}"))?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the Murasame desktop pet");

    app.run(|handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            if let Some(window) = handle.get_webview_window("pet") {
                if let Ok(position) = window.outer_position() {
                    let _ = pet_window::save_position(handle, position.x, position.y);
                }
            }
        }
    });
}
