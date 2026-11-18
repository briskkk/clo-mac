//! 菜单栏左键的原生概览。用 AppKit 的 NSPopover，不打开网页工作台。

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::CStr;
    use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
    use std::sync::{Mutex, Once};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[link(name = "System", kind = "dylib")]
    extern "C" {
        static _dispatch_main_q: u8;
        fn dispatch_async_f(
            queue: *mut std::ffi::c_void,
            context: *mut std::ffi::c_void,
            work: extern "C" fn(*mut std::ffi::c_void),
        );
    }

    use cocoa::base::{id, nil, YES};
    use cocoa::foundation::{NSPoint, NSRect, NSSize, NSString};
    use glance_core::{glance_lines, tone_rgba, GlanceLine, Tone};
    use objc::declare::ClassDecl;
    use objc::runtime::{Object, Sel, BOOL};
    use objc::{class, msg_send, sel, sel_impl, Encode};
    use tauri::{AppHandle, Manager};

    use crate::macos;
    use crate::AppState;

    const WIDTH: f64 = 292.0;
    const PAD: f64 = 16.0;
    const EDGE_MIN_Y: usize = 1;
    const ALIGN_LEFT: usize = 0;
    const ALIGN_CENTER: usize = 1;
    const ALIGN_RIGHT: usize = 2;
    const WEIGHT_REGULAR: f64 = 0.0;
    const WEIGHT_SEMIBOLD: f64 = 0.3;

    static REGISTER: Once = Once::new();
    static POPOVER: AtomicPtr<Object> = AtomicPtr::new(std::ptr::null_mut());
    static CONTROLLER: AtomicPtr<Object> = AtomicPtr::new(std::ptr::null_mut());
    static ANCHOR: AtomicPtr<Object> = AtomicPtr::new(std::ptr::null_mut());
    static VISIBLE: AtomicBool = AtomicBool::new(false);
    static SUPPRESS: AtomicBool = AtomicBool::new(false);
    static CLOSED_MS: AtomicU64 = AtomicU64::new(0);
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    static APP: Mutex<Option<AppHandle>> = Mutex::new(None);

    struct ShowLater {
        generation: u64,
        view: id,
    }

    pub fn on_left_mouse_down() {
        // 瞬态 popover 会在这次按下之前先关掉自己。刚关过也要吞掉随后的抬起，否则左键会马上又打开。
        let was_shown = shown();
        if was_shown {
            hide();
        }
        if was_shown || closed_just_now() {
            SUPPRESS.store(true, Ordering::SeqCst);
        }
    }

    pub fn on_left_mouse_up(app: &AppHandle) {
        if SUPPRESS.swap(false, Ordering::SeqCst) {
            return;
        }
        if shown() {
            hide();
            return;
        }
        present(app);
    }

    pub fn hide() {
        VISIBLE.store(false, Ordering::SeqCst);
        let _ = next_generation();
        let popover = POPOVER.load(Ordering::SeqCst);
        if popover.is_null() {
            return;
        }
        unsafe {
            let is_shown: i8 = msg_send![popover, isShown];
            if is_shown != 0 {
                let _: () = msg_send![popover, close];
            }
        }
    }

    pub fn refresh(app: &AppHandle) {
        if !VISIBLE.load(Ordering::SeqCst) {
            return;
        }
        let app = app.clone();
        let _ = app.run_on_main_thread(move || {
            if shown() {
                rebuild(&app);
            } else {
                VISIBLE.store(false, Ordering::SeqCst);
            }
        });
    }

    fn present(app: &AppHandle) {
        *APP.lock().unwrap_or_else(|err| err.into_inner()) = Some(app.clone());
        let _ = ensure_popover();
        rebuild(app);
        let view = unsafe { capture_status_view() };
        if view.is_null() {
            return;
        }
        let generation = next_generation();
        let ctx = Box::into_raw(Box::new(ShowLater { generation, view }));
        unsafe {
            let queue = std::ptr::addr_of!(_dispatch_main_q) as *mut std::ffi::c_void;
            dispatch_async_f(queue, ctx.cast::<std::ffi::c_void>(), deferred_show);
        }
    }

    fn rebuild(app: &AppHandle) {
        let popover = ensure_popover();
        if popover.is_null() {
            return;
        }
        let lines = current_lines(app);
        let appearance = macos::appearance();
        let (view, width, height) = unsafe { build_content(&lines, appearance) };
        if view.is_null() {
            return;
        }
        unsafe { install(popover, view, width, height) };
    }

    fn current_lines(app: &AppHandle) -> Vec<GlanceLine> {
        let state = app.state::<AppState>();
        let bar = state
            .bar
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone();
        glance_lines(&bar)
    }

    fn shown() -> bool {
        let popover = POPOVER.load(Ordering::SeqCst);
        if popover.is_null() {
            return false;
        }
        unsafe {
            let is_shown: i8 = msg_send![popover, isShown];
            is_shown != 0
        }
    }

    fn next_generation() -> u64 {
        GENERATION.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn ensure_popover() -> id {
        let existing = POPOVER.load(Ordering::SeqCst);
        if !existing.is_null() {
            return existing;
        }
        let controller = controller();
        unsafe {
            let popover: id = msg_send![class!(NSPopover), alloc];
            let popover: id = msg_send![popover, init];
            if popover.is_null() {
                return nil;
            }
            let _: () = msg_send![popover, setDelegate: controller];
            let _: () = msg_send![popover, setBehavior: 1usize];
            let _: () = msg_send![popover, setAnimates: YES];
            POPOVER.store(popover, Ordering::SeqCst);
            popover
        }
    }

    fn controller() -> id {
        REGISTER.call_once(register_class);
        let existing = CONTROLLER.load(Ordering::SeqCst);
        if !existing.is_null() {
            return existing;
        }
        unsafe {
            let object: id = msg_send![class!(CloFamilyWealthGlance), alloc];
            let object: id = msg_send![object, init];
            CONTROLLER.store(object, Ordering::SeqCst);
            object
        }
    }

    fn register_class() {
        let mut decl =
            ClassDecl::new("CloFamilyWealthGlance", class!(NSObject)).expect("glance class");
        unsafe {
            decl.add_method(
                sel!(popoverShouldClose:),
                should_close as extern "C" fn(&mut Object, Sel, id) -> BOOL,
            );
            decl.add_method(
                sel!(onClick:),
                on_click as extern "C" fn(&mut Object, Sel, id),
            );
        }
        decl.register();
    }

    extern "C" fn should_close(_this: &mut Object, _sel: Sel, _popover: id) -> BOOL {
        VISIBLE.store(false, Ordering::SeqCst);
        if let Some(click) = unsafe { current_click() } {
            if click.left {
                let anchor = ANCHOR.load(Ordering::SeqCst);
                let on_anchor = !click.window.is_null() && click.window == anchor;
                if on_anchor || click.window.is_null() {
                    mark_closed();
                }
                if on_anchor {
                    SUPPRESS.store(true, Ordering::SeqCst);
                }
            }
        }
        YES
    }

    extern "C" fn on_click(_this: &mut Object, _sel: Sel, sender: id) {
        let action = unsafe { view_identifier(sender) };
        let app = APP.lock().unwrap_or_else(|err| err.into_inner()).clone();
        hide();
        if let Some(app) = app {
            if !action.is_empty() {
                crate::handle_menu(&app, &action);
            }
        }
    }

    extern "C" fn deferred_show(ctx: *mut std::ffi::c_void) {
        let later = unsafe { Box::from_raw(ctx.cast::<ShowLater>()) };
        let view = later.view;
        let release_view = |view: id| unsafe {
            if !view.is_null() {
                let _: () = msg_send![view, release];
            }
        };
        if view.is_null() || later.generation != GENERATION.load(Ordering::SeqCst) {
            release_view(view);
            return;
        }
        let popover = POPOVER.load(Ordering::SeqCst);
        if popover.is_null() {
            release_view(view);
            return;
        }
        unsafe {
            macos::activate();
            let bounds: NSRect = msg_send![view, bounds];
            let window: id = msg_send![view, window];
            if !window.is_null() {
                ANCHOR.store(window, Ordering::SeqCst);
            }
            let _: () = msg_send![popover, showRelativeToRect: bounds ofView: view preferredEdge: EDGE_MIN_Y];
            let is_shown: i8 = msg_send![popover, isShown];
            VISIBLE.store(is_shown != 0, Ordering::SeqCst);
        }
        release_view(view);
    }

    unsafe fn capture_status_view() -> id {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let event: id = msg_send![app, currentEvent];
        if event.is_null() {
            return nil;
        }
        let window: id = msg_send![event, window];
        if window.is_null() {
            return nil;
        }
        let view: id = msg_send![window, contentView];
        if view.is_null() {
            return nil;
        }
        let _: id = msg_send![view, retain];
        view
    }

    struct ClickInfo {
        left: bool,
        window: id,
    }

    unsafe fn current_click() -> Option<ClickInfo> {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let event: id = msg_send![app, currentEvent];
        if event.is_null() {
            return None;
        }
        let button: isize = msg_send![event, buttonNumber];
        let window: id = msg_send![event, window];
        Some(ClickInfo {
            left: button == 0,
            window,
        })
    }

    fn mark_closed() {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0);
        CLOSED_MS.store(ms, Ordering::SeqCst);
    }

    fn closed_just_now() -> bool {
        let then = CLOSED_MS.load(Ordering::SeqCst);
        if then == 0 {
            return false;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0);
        now.saturating_sub(then) < 500
    }

    unsafe fn view_identifier(view: id) -> String {
        if view.is_null() {
            return String::new();
        }
        let ident: id = msg_send![view, identifier];
        if ident.is_null() {
            return String::new();
        }
        let bytes: *const i8 = msg_send![ident, UTF8String];
        if bytes.is_null() {
            return String::new();
        }
        CStr::from_ptr(bytes).to_string_lossy().into_owned()
    }

    unsafe fn install(popover: id, view: id, width: f64, height: f64) {
        let controller: id = msg_send![class!(NSViewController), alloc];
        let controller: id = msg_send![controller, init];
        let _: () = msg_send![controller, setView: view];
        let _: () = msg_send![popover, setContentSize: NSSize::new(width, height)];
        let _: () = msg_send![popover, setContentViewController: controller];
        let _: () = msg_send![view, release];
        let _: () = msg_send![controller, release];
    }

    unsafe fn build_content(
        lines: &[GlanceLine],
        appearance: glance_core::Appearance,
    ) -> (id, f64, f64) {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let heights: Vec<f64> = lines.iter().map(line_height).collect();
        let height = PAD + heights.iter().sum::<f64>() + PAD;
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, height));
        let view: id = msg_send![class!(NSView), alloc];
        let view: id = msg_send![view, initWithFrame: frame];
        let primary: id = msg_send![class!(NSColor), labelColor];
        let secondary: id = msg_send![class!(NSColor), secondaryLabelColor];
        let inner = WIDTH - PAD * 2.0;
        let mut cursor = height - PAD;
        for (line, line_h) in lines.iter().zip(heights) {
            cursor -= line_h;
            place_line(
                view, line, cursor, inner, line_h, primary, secondary, appearance,
            );
        }
        let _: () = msg_send![pool, drain];
        (view, WIDTH, height)
    }

    unsafe fn place_line(
        parent: id,
        line: &GlanceLine,
        bottom: f64,
        inner: f64,
        line_h: f64,
        primary: id,
        secondary: id,
        appearance: glance_core::Appearance,
    ) {
        match line {
            GlanceLine::Status(text) => {
                let text_h = 30.0;
                add_label(
                    parent,
                    text,
                    NSRect::new(
                        NSPoint::new(PAD, bottom + (line_h - text_h) / 2.0),
                        NSSize::new(inner, text_h),
                    ),
                    22.0,
                    WEIGHT_SEMIBOLD,
                    primary,
                    ALIGN_CENTER,
                );
            }
            GlanceLine::PnlLabel(text) | GlanceLine::Section(text) => {
                add_label(
                    parent,
                    text,
                    NSRect::new(NSPoint::new(PAD, bottom), NSSize::new(inner, 16.0)),
                    12.0,
                    WEIGHT_SEMIBOLD,
                    secondary,
                    ALIGN_LEFT,
                );
            }
            GlanceLine::PnlAmount { text, tone } => {
                add_label(
                    parent,
                    text,
                    NSRect::new(NSPoint::new(PAD, bottom), NSSize::new(inner, 30.0)),
                    26.0,
                    WEIGHT_SEMIBOLD,
                    tone_color(*tone, appearance),
                    ALIGN_LEFT,
                );
            }
            GlanceLine::PnlPercent { text, tone } => {
                add_label(
                    parent,
                    text,
                    NSRect::new(NSPoint::new(PAD, bottom), NSSize::new(inner, 18.0)),
                    13.0,
                    WEIGHT_REGULAR,
                    tone_color(*tone, appearance),
                    ALIGN_LEFT,
                );
            }
            GlanceLine::Metric { label, value } => {
                let label_w = inner * 0.46;
                add_label(
                    parent,
                    label,
                    NSRect::new(NSPoint::new(PAD, bottom), NSSize::new(label_w, 20.0)),
                    13.0,
                    WEIGHT_REGULAR,
                    secondary,
                    ALIGN_LEFT,
                );
                add_label(
                    parent,
                    value,
                    NSRect::new(
                        NSPoint::new(PAD + label_w, bottom),
                        NSSize::new(inner - label_w, 20.0),
                    ),
                    13.0,
                    WEIGHT_SEMIBOLD,
                    primary,
                    ALIGN_RIGHT,
                );
            }
            GlanceLine::Todo {
                id,
                label,
                enabled: true,
            } => {
                add_button(
                    parent,
                    label,
                    id,
                    NSRect::new(NSPoint::new(PAD, bottom + 4.0), NSSize::new(inner, 28.0)),
                );
            }
            GlanceLine::Todo {
                label,
                enabled: false,
                ..
            }
            | GlanceLine::Note(label) => {
                add_label(
                    parent,
                    label,
                    NSRect::new(NSPoint::new(PAD, bottom), NSSize::new(inner, 18.0)),
                    12.0,
                    WEIGHT_REGULAR,
                    secondary,
                    ALIGN_LEFT,
                );
            }
            GlanceLine::Workbench { id, label } => {
                add_button(
                    parent,
                    label,
                    id,
                    NSRect::new(NSPoint::new(PAD, bottom), NSSize::new(inner, 32.0)),
                );
            }
        }
    }

    fn line_height(line: &GlanceLine) -> f64 {
        match line {
            GlanceLine::Status(_) => 48.0,
            GlanceLine::PnlLabel(_) => 18.0,
            GlanceLine::PnlAmount { .. } => 34.0,
            GlanceLine::PnlPercent { .. } => 20.0,
            GlanceLine::Metric { .. } => 26.0,
            GlanceLine::Section(_) => 28.0,
            GlanceLine::Todo { .. } => 36.0,
            GlanceLine::Note(_) => 22.0,
            GlanceLine::Workbench { .. } => 50.0,
        }
    }

    unsafe fn add_label(
        parent: id,
        text: &str,
        frame: NSRect,
        size: f64,
        weight: f64,
        color: id,
        align: usize,
    ) {
        let string = NSString::alloc(nil).init_str(text);
        let field: id = msg_send![class!(NSTextField), labelWithString: string];
        let _: () = msg_send![string, release];
        let font: id = msg_send![class!(NSFont), systemFontOfSize: size weight: weight];
        let _: () = msg_send![field, setFont: font];
        let _: () = msg_send![field, setTextColor: color];
        let _: () = msg_send![field, setAlignment: align];
        let _: () = msg_send![field, setLineBreakMode: 4usize];
        let _: () = msg_send![field, setMaximumNumberOfLines: 1isize];
        let _: () = msg_send![field, setFrame: frame];
        let _: () = msg_send![field, setRefusesFirstResponder: YES];
        let _: () = msg_send![parent, addSubview: field];
    }

    unsafe fn add_button(parent: id, title: &str, action_id: &str, frame: NSRect) {
        let string = NSString::alloc(nil).init_str(title);
        let button: id = msg_send![class!(NSButton), buttonWithTitle: string target: controller() action: sel!(onClick:)];
        let _: () = msg_send![string, release];
        let ident = NSString::alloc(nil).init_str(action_id);
        let _: () = msg_send![button, setIdentifier: ident];
        let _: () = msg_send![ident, release];
        let _: () = msg_send![button, setBezelStyle: 1usize];
        let font: id = msg_send![class!(NSFont), systemFontOfSize: 13.0 weight: WEIGHT_REGULAR];
        let _: () = msg_send![button, setFont: font];
        let _: () = msg_send![button, setFrame: frame];
        let _: () = msg_send![parent, addSubview: button];
    }

    unsafe fn tone_color(tone: Tone, appearance: glance_core::Appearance) -> id {
        let [red, green, blue, alpha] = tone_rgba(tone, appearance);
        msg_send![class!(NSColor), colorWithCalibratedRed: f64::from(red) / 255.0 green: f64::from(green) / 255.0 blue: f64::from(blue) / 255.0 alpha: f64::from(alpha) / 255.0]
    }
}

#[cfg(target_os = "macos")]
pub use mac::{hide, on_left_mouse_down, on_left_mouse_up, refresh};

#[cfg(not(target_os = "macos"))]
pub fn on_left_mouse_down() {}

#[cfg(not(target_os = "macos"))]
pub fn on_left_mouse_up(_app: &tauri::AppHandle) {}

#[cfg(not(target_os = "macos"))]
pub fn hide() {}

#[cfg(not(target_os = "macos"))]
pub fn refresh(_app: &tauri::AppHandle) {}
