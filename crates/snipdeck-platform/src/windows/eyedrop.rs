//! The screen eyedropper: a freeze-frame of every monitor under one overlay window each, a loupe
//! that follows the mouse (or a finger), and the pick.
//!
//! [`WinEyedropper`] implements [`Eyedropper`]. One run is one thread:
//!
//! 1. the thread becomes per-monitor-v2 aware, so every coordinate below is in physical pixels;
//! 2. it waits for the Snipdeck window to be hidden (every top-level window of the process
//!    hidden, minimised or `DWMWA_CLOAKED`), at most `settle`, and `DwmFlush`es;
//! 3. it lists the monitors (rectangle, device name, scale) and, with the display configuration
//!    API, which of them are not plain SDR (`DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO_2`, or the
//!    older query where that one does not exist);
//! 4. it copies each monitor with `BitBlt(.. SRCCOPY | CAPTUREBLT)` into a 32-bit top-down DIB
//!    section (per monitor, so no buffer spans a gap or a negative origin) and keeps the DIB both
//!    to paint the overlay from and, through [`Pixels`], as the memory the portable [`Shot`]
//!    samples: one copy of each monitor. A monitor that cannot be copied is skipped (grey, with a
//!    note); only all of them failing is a failure;
//! 5. it opens one `WS_POPUP` overlay per monitor, on that monitor, and takes the foreground;
//!    if it did not get it 100 ms later it registers every key of the focused path as a hotkey
//!    for the rest of the run and records the keys another app refused ([`EyedropNote`]);
//! 6. messages become [`Input`] for the portable [`Session`], whose [`Step`] says what to draw,
//!    where to move the cursor and when the run is over. Touch-made mouse messages become the
//!    touch inputs;
//! 7. everything is released (hotkeys, timers, windows, class, bitmaps, DCs) and `done` is called.
//!
//! The decisions (sampling, averaging, loupe place, nudging, zoom, collecting, touch, the hint's
//! fade and side, the keys, the end reasons) are in [`crate::eyedrop`], tested on Linux. What is
//! here is the Win32 glue. Every GDI object is owned by a guard that frees it on every path; the
//! run's state lives in a thread-local because the window procedure has no other way to reach it,
//! and it is never borrowed across a call that can dispatch messages.
//!
//! Values are what GDI composed (what apps drew), before Night light and colour filters. Windows
//! with `WDA_MONITOR` affinity and protected video capture black; `WDA_EXCLUDEFROMCAPTURE`
//! windows are absent.
//!
//! **Unverified on Windows** (`tools/windows-test/tests/colour.json`): this file is only checked
//! by cross-compiling.

use std::cell::RefCell;
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_DEVICE_INFO_TYPE,
    DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes,
    QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::{
    COLORREF, ERROR_CLASS_ALREADY_EXISTS, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, GetLastError,
    HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmFlush, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_OVER, AlphaBlend, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, BeginPaint,
    BitBlt, CAPTUREBLT, CLIP_DEFAULT_PRECIS, COLORONCOLOR, CreateCompatibleBitmap,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET,
    DEFAULT_PITCH, DIB_RGB_COLORS, DT_CALCRECT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, DeleteDC, DeleteObject, DrawTextW, EndPaint,
    EnumDisplayMonitors, FF_DONTCARE, FillRect, GdiFlush, GetDC, GetMonitorInfoW, HBRUSH, HDC,
    HFONT, HGDIOBJ, HMONITOR, InvalidateRect, MONITORINFO, MONITORINFOEXW, OUT_DEFAULT_PRECIS,
    PAINTSTRUCT, ReleaseDC, SRCCOPY, SelectObject, SetBkMode, SetStretchBltMode, SetTextColor,
    StretchBlt, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::NotifyWinEvent;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, HOT_KEY_MODIFIERS, MOD_CONTROL, MOD_SHIFT, RegisterHotKey, ReleaseCapture,
    SetCapture, SetFocus, UnregisterHotKey, VK_CONTROL, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    EVENT_OBJECT_NAMECHANGE, EnumWindows, GetForegroundWindow, GetMessageExtraInfo, GetMessageW,
    GetWindowThreadProcessId, IDC_CROSS, IsIconic, IsWindow, IsWindowVisible, KillTimer,
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
    CELLS, COLOR_MODE_SDR, EyedropDone, EyedropNote, EyedropOutcome, EyedropTexts, Eyedropper,
    FALLBACK_KEYS, FailReason, HintSide, Input, PAD, Pixels, READOUT, Session, Shot, Step,
    WindowState, clamp_text_scale, cloak_settled, colour_is_approximate, fallback_input,
    font_height_px, guarded, hint_animating, hint_content, hint_rect, hint_side,
    is_touch_extra_info, keep_captured, key_for_vk, key_input, keys_of_ids, loupe_cells,
    monitor_at, pick_frame, readout_lines, scaled, signed, wait_until, window_title,
};
use crate::route::ScreenRect;

/// The overlay's window class.
const CLASS_NAME: &str = "SnipdeckEyedropOverlay";

/// The timer that checks, 100 ms after the overlay opened, whether it got the foreground.
const TIMER_FOCUS: usize = 1;
/// The timer that ends a run nobody finished.
const TIMER_TIMEOUT: usize = 2;
/// The timer that repaints the hint bar while it fades.
const TIMER_HINT: usize = 3;
/// Wait for the foreground before falling back to hotkeys.
const FOCUS_CHECK_MS: u32 = 100;
/// How long the overlay may stay open.
const TIMEOUT_MS: u32 = 120_000;
/// The hint fade's repaint interval (about 30 frames a second).
const HINT_TICK_MS: u32 = 33;
/// How often the wait for the Snipdeck window to disappear looks again.
const CLOAK_POLL: Duration = Duration::from_millis(10);

impl From<Error> for FailReason {
    /// An error of a call, for the log: the call and its code, never screen content.
    fn from(error: Error) -> Self {
        Self::Other(error.to_string())
    }
}

/// The screen eyedropper. See the module documentation.
#[derive(Debug, Default)]
pub struct WinEyedropper {
    running: Arc<AtomicBool>,
    notes: Arc<Mutex<Vec<EyedropNote>>>,
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
        if let Ok(mut notes) = self.notes.lock() {
            notes.clear();
        }
        let running = Arc::clone(&self.running);
        let notes = Arc::clone(&self.notes);
        let spawned = std::thread::Builder::new()
            .name("eyedropper".into())
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| run(&texts, settle, &notes)))
                    .unwrap_or_else(|_| EyedropOutcome::Failed(FailReason::Other("panic".into())));
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

    fn notes(&self) -> Vec<EyedropNote> {
        self.notes.lock().map(|n| n.clone()).unwrap_or_default()
    }
}

/// The whole run on its thread.
fn run(
    texts: &EyedropTexts,
    settle: Duration,
    notes: &Arc<Mutex<Vec<EyedropNote>>>,
) -> EyedropOutcome {
    match try_run(texts, settle, notes) {
        Ok(outcome) => outcome,
        Err(reason) => EyedropOutcome::Failed(reason),
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
    // A panic must not unwind into Windows: a monitor that panics is skipped, the walk goes on.
    guarded(BOOL::from(true), || {
        // SAFETY: `data` is the `&mut Vec<MonitorFacts>` that `monitors` passed to
        // `EnumDisplayMonitors`, which calls this on the same thread while that borrow is alive.
        let list = unsafe { &mut *(data.0 as *mut Vec<MonitorFacts>) };
        if let Some(facts) = facts_of(monitor) {
            list.push(facts);
        }
        BOOL::from(true)
    })
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

/// `DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO_2` (Windows 11 24H2): the type the
/// `windows` crate does not have yet.
const GET_ADVANCED_COLOR_INFO_2: DISPLAYCONFIG_DEVICE_INFO_TYPE =
    DISPLAYCONFIG_DEVICE_INFO_TYPE(15);

/// `DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO_2`, declared here for the same reason. Layout: the
/// header, a word of flags, `colorEncoding`, `bitsPerColorChannel` and `activeColorMode`
/// (0 SDR, 1 wide colour gamut, 2 HDR).
#[repr(C)]
struct AdvancedColorInfo2 {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    value: u32,
    colour_encoding: u32,
    bits_per_colour_channel: u32,
    active_colour_mode: u32,
}

/// Does `GET_ADVANCED_COLOR_INFO_2` exist on this Windows, and what does it say about the path's
/// target? `None` when the query is refused (older Windows).
fn active_colour_mode(path: &DISPLAYCONFIG_PATH_INFO) -> Option<u32> {
    let mut info = AdvancedColorInfo2 {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: GET_ADVANCED_COLOR_INFO_2,
            size: u32::try_from(size_of::<AdvancedColorInfo2>()).ok()?,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        },
        value: 0,
        colour_encoding: 0,
        bits_per_colour_channel: 0,
        active_colour_mode: COLOR_MODE_SDR,
    };
    // SAFETY: `info` is `repr(C)` with the request's header (type and size) first, as the call
    // requires, and outlives the call; a Windows that does not know the type returns an error
    // and writes nothing.
    let status = unsafe { DisplayConfigGetDeviceInfo(&raw mut info.header) };
    (status == 0).then_some(info.active_colour_mode)
}

/// The older advanced-colour bits of the path's target, `None` when refused.
fn legacy_colour_bits(path: &DISPLAYCONFIG_PATH_INFO) -> Option<u32> {
    let mut colour = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
            size: u32::try_from(size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>()).ok()?,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        },
        ..Default::default()
    };
    // SAFETY: `colour` is a DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO whose header carries its type
    // and size, as the call requires; it outlives the call.
    if unsafe { DisplayConfigGetDeviceInfo(&raw mut colour.header) } != 0 {
        return None;
    }
    // SAFETY: the union is plain bits; `value` is its whole-word view.
    Some(unsafe { colour.Anonymous.value })
}

/// The GDI device names (`\\.\DISPLAY1`) of the monitors whose pixels are approximate: HDR, or
/// wide colour gamut with automatic colour management (see [`colour_is_approximate`]). Empty when
/// the display configuration cannot be read: the pick is then not marked approximate.
fn hdr_devices() -> Vec<String> {
    let Some(paths) = active_paths() else {
        return Vec::new();
    };
    let mut devices = Vec::new();
    for path in &paths {
        let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: u32::try_from(size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>()).unwrap_or(0),
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
        let mode = active_colour_mode(path);
        // The old query is only needed when the new one does not exist.
        let legacy = if mode.is_none() {
            legacy_colour_bits(path)
        } else {
            None
        };
        if colour_is_approximate(mode, legacy) {
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

/// The pixels of a DIB section, read where GDI wrote them: no second copy of the monitor. The
/// surface stays alive as long as any clone of the [`Pixels`] made from this.
struct DibPixels {
    surface: Rc<Surface>,
    len: usize,
}

impl AsRef<[u8]> for DibPixels {
    fn as_ref(&self) -> &[u8] {
        match self.surface.bits {
            // SAFETY: `bits` is the pixel memory of the 32-bit DIB section owned by `surface`,
            // which `self` keeps alive; `len` is its exact size, and nothing writes to it after
            // the capture (`GdiFlush`ed).
            Some(bits) => unsafe {
                std::slice::from_raw_parts(bits.cast::<u8>().cast_const(), self.len)
            },
            None => &[],
        }
    }
}

/// Copies one monitor. Returns the portable shot (which reads the surface's memory) and the
/// surface the overlay paints from.
fn capture(
    screen: &ScreenDc,
    monitor: &MonitorFacts,
    hdr: bool,
) -> Result<(Shot, Rc<Surface>), Error> {
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
    // The DIB is read directly: finish any batched drawing first.
    // SAFETY: no preconditions.
    let _ = unsafe { GdiFlush() };
    let len = usize::try_from(w)
        .ok()
        .zip(usize::try_from(h).ok())
        .and_then(|(w, h)| w.checked_mul(h)?.checked_mul(4))
        .ok_or_else(too_big)?;
    if surface.bits.is_none() {
        return Err(Error::Unavailable("the bitmap has no pixels"));
    }
    let surface = Rc::new(surface);
    let pixels = Pixels::new(DibPixels {
        surface: Rc::clone(&surface),
        len,
    });
    let shot = Shot::new(monitor.bounds, monitor.scale, hdr, pixels);
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

/// What each overlay paints from (`None`: its monitor could not be captured).
type Frozen = Vec<Option<Rc<Surface>>>;

/// Everything one run holds. Field order is drop order: hotkeys and timers go before the
/// windows they belong to, the windows before the bitmaps they paint and the class last.
struct Run {
    hotkeys: Hotkeys,
    timers: Timers,
    windows: Vec<Overlay>,
    frozen: Frozen,
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
    /// When the overlays were shown: the hint fades from then.
    started: Instant,
    /// The biggest the hint bar gets on each monitor (physical pixels), measured once; the side
    /// it sits at is decided by this, so it does not jump when the text shrinks.
    hint_bound: Vec<(u32, u32)>,
    /// The edge each monitor's hint bar sits at now.
    hint_sides: Vec<HintSide>,
    /// Where the keys another app refused are reported ([`Eyedropper::notes`]).
    notes: Arc<Mutex<Vec<EyedropNote>>>,
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

unsafe extern "system" fn collect_window(hwnd: HWND, data: LPARAM) -> BOOL {
    // As in `collect_monitor`: a panic must not unwind into Windows.
    guarded(BOOL::from(true), || {
        let mut process = 0_u32;
        // SAFETY: a window handle from the enumeration; `process` is a valid out-pointer.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut process)) };
        // SAFETY: no preconditions.
        if process != unsafe { GetCurrentProcessId() } {
            return BOOL::from(true);
        }
        let mut cloak = 0_u32;
        // SAFETY: `cloak` is a valid out-buffer of the 4 bytes `DWMWA_CLOAKED` writes.
        let queried = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                (&raw mut cloak).cast::<c_void>(),
                u32::try_from(size_of::<u32>()).unwrap_or(4),
            )
        };
        // SAFETY: both only read the window's state.
        let (visible, minimized) =
            unsafe { (IsWindowVisible(hwnd).as_bool(), IsIconic(hwnd).as_bool()) };
        let state = WindowState {
            visible,
            cloaked: queried.is_ok() && cloak != 0,
            minimized,
        };
        // SAFETY: `data` is the `&mut Vec<WindowState>` that `process_windows` passed to
        // `EnumWindows`, which calls this on the same thread while that borrow is alive.
        let list = unsafe { &mut *(data.0 as *mut Vec<WindowState>) };
        list.push(state);
        BOOL::from(true)
    })
}

/// The state of every top-level window of this process.
fn process_windows() -> Vec<WindowState> {
    let mut list: Vec<WindowState> = Vec::new();
    // SAFETY: the callback only runs during this call, on this thread, and only touches the
    // vector whose address is passed as `data`.
    let _ = unsafe { EnumWindows(Some(collect_window), LPARAM((&raw mut list) as isize)) };
    list
}

/// Waits until Snipdeck's own windows are gone from the screen (hidden, minimised or cloaked,
/// which is what the Panel's hide does), at most `settle`: the cap is the answer for a window
/// that stays (a docked tab), and for a host that passes a short settle.
fn wait_for_own_windows(settle: Duration) {
    if settle.is_zero() {
        return;
    }
    let begin = Instant::now();
    let _ = wait_until(
        settle,
        CLOAK_POLL,
        || begin.elapsed(),
        std::thread::sleep,
        || cloak_settled(&process_windows()),
    );
}

fn try_run(
    texts: &EyedropTexts,
    settle: Duration,
    notes: &Arc<Mutex<Vec<EyedropNote>>>,
) -> Result<EyedropOutcome, FailReason> {
    // Every coordinate from here on is physical, even on a mixed-DPI desktop.
    let _ = dpi::aware_thread();
    wait_for_own_windows(settle);
    // The hidden Panel must be gone from the next capture.
    // SAFETY: no preconditions.
    let _ = unsafe { DwmFlush() };

    let facts = monitors();
    if facts.is_empty() {
        return Err(FailReason::NoMonitor);
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
    let hint_bound = hint_bounds(texts, &shots)?;
    let hint_sides = shots
        .iter()
        .zip(&hint_bound)
        .map(|(shot, bound)| hint_side(start, shot.bounds, shot.scale, *bound))
        .collect();
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
        session: Session::new(&shots, start).with_text_scale(texts.text_scale),
        shots,
        texts: texts.clone(),
        armed: false,
        title: None,
        title_set: false,
        press: Press::default(),
        started: Instant::now(),
        hint_bound,
        hint_sides,
        notes: Arc::clone(notes),
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
    outcome.ok_or_else(|| {
        FailReason::Other(
            if loop_failed {
                "message loop failed"
            } else {
                "no result"
            }
            .to_owned(),
        )
    })
}

/// Copies every monitor: the portable shots and the surfaces the overlays paint from. A monitor
/// that refuses is skipped (an uncaptured shot, no surface); only all refusing is an error.
fn capture_all(facts: &[MonitorFacts]) -> Result<(Vec<Shot>, Frozen), FailReason> {
    let hdr = hdr_devices();
    let screen = ScreenDc::get()?;
    let results = facts
        .iter()
        .map(|monitor| capture(&screen, monitor, hdr.contains(&monitor.device)))
        .collect();
    let kept = keep_captured(results)?;
    let mut shots = Vec::with_capacity(facts.len());
    let mut frozen = Vec::with_capacity(facts.len());
    for (monitor, captured) in facts.iter().zip(kept) {
        if let Some((shot, surface)) = captured {
            shots.push(shot);
            frozen.push(Some(surface));
        } else {
            shots.push(Shot::uncaptured(monitor.bounds, monitor.scale));
            frozen.push(None);
        }
    }
    Ok((shots, frozen))
}

/// The biggest hint bar (text, the HDR note and the padding) on each monitor.
fn hint_bounds(texts: &EyedropTexts, shots: &[Shot]) -> Result<Vec<(u32, u32)>, Error> {
    let screen = ScreenDc::get()?;
    Ok(shots
        .iter()
        .map(|shot| {
            hint_content(texts, shot.hdr, Duration::ZERO).map_or((0, 0), |(text, _)| {
                hint_bar_size(screen.0, &text, shot, texts.text_scale)
            })
        })
        .collect())
}

/// Registers the class, creates one overlay per monitor and sets the timers. Everything made is
/// owned by `state` at once, so an error part-way leaves nothing behind. Returns the windows.
fn open_overlays(state: &mut Run, facts: &[MonitorFacts]) -> Result<Vec<HWND>, Error> {
    register_class()?;
    state.class = Some(ClassRegistration);
    for monitor in facts {
        let hwnd = create_overlay(monitor, &state.texts)?;
        state.windows.push(Overlay { hwnd });
    }
    let first = state.windows[0].hwnd;
    state.hotkeys.hwnd = first;
    state.timers.hwnd = first;
    state.timers.set(TIMER_FOCUS, FOCUS_CHECK_MS);
    state.timers.set(TIMER_TIMEOUT, TIMEOUT_MS);
    state.timers.set(TIMER_HINT, HINT_TICK_MS);
    state.started = Instant::now();
    Ok(state.windows.iter().map(|o| o.hwnd).collect())
}

/// The index of the monitor that has `at` (captured or not: the overlay is there either way).
fn shots_index(shots: &[Shot], at: (i32, i32)) -> Option<usize> {
    shots.iter().position(|s| s.contains(at.0, at.1))
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
fn create_overlay(monitor: &MonitorFacts, texts: &EyedropTexts) -> Result<HWND, Error> {
    let class = HSTRING::from(CLASS_NAME);
    // SAFETY: a null module name asks for this process's own module handle.
    let hinstance = unsafe { GetModuleHandleW(None) }
        .map_err(|e| Error::from_windows("GetModuleHandleW", &e))?;
    let title = HSTRING::from(window_title(texts, None));
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
    run.session.loupe(&run.shots)
}

/// Every overlay with the monitor it covers.
fn windows_of(run: &Run) -> Vec<(HWND, ScreenRect)> {
    run.windows
        .iter()
        .zip(&run.shots)
        .map(|(overlay, shot)| (overlay.hwnd, shot.bounds))
        .collect()
}

/// A mouse button that picks (left) or cancels (right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Left,
    Right,
}

/// Which button is held on the overlay, and whether a finger made it. A pick or cancel happens
/// when that button is let go, so the button-up is swallowed by the overlay and never reaches (or
/// opens a context menu in) the window underneath, which a decision on the button-down would let
/// through.
#[derive(Debug, Default)]
struct Press {
    held: Option<Button>,
    touch: bool,
}

impl Press {
    /// A button went down (`touch`: a finger made the message). True when it starts a press (the
    /// overlay then captures the mouse); a second button pressed while one is held is ignored.
    fn down(&mut self, button: Button, touch: bool) -> bool {
        if self.held.is_some() {
            return false;
        }
        self.held = Some(button);
        self.touch = touch;
        true
    }

    /// A finger is down on the overlay.
    fn touching(&self) -> bool {
        self.held.is_some() && self.touch
    }

    /// A button went up: the decision when it is the one held, `None` for any other. `shift` is
    /// whether Shift is held (a left click then collects instead of picking); `at` is where the
    /// finger lifted.
    fn up(&mut self, button: Button, shift: bool, at: Option<(i32, i32)>) -> Option<Input> {
        if self.held != Some(button) {
            return None;
        }
        self.held = None;
        if std::mem::take(&mut self.touch) {
            return at.map(|(x, y)| Input::TouchUp(x, y));
        }
        Some(match button {
            Button::Left if shift => Input::Collect,
            Button::Left => Input::Pick,
            Button::Right => Input::Cancel,
        })
    }

    /// The mouse capture was lost (another window took it): the press is over, with no decision.
    fn lost(&mut self) {
        self.held = None;
        self.touch = false;
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
        let moved = matches!(step, Step::Redraw | Step::MoveCursor(..));
        match step {
            Step::MoveCursor(x, y) => effects.cursor = Some((x, y)),
            Step::Finished(outcome) => {
                run.outcome = Some(outcome);
                effects.quit = true;
            }
            Step::Redraw | Step::Nothing => {}
        }
        if moved {
            effects.invalidate.extend(old);
            effects.invalidate.extend(current_loupe(run));
        }
        // The hint bar steps aside when the cursor comes near it.
        let cursor = run.session.cursor();
        for ((shot, bound), side) in run
            .shots
            .iter()
            .zip(&run.hint_bound)
            .zip(run.hint_sides.iter_mut())
        {
            let now = hint_side(cursor, shot.bounds, shot.scale, *bound);
            if now != *side {
                effects
                    .invalidate
                    .push(hint_rect(shot.bounds, shot.scale, *bound, *side));
                effects
                    .invalidate
                    .push(hint_rect(shot.bounds, shot.scale, *bound, now));
                *side = now;
            }
        }
        let colour = run.session.colour(&run.shots);
        if !run.title_set || run.title != colour {
            run.title_set = true;
            run.title = colour;
            effects.title = Some(window_title(&run.texts, colour));
        }
        effects.windows = windows_of(run);
    });
    effects
}

/// The hint's timer: repaint where the bars are, until the fade is over.
fn hint_tick() -> Effects {
    let mut effects = Effects::default();
    RUN.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return;
        };
        let Some(run) = slot.as_mut() else {
            return;
        };
        // The last tick still repaints, so the bar disappears.
        if !hint_animating(run.started.elapsed()) {
            run.timers.kill(TIMER_HINT);
        }
        for ((shot, bound), side) in run.shots.iter().zip(&run.hint_bound).zip(&run.hint_sides) {
            effects
                .invalidate
                .push(hint_rect(shot.bounds, shot.scale, *bound, *side));
        }
        effects.windows = windows_of(run);
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

fn key_is_down(vk: u16) -> bool {
    // SAFETY: no preconditions; the high bit is "down".
    unsafe { GetKeyState(i32::from(vk)) }.cast_unsigned() & 0x8000 != 0
}

fn shift_down() -> bool {
    key_is_down(VK_SHIFT.0)
}

fn ctrl_down() -> bool {
    key_is_down(VK_CONTROL.0)
}

fn mouse_input() -> Option<Input> {
    // The position comes from `GetCursorPos`, not `lParam`, so every overlay agrees.
    screen::cursor_position().map(|(x, y)| Input::Moved(x, y))
}

/// The input for a key of the focused path, if it is one of ours.
fn keyboard_input(vk: u16) -> Option<Input> {
    Some(key_input(key_for_vk(vk)?, shift_down(), ctrl_down()))
}

/// The timers: the focus check (register the hotkeys if the foreground is not ours), the hint's
/// repaint and the timeout.
fn on_timer(id: usize) {
    match id {
        TIMER_TIMEOUT => apply(&feed(Some(Input::TimedOut))),
        TIMER_HINT => apply(&hint_tick()),
        TIMER_FOCUS => check_focus(),
        _ => {}
    }
}

/// 100 ms after the overlay opened: if the foreground is not ours, the foreground lock kept us
/// out, so take the keys the other way (every key of the focused path), for this run only, and
/// record the ones another app refused.
fn check_focus() {
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
        let mut refused = Vec::new();
        for key in FALLBACK_KEYS {
            let mut modifiers = HOT_KEY_MODIFIERS(0);
            if key.shift {
                modifiers |= MOD_SHIFT;
            }
            if key.ctrl {
                modifiers |= MOD_CONTROL;
            }
            // SAFETY: registers for a window of this thread; a refusal (key taken by another
            // app) just leaves that key to the window.
            if unsafe {
                RegisterHotKey(Some(run.hotkeys.hwnd), key.id, modifiers, u32::from(key.vk))
            }
            .is_ok()
            {
                run.hotkeys.ids.push(key.id);
            } else {
                refused.push(key.id);
            }
        }
        let keys = keys_of_ids(&refused);
        if !keys.is_empty()
            && let Ok(mut notes) = run.notes.lock()
        {
            notes.push(EyedropNote::KeysNotRegistered(keys));
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
                        .get_or_insert(EyedropOutcome::Failed(FailReason::Other("panic".into())));
                }
            });
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
    }
}

fn handle(hwnd: HWND, msg: u32, wparam: WPARAM) -> Option<LRESULT> {
    // Read first: any later message call replaces it. A finger's mouse messages carry a
    // signature in the extra info (a pen is precise and acts as a mouse).
    // SAFETY: no preconditions.
    let touch = is_touch_extra_info(unsafe { GetMessageExtraInfo() }.0.cast_unsigned());
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
        WM_MOUSEMOVE if touch => {
            // A finger's moves are only meant while it is down (one comes just before the down).
            match (with_press(|p| p.touching()), screen::cursor_position()) {
                (Some(true), Some((x, y))) => input(Some(Input::TouchMove(x, y))),
                _ => Some(LRESULT(0)),
            }
        }
        WM_MOUSEMOVE => input(mouse_input()),
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            let button = if msg == WM_LBUTTONDOWN {
                Button::Left
            } else {
                Button::Right
            };
            if touch && button == Button::Right {
                // A long press makes a right click: it must not cancel the run.
                return Some(LRESULT(0));
            }
            let started = with_press(|p| p.down(button, touch)).unwrap_or(false);
            if started {
                // SAFETY: a live window of this thread; the mouse is ours until the button-up.
                let _ = unsafe { SetCapture(hwnd) };
            }
            match (touch, started, screen::cursor_position()) {
                (true, true, Some((x, y))) => input(Some(Input::TouchDown(x, y))),
                (true, ..) => Some(LRESULT(0)),
                _ => input(mouse_input()),
            }
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let button = if msg == WM_LBUTTONUP {
                Button::Left
            } else {
                Button::Right
            };
            let at = screen::cursor_position();
            // Always swallowed (no default handling, so no context menu from the right button).
            match with_press(|p| p.up(button, shift_down(), at)).flatten() {
                Some(decision) => {
                    // SAFETY: no preconditions; the capture this window set on the button-down.
                    let _ = unsafe { ReleaseCapture() };
                    if !matches!(decision, Input::TouchUp(..)) {
                        apply(&feed(mouse_input()));
                    }
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
        WM_KEYDOWN => {
            keyboard_input(u16::try_from(wparam.0).unwrap_or(0)).and_then(|i| input(Some(i)))
        }
        WM_HOTKEY => {
            let id = i32::try_from(wparam.0).unwrap_or(0);
            fallback_input(id).and_then(|i| input(Some(i)))
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
    let Some(shot) = run.shots.get(index) else {
        return;
    };
    match run.frozen.get(index).and_then(Option::as_ref) {
        Some(frozen) => {
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
        }
        None => draw_unavailable(run, shot, hdc, clip),
    }
    draw_hint(run, index, shot, hdc, clip);
    let Some(loupe) = current_loupe(run) else {
        return;
    };
    let (cx, cy) = run.session.cursor();
    if monitor_at(&run.shots, cx, cy).map(|s| s.bounds) != Some(shot.bounds) {
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

/// A UI font of `points` at the monitor `scale` and the system `text_scale`, freed on drop.
fn font(points: u32, scale: f32, text_scale: f32) -> (HFONT, GdiObject) {
    let height = font_height_px(points, scale, text_scale);
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

/// The size of the hint bar for `text` on `shot`'s monitor: the text wrapped to the monitor's width
/// with padding around it.
fn hint_bar_size(dc: HDC, text: &str, shot: &Shot, text_scale: f32) -> (u32, u32) {
    let px = |n: u32| signed(scaled(n, shot.scale));
    let (font, _font_guard) = font(12, shot.scale, text_scale);
    // SAFETY: a live DC and font; the previous font is selected back below.
    let previous = unsafe { SelectObject(dc, HGDIOBJ(font.0)) };
    let mut measure = RECT {
        left: 0,
        top: 0,
        right: (signed(shot.bounds.w) - 2 * px(32)).max(px(100)),
        bottom: 0,
    };
    draw_text(
        dc,
        text,
        &mut measure,
        DT_CALCRECT | DT_CENTER | DT_NOPREFIX | DT_WORDBREAK,
    );
    // SAFETY: selects the DC's own font back so the guard may delete ours.
    unsafe { SelectObject(dc, previous) };
    let pad = px(10);
    let size = |extent: i32| u32::try_from(extent + 2 * pad).unwrap_or(0);
    (
        size(measure.right - measure.left),
        size(measure.bottom - measure.top),
    )
}

/// A monitor that could not be captured: grey, with the host's note in the middle.
fn draw_unavailable(run: &Run, shot: &Shot, hdc: HDC, clip: &RECT) {
    let (grey, _grey_guard) = solid_brush(0x30, 0x30, 0x30);
    // SAFETY: a live DC and brush and a valid rectangle.
    unsafe { FillRect(hdc, clip, grey) };
    if run.texts.unavailable.is_empty() {
        return;
    }
    let (font, _font_guard) = font(12, shot.scale, run.texts.text_scale);
    // SAFETY: a live DC and font; the previous font is selected back below.
    let previous = unsafe { SelectObject(hdc, HGDIOBJ(font.0)) };
    // SAFETY: a live DC.
    unsafe {
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, colorref(0xdd, 0xdd, 0xdd));
    }
    let mut area = RECT {
        left: 0,
        top: 0,
        right: signed(shot.bounds.w),
        bottom: signed(shot.bounds.h),
    };
    draw_text(
        hdc,
        &run.texts.unavailable,
        &mut area,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    // SAFETY: selects the DC's own font back so the guard may delete ours.
    unsafe { SelectObject(hdc, previous) };
}

/// The hint (and the HDR note) in a bar at the top middle of the monitor, at the bottom while the
/// cursor is near the top; fading out two seconds after the run began.
fn draw_hint(run: &Run, index: usize, shot: &Shot, hdc: HDC, clip: &RECT) {
    let Some((text, alpha)) = hint_content(&run.texts, shot.hdr, run.started.elapsed()) else {
        return;
    };
    let side = run.hint_sides.get(index).copied().unwrap_or(HintSide::Top);
    let (bar_w, bar_h) = hint_bar_size(hdc, &text, shot, run.texts.text_scale);
    if bar_w == 0 || bar_h == 0 {
        return;
    }
    let place = hint_rect(shot.bounds, shot.scale, (bar_w, bar_h), side);
    let (bar_x, bar_y) = (place.x - shot.bounds.x, place.y - shot.bounds.y);
    let (w, h) = (signed(bar_w), signed(bar_h));
    if bar_x >= clip.right
        || bar_x + w <= clip.left
        || bar_y >= clip.bottom
        || bar_y + h <= clip.top
    {
        return;
    }
    let Ok(buffer) = Surface::compatible(hdc, w, h) else {
        return;
    };
    let dc = buffer.dc.0;
    let (background, _background_guard) = solid_brush(0x1e, 0x1e, 0x1e);
    fill(dc, 0, 0, w, h, background);
    let pad = signed(scaled(10, shot.scale));
    let (font, _font_guard) = font(12, shot.scale, run.texts.text_scale);
    // SAFETY: a live DC and font; the previous font is selected back below.
    let previous = unsafe { SelectObject(dc, HGDIOBJ(font.0)) };
    // SAFETY: a live DC.
    unsafe {
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, colorref(255, 255, 255));
    }
    let mut rect = RECT {
        left: pad,
        top: pad,
        right: w - pad,
        bottom: h - pad,
    };
    draw_text(dc, &text, &mut rect, DT_CENTER | DT_NOPREFIX | DT_WORDBREAK);
    // SAFETY: selects the DC's own font back so the guard may delete ours.
    unsafe { SelectObject(dc, previous) };
    let blend = BLENDFUNCTION {
        BlendOp: u8::try_from(AC_SRC_OVER).unwrap_or(0),
        BlendFlags: 0,
        SourceConstantAlpha: alpha,
        AlphaFormat: 0,
    };
    // SAFETY: the target DC is the paint DC and the source the buffer's, both live; the blend is
    // clipped to the update region by the paint DC. A failure leaves the bar undrawn.
    let _ = unsafe { AlphaBlend(hdc, bar_x, bar_y, w, h, dc, 0, 0, w, h, blend) };
}

/// The readout bar of the loupe, into its off-screen DC `dc` of `box_size`: a swatch, the colour
/// in the host's format, a second line with the hex, the aperture and how many colours were kept;
/// on touch, the Pick button.
fn draw_readout(run: &Run, shot: &Shot, dc: HDC, loupe: &ScreenRect, box_size: (i32, i32)) {
    let (box_w, box_h) = box_size;
    let scale = shot.scale;
    let text_scale = clamp_text_scale(run.texts.text_scale);
    let px = |n: u32| signed(scaled(n, scale));
    let text_px = |n: u32| signed(scaled(n, scale * text_scale));
    let bar_h = text_px(READOUT);
    let bar_y = box_h - bar_h;
    let swatch = text_px(16);
    let swatch_x = px(8);
    let swatch_y = bar_y + (bar_h - swatch) / 2;
    let colour = run.session.colour(&run.shots);
    let [r, g, b] = colour.unwrap_or([0x40, 0x40, 0x40]);
    let (fill_colour, _g5) = solid_brush(r, g, b);
    let (edge, _g6) = solid_brush(0xcc, 0xcc, 0xcc);
    fill(dc, swatch_x, swatch_y, swatch, swatch, fill_colour);
    frame(dc, swatch_x, swatch_y, swatch, swatch, 1, edge);
    let (font, _font_guard) = font(12, scale, text_scale);
    // SAFETY: a live DC and font; the previous font is selected back below.
    let previous = unsafe { SelectObject(dc, HGDIOBJ(font.0)) };
    // SAFETY: a live DC.
    unsafe { SetBkMode(dc, TRANSPARENT) };
    let button = run.session.pick_button(&run.shots);
    let text_right = button.map_or(box_w - px(8), |b| b.x - loupe.x - px(8));
    let text_left = swatch_x + swatch + px(8);
    let half = bar_h / 2;
    let (first_line, second_line) = readout_lines(
        &run.texts,
        colour,
        run.session.aperture(),
        run.session.collected().len(),
    );
    let row = DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS;
    let mut first_rect = RECT {
        left: text_left,
        top: bar_y,
        right: text_right,
        bottom: bar_y + half,
    };
    let mut second_rect = RECT {
        left: text_left,
        top: bar_y + half,
        right: text_right,
        bottom: box_h,
    };
    // SAFETY: a live DC.
    unsafe { SetTextColor(dc, colorref(255, 255, 255)) };
    draw_text(dc, &first_line, &mut first_rect, row);
    // SAFETY: a live DC.
    unsafe { SetTextColor(dc, colorref(0xb0, 0xb0, 0xb0)) };
    draw_text(dc, &second_line, &mut second_rect, row);
    if let Some(button) = button {
        let mut area = RECT {
            left: button.x - loupe.x,
            top: button.y - loupe.y,
            right: button.x - loupe.x + signed(button.w),
            bottom: button.y - loupe.y + signed(button.h),
        };
        let (accent, _g8) = solid_brush(0x2f, 0x6f, 0xdb);
        let (rim, _g9) = solid_brush(255, 255, 255);
        fill(
            dc,
            area.left,
            area.top,
            signed(button.w),
            signed(button.h),
            accent,
        );
        frame(
            dc,
            area.left,
            area.top,
            signed(button.w),
            signed(button.h),
            1,
            rim,
        );
        // SAFETY: a live DC.
        unsafe { SetTextColor(dc, colorref(255, 255, 255)) };
        draw_text(
            dc,
            &run.texts.pick_label,
            &mut area,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }
    // SAFETY: selects the DC's own font back so the guard may delete ours.
    unsafe { SelectObject(dc, previous) };
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
    let grid_y = px(PAD);
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
    // Every cell is at least 8 physical pixels, so the grid lines always have room.
    let (line, _g2) = solid_brush(0x40, 0x40, 0x40);
    for i in 0..=side_cells {
        fill(dc, grid_x + i * cell, grid_y, 1, side, line);
        fill(dc, grid_x, grid_y + i * cell, side, 1, line);
    }
    // The picked block (one cell, or the 3 x 3 / 5 x 5 that is averaged): framed OUTSIDE, in the
    // padding and the neighbouring cells, so the pixel being picked stays visible.
    let block = signed(run.session.aperture().side());
    let first = side_cells / 2 - block / 2;
    let (block_x, block_y) = (grid_x + first * cell, grid_y + first * cell);
    for ring in pick_frame(block_x, block_y, block * cell) {
        let (shade, _ring_guard) = if ring.white {
            solid_brush(255, 255, 255)
        } else {
            solid_brush(0, 0, 0)
        };
        frame(dc, ring.x, ring.y, ring.w, ring.h, ring.thick, shade);
    }

    draw_readout(run, shot, dc, loupe, (box_w, box_h));

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
        assert!(press.down(Button::Left, false));
        // Nothing is decided by the down; a wrong button's up is not the decision either.
        assert_eq!(press.up(Button::Right, false, None), None);
        assert_eq!(press.up(Button::Left, false, None), Some(Input::Pick));
        // The press is over: a stray second up decides nothing.
        assert_eq!(press.up(Button::Left, false, None), None);

        assert!(press.down(Button::Right, false));
        assert_eq!(press.up(Button::Right, false, None), Some(Input::Cancel));
    }

    #[test]
    fn shift_makes_a_left_click_collect_and_a_finger_lifting_decides_where() {
        let mut press = Press::default();
        assert!(press.down(Button::Left, false));
        assert_eq!(press.up(Button::Left, true, None), Some(Input::Collect));
        // Shift does not change the right button.
        assert!(press.down(Button::Right, false));
        assert_eq!(press.up(Button::Right, true, None), Some(Input::Cancel));
        // A finger: the lift is a touch input at the place it lifted, shift or not.
        assert!(press.down(Button::Left, true));
        assert!(press.touching());
        assert_eq!(
            press.up(Button::Left, true, Some((4, 5))),
            Some(Input::TouchUp(4, 5))
        );
        assert!(!press.touching());
        // And the next mouse press is a mouse press again.
        assert!(press.down(Button::Left, false));
        assert!(!press.touching());
        assert_eq!(press.up(Button::Left, false, None), Some(Input::Pick));
    }

    #[test]
    fn a_second_button_while_one_is_held_is_ignored_and_lost_capture_ends_the_press() {
        let mut press = Press::default();
        assert!(press.down(Button::Left, false));
        assert!(!press.down(Button::Right, false));
        assert_eq!(press.up(Button::Right, false, None), None);
        press.lost();
        assert_eq!(press.up(Button::Left, false, None), None);
        // A new press works after the capture was lost.
        assert!(press.down(Button::Right, false));
        assert_eq!(press.up(Button::Right, false, None), Some(Input::Cancel));
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
