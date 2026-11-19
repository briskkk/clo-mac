use glance_core::{menu_entries, tooltip, tray_label, BarState, ServerPreset};
use tauri::image::Image;
use tauri::menu::{CheckMenuItemBuilder, Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::{handle_menu, macos, panel, AppState};

const TEMPLATE_ICON: &[u8] = include_bytes!("../icons/tray-template.png");

pub fn create(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let menu = Menu::new(app)?;
    let icon = Image::from_bytes(TEMPLATE_ICON)?;
    let tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("家庭财富终端")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Down,
                ..
            } => panel::on_left_mouse_down(),
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => panel::on_left_mouse_up(tray.app_handle()),
            TrayIconEvent::Click {
                button: MouseButton::Right,
                button_state: MouseButtonState::Down,
                ..
            } => panel::hide(),
            _ => {}
        })
        .on_menu_event(|app, event| {
            handle_menu(app, event.id.as_ref());
        })
        .build(app)?;
    Ok(tray)
}

pub fn apply(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let state = app.state::<AppState>();
    let bar = state
        .bar
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clone();
    let base = state
        .base_url
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clone();
    let preset = glance_core::preset_of(&base);
    let _ = refresh_menu(app, &tray, &bar, preset);
    let _ = refresh_icon(app, &tray, &bar);
    let _ = tray.set_tooltip(Some(tooltip(&bar)));
    panel::refresh(app);
}

fn refresh_menu(
    app: &AppHandle,
    tray: &TrayIcon,
    bar: &BarState,
    preset: ServerPreset,
) -> tauri::Result<()> {
    let menu = Menu::new(app)?;
    for entry in menu_entries(bar, preset) {
        if entry.separator {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
            continue;
        }
        if entry.id.starts_with("server-") {
            let item = CheckMenuItemBuilder::with_id(entry.id.as_str(), entry.label.as_str())
                .enabled(entry.enabled)
                .checked(entry.checked)
                .build(app)?;
            menu.append(&item)?;
            continue;
        }
        let item = MenuItemBuilder::with_id(entry.id.as_str(), entry.label.as_str())
            .enabled(entry.enabled)
            .build(app)?;
        menu.append(&item)?;
    }
    tray.set_menu(Some(menu))?;
    Ok(())
}

fn refresh_icon(app: &AppHandle, tray: &TrayIcon, bar: &BarState) -> tauri::Result<()> {
    // 状态项必须始终有系统标题和模板图标。自定义位图一旦失败、尺寸无效，或只画得出 ASCII，
    // 再把标题设成空字符串，菜单栏额外项就会缩成 0 宽，重启后只要一刷到盈亏又会消失。
    let label = tray_label(bar);
    let icon = Image::from_bytes(TEMPLATE_ICON)?;
    tray.set_icon_with_as_template(Some(icon), true)?;
    tray.set_title(Some(label.title.as_str()))?;
    tint_title(app, label.title, label.tone);
    Ok(())
}

fn tint_title(app: &AppHandle, title: String, tone: Option<glance_core::Tone>) {
    #[cfg(target_os = "macos")]
    {
        let app = app.clone();
        let _ = app.run_on_main_thread(move || {
            macos::apply_status_title_tint(&title, tone);
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, title, tone);
    }
}
