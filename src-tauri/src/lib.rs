mod pet_window;
mod tray;

use std::sync::mpsc::{RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::Manager;

/// How long the window must stay still before its position is written to disk.
const POSITION_QUIET_PERIOD: Duration = Duration::from_millis(350);

static POSITION_SENDER: OnceLock<Sender<(i32, i32)>> = OnceLock::new();

struct PetState(Arc<Mutex<pet_window::PetSettings>>);

fn lock_settings<'a>(
    state: &'a tauri::State<'_, PetState>,
) -> Result<std::sync::MutexGuard<'a, pet_window::PetSettings>, String> {
    state
        .0
        .lock()
        .map_err(|_| "the pet settings lock is poisoned".to_string())
}

/// Persists the position once the window has been still for a moment, so the
/// last position of a drag is never lost to write throttling.
fn start_position_saver(handle: tauri::AppHandle, shared: Arc<Mutex<pet_window::PetSettings>>) {
    let (sender, receiver) = std::sync::mpsc::channel::<(i32, i32)>();
    let _ = POSITION_SENDER.set(sender);

    std::thread::spawn(move || {
        let mut pending: Option<(i32, i32)> = None;
        loop {
            if pending.is_none() {
                match receiver.recv() {
                    Ok(position) => pending = Some(position),
                    Err(_) => return,
                }
            }

            loop {
                match receiver.recv_timeout(POSITION_QUIET_PERIOD) {
                    Ok(position) => pending = Some(position),
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }

            let Some((x, y)) = pending.take() else {
                continue;
            };
            let Ok(mut settings) = shared.lock() else {
                continue;
            };
            settings.x = Some(x);
            settings.y = Some(y);
            let _ = pet_window::save(&handle, &settings);
        }
    });
}

fn report_position(x: i32, y: i32) {
    if let Some(sender) = POSITION_SENDER.get() {
        let _ = sender.send((x, y));
    }
}

#[tauri::command]
fn begin_drag(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|error| error.to_string())
}

/// Right-clicking the pet hides it; the tray icon brings it back.
#[tauri::command]
fn hide_pet(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("pet")
        .ok_or_else(|| "pet window is missing".to_string())?;
    window.hide().map_err(|error| error.to_string())
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

#[derive(serde::Serialize)]
struct AudioSettings {
    volume: f64,
    muted: bool,
}

#[tauri::command]
fn get_audio_settings(state: tauri::State<'_, PetState>) -> Result<AudioSettings, String> {
    let settings = { *lock_settings(&state)? };
    Ok(AudioSettings {
        volume: settings.volume,
        muted: settings.muted,
    })
}

#[tauri::command]
fn set_audio_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, PetState>,
    volume: f64,
    muted: bool,
) -> Result<AudioSettings, String> {
    let updated = {
        let mut settings = lock_settings(&state)?;
        settings.volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            settings.volume
        };
        settings.muted = muted;
        *settings
    };
    pet_window::save(&app, &updated)?;
    Ok(AudioSettings {
        volume: updated.volume,
        muted: updated.muted,
    })
}

/// Whether the autostart entry can be written from this build.
///
/// A `tauri dev` binary lives in `target/debug` and needs the Vite server, so
/// registering it would only produce a broken entry after the next logon. The
/// env var is a deliberate escape hatch for exercising the real code path while
/// developing.
fn autostart_writable() -> bool {
    !cfg!(debug_assertions) || std::env::var_os("MURASAME_DEV_AUTOSTART").is_some()
}

#[derive(serde::Serialize)]
struct AutostartState {
    enabled: bool,
    available: bool,
}

#[tauri::command]
fn get_autostart(app: tauri::AppHandle) -> Result<AutostartState, String> {    use tauri_plugin_autostart::ManagerExt;
    let enabled = app
        .autolaunch()
        .is_enabled()
        .map_err(|error| format!("is_enabled: {error}"))?;
    Ok(AutostartState {
        enabled,
        available: autostart_writable(),
    })
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<AutostartState, String> {
    use tauri_plugin_autostart::ManagerExt;
    if !autostart_writable() {
        return Err("开发模式下的程序不写入开机自启".to_string());
    }
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|error| format!("enable: {error}"))?;
    } else {
        manager
            .disable()
            .map_err(|error| format!("disable: {error}"))?;
    }
    let now = manager
        .is_enabled()
        .map_err(|error| format!("is_enabled: {error}"))?;
    Ok(AutostartState {
        enabled: now,
        available: true,
    })
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            begin_drag,
            hide_pet,
            reset_pet_position,
            get_pet_scale,
            set_pet_scale,
            get_audio_settings,
            set_audio_settings,
            get_autostart,
            set_autostart
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let shared = Arc::new(Mutex::new(pet_window::load(&handle)));
            app.manage(PetState(shared.clone()));
            start_position_saver(handle.clone(), shared.clone());

            if let Some(window) = app.get_webview_window("pet") {
                {
                    let mut settings = shared
                        .lock()
                        .map_err(|_| std::io::Error::other("pet settings lock poisoned"))?;
                    pet_window::restore(&window, &mut settings)?;
                }

                let event_handle = handle.clone();
                let event_settings = shared.clone();
                window.on_window_event(move |event| match event {
                    tauri::WindowEvent::Moved(position) => {
                        // try_lock, never lock: blocking the main thread here
                        // would deadlock a command that is resizing the window.
                        if let Ok(mut settings) = event_settings.try_lock() {
                            settings.x = Some(position.x);
                            settings.y = Some(position.y);
                        }
                        report_position(position.x, position.y);
                    }
                    // Windows rescales the window when it crosses a monitor with
                    // a different DPI; physical sizing means we resize it back.
                    tauri::WindowEvent::ScaleFactorChanged { .. } => {
                        let mut settings = match event_settings.try_lock() {
                            Ok(guard) => *guard,
                            Err(_) => return,
                        };
                        if pet_window::handle_scale_factor_change(&event_handle, &mut settings)
                            .is_ok()
                        {
                            if let Ok(mut guard) = event_settings.try_lock() {
                                *guard = settings;
                            }
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

            // Dev helper: `MURASAME_OPEN_SETTINGS=1` pops the settings window at
            // startup so it can be inspected without clicking through the tray.
            #[cfg(debug_assertions)]
            if std::env::var_os("MURASAME_OPEN_SETTINGS").is_some() {
                if let Some(window) = app.get_webview_window("settings") {
                    let _ = pet_window::center_settings(&handle);
                    let _ = window.show();
                }
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the Murasame desktop pet");

    app.run(|handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            if let Some(state) = handle.try_state::<PetState>() {
                if let Ok(settings) = state.0.lock() {
                    let _ = pet_window::save(handle, &settings);
                }
            }
        }
    });
}
