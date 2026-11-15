//! The screen eyedropper: a freeze-frame of every monitor under one overlay window each, a loupe
//! that follows the mouse, and the pick.
//!
//! [`WinEyedropper`] implements [`Eyedropper`]. One run is one thread:
//!
//! 1. the thread becomes per-monitor-v2 aware, so every coordinate below is in physical pixels;
//! 2. it waits `settle` (the Panel hiding) and `DwmFlush`es;
//! 3. it lists the monitors (rectangle, device name, scale) and, with the display configuration
//!    API, which of them are in HDR mode;
//! 4. it copies each monitor with `BitBlt(.. SRCCOPY | CAPTUREBLT)` into a 32-bit top-down DIB
//!    section (per monitor, so no buffer spans a gap or a negative origin), keeps the DIB to
//!    paint the overlay from, and copies the bytes into a portable [`Shot`];
//! 5. it opens one `WS_POPUP` overlay per monitor, on that monitor, and takes the foreground;
//!    if it did not get it 100 ms later it registers Esc / Enter / Space / arrows as hotkeys for
//!    the rest of the run;
//! 6. messages become [`Input`] for the portable [`Session`], whose [`Step`] says what to draw,
//!    where to move the cursor and when the run is over;
//! 7. everything is released (hotkeys, timers, windows, class, bitmaps, DCs) and `done` is called.
//!
//! The decisions (sampling, loupe place, nudging, zoom) are in [`crate::eyedrop`], tested on
//! Linux. What is here is the Win32 glue. Every GDI object is owned by a guard that frees it on
//! every path; the run's state lives in a thread-local because the window procedure has no other
//! way to reach it, and it is never borrowed across a call that can dispatch messages.
//!
//! Values are what GDI composed (what apps drew), before Night light and colour filters. Windows
//! with `WDA_MONITOR` affinity and protected video capture black; `WDA_EXCLUDEFROMCAPTURE`
//! windows are absent.
//!
//! **Unverified on Windows** (`tools/windows-test/tests/colour.json`).

use std::cell::RefCell;
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
    QueryDisplayConfig,
};
use windows::Win32::Foundation::{
    COLORREF, ERROR_CLASS_ALREADY_EXISTS, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, GetLastError,
    HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CAPTUREBLT, CLIP_DEFAULT_PRECIS,
    COLORONCOLOR, CreateCompatibleBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW,
    CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CALCRECT, DT_CENTER,
    DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, DeleteDC, DeleteObject,
    DrawTextW, EndPaint, EnumDisplayMonitors, FF_DONTCARE, FillRect, GdiFlush, GetDC,
    GetMonitorInfoW, HBRUSH, HDC, HFONT, HGDIOBJ, HMONITOR, InvalidateRect, MONITORINFO,
    MONITORINFOEXW, OUT_DEFAULT_PRECIS, PAINTSTRUCT, ReleaseDC, SRCCOPY, SelectObject, SetBkMode,
    SetStretchBltMode, SetTextColor, StretchBlt, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::NotifyWinEvent;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, HOT_KEY_MODIFIERS, MOD_SHIFT, RegisterHotKey, ReleaseCapture, SetCapture,
    SetFocus, UnregisterHotKey, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SHIFT,
    VK_SPACE, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    EVENT_OBJECT_NAMECHANGE, GetForegroundWindow, GetMessageW, IDC_CROSS, IsWindow, KillTimer,
    LoadCursorW, MSG, OBJID_WINDOW, PostQuitMessage, RegisterClassExW, SW_SHOW, SetCursorPos,
    SetForegroundWindow, SetTimer, SetWindowTextW, ShowWindow, TranslateMessage, UnregisterClassW,
    WM_ACTIVATEAPP, WM_CAPTURECHANGED, WM_CLOSE, WM_DISPLAYCHANGE, WM_ERASEBKGND, WM_HOTKEY,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{BOOL, HSTRING, PCWSTR, w};

use super::dpi;
use super::screen;
use crate::error::Error;
use crate::eyedrop::{
    CELLS, EyedropDone, EyedropOutcome, EyedropTexts, Eyedropper, Input, Nav, Session, Shot, Step,
    hex_text, loupe_cells, loupe_rect, scaled, shot_at, signed, window_title,
};
use crate::route::ScreenRect;

/// The overlay's window class.
const CLASS_NAME: &str = "SnipdeckEyedropOverlay";

/// The timer that checks, 100 ms after the overlay opened, whether it got the foreground.
const TIMER_FOCUS: usize = 1;
/// The timer that ends a run nobody finished.
const TIMER_TIMEOUT: usize = 2;
/// Wait for the foreground before falling back to hotkeys.
const FOCUS_CHECK_MS: u32 = 100;
/// How long the overlay may stay open.
const TIMEOUT_MS: u32 = 120_000;

/// The keys registered as hotkeys when the overlay could not take the foreground: id, virtual
/// key and whether Shift is part of the chord. Ids are per window, so they cannot clash with the
/// app's own. See [`hotkey_input`] for what each stands for.
const HOTKEYS: [(i32, u16, bool); 11] = [
    (1, VK_ESCAPE.0, false),
    (2, VK_RETURN.0, false),
    (3, VK_SPACE.0, false),
    (4, VK_LEFT.0, false),
    (5, VK_RIGHT.0, false),
    (6, VK_UP.0, false),
    (7, VK_DOWN.0, false),
    (8, VK_LEFT.0, true),
    (9, VK_RIGHT.0, true),
    (10, VK_UP.0, true),
    (11, VK_DOWN.0, true),
];

/// The input a hotkey id of [`HOTKEYS`] stands for.
fn hotkey_input(id: i32) -> Option<Input> {
    const NAVS: [Nav; 4] = [Nav::Left, Nav::Right, Nav::Up, Nav::Down];
    match id {
        1 => Some(Input::Cancel),
        2 | 3 => Some(Input::Pick),
        4..=11 => Some(Input::Key {
            nav: *NAVS.get(usize::try_from(id - 4).ok()? % 4)?,
            shift: id >= 8,
        }),
        _ => None,
    }
}

/// The screen eyedropper. See the module documentation.
#[derive(Debug, Default)]
pub struct WinEyedropper {
    running: Arc<AtomicBool>,
}

impl WinEyedropper {
    /// An eyedropper with no run going.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Eyedropper for WinEyedropper {
    fn start(
        &mut self,
        texts: EyedropTexts,
        settle: Duration,
        done: EyedropDone,
    ) -> Result<(), Error> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(Error::Unavailable("an eyedropper run is already going"));
        }
        let running = Arc::clone(&self.running);
        let spawned = std::thread::Builder::new()
            .name("eyedropper".into())
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| run(&texts, settle)))
                    .unwrap_or_else(|_| {
                        EyedropOutcome::Failed("the eyedropper stopped unexpectedly".to_owned())
                    });
                // Cleared before `done`, so `done` may start the next run.
                running.store(false, Ordering::SeqCst);
                done(outcome);
            });
        if spawned.is_err() {
            self.running.store(false, Ordering::SeqCst);
            return Err(Error::Unavailable("could not start the eyedropper thread"));
        }
        Ok(())
    }

    fn running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// The whole run on its thread.
fn run(texts: &EyedropTexts, settle: Duration) -> EyedropOutcome {
    match try_run(texts, settle) {
        Ok(outcome) => outcome,
        Err(error) => EyedropOutcome::Failed(error.to_string()),
    }
}

// --- GDI guards ---------------------------------------------------------------------------

/// The screen's device context (`GetDC(None)`), released on drop.
struct ScreenDc(HDC);

impl ScreenDc {
    fn get() -> Result<Self, Error> {
        // SAFETY: no preconditions; `None` asks for the DC of the whole screen.
        let dc = unsafe { GetDC(None) };
        if dc.is_invalid() {
            return Err(Error::Unavailable("the screen has no device context"));
        }
        Ok(Self(dc))
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: the DC came from `GetDC(None)` and is released once, here.
        unsafe { ReleaseDC(None, self.0) };
    }
}

/// A memory device context, deleted on drop.
struct MemDc(HDC);

impl MemDc {
    fn new(reference: HDC) -> Result<Self, Error> {
        // SAFETY: `reference` is a live DC; the result is owned by the guard.
        let dc = unsafe { CreateCompatibleDC(Some(reference)) };
        if dc.is_invalid() {
            return Err(Error::Unavailable(
                "could not create a memory device context",
            ));
        }
        Ok(Self(dc))
    }
}

impl Drop for MemDc {
    fn drop(&mut self) {
        // SAFETY: the DC came from `CreateCompatibleDC` and is deleted once, here. The
        // owner has already selected its bitmap out.
        let _ = unsafe { DeleteDC(self.0) };
    }
}

/// A GDI object (bitmap, brush, font) deleted on drop. It must not be selected into a DC then.
struct GdiObject(HGDIOBJ);

impl GdiObject {
    fn is_invalid(&self) -> bool {
        self.0.is_invalid()
    }
}

impl Drop for GdiObject {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the object was created by this module and is deleted once, here, after the
            // owner selected it out of any DC.
            let _ = unsafe { DeleteObject(self.0) };
        }
    }
}

/// A colour for GDI, from 8-bit channels.
fn colorref(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16)
}

fn solid_brush(r: u8, g: u8, b: u8) -> (HBRUSH, GdiObject) {
    // SAFETY: plain call; a null brush (failure) is handled by `FillRect` failing harmlessly and
    // by `GdiObject` not deleting it.
    let brush = unsafe { CreateSolidBrush(colorref(r, g, b)) };
    (brush, GdiObject(HGDIOBJ(brush.0)))
}

/// A bitmap with a memory DC it is selected into: the frozen monitor, the loupe's off-screen
/// buffer and the 11 x 11 source of the magnified cells.
struct Surface {
    // Field order is drop order: the bitmap goes after `Drop::drop` selected it out, then the DC.
    _bitmap: GdiObject,
    dc: MemDc,
    previous: HGDIOBJ,
    /// The pixels of a DIB section (`None` for a compatible bitmap).
    bits: Option<*mut u32>,
}

impl Surface {
    /// A 32-bit top-down DIB section of `w` x `h`; its pixels are `0x00RRGGBB`.
    fn dib(reference: HDC, w: i32, h: i32) -> Result<Self, Error> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap_or(0),
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: `info` is fully initialised and outlives the call; `bits` is a valid
        // out-pointer; no file mapping (`None`, offset 0).
        let bitmap = unsafe {
            CreateDIBSection(
                Some(reference),
                &raw const info,
                DIB_RGB_COLORS,
                &raw mut bits,
                None,
                0,
            )
        }
        .map_err(|e| Error::from_windows("CreateDIBSection", &e))?;
        let bitmap = GdiObject(HGDIOBJ(bitmap.0));
        if bits.is_null() {
            return Err(Error::Unavailable("the bitmap has no pixels"));
        }
        Self::select(reference, bitmap, Some(bits.cast::<u32>()))
    }

    /// A bitmap compatible with `reference`, for drawing.
    fn compatible(reference: HDC, w: i32, h: i32) -> Result<Self, Error> {
        // SAFETY: `reference` is a live DC; the result is owned by the guard.
        let bitmap = unsafe { CreateCompatibleBitmap(reference, w, h) };
        let bitmap = GdiObject(HGDIOBJ(bitmap.0));
        if bitmap.is_invalid() {
            return Err(Error::Unavailable("could not create a bitmap"));
        }
        Self::select(reference, bitmap, None)
    }

    fn select(reference: HDC, bitmap: GdiObject, bits: Option<*mut u32>) -> Result<Self, Error> {
        let dc = MemDc::new(reference)?;
        // SAFETY: both handles are live; the bitmap is not selected into another DC.
        let previous = unsafe { SelectObject(dc.0, bitmap.0) };
        if previous.is_invalid() {
            return Err(Error::Unavailable("could not select a bitmap"));
        }
        Ok(Self {
            _bitmap: bitmap,
            dc,
            previous,
            bits,
        })
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: puts back the DC's original bitmap so that the field drops that follow can
        // delete our bitmap (a bitmap selected into a DC cannot be deleted).
        unsafe { SelectObject(self.dc.0, self.previous) };
    }
}

// --- monitors, capture, HDR -----------------------------------------------------------------

#[derive(Debug, Clone)]
struct MonitorFacts {
    device: String,
    bounds: ScreenRect,
    scale: f32,
}

fn rect_of(r: RECT) -> Option<ScreenRect> {
    Some(ScreenRect {
        x: r.left,
        y: r.top,
        w: u32::try_from(r.right.checked_sub(r.left)?).ok()?,
        h: u32::try_from(r.bottom.checked_sub(r.top)?).ok()?,
    })
}

/// The facts of one monitor, `None` if Windows will not say or it is empty.
fn facts_of(monitor: HMONITOR) -> Option<MonitorFacts> {
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: u32::try_from(size_of::<MONITORINFOEXW>()).ok()?,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: `info` is initialised with its size, as the call requires (a MONITORINFOEXW starts
    // with a MONITORINFO), and outlives the call.
    if !unsafe { GetMonitorInfoW(monitor, (&raw mut info).cast::<MONITORINFO>()) }.as_bool() {
        return None;
    }
    let device_len = info.szDevice.iter().position(|c| *c == 0).unwrap_or(0);
    let device = String::from_utf16_lossy(&info.szDevice[..device_len]);
    let (mut dpi_x, mut dpi_y) = (96_u32, 96_u32);
    // SAFETY: both out-parameters are valid and outlive the call; a failure leaves 96 DPI.
    let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y) };
    #[expect(
        clippy::cast_precision_loss,
        reason = "a DPI value is a few hundred at most"
    )]
    let scale = dpi_x.max(1) as f32 / 96.0;
    let bounds = rect_of(info.monitorInfo.rcMonitor)?;
    (bounds.w > 0 && bounds.h > 0).then_some(MonitorFacts {
        device,
        bounds,
        scale,
    })
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<MonitorFacts>` that `monitors` passed to
    // `EnumDisplayMonitors`, which calls this on the same thread while that borrow is alive.
    let list = unsafe { &mut *(data.0 as *mut Vec<MonitorFacts>) };
    if let Some(facts) = facts_of(monitor) {
        list.push(facts);
    }
    BOOL::from(true)
}

fn monitors() -> Vec<MonitorFacts> {
    let mut list: Vec<MonitorFacts> = Vec::new();
    // SAFETY: the callback only runs during this call, on this thread, and only touches the
    // vector whose address is passed as `data`.
    let _ = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM((&raw mut list) as isize),
        )
    };
    list
}

/// The active display paths, `None` when the display configuration cannot be read.
fn active_paths() -> Option<Vec<DISPLAYCONFIG_PATH_INFO>> {
    // The configuration can change between the size query and the read: retry a few times.
    for _ in 0..3 {
        let (mut path_count, mut mode_count) = (0_u32, 0_u32);
        // SAFETY: both counts are valid out-pointers.
        let sizes = unsafe {
            GetDisplayConfigBufferSizes(
                QDC_ONLY_ACTIVE_PATHS,
                &raw mut path_count,
                &raw mut mode_count,
            )
        };
        if sizes != ERROR_SUCCESS {
            return None;
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        // SAFETY: the arrays have exactly the element counts passed in, which the call updates
        // to the number it filled; no topology id is requested for active paths.
        let result = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &raw mut path_count,
                paths.as_mut_ptr(),
                &raw mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if result == ERROR_SUCCESS {
            paths.truncate(path_count as usize);
            return Some(paths);
        }
        if result != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
    }
    None
}

/// The GDI device names (`\\.\DISPLAY1`) of the monitors in HDR (advanced colour) mode. Empty
/// when the display configuration cannot be read: the pick is then not marked approximate.
fn hdr_devices() -> Vec<String> {
    let Some(paths) = active_paths() else {
        return Vec::new();
    };
    let header_size = |n: usize| u32::try_from(n).unwrap_or(0);
    let mut devices = Vec::new();
    for path in &paths {
        let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: header_size(size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>()),
                adapterId: path.sourceInfo.adapterId,
                id: path.sourceInfo.id,
            },
            ..Default::default()
        };
        // SAFETY: `name` is a DISPLAYCONFIG_SOURCE_DEVICE_NAME whose header carries its type and
        // size, as the call requires; it outlives the call.
        if unsafe { DisplayConfigGetDeviceInfo(&raw mut name.header) } != 0 {
            continue;
        }
        let mut colour = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
                size: header_size(size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>()),
                adapterId: path.targetInfo.adapterId,
                id: path.targetInfo.id,
            },
            ..Default::default()
        };
        // SAFETY: as above, for the advanced-colour request of the path's target.
        if unsafe { DisplayConfigGetDeviceInfo(&raw mut colour.header) } != 0 {
            continue;
        }
        // SAFETY: the union is plain bits; `value` is its whole-word view.
        let value = unsafe { colour.Anonymous.value };
        if is_hdr(value) {
            let len = name
                .viewGdiDeviceName
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(name.viewGdiDeviceName.len());
            devices.push(String::from_utf16_lossy(&name.viewGdiDeviceName[..len]));
        }
    }
    devices
}

/// Is a monitor with these `DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO` bits in HDR mode? Bit 0 is
/// `advancedColorSupported`, bit 1 `advancedColorEnabled`, bit 2 `wideColorEnforced`: advanced
/// colour that is on only because wide colour is enforced (SDR, wide gamut) is not HDR.
fn is_hdr(value: u32) -> bool {
    const ENABLED: u32 = 0b10;
    const WIDE_COLOR_ENFORCED: u32 = 0b100;
    value & ENABLED != 0 && value & WIDE_COLOR_ENFORCED == 0
}

/// Copies one monitor. Returns the portable shot and the surface the overlay paints from.
fn capture(screen: &ScreenDc, monitor: &MonitorFacts, hdr: bool) -> Result<(Shot, Surface), Error> {
    let too_big = || Error::Unavailable("a monitor is too large to capture");
    let w = i32::try_from(monitor.bounds.w).map_err(|_| too_big())?;
    let h = i32::try_from(monitor.bounds.h).map_err(|_| too_big())?;
    let surface = Surface::dib(screen.0, w, h)?;
    // SAFETY: destination and source DCs are live; the rectangle is inside the destination
    // bitmap. `CAPTUREBLT` includes layered windows.
    unsafe {
        BitBlt(
            surface.dc.0,
            0,
            0,
            w,
            h,
            Some(screen.0),
            monitor.bounds.x,
            monitor.bounds.y,
            SRCCOPY | CAPTUREBLT,
        )
    }
    .map_err(|e| Error::from_windows("BitBlt", &e))?;
    // The DIB is read directly below: finish any batched drawing first.
    // SAFETY: no preconditions.
    let _ = unsafe { GdiFlush() };
    let bits = surface
        .bits
        .ok_or(Error::Unavailable("the bitmap has no pixels"))?;
    let len = usize::try_from(w)
        .ok()
        .zip(usize::try_from(h).ok())
        .and_then(|(w, h)| w.checked_mul(h)?.checked_mul(4))
        .ok_or_else(too_big)?;
    // SAFETY: `bits` is the pixel memory of the 32-bit `w` x `h` DIB section owned by `surface`,
    // which is alive for this whole call: exactly `w * h * 4` readable bytes.
    let bgra = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), len) }.to_vec();
    let shot = Shot {
        bounds: monitor.bounds,
        scale: monitor.scale,
        hdr,
        bgra,
    };
    Ok((shot, surface))
}

// --- the run's state --------------------------------------------------------------------------

/// A destroyed-on-drop overlay window.
struct Overlay {
    hwnd: HWND,
}

impl Drop for Overlay {
    fn drop(&mut self) {
        // SAFETY: `IsWindow` is false for a handle that is already gone; the window belongs to
        // this thread, which is the only one to destroy it.
        if unsafe { IsWindow(Some(self.hwnd)) }.as_bool() {
            // SAFETY: as above.
            let _ = unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

/// The hotkeys registered for the run, unregistered on drop.
struct Hotkeys {
    hwnd: HWND,
    ids: Vec<i32>,
}

impl Drop for Hotkeys {
    fn drop(&mut self) {
        for id in &self.ids {
            // SAFETY: the id was registered for this window by this thread; a window that is
            // already gone makes the call fail harmlessly.
            let _ = unsafe { UnregisterHotKey(Some(self.hwnd), *id) };
        }
    }
}

/// The timers set for the run, killed on drop.
struct Timers {
    hwnd: HWND,
    ids: Vec<usize>,
}

impl Timers {
    fn set(&mut self, id: usize, ms: u32) {
        // SAFETY: a timer of this window, set from the window's own thread, no callback.
        if unsafe { SetTimer(Some(self.hwnd), id, ms, None) } != 0 {
            self.ids.push(id);
        }
    }

    fn kill(&mut self, id: usize) {
        if let Some(at) = self.ids.iter().position(|t| *t == id) {
            self.ids.swap_remove(at);
            // SAFETY: the timer was set for this window by this thread.
            let _ = unsafe { KillTimer(Some(self.hwnd), id) };
        }
    }
}

impl Drop for Timers {
    fn drop(&mut self) {
        for id in std::mem::take(&mut self.ids) {
            // SAFETY: as in `kill`.
            let _ = unsafe { KillTimer(Some(self.hwnd), id) };
        }
    }
}

/// The registered window class, unregistered on drop.
struct ClassRegistration;

impl Drop for ClassRegistration {
    fn drop(&mut self) {
        let class = HSTRING::from(CLASS_NAME);
        // SAFETY: the class name is a valid string for the call; the windows of the class are
        // destroyed before this guard drops (field order of `Run`).
        let _ = unsafe { UnregisterClassW(PCWSTR(class.as_ptr()), None) };
    }
}

/// Everything one run holds. Field order is drop order: hotkeys and timers go before the
/// windows they belong to, the windows before the bitmaps they paint and the class last.
struct Run {
    hotkeys: Hotkeys,
    timers: Timers,
    windows: Vec<Overlay>,
    frozen: Vec<Surface>,
    class: Option<ClassRegistration>,
    shots: Vec<Shot>,
    session: Session,
    texts: EyedropTexts,
    /// The overlay had its chance to take the foreground, so losing it now is the user leaving.
    armed: bool,
    /// The mouse button held on the overlay, if any.
    press: Press,
    /// The colour last put in the window title (`None`: the title without a colour).
    title: Option<[u8; 3]>,
    /// A title has been set at all (the first one has no colour to compare with).
    title_set: bool,
    outcome: Option<EyedropOutcome>,
}

thread_local! {
    /// The run of this thread. The window procedure reaches it through here.
    static RUN: RefCell<Option<Run>> = const { RefCell::new(None) };
}

/// Takes the run out of the thread-local when dropped, freeing it with the borrow released (its
/// windows' destruction messages reach the procedure, which must find the slot free).
struct RunSlot;

impl Drop for RunSlot {
    fn drop(&mut self) {
        let run = RUN.with(|slot| slot.borrow_mut().take());
        drop(run);
    }
}

fn try_run(texts: &EyedropTexts, settle: Duration) -> Result<EyedropOutcome, Error> {
    // Every coordinate from here on is physical, even on a mixed-DPI desktop.
    let _ = dpi::aware_thread();
    if !settle.is_zero() {
        std::thread::sleep(settle);
    }
    // The hidden Panel must be gone from the next capture.
    // SAFETY: no preconditions.
    let _ = unsafe { DwmFlush() };

    let facts = monitors();
    if facts.is_empty() {
        return Err(Error::Unavailable("no monitor to capture"));
    }
    let (shots, frozen) = capture_all(&facts)?;
    let start = screen::cursor_position().unwrap_or((
        facts[0]
            .bounds
            .x
            .saturating_add(signed(facts[0].bounds.w / 2)),
        facts[0]
            .bounds
            .y
            .saturating_add(signed(facts[0].bounds.h / 2)),
    ));
    let under_cursor = shots_index(&shots, start).unwrap_or(0);
    let mut state = Run {
        hotkeys: Hotkeys {
            hwnd: HWND::default(),
            ids: Vec::new(),
        },
        timers: Timers {
            hwnd: HWND::default(),
            ids: Vec::new(),
        },
        windows: Vec::new(),
        frozen,
        class: None,
        session: Session::new(&shots, start),
        shots,
        texts: texts.clone(),
        armed: false,
        title: None,
        title_set: false,
        press: Press::default(),
        outcome: None,
    };
    let handles = open_overlays(&mut state, &facts)?;

    RUN.with(|slot| *slot.borrow_mut() = Some(state));
    let slot_guard = RunSlot;

    for hwnd in &handles {
        // SAFETY: a window of this thread. The messages it sends reach the procedure, with the
        // run installed and not borrowed here.
        let _ = unsafe { ShowWindow(*hwnd, SW_SHOW) };
    }
    let focus = handles[under_cursor];
    // SAFETY: windows of this thread; both calls may be refused by the foreground lock, which
    // the focus timer notices.
    unsafe {
        let _ = SetForegroundWindow(focus);
        let _ = SetFocus(Some(focus));
    }
    // The colour under the cursor goes in the title for screen readers straight away.
    apply(&feed(None));

    let mut msg = MSG::default();
    let mut loop_failed = false;
    loop {
        // SAFETY: `msg` is a valid out-pointer; no window filter, so all of this thread's
        // messages are returned. 0 is `WM_QUIT`, -1 an error.
        let got = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
        if got.0 == 0 {
            break;
        }
        if got.0 < 0 {
            loop_failed = true;
            break;
        }
        // SAFETY: `msg` was filled in by `GetMessageW` just above.
        unsafe {
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }
    }
    let outcome = RUN.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .and_then(|run| run.outcome.take())
    });
    // The shots, bitmaps, windows, hotkeys, timers and class go here, with the guard.
    drop(slot_guard);
    outcome.ok_or(Error::Unavailable(if loop_failed {
        "the eyedropper's message loop failed"
    } else {
        "the eyedropper ended without a result"
    }))
}

/// Copies every monitor: the portable shots and the surfaces the overlays paint from.
fn capture_all(facts: &[MonitorFacts]) -> Result<(Vec<Shot>, Vec<Surface>), Error> {
    let hdr = hdr_devices();
    let mut shots = Vec::with_capacity(facts.len());
    let mut frozen = Vec::with_capacity(facts.len());
    let screen = ScreenDc::get()?;
    for monitor in facts {
        let (shot, surface) = capture(&screen, monitor, hdr.contains(&monitor.device))?;
        shots.push(shot);
        frozen.push(surface);
    }
    Ok((shots, frozen))
}

/// Registers the class, creates one overlay per monitor and sets the timers. Everything made is
/// owned by `state` at once, so an error part-way leaves nothing behind. Returns the windows.
fn open_overlays(state: &mut Run, facts: &[MonitorFacts]) -> Result<Vec<HWND>, Error> {
    register_class()?;
    state.class = Some(ClassRegistration);
    for monitor in facts {
        let hwnd = create_overlay(monitor)?;
        state.windows.push(Overlay { hwnd });
    }
    let first = state.windows[0].hwnd;
    state.hotkeys.hwnd = first;
    state.timers.hwnd = first;
    state.timers.set(TIMER_FOCUS, FOCUS_CHECK_MS);
    state.timers.set(TIMER_TIMEOUT, TIMEOUT_MS);
    Ok(state.windows.iter().map(|o| o.hwnd).collect())
}

/// The index of the shot that has `at`.
fn shots_index(shots: &[Shot], at: (i32, i32)) -> Option<usize> {
    shots.iter().position(|s| s.covers(at.0, at.1))
}

// --- windows ----------------------------------------------------------------------------------

fn register_class() -> Result<(), Error> {
    let class = HSTRING::from(CLASS_NAME);
    // SAFETY: a null module name asks for this process's own module handle.
    let hinstance = unsafe { GetModuleHandleW(None) }
        .map_err(|e| Error::from_windows("GetModuleHandleW", &e))?;
    // SAFETY: a stock cursor, shared and never freed.
    let cursor = unsafe { LoadCursorW(None, IDC_CROSS) }.unwrap_or_default();
    let info = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(0),
        lpfnWndProc: Some(overlay_proc),
        hInstance: hinstance.into(),
        hCursor: cursor,
        // No background brush: `WM_ERASEBKGND` is answered and `WM_PAINT` covers everything.
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    // SAFETY: `info` is fully initialised and its strings outlive the call; the window
    // procedure is a valid `extern "system"` function for the life of the process.
    if unsafe { RegisterClassExW(&raw const info) } == 0 {
        // SAFETY: reads this thread's last error, set by the call above.
        let code = unsafe { GetLastError() };
        // A class left over from an earlier run is the same class.
        if code != ERROR_CLASS_ALREADY_EXISTS {
            return Err(Error::Os {
                op: "RegisterClassExW",
                code: code.0,
            });
        }
    }
    Ok(())
}

/// A pop-up covering `monitor`, created on it so it gets that monitor's DPI.
fn create_overlay(monitor: &MonitorFacts) -> Result<HWND, Error> {
    let class = HSTRING::from(CLASS_NAME);
    // SAFETY: a null module name asks for this process's own module handle.
    let hinstance = unsafe { GetModuleHandleW(None) }
        .map_err(|e| Error::from_windows("GetModuleHandleW", &e))?;
    let title = HSTRING::from(window_title(None));
    // SAFETY: the class is registered by `register_class`; the strings outlive the call; no
    // parent, menu or creation parameter. The window belongs to this thread.
    unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            PCWSTR(class.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            monitor.bounds.x,
            monitor.bounds.y,
            signed(monitor.bounds.w),
            signed(monitor.bounds.h),
            None,
            None,
            Some(hinstance.into()),
            None,
        )
    }
    .map_err(|e| Error::from_windows("CreateWindowExW", &e))
}

// --- input ------------------------------------------------------------------------------------

/// What to do after an input, once the run is no longer borrowed.
#[derive(Default)]
struct Effects {
    cursor: Option<(i32, i32)>,
    /// Screen rectangles to repaint.
    invalidate: Vec<ScreenRect>,
    /// The windows (with the monitor each covers).
    windows: Vec<(HWND, ScreenRect)>,
    title: Option<String>,
    quit: bool,
}

/// The magnifier's box now, if the cursor is on a monitor.
fn current_loupe(run: &Run) -> Option<ScreenRect> {
    let (x, y) = run.session.cursor();
    let shot = shot_at(&run.shots, x, y)?;
    Some(loupe_rect(
        (x, y),
        shot.bounds,
        shot.scale,
        run.session.zoom(),
    ))
}

/// A mouse button that picks (left) or cancels (right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Left,
    Right,
}

/// Which button is held on the overlay. A pick or cancel happens when that button is let go, so
/// the button-up is swallowed by the overlay and never reaches (or opens a context menu in) the
/// window underneath, which a decision on the button-down would let through.
#[derive(Debug, Default)]
struct Press {
    held: Option<Button>,
}

impl Press {
    /// A button went down. True when it starts a press (the overlay then captures the mouse);
    /// a second button pressed while one is held is ignored.
    fn down(&mut self, button: Button) -> bool {
        if self.held.is_some() {
            return false;
        }
        self.held = Some(button);
        true
    }

    /// A button went up: the decision when it is the one held, `None` for any other.
    fn up(&mut self, button: Button) -> Option<Input> {
        if self.held != Some(button) {
            return None;
        }
        self.held = None;
        Some(match button {
            Button::Left => Input::Pick,
            Button::Right => Input::Cancel,
        })
    }

    /// The mouse capture was lost (another window took it): the press is over, with no decision.
    fn lost(&mut self) {
        self.held = None;
    }
}

/// Runs `f` on the run's press state; `None` when the run is gone or busy.
fn with_press<T>(f: impl FnOnce(&mut Press) -> T) -> Option<T> {
    RUN.with(|slot| {
        let mut slot = slot.try_borrow_mut().ok()?;
        slot.as_mut().map(|run| f(&mut run.press))
    })
}

/// Gives `input` (if any; `None` only refreshes the title) to the session and says what follows.
fn feed(input: Option<Input>) -> Effects {
    let mut effects = Effects::default();
    RUN.with(|slot| {
        // Busy means a message arrived while the run was being changed; drop it.
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return;
        };
        let Some(run) = slot.as_mut() else {
            return;
        };
        if run.outcome.is_some() {
            return;
        }
        let old = current_loupe(run);
        let step = input.map_or(Step::Nothing, |input| run.session.on(&run.shots, input));
        match step {
            Step::MoveCursor(x, y) => effects.cursor = Some((x, y)),
            Step::Picked(rgb) => {
                let (x, y) = run.session.cursor();
                let hdr = shot_at(&run.shots, x, y).is_some_and(|s| s.hdr);
                run.outcome = Some(EyedropOutcome::Picked { rgb, hdr });
                effects.quit = true;
            }
            Step::Cancelled => {
                run.outcome = Some(EyedropOutcome::Cancelled);
                effects.quit = true;
            }
            Step::Redraw | Step::Nothing => {}
        }
        if matches!(step, Step::Redraw | Step::MoveCursor(..)) {
            effects.invalidate.extend(old);
            effects.invalidate.extend(current_loupe(run));
        }
        let colour = run.session.colour(&run.shots);
        if !run.title_set || run.title != colour {
            run.title_set = true;
            run.title = colour;
            effects.title = Some(window_title(colour));
        }
        effects.windows = run
            .windows
            .iter()
            .zip(&run.shots)
            .map(|(overlay, shot)| (overlay.hwnd, shot.bounds))
            .collect();
    });
    effects
}

/// The part of `r` inside `monitor`, relative to the monitor's corner (a window's client area).
fn client_rect(r: ScreenRect, monitor: ScreenRect) -> Option<RECT> {
    let edge = |origin: i32, size: u32| i64::from(origin) + i64::from(size);
    let left = i64::from(r.x).max(i64::from(monitor.x));
    let top = i64::from(r.y).max(i64::from(monitor.y));
    let right = edge(r.x, r.w).min(edge(monitor.x, monitor.w));
    let bottom = edge(r.y, r.h).min(edge(monitor.y, monitor.h));
    if right <= left || bottom <= top {
        return None;
    }
    let rel = |v: i64, origin: i32| i32::try_from(v - i64::from(origin)).ok();
    Some(RECT {
        left: rel(left, monitor.x)?,
        top: rel(top, monitor.y)?,
        right: rel(right, monitor.x)?,
        bottom: rel(bottom, monitor.y)?,
    })
}

/// Does what [`feed`] decided. Nothing here is under a borrow, so it may dispatch messages.
fn apply(effects: &Effects) {
    if let Some((x, y)) = effects.cursor {
        // SAFETY: plain call in physical pixels (the thread is per-monitor aware).
        let _ = unsafe { SetCursorPos(x, y) };
    }
    for rect in &effects.invalidate {
        for (hwnd, monitor) in &effects.windows {
            if let Some(client) = client_rect(*rect, *monitor) {
                // SAFETY: a live window of this thread and a valid rectangle; no erase.
                let _ = unsafe { InvalidateRect(Some(*hwnd), Some(&raw const client), false) };
            }
        }
    }
    if let Some(title) = &effects.title {
        let text = HSTRING::from(title.as_str());
        for (hwnd, _) in &effects.windows {
            // SAFETY: a live window of this thread; the string outlives the call.
            let _ = unsafe { SetWindowTextW(*hwnd, PCWSTR(text.as_ptr())) };
            // SAFETY: announces the name change to assistive technology; plain values.
            unsafe {
                NotifyWinEvent(
                    EVENT_OBJECT_NAMECHANGE,
                    *hwnd,
                    OBJID_WINDOW.0,
                    i32::try_from(CHILDID_SELF).unwrap_or(0),
                );
            }
        }
    }
    if effects.quit {
        // SAFETY: ends this thread's message loop.
        unsafe { PostQuitMessage(0) };
    }
}

fn shift_down() -> bool {
    // SAFETY: no preconditions; the high bit is "down".
    unsafe { GetKeyState(i32::from(VK_SHIFT.0)) }.cast_unsigned() & 0x8000 != 0
}

fn mouse_input() -> Option<Input> {
    // The position comes from `GetCursorPos`, not `lParam`, so every overlay agrees.
    screen::cursor_position().map(|(x, y)| Input::Moved(x, y))
}

/// The input for a key, if it is one of ours.
fn key_input(vk: u16) -> Option<Input> {
    let nav = |nav| Input::Key {
        nav,
        shift: shift_down(),
    };
    match vk {
        v if v == VK_LEFT.0 => Some(nav(Nav::Left)),
        v if v == VK_RIGHT.0 => Some(nav(Nav::Right)),
        v if v == VK_UP.0 => Some(nav(Nav::Up)),
        v if v == VK_DOWN.0 => Some(nav(Nav::Down)),
        v if v == VK_RETURN.0 || v == VK_SPACE.0 => Some(Input::Pick),
        v if v == VK_ESCAPE.0 => Some(Input::Cancel),
        _ => None,
    }
}

/// The timers: the focus check (register the hotkeys if the foreground is not ours) and the
/// timeout.
fn on_timer(id: usize) {
    if id == TIMER_TIMEOUT {
        apply(&feed(Some(Input::TimedOut)));
        return;
    }
    if id != TIMER_FOCUS {
        return;
    }
    RUN.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return;
        };
        let Some(run) = slot.as_mut() else {
            return;
        };
        run.timers.kill(TIMER_FOCUS);
        run.armed = true;
        // SAFETY: no preconditions.
        let foreground = unsafe { GetForegroundWindow() };
        if run.windows.iter().any(|o| o.hwnd == foreground) {
            return;
        }
        // The foreground lock kept us out: take the keys the other way, for this run only.
        for (id, vk, shift) in HOTKEYS {
            let modifiers = if shift {
                MOD_SHIFT
            } else {
                HOT_KEY_MODIFIERS(0)
            };
            // SAFETY: registers for a window of this thread; a refusal (key taken by another
            // app) just leaves that key to the window.
            if unsafe { RegisterHotKey(Some(run.hotkeys.hwnd), id, modifiers, u32::from(vk)) }
                .is_ok()
            {
                run.hotkeys.ids.push(id);
            }
        }
    });
}

unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // A panic must not unwind through the Win32 caller; it ends the run instead.
    let handled = catch_unwind(AssertUnwindSafe(|| handle(hwnd, msg, wparam)));
    match handled {
        Ok(Some(result)) => result,
        Ok(None) => {
            // SAFETY: default handling of the same message with the same arguments.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        Err(_) => {
            RUN.with(|slot| {
                if let Ok(mut slot) = slot.try_borrow_mut()
                    && let Some(run) = slot.as_mut()
                {
                    run.outcome
                        .get_or_insert(EyedropOutcome::Failed("the eyedropper failed".to_owned()));
                }
            });
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
    }
}

fn handle(hwnd: HWND, msg: u32, wparam: WPARAM) -> Option<LRESULT> {
    let input = |input: Option<Input>| {
        apply(&feed(input));
        Some(LRESULT(0))
    };
    match msg {
        WM_ERASEBKGND => Some(LRESULT(1)),
        WM_PAINT => {
            paint(hwnd);
            Some(LRESULT(0))
        }
        WM_MOUSEMOVE => input(mouse_input()),
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            let button = if msg == WM_LBUTTONDOWN {
                Button::Left
            } else {
                Button::Right
            };
            if with_press(|p| p.down(button)).unwrap_or(false) {
                // SAFETY: a live window of this thread; the mouse is ours until the button-up.
                let _ = unsafe { SetCapture(hwnd) };
            }
            input(mouse_input())
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let button = if msg == WM_LBUTTONUP {
                Button::Left
            } else {
                Button::Right
            };
            // Always swallowed (no default handling, so no context menu from the right button).
            match with_press(|p| p.up(button)).flatten() {
                Some(decision) => {
                    // SAFETY: no preconditions; the capture this window set on the button-down.
                    let _ = unsafe { ReleaseCapture() };
                    apply(&feed(mouse_input()));
                    input(Some(decision))
                }
                None => Some(LRESULT(0)),
            }
        }
        WM_CAPTURECHANGED => {
            with_press(Press::lost);
            Some(LRESULT(0))
        }
        WM_CLOSE => input(Some(Input::Cancel)),
        WM_MOUSEWHEEL => {
            // The high word of wParam is the signed wheel delta.
            let word = u16::try_from((wparam.0 >> 16) & 0xFFFF).unwrap_or(0);
            let delta = i32::from(word.cast_signed());
            input(Some(Input::Wheel(delta)))
        }
        WM_KEYDOWN => key_input(u16::try_from(wparam.0).unwrap_or(0)).and_then(|i| input(Some(i))),
        WM_HOTKEY => {
            let id = i32::try_from(wparam.0).unwrap_or(0);
            hotkey_input(id).and_then(|i| input(Some(i)))
        }
        WM_ACTIVATEAPP if wparam.0 == 0 => {
            let armed = RUN.with(|slot| {
                slot.try_borrow()
                    .ok()
                    .and_then(|s| s.as_ref().map(|r| r.armed))
                    .unwrap_or(false)
            });
            if armed {
                input(Some(Input::LostFocus))
            } else {
                None
            }
        }
        WM_DISPLAYCHANGE => input(Some(Input::DisplayChanged)),
        WM_TIMER => {
            on_timer(wparam.0);
            Some(LRESULT(0))
        }
        _ => None,
    }
}

// --- painting ---------------------------------------------------------------------------------

fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: `ps` is a valid out-structure; paired with `EndPaint` below on every path, which
    // also stops the paint message from repeating.
    let hdc = unsafe { BeginPaint(hwnd, &raw mut ps) };
    if !hdc.is_invalid() {
        RUN.with(|slot| {
            if let Ok(slot) = slot.try_borrow()
                && let Some(run) = slot.as_ref()
            {
                paint_run(run, hwnd, hdc, &ps.rcPaint);
            }
        });
    }
    // SAFETY: ends the paint begun above with the same structure.
    let _ = unsafe { EndPaint(hwnd, &raw const ps) };
}

fn paint_run(run: &Run, hwnd: HWND, hdc: HDC, clip: &RECT) {
    let Some(index) = run.windows.iter().position(|o| o.hwnd == hwnd) else {
        return;
    };
    let (Some(shot), Some(frozen)) = (run.shots.get(index), run.frozen.get(index)) else {
        return;
    };
    let (width, height) = (clip.right - clip.left, clip.bottom - clip.top);
    // SAFETY: the paint DC and the frozen shot's memory DC are live; the rectangle is the
    // update region, inside the window and so inside the bitmap.
    let _ = unsafe {
        BitBlt(
            hdc,
            clip.left,
            clip.top,
            width,
            height,
            Some(frozen.dc.0),
            clip.left,
            clip.top,
            SRCCOPY,
        )
    };
    draw_hint(run, shot, hdc, clip);
    let Some(loupe) = current_loupe(run) else {
        return;
    };
    let (cx, cy) = run.session.cursor();
    if shot_at(&run.shots, cx, cy).map(|s| s.bounds) != Some(shot.bounds) {
        return; // the loupe belongs to another monitor's overlay
    }
    let Some(area) = client_rect(loupe, shot.bounds) else {
        return;
    };
    if area.right <= clip.left
        || area.left >= clip.right
        || area.bottom <= clip.top
        || area.top >= clip.bottom
    {
        return;
    }
    let _ = draw_loupe(run, shot, hdc, &loupe, area.left, area.top);
}

/// A UI font of `points` at `scale`, freed on drop.
fn font(points: u32, scale: f32) -> (HFONT, GdiObject) {
    // 1 pt = 4/3 px at 96 DPI.
    let height = -signed(scaled(points * 4 / 3, scale));
    // SAFETY: plain call; a null font (failure) selects nothing and `GdiObject` does not delete
    // it.
    let font = unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            windows::Win32::Graphics::Gdi::CLEARTYPE_QUALITY,
            u32::from(DEFAULT_PITCH.0 | FF_DONTCARE.0),
            w!("Segoe UI"),
        )
    };
    (font, GdiObject(HGDIOBJ(font.0)))
}

fn fill(dc: HDC, left: i32, top: i32, width: i32, height: i32, brush: HBRUSH) {
    let rect = RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    };
    // SAFETY: a live DC and brush and a valid rectangle.
    unsafe { FillRect(dc, &raw const rect, brush) };
}

/// A frame of `thick` pixels inside the rectangle.
fn frame(dc: HDC, left: i32, top: i32, width: i32, height: i32, thick: i32, brush: HBRUSH) {
    fill(dc, left, top, width, thick, brush);
    fill(dc, left, top + height - thick, width, thick, brush);
    fill(dc, left, top + thick, thick, height - 2 * thick, brush);
    fill(
        dc,
        left + width - thick,
        top + thick,
        thick,
        height - 2 * thick,
        brush,
    );
}

fn draw_text(
    dc: HDC,
    text: &str,
    rect: &mut RECT,
    format: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT,
) {
    let mut utf16: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: a live DC; `utf16` and `rect` are valid for the call.
    unsafe { DrawTextW(dc, &mut utf16, &raw mut *rect, format) };
}

/// The hint (and the HDR note) in a bar at the top middle of the monitor.
fn draw_hint(run: &Run, shot: &Shot, hdc: HDC, clip: &RECT) {
    let mut text = run.texts.hint.clone();
    if shot.hdr && !run.texts.hdr_note.is_empty() {
        text.push('\n');
        text.push_str(&run.texts.hdr_note);
    }
    if text.is_empty() {
        return;
    }
    let scale = shot.scale;
    let px = |n: u32| signed(scaled(n, scale));
    let (font, _font_guard) = font(12, scale);
    // SAFETY: a live DC and font; the previous font is selected back below.
    let previous = unsafe { SelectObject(hdc, HGDIOBJ(font.0)) };
    let mut measure = RECT {
        left: 0,
        top: 0,
        right: (signed(shot.bounds.w) - 2 * px(32)).max(px(100)),
        bottom: 0,
    };
    draw_text(
        hdc,
        &text,
        &mut measure,
        DT_CALCRECT | DT_CENTER | DT_NOPREFIX | DT_WORDBREAK,
    );
    let pad = px(10);
    let (text_w, text_h) = (measure.right - measure.left, measure.bottom - measure.top);
    let (bar_w, bar_h) = (text_w + 2 * pad, text_h + 2 * pad);
    let bar_x = (signed(shot.bounds.w) - bar_w) / 2;
    let bar_y = px(16);
    if bar_x < clip.right
        && bar_x + bar_w > clip.left
        && bar_y < clip.bottom
        && bar_y + bar_h > clip.top
    {
        let (background, _background_guard) = solid_brush(0x1e, 0x1e, 0x1e);
        fill(hdc, bar_x, bar_y, bar_w, bar_h, background);
        let mut rect = RECT {
            left: bar_x + pad,
            top: bar_y + pad,
            right: bar_x + pad + text_w,
            bottom: bar_y + pad + text_h,
        };
        // SAFETY: a live DC.
        unsafe {
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, colorref(255, 255, 255));
        }
        draw_text(
            hdc,
            &text,
            &mut rect,
            DT_CENTER | DT_NOPREFIX | DT_WORDBREAK,
        );
    }
    // SAFETY: selects the DC's own font back so the guard may delete ours.
    unsafe { SelectObject(hdc, previous) };
}

/// Draws the magnifier into an off-screen bitmap and blits it once to `(dest_x, dest_y)`.
fn draw_loupe(
    run: &Run,
    shot: &Shot,
    hdc: HDC,
    loupe: &ScreenRect,
    dest_x: i32,
    dest_y: i32,
) -> Result<(), Error> {
    let scale = shot.scale;
    let px = |n: u32| signed(scaled(n, scale));
    let (box_w, box_h) = (signed(loupe.w), signed(loupe.h));
    let (cx, cy) = run.session.cursor();
    let cells = loupe_cells(&run.shots, cx, cy, CELLS);
    let zoom = run.session.zoom();

    // The 11 x 11 source: gaps as a checker.
    let side_cells = signed(CELLS);
    let source = Surface::dib(hdc, side_cells, side_cells)?;
    if let Some(bits) = source.bits {
        // SAFETY: `bits` is the pixel memory of the 11 x 11 32-bit DIB owned by `source`.
        let pixels = unsafe { std::slice::from_raw_parts_mut(bits, cells.len()) };
        for (i, (pixel, cell)) in pixels.iter_mut().zip(&cells).enumerate() {
            *pixel = match cell {
                Some([r, g, b]) => u32::from(*r) << 16 | u32::from(*g) << 8 | u32::from(*b),
                None if (i / CELLS as usize + i % CELLS as usize).is_multiple_of(2) => 0x0080_8080,
                None => 0x00c0_c0c0,
            };
        }
    }

    let buffer = Surface::compatible(hdc, box_w, box_h)?;
    let dc = buffer.dc.0;
    let (background, _g1) = solid_brush(0x1e, 0x1e, 0x1e);
    fill(dc, 0, 0, box_w, box_h, background);

    let cell = signed(scaled(zoom, scale));
    let side = cell * side_cells;
    let grid_x = (box_w - side) / 2;
    let grid_y = px(4);
    // SAFETY: both DCs are live; `COLORONCOLOR` keeps the magnified pixels hard-edged.
    unsafe {
        SetStretchBltMode(dc, COLORONCOLOR);
        let _ = StretchBlt(
            dc,
            grid_x,
            grid_y,
            side,
            side,
            Some(source.dc.0),
            0,
            0,
            side_cells,
            side_cells,
            SRCCOPY,
        );
    }
    if zoom >= 8 {
        let (line, _g2) = solid_brush(0x40, 0x40, 0x40);
        for i in 0..=side_cells {
            fill(dc, grid_x + i * cell, grid_y, 1, side, line);
            fill(dc, grid_x, grid_y + i * cell, side, 1, line);
        }
    }
    // The centre cell: 2 px white inside 1 px black, visible on any colour.
    let middle = side_cells / 2;
    let (mx, my) = (grid_x + middle * cell, grid_y + middle * cell);
    let (black, _g3) = solid_brush(0, 0, 0);
    let (white, _g4) = solid_brush(255, 255, 255);
    frame(dc, mx, my, cell, cell, 1, black);
    frame(
        dc,
        mx + 1,
        my + 1,
        (cell - 2).max(0),
        (cell - 2).max(0),
        2,
        white,
    );

    // The readout: a swatch and the hex text.
    let bar_h = px(40);
    let bar_y = box_h - bar_h;
    let swatch = px(16);
    let swatch_x = px(8);
    let swatch_y = bar_y + (bar_h - swatch) / 2;
    let centre = cells.get(cells.len() / 2).copied().flatten();
    let [r, g, b] = centre.unwrap_or([0x40, 0x40, 0x40]);
    let (colour, _g5) = solid_brush(r, g, b);
    let (edge, _g6) = solid_brush(0xcc, 0xcc, 0xcc);
    fill(dc, swatch_x, swatch_y, swatch, swatch, colour);
    frame(dc, swatch_x, swatch_y, swatch, swatch, 1, edge);
    let (font, _font_guard) = font(12, scale);
    // SAFETY: a live DC and font; the previous font is selected back below.
    let previous = unsafe { SelectObject(dc, HGDIOBJ(font.0)) };
    // SAFETY: a live DC.
    unsafe {
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, colorref(255, 255, 255));
    }
    let mut text_rect = RECT {
        left: swatch_x + swatch + px(8),
        top: bar_y,
        right: box_w - px(8),
        bottom: box_h,
    };
    let readout = centre.map_or_else(|| "-".to_owned(), hex_text);
    draw_text(
        dc,
        &readout,
        &mut text_rect,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    // SAFETY: selects the DC's own font back so the guard may delete ours.
    unsafe { SelectObject(dc, previous) };

    let (border, _g7) = solid_brush(0x5a, 0x5a, 0x5a);
    frame(dc, 0, 0, box_w, box_h, 1, border);

    // SAFETY: the target DC is the paint DC and the source the buffer's, both live; the blit is
    // clipped to the update region by the paint DC.
    unsafe { BitBlt(hdc, dest_x, dest_y, box_w, box_h, Some(dc), 0, 0, SRCCOPY) }
        .map_err(|e| Error::from_windows("BitBlt", &e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_rect_is_the_part_inside_the_monitor_relative_to_its_corner() {
        let monitor = ScreenRect {
            x: -1920,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let inside = ScreenRect {
            x: -1000,
            y: 100,
            w: 96,
            h: 136,
        };
        let r = client_rect(inside, monitor).unwrap();
        assert_eq!((r.left, r.top, r.right, r.bottom), (920, 100, 1016, 236));
        let clipped = ScreenRect {
            x: -50,
            y: 1000,
            w: 96,
            h: 136,
        };
        let r = client_rect(clipped, monitor).unwrap();
        assert_eq!((r.left, r.top, r.right, r.bottom), (1870, 1000, 1920, 1080));
        let outside = ScreenRect {
            x: 10,
            y: 0,
            w: 5,
            h: 5,
        };
        assert!(client_rect(outside, monitor).is_none());
    }

    #[test]
    fn a_button_down_alone_decides_nothing_and_the_matching_up_does() {
        let mut press = Press::default();
        assert!(press.down(Button::Left));
        // Nothing is decided by the down; a wrong button's up is not the decision either.
        assert_eq!(press.up(Button::Right), None);
        assert_eq!(press.up(Button::Left), Some(Input::Pick));
        // The press is over: a stray second up decides nothing.
        assert_eq!(press.up(Button::Left), None);

        assert!(press.down(Button::Right));
        assert_eq!(press.up(Button::Right), Some(Input::Cancel));
    }

    #[test]
    fn a_second_button_while_one_is_held_is_ignored_and_lost_capture_ends_the_press() {
        let mut press = Press::default();
        assert!(press.down(Button::Left));
        assert!(!press.down(Button::Right));
        assert_eq!(press.up(Button::Right), None);
        press.lost();
        assert_eq!(press.up(Button::Left), None);
        // A new press works after the capture was lost.
        assert!(press.down(Button::Right));
        assert_eq!(press.up(Button::Right), Some(Input::Cancel));
    }

    #[test]
    fn advanced_colour_is_hdr_only_when_enabled_and_not_just_wide_colour_enforced() {
        assert!(!is_hdr(0b000));
        assert!(!is_hdr(0b001)); // supported, off
        assert!(is_hdr(0b011)); // supported and enabled
        assert!(!is_hdr(0b111)); // enabled only because wide colour is enforced: SDR
        assert!(!is_hdr(0b100));
    }

    #[test]
    fn a_second_start_while_running_is_refused() {
        let mut eyedropper = WinEyedropper::new();
        eyedropper.running.store(true, Ordering::SeqCst);
        assert!(eyedropper.running());
        assert_eq!(
            eyedropper.start(EyedropTexts::default(), Duration::ZERO, Box::new(|_| {})),
            Err(Error::Unavailable("an eyedropper run is already going"))
        );
    }
}
