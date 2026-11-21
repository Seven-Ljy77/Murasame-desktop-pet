use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewWindow};

const EDGE_MARGIN: i32 = 24;
const STATE_FILE: &str = "pet-window.json";

#[derive(Serialize, Deserialize)]
struct WindowState {
    x: i32,
    y: i32,
}

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|directory| directory.join(STATE_FILE))
}

fn load_position(app: &AppHandle) -> Option<(i32, i32)> {
    let path = state_path(app)?;
    let raw = fs::read_to_string(path).ok()?;
    let state: WindowState = serde_json::from_str(&raw).ok()?;
    Some((state.x, state.y))
}

pub fn save_position(app: &AppHandle, x: i32, y: i32) -> Result<(), String> {
    let path = state_path(app)
        .ok_or_else(|| "unable to locate the app data directory".to_string())?;
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }
    let payload =
        serde_json::to_string(&WindowState { x, y }).map_err(|error| error.to_string())?;
    fs::write(path, payload).map_err(|error| error.to_string())
}

/// Keeps the window fully inside the work area of the monitor it overlaps, so
/// a restored position never lands off-screen after a resolution change.
fn clamp_to_work_area(
    window: &WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> (i32, i32) {
    let Ok(monitors) = window.available_monitors() else {
        return (x, y);
    };

    let center_x = x + width / 2;
    let center_y = y + height / 2;
    let monitor = monitors
        .iter()
        .find(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            center_x >= position.x
                && center_x < position.x + size.width as i32
                && center_y >= position.y
                && center_y < position.y + size.height as i32
        })
        .or_else(|| monitors.first());

    let Some(monitor) = monitor else {
        return (x, y);
    };

    let area = monitor.work_area();
    let minimum_x = area.position.x;
    let minimum_y = area.position.y;
    let maximum_x = area.position.x + area.size.width as i32 - width;
    let maximum_y = area.position.y + area.size.height as i32 - height;

    (
        x.clamp(minimum_x, maximum_x.max(minimum_x)),
        y.clamp(minimum_y, maximum_y.max(minimum_y)),
    )
}

fn default_position(window: &WebviewWindow, width: i32, height: i32) -> (i32, i32) {
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let area = monitor.work_area();
        return (
            area.position.x + area.size.width as i32 - width - EDGE_MARGIN,
            area.position.y + area.size.height as i32 - height - EDGE_MARGIN,
        );
    }
    (EDGE_MARGIN, EDGE_MARGIN)
}

/// Restores the last position (or parks the pet in the bottom-right corner) and
/// only then shows the window, so it never flashes at the default origin.
pub fn restore(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    let size = window
        .outer_size()
        .map_err(|error| format!("outer_size: {error}"))?;
    let width = size.width as i32;
    let height = size.height as i32;

    let (x, y) = load_position(app).unwrap_or_else(|| default_position(window, width, height));
    let (x, y) = clamp_to_work_area(window, x, y, width, height);

    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| format!("set_position: {error}"))?;
    window.show().map_err(|error| format!("show: {error}"))?;
    Ok(())
}

pub fn reset_position(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("pet")
        .ok_or_else(|| "the pet window is unavailable".to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let width = size.width as i32;
    let height = size.height as i32;

    let (x, y) = default_position(&window, width, height);
    let (x, y) = clamp_to_work_area(&window, x, y, width, height);

    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    save_position(app, x, y)
}
