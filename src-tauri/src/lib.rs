mod pet_window;
mod tray;

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Manager;

/// Window moves arrive in bursts while dragging; at most one disk write per
/// interval keeps the state file fresh without hammering the disk.
const POSITION_SAVE_INTERVAL: Duration = Duration::from_millis(400);

static LAST_POSITION_SAVE: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

struct PetState(Mutex<pet_window::PetSettings>);

fn lock_settings<'a>(
    state: &'a tauri::State<'_, PetState>,
) -> Result<std::sync::MutexGuard<'a, pet_window::PetSettings>, String> {
    state
        .0
        .lock()
        .map_err(|_| "the pet settings lock is poisoned".to_string())
}

#[tauri::command]
fn begin_drag(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|error| error.to_string())
}

#[tauri::command]
fn reset_pet_position(
    app: tauri::AppHandle,
    state: tauri::State<'_, PetState>,
) -> Result<(), String> {
    // Never hold the settings lock across window operations: the Moved event
    // handler runs on the main thread and needs the same lock.
    let mut settings = { *lock_settings(&state)? };
    pet_window::reset_position(&app, &mut settings)?;
    *lock_settings(&state)? = settings;
    Ok(())
}

#[tauri::command]
fn get_pet_scale(state: tauri::State<'_, PetState>) -> Result<f64, String> {
    Ok(lock_settings(&state)?.scale)
}

#[tauri::command]
fn set_pet_scale(
    app: tauri::AppHandle,
    state: tauri::State<'_, PetState>,
    scale: f64,
) -> Result<f64, String> {
    let mut settings = { *lock_settings(&state)? };
    let applied = pet_window::apply_scale(&app, &mut settings, scale)?;
    *lock_settings(&state)? = settings;
    Ok(applied)
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
        .invoke_handler(tauri::generate_handler![
            begin_drag,
            reset_pet_position,
            get_pet_scale,
            set_pet_scale
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(PetState(Mutex::new(pet_window::load(&handle))));

            if let Some(window) = app.get_webview_window("pet") {
                {
                    let state = app.state::<PetState>();
                    let mut settings = *state
                        .0
                        .lock()
                        .map_err(|_| std::io::Error::other("pet settings lock poisoned"))?;
                    pet_window::restore(&window, &mut settings)?;
                    *state
                        .0
                        .lock()
                        .map_err(|_| std::io::Error::other("pet settings lock poisoned"))? =
                        settings;
                }

                let event_handle = handle.clone();
                window.on_window_event(move |event| match event {
                    tauri::WindowEvent::Moved(position) => {
                        let Some(state) = event_handle.try_state::<PetState>() else {
                            return;
                        };
                        // try_lock, never lock: blocking the main thread here
                        // would deadlock a command that is resizing the window.
                        let Ok(mut guard) = state.0.try_lock() else {
                            return;
                        };
                        guard.x = Some(position.x);
                        guard.y = Some(position.y);
                        let snapshot = *guard;
                        drop(guard);
                        if position_save_due() {
                            let _ = pet_window::save(&event_handle, &snapshot);
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

            // The settings window is long-lived: closing it just hides it.
            if let Some(settings_window) = app.get_webview_window("settings") {
                let settings_handle = settings_window.clone();
                settings_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = settings_handle.hide();
                    }
                });
            }

            tray::create(&handle).map_err(|error| format!("create tray icon: {error}"))?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the Murasame desktop pet");

    app.run(|handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            if let Some(state) = handle.try_state::<PetState>() {
                if let Ok(guard) = state.0.try_lock() {
                    let snapshot = *guard;
                    drop(guard);
                    let _ = pet_window::save(handle, &snapshot);
                }
            }
        }
    });
}
