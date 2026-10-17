//! Windows implementation: system appearance queries, DWM window attributes, change notifications.
//!
//! Minimum builds (Microsoft Learn, DWMWINDOWATTRIBUTE / DWM_SYSTEMBACKDROP_TYPE):
//! - DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWA_BORDER_COLOR: 22000
//! - DWMWA_SYSTEMBACKDROP_TYPE: 22621

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::winit::platform::windows::{
    BackdropType, CornerPreference, WindowAttributesExtWindows,
};
use windows::core::{w, BOOL, PCWSTR};
use windows::Foundation::TypedEventHandler;
use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMSBT_MAINWINDOW, DWMSBT_NONE,
    DWMSBT_TABBEDWINDOW, DWMSBT_TRANSIENTWINDOW, DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT,
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND, DWMWINDOWATTRIBUTE,
};
use windows::Win32::Graphics::Gdi::{
    GetSysColor, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW,
    COLOR_WINDOWTEXT,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, SystemParametersInfoW,
    GWL_EXSTYLE, SPI_GETHIGHCONTRAST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WM_DPICHANGED,
    WM_DWMCOLORIZATIONCOLORCHANGED, WM_DWMCOMPOSITIONCHANGED, WM_NCACTIVATE, WM_SETTINGCHANGE,
    WM_SYSCOLORCHANGE, WM_THEMECHANGED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows::UI::ViewManagement::UISettings;

use crate::appearance::{
    colorref_from_rgb, rgb_from_colorref, text_scale_from_factor, text_scale_from_percent,
    BackdropKind, ChromeMaterial, Config, ExtendFrame, HighContrastColors, Rgb, SolidReason,
    SystemAppearance, MIN_BUILD_SYSTEM_BACKDROP,
};
use crate::{schedule_refresh, PanelWindow};

const PERSONALIZE: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
const ACCESSIBILITY: PCWSTR = w!("Software\\Microsoft\\Accessibility");
const SUBCLASS_ID: usize = 0x5334; // "S4"

static KEEP_ACTIVE_LOOK: AtomicBool = AtomicBool::new(false);
/// The `lParam` string of the last WM_SETTINGCHANGE (e.g. "ImmersiveColorSet"), for diagnostics.
static LAST_SETTING_CHANGE: Mutex<String> = Mutex::new(String::new());

thread_local! {
    /// WinRT UISettings: the documented source for text scale and transparency effects.
    /// Kept alive for the process so its change events keep firing.
    static UI_SETTINGS: RefCell<Option<UISettings>> = const { RefCell::new(None) };
}

// ---------------------------------------------------------------------------------------------
// System appearance
// ---------------------------------------------------------------------------------------------

fn os_build() -> Option<u32> {
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // RtlGetVersion is not subject to the GetVersionEx manifest shim.
    let status = unsafe { RtlGetVersion(&mut info) };
    (status.is_ok() && info.dwMajorVersion == 10).then_some(info.dwBuildNumber)
}

fn reg_dword(subkey: PCWSTR, value: PCWSTR) -> Option<u32> {
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    (rc == ERROR_SUCCESS).then_some(data)
}

fn high_contrast_on() -> bool {
    let mut hc = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            Some(&mut hc as *mut HIGHCONTRASTW as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_ok() && hc.dwFlags.contains(HCF_HIGHCONTRASTON)
}

fn sys_colors() -> HighContrastColors {
    let c = |i| rgb_from_colorref(unsafe { GetSysColor(i) });
    HighContrastColors {
        window: c(COLOR_WINDOW),
        window_text: c(COLOR_WINDOWTEXT),
        highlight: c(COLOR_HIGHLIGHT),
        highlight_text: c(COLOR_HIGHLIGHTTEXT),
        gray_text: c(COLOR_GRAYTEXT),
    }
}

fn ui_settings() -> Option<UISettings> {
    UI_SETTINGS.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = UISettings::new().ok();
        }
        cell.clone()
    })
}

pub fn read_appearance() -> SystemAppearance {
    let os_build = os_build();
    let reg_light = reg_dword(PERSONALIZE, w!("AppsUseLightTheme"));
    let reg_transparency = reg_dword(PERSONALIZE, w!("EnableTransparency"));
    let reg_text = reg_dword(ACCESSIBILITY, w!("TextScaleFactor"));
    let ui = ui_settings();
    let uis_transparency = ui.as_ref().and_then(|s| s.AdvancedEffectsEnabled().ok());
    let uis_text = ui.as_ref().and_then(|s| s.TextScaleFactor().ok());
    let high_contrast = high_contrast_on();

    let text_scale = uis_text
        .map(text_scale_from_factor)
        .or(reg_text.map(text_scale_from_percent))
        .unwrap_or(1.0);
    let last_setting = LAST_SETTING_CHANGE
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default();

    SystemAppearance {
        os_build,
        apps_use_light_theme: reg_light.is_none_or(|v| v != 0),
        transparency_effects: uis_transparency
            .or(reg_transparency.map(|v| v != 0))
            .unwrap_or(true),
        high_contrast,
        text_scale,
        hc_colors: high_contrast.then(sys_colors),
        sources: format!(
            "reg AppsUseLightTheme={reg_light:?} EnableTransparency={reg_transparency:?} \
             TextScaleFactor={reg_text:?}; UISettings AdvancedEffects={uis_transparency:?} \
             TextScale={uis_text:?}; last WM_SETTINGCHANGE={last_setting:?}"
        ),
    }
}

// ---------------------------------------------------------------------------------------------
// Window creation and DWM attributes
// ---------------------------------------------------------------------------------------------

fn backdrop_type(material: ChromeMaterial) -> BackdropType {
    match material {
        ChromeMaterial::Backdrop(BackdropKind::Mica) => BackdropType::MainWindow,
        ChromeMaterial::Backdrop(BackdropKind::MicaAlt) => BackdropType::TabbedWindow,
        ChromeMaterial::Backdrop(BackdropKind::Acrylic) => BackdropType::TransientWindow,
        ChromeMaterial::Solid(_) => BackdropType::None,
    }
}

/// Creation-time attributes via winit, so DWM has them before the first frame is presented.
/// Slint already creates every winit window with `with_transparent(true)` on Windows, which
/// makes winit call `DwmEnableBlurBehindWindow` with an empty region: that is what lets the
/// renderer's per-pixel alpha reach DWM.
pub fn configure_backend(
    selector: slint::BackendSelector,
    cfg: Config,
    initial: ChromeMaterial,
) -> slint::BackendSelector {
    selector.with_winit_window_attributes_hook(move |attrs| {
        attrs
            .with_undecorated_shadow(cfg.undecorated_shadow)
            .with_corner_preference(CornerPreference::Round)
            .with_system_backdrop(backdrop_type(initial))
    })
}

fn hwnd_of(window: &slint::Window) -> Option<HWND> {
    let handle = window.window_handle();
    match handle.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut c_void)),
        _ => None,
    }
}

fn set_attr<T>(hwnd: HWND, attr: DWMWINDOWATTRIBUTE, value: &T) -> windows::core::Result<()> {
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attr,
            value as *const T as *const c_void,
            std::mem::size_of::<T>() as u32,
        )
    }
}

fn outcome(r: &windows::core::Result<()>) -> String {
    match r {
        Ok(()) => "ok".into(),
        Err(e) => format!("{:#010X}", e.code().0),
    }
}

/// Applies dark mode, corners, border colour, frame extension and the system backdrop.
/// Returns the material actually in effect (a failed backdrop call degrades to solid).
pub fn apply_window_material(
    window: &slint::Window,
    a: &SystemAppearance,
    wanted: ChromeMaterial,
    cfg: &Config,
    border: Rgb,
) -> (ChromeMaterial, String) {
    let Some(hwnd) = hwnd_of(window) else {
        return (wanted, "no HWND yet".into());
    };
    let mut report = Vec::new();

    // Mica's tint follows this flag, not the system theme: without it Mica stays light.
    let dark = BOOL::from(a.use_dark_tokens());
    report.push(format!(
        "dark={}",
        outcome(&set_attr(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark))
    ));

    if a.is_win11() {
        report.push(format!(
            "corners={}",
            outcome(&set_attr(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &DWMWCP_ROUND
            ))
        ));
        // Depth level 3: DWM shadow + 1px border.default. High contrast keeps the system border.
        let colorref = if a.high_contrast {
            DWMWA_COLOR_DEFAULT
        } else {
            colorref_from_rgb(border)
        };
        report.push(format!(
            "border={}",
            outcome(&set_attr(hwnd, DWMWA_BORDER_COLOR, &colorref))
        ));
    }

    let margins = match cfg.extend_frame {
        ExtendFrame::Full => MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        },
        ExtendFrame::OnePixel => MARGINS {
            cyTopHeight: 1,
            ..Default::default()
        },
        ExtendFrame::None => MARGINS::default(),
    };
    report.push(format!(
        "extend={:?}:{}",
        cfg.extend_frame,
        outcome(&unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) })
    ));

    let mut actual = wanted;
    if a.os_build.is_some_and(|b| b >= MIN_BUILD_SYSTEM_BACKDROP) {
        let sbt = match wanted {
            ChromeMaterial::Backdrop(BackdropKind::Mica) => DWMSBT_MAINWINDOW,
            ChromeMaterial::Backdrop(BackdropKind::MicaAlt) => DWMSBT_TABBEDWINDOW,
            ChromeMaterial::Backdrop(BackdropKind::Acrylic) => DWMSBT_TRANSIENTWINDOW,
            ChromeMaterial::Solid(_) => DWMSBT_NONE,
        };
        let r = set_attr(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &sbt);
        report.push(format!("backdrop={}:{}", sbt.0, outcome(&r)));
        if r.is_err() && matches!(wanted, ChromeMaterial::Backdrop(_)) {
            actual = ChromeMaterial::Solid(SolidReason::BackdropCallFailed);
        }
    }

    (actual, report.join(" "))
}

// ---------------------------------------------------------------------------------------------
// Change notifications
// ---------------------------------------------------------------------------------------------

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_SETTINGCHANGE => {
            if lparam.0 != 0 {
                let area =
                    unsafe { PCWSTR(lparam.0 as *const u16).to_string() }.unwrap_or_default();
                if let Ok(mut last) = LAST_SETTING_CHANGE.lock() {
                    *last = area;
                }
            }
            schedule_refresh("WM_SETTINGCHANGE");
        }
        WM_THEMECHANGED => schedule_refresh("WM_THEMECHANGED"),
        WM_SYSCOLORCHANGE => schedule_refresh("WM_SYSCOLORCHANGE"),
        WM_DWMCOMPOSITIONCHANGED => schedule_refresh("WM_DWMCOMPOSITIONCHANGED"),
        WM_DWMCOLORIZATIONCOLORCHANGED => schedule_refresh("WM_DWMCOLORIZATIONCOLORCHANGED"),
        WM_DPICHANGED => schedule_refresh("WM_DPICHANGED"),
        WM_NCACTIVATE if KEEP_ACTIVE_LOOK.load(Ordering::Relaxed) => {
            // Let winit see the real activation state first, then repaint the frame as active
            // (bypassing winit) so DWM keeps Mica's wallpaper tint. lParam -1: no NC repaint.
            let r = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            unsafe { DefWindowProcW(hwnd, WM_NCACTIVATE, WPARAM(1), LPARAM(-1)) };
            return r;
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

fn watch_ui_settings() -> &'static str {
    let Some(settings) = ui_settings() else {
        return "UISettings unavailable";
    };
    let handler = |reason: &'static str| {
        TypedEventHandler::<UISettings, windows::core::IInspectable>::new(move |_, _| {
            schedule_refresh(reason);
            Ok(())
        })
    };
    let a = settings.TextScaleFactorChanged(&handler("UISettings.TextScaleFactorChanged"));
    let b = settings
        .AdvancedEffectsEnabledChanged(&handler("UISettings.AdvancedEffectsEnabledChanged"));
    let c = settings.ColorValuesChanged(&handler("UISettings.ColorValuesChanged"));
    if a.is_ok() && b.is_ok() && c.is_ok() {
        "UISettings events registered"
    } else {
        "UISettings events partly unavailable"
    }
}

/// One-time set-up once the HWND exists: optional panel styles, the subclass that watches
/// system-change messages, and the WinRT UISettings events.
pub fn on_window_created(app: &PanelWindow, cfg: &Config) {
    use slint::ComponentHandle;
    let Some(hwnd) = hwnd_of(app.window()) else {
        eprintln!("[s4] no HWND after creation");
        return;
    };

    if cfg.no_activate {
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize,
            );
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    KEEP_ACTIVE_LOOK.store(cfg.keep_active_look, Ordering::Relaxed);
    if cfg.keep_active_look {
        unsafe { DefWindowProcW(hwnd, WM_NCACTIVATE, WPARAM(1), LPARAM(-1)) };
    }

    let subclassed = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) };
    eprintln!(
        "[s4] subclass={} {}",
        subclassed.as_bool(),
        watch_ui_settings()
    );
}
