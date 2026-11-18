pub fn appearance() -> glance_core::Appearance {
    #[cfg(target_os = "macos")]
    {
        let dark = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .to_ascii_lowercase()
                    .contains("dark")
            })
            .unwrap_or(false);
        if dark {
            return glance_core::Appearance::Dark;
        }
    }
    glance_core::Appearance::Light
}

#[cfg(target_os = "macos")]
pub fn activate() {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let Some(class) = Class::get("NSApplication") else {
            return;
        };
        let app: *mut Object = msg_send![class, sharedApplication];
        if app.is_null() {
            return;
        }
        let _: () = msg_send![app, activateIgnoringOtherApps: true];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn activate() {}
