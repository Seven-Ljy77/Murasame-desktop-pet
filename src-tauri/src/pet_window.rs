use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

/// Gap between the pet and the work-area corner, in physical pixels.
const EDGE_MARGIN: i32 = 32;
const STATE_FILE: &str = "pet-window.json";

/// Pet size at 100% in **physical pixels**. Physical sizing keeps the pet the
/// same size on every monitor, even when their DPI scale differs; 480px is what
/// the old 320 logical pixels looked like on a 150% display.
pub const BASE_PET_PIXELS: f64 = 480.0;
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

/// A monitor work area, expressed in physical pixels. Kept as a plain struct so
/// the clamping rules can be unit-tested without a live window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn normalize_scale(scale: f64) -> f64 {
    if scale.is_finite() {
        scale.clamp(MIN_PET_SCALE, MAX_PET_SCALE)
    } else {
        DEFAULT_PET_SCALE
    }
}

pub fn pet_pixels(scale: f64) -> u32 {
    (BASE_PET_PIXELS * normalize_scale(scale)).round().max(1.0) as u32
}

/// Picks the monitor the window centre falls into, otherwise the nearest one,
/// then keeps the whole window inside that work area.
///
/// Off-screen positions happen when a monitor is unplugged or its resolution
/// changes, so the nearest monitor is a better fallback than the first one.
pub fn clamp_to_areas(
    areas: &[WorkArea],
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> (i32, i32) {
    let Some(first) = areas.first() else {
        return (x, y);
    };

    let center_x = x + width / 2;
    let center_y = y + height / 2;

    let contains = |area: &&WorkArea| {
        center_x >= area.x
            && center_x < area.x + area.width
            && center_y >= area.y
            && center_y < area.y + area.height
    };

    let target = areas.iter().find(contains).unwrap_or_else(|| {
        areas
            .iter()
            .min_by_key(|area| {
                let dx = (center_x - (area.x + area.width / 2)) as i64;
                let dy = (center_y - (area.y + area.height / 2)) as i64;
                dx * dx + dy * dy
            })
            .unwrap_or(first)
    });

    let maximum_x = target.x + target.width - width;
    let maximum_y = target.y + target.height - height;

    (
        x.clamp(target.x, maximum_x.max(target.x)),
        y.clamp(target.y, maximum_y.max(target.y)),
    )
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

fn work_areas(window: &WebviewWindow) -> Vec<WorkArea> {
    let Ok(monitors) = window.available_monitors() else {
        return Vec::new();
    };
    monitors
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            WorkArea {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width as i32,
                height: area.size.height as i32,
            }
        })
        .collect()
}

fn clamp_to_work_area(
    window: &WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> (i32, i32) {
    clamp_to_areas(&work_areas(window), x, y, width, height)
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

fn apply_physical_size(window: &WebviewWindow, scale: f64) -> Result<(), String> {
    let pixels = pet_pixels(scale);
    window
        .set_size(PhysicalSize::new(pixels, pixels))
        .map_err(|error| format!("set_size: {error}"))
}

/// Applies the saved scale, restores the position and only then shows the
/// window, so it never flashes at the default origin.
pub fn restore(window: &WebviewWindow, settings: &mut PetSettings) -> Result<(), String> {
    let scale = normalize_scale(settings.scale);
    apply_physical_size(window, scale)?;

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

/// Parks the pet in the primary monitor's bottom-right corner.
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

    apply_physical_size(&window, scale)?;

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

/// Pulls the window back onto a monitor, e.g. after a display was unplugged
/// while the pet was hidden.
pub fn ensure_on_screen(app: &AppHandle) -> Result<(), String> {
    let window = pet_window(app)?;
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let (x, y) = clamp_to_work_area(
        &window,
        position.x,
        position.y,
        size.width as i32,
        size.height as i32,
    );
    if (x, y) != (position.x, position.y) {
        window
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Windows rescales a window when it crosses monitors with a different DPI.
/// Physical sizing means the pet must be resized back to its requested pixel
/// size after every such change.
pub fn handle_scale_factor_change(app: &AppHandle, settings: &mut PetSettings) -> Result<(), String> {
    let window = pet_window(app)?;
    apply_physical_size(&window, settings.scale)?;
    ensure_on_screen(app)?;

    let position = window.outer_position().map_err(|error| error.to_string())?;
    settings.x = Some(position.x);
    settings.y = Some(position.y);
    Ok(())
}

/// Centres the settings dialog on the monitor the pet currently lives on.
pub fn center_settings(app: &AppHandle) -> Result<(), String> {
    let Some(settings_window) = app.get_webview_window("settings") else {
        return Ok(());
    };
    let Some(pet) = app.get_webview_window("pet") else {
        return Ok(());
    };

    let pet_position = pet.outer_position().map_err(|error| error.to_string())?;
    let pet_size = pet.outer_size().map_err(|error| error.to_string())?;
    let dialog_size = settings_window
        .outer_size()
        .map_err(|error| error.to_string())?;

    let center_x = pet_position.x + pet_size.width as i32 / 2;
    let center_y = pet_position.y + pet_size.height as i32 / 2;

    let areas = work_areas(&pet);
    let Some(area) = areas
        .iter()
        .find(|area| {
            center_x >= area.x
                && center_x < area.x + area.width
                && center_y >= area.y
                && center_y < area.y + area.height
        })
        .or_else(|| areas.first())
    else {
        return Ok(());
    };

    let x = area.x + (area.width - dialog_size.width as i32) / 2;
    let y = area.y + (area.height - dialog_size.height as i32) / 2;
    settings_window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: WorkArea = WorkArea {
        x: 0,
        y: 0,
        width: 2560,
        height: 1440,
    };
    const LEFT: WorkArea = WorkArea {
        x: -1920,
        y: 0,
        width: 1920,
        height: 1080,
    };
    const RIGHT: WorkArea = WorkArea {
        x: 2560,
        y: 0,
        width: 1920,
        height: 1080,
    };

    #[test]
    fn clamps_back_into_a_single_monitor() {
        let areas = [PRIMARY];
        assert_eq!(clamp_to_areas(&areas, 2500, 1400, 480, 480), (2080, 960));
        assert_eq!(clamp_to_areas(&areas, -500, -500, 480, 480), (0, 0));
    }

    #[test]
    fn keeps_a_position_on_the_left_monitor() {
        let areas = [LEFT, PRIMARY];
        // A window whose centre lands on the left monitor stays there.
        assert_eq!(clamp_to_areas(&areas, -1500, 200, 480, 480), (-1500, 200));
        // Even though its x is negative.
        assert_eq!(clamp_to_areas(&areas, -1920, -100, 480, 480), (-1920, 0));
    }

    #[test]
    fn keeps_a_position_on_the_right_monitor() {
        let areas = [PRIMARY, RIGHT];
        assert_eq!(clamp_to_areas(&areas, 3000, 300, 480, 480), (3000, 300));
        // Past the right edge of the right monitor.
        assert_eq!(clamp_to_areas(&areas, 4400, 900, 480, 480), (4000, 600));
    }

    #[test]
    fn falls_back_to_the_nearest_monitor_when_one_disappears() {
        // The pet was on the right monitor; that monitor is now gone.
        let areas = [PRIMARY];
        assert_eq!(clamp_to_areas(&areas, 3000, 300, 480, 480), (2080, 300));
    }

    #[test]
    fn handles_a_window_taller_than_the_work_area() {
        let small = WorkArea {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        };
        let areas = [small];
        // Window bigger than the screen: pin to the top-left instead of
        // producing an inverted clamp range.
        assert_eq!(clamp_to_areas(&areas, 100, 100, 1000, 900), (0, 0));
    }

    #[test]
    fn empty_monitor_list_is_a_no_op() {
        assert_eq!(clamp_to_areas(&[], 123, 456, 480, 480), (123, 456));
    }

    #[test]
    fn pixel_size_follows_the_scale() {
        assert_eq!(pet_pixels(1.0), 480);
        // Below the 60% floor, so it clamps up before scaling.
        assert_eq!(pet_pixels(0.5), 288);
        assert_eq!(pet_pixels(1.6), 768);
        // Out-of-range values are clamped, NaN falls back to 100%.
        assert_eq!(pet_pixels(9.0), 768);
        assert_eq!(pet_pixels(f64::NAN), 480);
    }
}
