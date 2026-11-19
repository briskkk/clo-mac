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

/// 给本进程里的菜单栏按钮标题着色。文字本身已经由系统标题画出；这里只改颜色。
/// 找不到按钮时标题仍在，状态项不会消失。
#[cfg(target_os = "macos")]
pub fn apply_status_title_tint(title: &str, tone: Option<glance_core::Tone>) {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel_impl};

    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let pool: *mut Object = msg_send![class!(NSAutoreleasePool), new];
        let windows: *mut Object = msg_send![app, windows];
        if !windows.is_null() {
            let count: usize = msg_send![windows, count];
            for index in 0..count {
                let window: *mut Object = msg_send![windows, objectAtIndex: index];
                if window.is_null() {
                    continue;
                }
                let content: *mut Object = msg_send![window, contentView];
                tint_status_view(content, title, tone);
                if content.is_null() {
                    continue;
                }
                let subs: *mut Object = msg_send![content, subviews];
                if subs.is_null() {
                    continue;
                }
                let sub_count: usize = msg_send![subs, count];
                for sub_index in 0..sub_count {
                    let sub: *mut Object = msg_send![subs, objectAtIndex: sub_index];
                    tint_status_view(sub, title, tone);
                }
            }
        }
        let _: () = msg_send![pool, drain];
    }
}

#[cfg(target_os = "macos")]
fn tint_status_view(
    view: *mut objc::runtime::Object,
    title: &str,
    tone: Option<glance_core::Tone>,
) {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel_impl};

    unsafe {
        if view.is_null() || !class_name(view).contains("StatusBar") {
            return;
        }
        let color = title_color(tone);
        let font: *mut Object = msg_send![class!(NSFont), menuBarFontOfSize: 0.0f64];
        let font: *mut Object = if font.is_null() {
            msg_send![class!(NSFont), systemFontOfSize: 13.0f64]
        } else {
            font
        };
        if color.is_null() || font.is_null() {
            return;
        }
        let string: *mut Object = msg_send![class!(NSString), alloc];
        let string: *mut Object =
            msg_send![string, initWithBytes: title.as_ptr() length: title.len() encoding: 4usize];
        let keys = [NSForegroundColorAttributeName, NSFontAttributeName];
        let values = [color, font];
        let attrs: *mut Object = msg_send![
            class!(NSDictionary),
            dictionaryWithObjects: values.as_ptr() forKeys: keys.as_ptr() count: 2usize
        ];
        let attributed: *mut Object = msg_send![class!(NSAttributedString), alloc];
        let attributed: *mut Object =
            msg_send![attributed, initWithString: string attributes: attrs];
        let _: () = msg_send![string, release];
        let _: () = msg_send![view, setAttributedTitle: attributed];
        let _: () = msg_send![attributed, release];
    }
}

#[cfg(target_os = "macos")]
fn title_color(tone: Option<glance_core::Tone>) -> *mut objc::runtime::Object {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel_impl};

    unsafe {
        let Some(tone) = tone else {
            return msg_send![class!(NSColor), labelColor];
        };
        let [red, green, blue, alpha] = glance_core::tone_rgba(tone, appearance());
        msg_send![
            class!(NSColor),
            colorWithCalibratedRed: f64::from(red) / 255.0
            green: f64::from(green) / 255.0
            blue: f64::from(blue) / 255.0
            alpha: f64::from(alpha) / 255.0
        ]
    }
}

#[cfg(target_os = "macos")]
fn class_name(obj: *mut objc::runtime::Object) -> String {
    extern "C" {
        fn object_getClassName(obj: *mut objc::runtime::Object) -> *const i8;
    }
    unsafe {
        let ptr = object_getClassName(obj);
        if ptr.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {
    static NSForegroundColorAttributeName: *mut objc::runtime::Object;
    static NSFontAttributeName: *mut objc::runtime::Object;
}
