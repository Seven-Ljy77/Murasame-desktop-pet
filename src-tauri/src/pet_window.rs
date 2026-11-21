use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewWindow};

const EDGE_MARGIN: i32 = 24;
const STATE_FILE: &str = "pet-window.json";

/// Window size at 100%; the slider scales this.
pub const BASE_PET_SIZE: f64 = 320.0;
pub const DEFAULT_PET_SCALE: f64 = 1.0;
pub const MIN_PET_SCALE: f64 = 0.6;
pub const MAX_PET_SCALE: f64 = 1.6;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PetSettings {
    pub x: Option<i32>,
    pub y: Option<i32>,
    #[serde(default = "default_scale")]
    pub scale: f64,
}

fn default_scale() -> f64 {
    DEFAULT_PET_SCALE
}

impl Default for PetSettings {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            scale: DEFAULT_PET_SCALE,
        }
    }
}

pub fn normalize_scale(scale: f64) -> f64 {
    if scale.is_finite() {
        scale.clamp(MIN_PET_SCALE, MAX_PET_SCALE)
    } else {
        DEFAULT_PET_SCALE
    }
}

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|directory| directory.join(STATE_FILE))
}

/// Missing or corrupt state files fall back to defaults instead of failing.
pub fn load(app: &AppHandle) -> PetSettings {
    let Some(path) = state_path(app) else {
        return PetSettings::default();
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return PetSettings::default();
    };
    match serde_json::from_str::<PetSettings>(&raw) {
        Ok(mut settings) => {
            settings.scale = normalize_scale(settings.scale);
            settings
        }
        Err(_) => PetSettings::default(),
    }
}

pub fn save(app: &AppHandle, settings: &PetSettings) -> Result<(), String> {
    let path = state_path(app)
        .ok_or_else(|| "unable to locate the app data directory".to_string())?;
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }
    let payload = serde_json::to_string(settings).map_err(|error| error.to_string())?;
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

fn pet_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("pet")
        .ok_or_else(|| "the pet window is unavailable".to_string())
}

/// Applies the saved scale, restores the position and only then shows the
/// window, so it never flashes at the default origin.
pub fn restore(window: &WebviewWindow, settings: &mut PetSettings) -> Result<(), String> {
    let scale = normalize_scale(settings.scale);
    window
        .set_size(LogicalSize::new(
            BASE_PET_SIZE * scale,
            BASE_PET_SIZE * scale,
        ))
        .map_err(|error| format!("set_size: {error}"))?;

    let size = window
        .outer_size()
        .map_err(|error| format!("outer_size: {error}"))?;
    let width = size.width as i32;
    let height = size.height as i32;

    let (x, y) = match (settings.x, settings.y) {
        (Some(x), Some(y)) => (x, y),
        _ => default_position(window, width, height),
    };
    let (x, y) = clamp_to_work_area(window, x, y, width, height);

    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| format!("set_position: {error}"))?;
    settings.x = Some(x);
    settings.y = Some(y);
    settings.scale = scale;
    window.show().map_err(|error| format!("show: {error}"))?;
    Ok(())
}

pub fn reset_position(app: &AppHandle, settings: &mut PetSettings) -> Result<(), String> {
    let window = pet_window(app)?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let (x, y) = default_position(&window, size.width as i32, size.height as i32);
    let (x, y) = clamp_to_work_area(&window, x, y, size.width as i32, size.height as i32);

    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;

    settings.x = Some(x);
    settings.y = Some(y);
    save(app, settings)
}

/// Resizes around the bottom-right corner, which is what a corner-parked pet
/// should look like while the slider moves.
pub fn apply_scale(
    app: &AppHandle,
    settings: &mut PetSettings,
    scale: f64,
) -> Result<f64, String> {
    let scale = normalize_scale(scale);
    let window = pet_window(app)?;

    let previous_position = window.outer_position().map_err(|error| error.to_string())?;
    let previous_size = window.outer_size().map_err(|error| error.to_string())?;

    window
        .set_size(LogicalSize::new(
            BASE_PET_SIZE * scale,
            BASE_PET_SIZE * scale,
        ))
        .map_err(|error| error.to_string())?;

    let resized = window.outer_size().map_err(|error| error.to_string())?;
    let x = previous_position.x + previous_size.width as i32 - resized.width as i32;
    let y = previous_position.y + previous_size.height as i32 - resized.height as i32;
    let (x, y) = clamp_to_work_area(&window, x, y, resized.width as i32, resized.height as i32);

    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;

    settings.scale = scale;
    settings.x = Some(x);
    settings.y = Some(y);
    save(app, settings)?;
    Ok(scale)
}
