use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示丛雨", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "隐藏", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset", "重置位置", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &hide, &settings, &reset, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("murasame-tray")
        .tooltip("丛雨桌宠")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => set_visible(app, true),
            "hide" => set_visible(app, false),
            "settings" => open_settings(app),
            "reset" => reset_position(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let visible = app
                    .get_webview_window("pet")
                    .and_then(|window| window.is_visible().ok())
                    .unwrap_or(false);
                set_visible(app, !visible);
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

fn set_visible(app: &AppHandle, visible: bool) {
    if let Some(window) = app.get_webview_window("pet") {
        if visible {
            // A display may have been unplugged while the pet was hidden.
            let _ = crate::pet_window::ensure_on_screen(app);
            let _ = window.show();
        } else {
            let _ = window.hide();
        }
    }
}

fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = crate::pet_window::center_settings(app);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn reset_position(app: &AppHandle) {
    let Some(state) = app.try_state::<crate::PetState>() else {
        return;
    };
    let Ok(guard) = state.0.lock() else {
        return;
    };
    let mut settings = *guard;
    drop(guard);
    if crate::pet_window::reset_position(app, &mut settings).is_ok() {
        if let Ok(mut guard) = state.0.lock() {
            *guard = settings;
        }
    }
}
