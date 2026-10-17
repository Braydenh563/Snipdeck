<#
  Automation layer, part 2: Windows primitives (window lookup, SendInput mouse/keyboard, clipboard,
  clipboard history, UI Automation text, screen pixels, registry toggles with guaranteed restore).
  The C# below is compiled only by Initialize-AutoPrimitives, and only on Windows, so this file parses
  and loads anywhere. In -DryRun and -SelfTest, auto/mock/primitives.mock.ps1 replaces every function here.
  Keep this file ASCII-only and PowerShell 5.1 compatible (C# 5: no string interpolation, no `out var`).
#>

$script:AutoNativeSource = @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

namespace SnipAuto
{
    public class WinInfo
    {
        public long Hwnd; public string Title; public string ClassName; public int Pid; public bool Visible;
        public int L; public int T; public int R; public int B; public long ExStyle;
    }

    public static class Native
    {
        [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
        [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
        [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
        [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
        [StructLayout(LayoutKind.Sequential)] struct HARDWAREINPUT { public uint uMsg; public ushort wParamL; public ushort wParamH; }
        [StructLayout(LayoutKind.Explicit)] struct INPUTUNION { [FieldOffset(0)] public MOUSEINPUT mi; [FieldOffset(0)] public KEYBDINPUT ki; [FieldOffset(0)] public HARDWAREINPUT hi; }
        [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public INPUTUNION u; }
        [StructLayout(LayoutKind.Sequential)] struct GUITHREADINFO { public int cbSize; public int flags; public IntPtr hwndActive; public IntPtr hwndFocus; public IntPtr hwndCapture; public IntPtr hwndMenuOwner; public IntPtr hwndMoveSize; public IntPtr hwndCaret; public RECT rcCaret; }
        [StructLayout(LayoutKind.Sequential)] struct MONITORINFO { public int cbSize; public RECT rcMonitor; public RECT rcWork; public uint dwFlags; }
        delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);

        [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
        [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc f, IntPtr l);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
        [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
        [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
        [DllImport("user32.dll")] static extern bool IsWindow(IntPtr h);
        [DllImport("user32.dll")] static extern bool IsIconic(IntPtr h);
        [DllImport("user32.dll")] static extern bool IsZoomed(IntPtr h);
        [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
        [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h, out RECT r);
        [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr h, ref POINT p);
        [DllImport("user32.dll", EntryPoint = "GetWindowLongW")] static extern int GetWindowLong32(IntPtr h, int i);
        [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
        [DllImport("user32.dll")] static extern bool BringWindowToTop(IntPtr h);
        [DllImport("user32.dll")] static extern bool AttachThreadInput(uint a, uint b, bool attach);
        [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
        [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h, int cmd);
        [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll")] static extern bool GetGUIThreadInfo(uint tid, ref GUITHREADINFO info);
        [DllImport("user32.dll")] static extern short GetAsyncKeyState(int vk);
        [DllImport("user32.dll")] static extern bool GetCursorPos(out POINT p);
        [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll", SetLastError = true)] static extern uint SendInput(uint n, INPUT[] inputs, int size);
        [DllImport("user32.dll")] static extern int GetSystemMetrics(int i);
        [DllImport("user32.dll")] static extern uint MapVirtualKeyW(uint code, uint type);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr SendMessageTimeoutW(IntPtr h, uint msg, IntPtr w, IntPtr l, uint flags, uint timeout, out IntPtr result);
        [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "SendMessageTimeoutW")] static extern IntPtr SendMessageTimeoutSb(IntPtr h, uint msg, IntPtr w, StringBuilder l, uint flags, uint timeout, out IntPtr result);
        [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "SendMessageTimeoutW")] static extern IntPtr SendMessageTimeoutStr(IntPtr h, uint msg, IntPtr w, string l, uint flags, uint timeout, out IntPtr result);
        [DllImport("user32.dll")] static extern uint GetClipboardSequenceNumber();
        [DllImport("user32.dll")] static extern IntPtr MonitorFromWindow(IntPtr h, uint flags);
        [DllImport("user32.dll")] static extern bool GetMonitorInfoW(IntPtr m, ref MONITORINFO mi);
        [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
        [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr ctx);
        [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
        [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);

        const uint INPUT_MOUSE = 0, INPUT_KEYBOARD = 1;
        const uint KEYEVENTF_EXTENDEDKEY = 1, KEYEVENTF_KEYUP = 2, KEYEVENTF_UNICODE = 4;
        const uint MOUSEEVENTF_MOVE = 1, MOUSEEVENTF_LEFTDOWN = 2, MOUSEEVENTF_LEFTUP = 4, MOUSEEVENTF_VIRTUALDESK = 0x4000, MOUSEEVENTF_ABSOLUTE = 0x8000;
        static readonly IntPtr HWND_TOPMOST = new IntPtr(-1), HWND_NOTOPMOST = new IntPtr(-2);
        const uint SWP_NOSIZE = 1, SWP_NOMOVE = 2, SWP_NOACTIVATE = 0x10, SWP_SHOWWINDOW = 0x40;
        const uint SMTO_ABORTIFHUNG = 2;

        // Per-monitor DPI on this thread, so window rects, cursor and screen pixels share one coordinate space.
        public static void EnsureDpi()
        {
            try
            {
                if (SetThreadDpiAwarenessContext(new IntPtr(-4)) == IntPtr.Zero) SetThreadDpiAwarenessContext(new IntPtr(-3));
            }
            catch (EntryPointNotFoundException) { try { SetProcessDPIAware(); } catch { } }
        }

        static IntPtr H(long h) { return new IntPtr(h); }

        public static WinInfo Info(long hwnd)
        {
            IntPtr h = H(hwnd);
            WinInfo w = new WinInfo();
            w.Hwnd = hwnd;
            StringBuilder sb = new StringBuilder(512); GetWindowTextW(h, sb, sb.Capacity); w.Title = sb.ToString();
            StringBuilder cb = new StringBuilder(256); GetClassNameW(h, cb, cb.Capacity); w.ClassName = cb.ToString();
            uint pid; GetWindowThreadProcessId(h, out pid); w.Pid = (int)pid;
            w.Visible = IsWindowVisible(h);
            RECT r = FrameRect(h);
            w.L = r.Left; w.T = r.Top; w.R = r.Right; w.B = r.Bottom;
            w.ExStyle = (long)(uint)GetWindowLong32(h, -20);
            return w;
        }

        // DWM extended frame bounds: the visible window without the invisible resize borders.
        static RECT FrameRect(IntPtr h)
        {
            RECT r;
            try { if (DwmGetWindowAttribute(h, 9, out r, Marshal.SizeOf(typeof(RECT))) == 0) return r; } catch { }
            GetWindowRect(h, out r);
            return r;
        }

        public static int[] Frame(long hwnd) { EnsureDpi(); RECT r = FrameRect(H(hwnd)); return new int[] { r.Left, r.Top, r.Right, r.Bottom }; }

        public static List<WinInfo> ListWindows()
        {
            EnsureDpi();
            List<IntPtr> hs = new List<IntPtr>();
            EnumWindows(delegate (IntPtr h, IntPtr l) { hs.Add(h); return true; }, IntPtr.Zero);
            List<WinInfo> list = new List<WinInfo>();
            foreach (IntPtr h in hs) list.Add(Info(h.ToInt64()));
            return list;
        }

        public static List<WinInfo> ListChildren(long hwnd)
        {
            EnsureDpi();
            List<IntPtr> hs = new List<IntPtr>();
            EnumChildWindows(H(hwnd), delegate (IntPtr h, IntPtr l) { hs.Add(h); return true; }, IntPtr.Zero);
            List<WinInfo> list = new List<WinInfo>();
            foreach (IntPtr h in hs) list.Add(Info(h.ToInt64()));
            return list;
        }

        public static bool Alive(long hwnd) { return IsWindow(H(hwnd)); }
        public static long Foreground() { return GetForegroundWindow().ToInt64(); }

        public static int[] Client(long hwnd)
        {
            EnsureDpi();
            RECT r; GetClientRect(H(hwnd), out r);
            POINT p = new POINT(); ClientToScreen(H(hwnd), ref p);
            return new int[] { p.X, p.Y, p.X + r.Right, p.Y + r.Bottom };
        }

        public static int[] WorkArea(long hwnd)
        {
            EnsureDpi();
            MONITORINFO mi = new MONITORINFO(); mi.cbSize = Marshal.SizeOf(typeof(MONITORINFO));
            GetMonitorInfoW(MonitorFromWindow(H(hwnd), 2), ref mi);
            return new int[] { mi.rcWork.Left, mi.rcWork.Top, mi.rcWork.Right, mi.rcWork.Bottom };
        }

        public static int[] VirtualScreen() { return new int[] { GetSystemMetrics(76), GetSystemMetrics(77), GetSystemMetrics(78), GetSystemMetrics(79) }; }

        public static double DpiScale(long hwnd)
        {
            try { uint d = GetDpiForWindow(H(hwnd)); if (d > 0) return d / 96.0; } catch (EntryPointNotFoundException) { }
            return 1.0;
        }

        // flags (GUI_INMENUMODE = 4, GUI_SYSTEMMENUMODE = 8), focus, caret, active.
        public static long[] GuiInfo(long hwnd)
        {
            uint pid; uint tid = GetWindowThreadProcessId(H(hwnd), out pid);
            GUITHREADINFO gi = new GUITHREADINFO(); gi.cbSize = Marshal.SizeOf(typeof(GUITHREADINFO));
            if (!GetGUIThreadInfo(tid, ref gi)) return null;
            return new long[] { gi.flags, gi.hwndFocus.ToInt64(), gi.hwndCaret.ToInt64(), gi.hwndActive.ToInt64() };
        }

        // Moves/resizes a window and lifts it above normal windows WITHOUT activating it.
        public static void Place(long hwnd, int x, int y, int w, int h, bool move)
        {
            EnsureDpi();
            IntPtr hh = H(hwnd);
            if (IsIconic(hh) || IsZoomed(hh)) ShowWindow(hh, 4);
            uint f = SWP_NOACTIVATE | SWP_SHOWWINDOW; if (!move) f |= SWP_NOMOVE | SWP_NOSIZE;
            SetWindowPos(hh, HWND_TOPMOST, x, y, w, h, f);
            SetWindowPos(hh, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }

        // SetForegroundWindow with the input-queue attach that lets a background process hand focus back.
        public static bool ForceForeground(long hwnd)
        {
            IntPtr h = H(hwnd);
            IntPtr fg = GetForegroundWindow();
            if (fg == h) return true;
            uint pid; uint fgThread = GetWindowThreadProcessId(fg, out pid);
            uint me = GetCurrentThreadId();
            bool attached = false;
            if (fgThread != 0 && fgThread != me) attached = AttachThreadInput(me, fgThread, true);
            try { if (IsIconic(h)) ShowWindow(h, 9); BringWindowToTop(h); SetForegroundWindow(h); }
            finally { if (attached) AttachThreadInput(me, fgThread, false); }
            return GetForegroundWindow() == h;
        }

        public static bool KeyDown(int vk) { return (GetAsyncKeyState(vk) & 0x8000) != 0; }

        public static int[] Cursor() { EnsureDpi(); POINT p; GetCursorPos(out p); return new int[] { p.X, p.Y }; }

        static bool Send(INPUT[] inputs) { return SendInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(INPUT))) == (uint)inputs.Length; }

        public static bool MoveMouse(int x, int y)
        {
            EnsureDpi();
            int[] vs = VirtualScreen();
            INPUT i = new INPUT(); i.type = INPUT_MOUSE;
            i.u.mi.dx = (int)Math.Round((x - vs[0]) * 65535.0 / Math.Max(1, vs[2] - 1));
            i.u.mi.dy = (int)Math.Round((y - vs[1]) * 65535.0 / Math.Max(1, vs[3] - 1));
            i.u.mi.dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
            Send(new INPUT[] { i });
            POINT p; GetCursorPos(out p);
            if (Math.Abs(p.X - x) > 1 || Math.Abs(p.Y - y) > 1) { SetCursorPos(x, y); GetCursorPos(out p); }
            return Math.Abs(p.X - x) <= 1 && Math.Abs(p.Y - y) <= 1;
        }

        public static bool MouseButton(bool down)
        {
            INPUT i = new INPUT(); i.type = INPUT_MOUSE;
            i.u.mi.dwFlags = down ? MOUSEEVENTF_LEFTDOWN : MOUSEEVENTF_LEFTUP;
            return Send(new INPUT[] { i });
        }

        static bool IsExtended(int vk)
        {
            return vk == 0x5B || vk == 0x5C || vk == 0xA3 || vk == 0xA5 || (vk >= 0x21 && vk <= 0x28) || vk == 0x2D || vk == 0x2E;
        }

        public static bool Key(int vk, bool up)
        {
            INPUT i = new INPUT(); i.type = INPUT_KEYBOARD;
            i.u.ki.wVk = (ushort)vk;
            i.u.ki.wScan = (ushort)MapVirtualKeyW((uint)vk, 0);
            i.u.ki.dwFlags = (up ? KEYEVENTF_KEYUP : 0) | (IsExtended(vk) ? KEYEVENTF_EXTENDEDKEY : 0);
            return Send(new INPUT[] { i });
        }

        // Types text as Unicode key events (VK_PACKET); line breaks become Enter.
        public static bool Unicode(string text)
        {
            foreach (char c in text)
            {
                if (c == '\r') continue;
                if (c == '\n') { Key(0x0D, false); Key(0x0D, true); continue; }
                INPUT d = new INPUT(); d.type = INPUT_KEYBOARD; d.u.ki.wScan = c; d.u.ki.dwFlags = KEYEVENTF_UNICODE;
                INPUT u = d; u.u.ki.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;
                if (!Send(new INPUT[] { d, u })) return false;
            }
            return true;
        }

        // WM_GETTEXT with a timeout, so a hung window cannot block the wizard.
        public static string GetText(long hwnd, int timeoutMs)
        {
            IntPtr res;
            if (SendMessageTimeoutW(H(hwnd), 0x000E, IntPtr.Zero, IntPtr.Zero, SMTO_ABORTIFHUNG, (uint)timeoutMs, out res) == IntPtr.Zero) return null;
            int len = (int)res.ToInt64();
            StringBuilder sb = new StringBuilder(len + 2);
            if (SendMessageTimeoutSb(H(hwnd), 0x000D, new IntPtr(len + 1), sb, SMTO_ABORTIFHUNG, (uint)timeoutMs, out res) == IntPtr.Zero) return null;
            return sb.ToString();
        }

        public static bool BroadcastSettingChange(string area, int timeoutMs)
        {
            IntPtr res;
            return SendMessageTimeoutStr(new IntPtr(0xFFFF), 0x001A, IntPtr.Zero, area, SMTO_ABORTIFHUNG, (uint)timeoutMs, out res) != IntPtr.Zero;
        }

        public static uint ClipboardSeq() { return GetClipboardSequenceNumber(); }
    }
}
'@

$script:AutoReady = $false
$script:AutoRestoreFile = $null
$script:AutoRegOriginals = [ordered]@{}   # "Key|Name" -> @{ Key; Name; Existed; Value }
$script:AutoHeld = @{}                    # modifier VKs this script pressed and has not released
$script:AutoMouse = @{ Active = $false; X = 0; Y = 0 }
$script:AutoProcNames = @{}

$script:AutoRegPaths = @{
    Personalize   = 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize'
    Accessibility = 'HKCU:\SOFTWARE\Microsoft\Accessibility'
}
$script:AutoVk = @{
    Ctrl = 0xA2; Shift = 0xA0; Alt = 0xA4; Win = 0x5B
    Delete = 0x2E; End = 0x23; Home = 0x24; Esc = 0x1B; Enter = 0x0D; Tab = 0x09
}

function Initialize-AutoPrimitives([string]$StateDir) {
    if ($env:OS -ne 'Windows_NT') { return @{ Ok = $false; Reason = 'not Windows' } }
    if ($StateDir) { $script:AutoRestoreFile = Join-Path $StateDir 'auto-restore.json' }
    if ($script:AutoReady) { return @{ Ok = $true } }
    try {
        if (-not ('SnipAuto.Native' -as [type])) { Add-Type -TypeDefinition $script:AutoNativeSource -Language CSharp }
        Add-Type -AssemblyName System.Drawing
        [SnipAuto.Native]::EnsureDpi()
        $script:AutoReady = $true
        return @{ Ok = $true }
    } catch {
        return @{ Ok = $false; Reason = "could not load the Windows helpers: $($_.Exception.Message)" }
    }
}

function Register-AutoApp($Ctx) { }
function Invoke-AutoSleep([int]$Ms) { Start-Sleep -Milliseconds $Ms }

# ---------------------------------------------------------------- abort guard

function Test-AutoEscHeld { return [SnipAuto.Native]::KeyDown(0x1B) }

function Start-AutoMouseGuard {
    $p = [SnipAuto.Native]::Cursor()
    $script:AutoMouse = @{ Active = $true; X = $p[0]; Y = $p[1] }
}
function Stop-AutoMouseGuard { $script:AutoMouse.Active = $false }

# Esc held, or the pointer is not where the automation last put it (the owner touched the mouse).
function Test-AutoAbortRequested {
    if (-not $script:AutoReady) { return $null }
    if ([SnipAuto.Native]::KeyDown(0x1B)) { return 'Esc is held down' }
    if ($script:AutoMouse.Active) {
        $p = [SnipAuto.Native]::Cursor()
        if ([Math]::Abs($p[0] - $script:AutoMouse.X) -gt 3 -or [Math]::Abs($p[1] - $script:AutoMouse.Y) -gt 3) { return 'the mouse was moved' }
    }
    return $null
}

# ---------------------------------------------------------------- windows

function Get-AutoProcessName([int]$ProcessId) {
    if (-not $ProcessId) { return '' }
    if (-not $script:AutoProcNames.ContainsKey($ProcessId)) {
        $n = ''
        try { $n = (Get-Process -Id $ProcessId -ErrorAction Stop).ProcessName } catch { }
        $script:AutoProcNames[$ProcessId] = $n
    }
    return $script:AutoProcNames[$ProcessId]
}

function ConvertTo-AutoWin($W) {
    if (-not $W) { return $null }
    return @{ Hwnd = [long]$W.Hwnd; Title = $W.Title; Class = $W.ClassName; Pid = $W.Pid; Visible = $W.Visible
        Rect = @{ L = $W.L; T = $W.T; R = $W.R; B = $W.B }; ExStyle = $W.ExStyle; Process = (Get-AutoProcessName $W.Pid) }
}

function Get-AutoWindows([int]$ProcessId = 0, [string]$TitleLike = '', [string]$ClassName = '', [switch]$IncludeHidden) {
    $out = @()
    foreach ($w in [SnipAuto.Native]::ListWindows()) {
        if (-not $IncludeHidden -and -not $w.Visible) { continue }
        if ($ProcessId -and $w.Pid -ne $ProcessId) { continue }
        if ($TitleLike -and $w.Title -notlike $TitleLike) { continue }
        if ($ClassName -and $w.ClassName -ne $ClassName) { continue }
        $out += ConvertTo-AutoWin $w
    }
    return $out
}

function Get-AutoWindowInfo([long]$Hwnd) {
    if (-not $Hwnd -or -not [SnipAuto.Native]::Alive($Hwnd)) { return $null }
    return ConvertTo-AutoWin ([SnipAuto.Native]::Info($Hwnd))
}

function Get-AutoForeground { return Get-AutoWindowInfo ([SnipAuto.Native]::Foreground()) }

function Get-AutoGuiInfo([long]$Hwnd) {
    $g = [SnipAuto.Native]::GuiInfo($Hwnd)
    if (-not $g) { return $null }
    return @{ InMenuMode = (($g[0] -band 0xC) -ne 0); FocusHwnd = $g[1]; CaretHwnd = $g[2] }
}

function ConvertTo-AutoRect($A) { return @{ L = $A[0]; T = $A[1]; R = $A[2]; B = $A[3] } }
function Get-AutoClientRect([long]$Hwnd) { return ConvertTo-AutoRect ([SnipAuto.Native]::Client($Hwnd)) }
function Get-AutoFrameRect([long]$Hwnd) { return ConvertTo-AutoRect ([SnipAuto.Native]::Frame($Hwnd)) }
function Get-AutoWorkArea([long]$Hwnd) { return ConvertTo-AutoRect ([SnipAuto.Native]::WorkArea($Hwnd)) }
function Get-AutoDpiScale([long]$Hwnd) { return [SnipAuto.Native]::DpiScale($Hwnd) }
function Get-AutoVirtualScreen { $v = [SnipAuto.Native]::VirtualScreen(); return @{ L = $v[0]; T = $v[1]; R = $v[0] + $v[2]; B = $v[1] + $v[3] } }

function Set-AutoWindowRect([long]$Hwnd, $Rect) {
    if ($Rect) { [SnipAuto.Native]::Place($Hwnd, $Rect.L, $Rect.T, $Rect.R - $Rect.L, $Rect.B - $Rect.T, $true) }
    else { [SnipAuto.Native]::Place($Hwnd, 0, 0, 0, 0, $false) }
}

function Set-AutoForeground([long]$Hwnd) { return [SnipAuto.Native]::ForceForeground($Hwnd) }

# ---------------------------------------------------------------- mouse and keyboard

function Move-AutoMouse([int]$X, [int]$Y) {
    Assert-AutoContinue
    $ok = [SnipAuto.Native]::MoveMouse($X, $Y)
    $script:AutoMouse.X = $X; $script:AutoMouse.Y = $Y
    if (-not $ok) { throw "could not move the mouse to $X,$Y" }
}

# Click with optional held modifiers. Every modifier pressed here is released in `finally`.
# Returns @{ HeldSeen } : the held modifiers the system reported as down at the moment of the click.
function Invoke-AutoClick([int]$X, [int]$Y, [string[]]$Hold = @()) {
    Move-AutoMouse $X $Y
    Invoke-AutoSleep 120
    $seen = @()
    try {
        foreach ($m in $Hold) {
            $vk = $script:AutoVk[$m]
            [void][SnipAuto.Native]::Key($vk, $false); $script:AutoHeld[$vk] = $true
            Invoke-AutoSleep 60
        }
        foreach ($m in $Hold) { if (Test-AutoModifierDown $m) { $seen += $m } }
        Assert-AutoContinue
        [void][SnipAuto.Native]::MouseButton($true)
        Invoke-AutoSleep 70
        [void][SnipAuto.Native]::MouseButton($false)
        Invoke-AutoSleep 150
    } finally {
        [void][SnipAuto.Native]::MouseButton($false)
        Clear-AutoModifiers
    }
    return @{ HeldSeen = $seen }
}

# Modifier state as the system sees it, captured while the modifiers are still held (inside a click).
function Test-AutoModifierDown([string]$Name) {
    switch ($Name) {
        'Shift' { return [SnipAuto.Native]::KeyDown(0x10) }
        'Ctrl' { return [SnipAuto.Native]::KeyDown(0x11) }
        'Alt' { return [SnipAuto.Native]::KeyDown(0x12) }
        'Win' { return ([SnipAuto.Native]::KeyDown(0x5B) -or [SnipAuto.Native]::KeyDown(0x5C)) }
    }
    return $false
}

function Get-AutoModifiersDown {
    $down = @()
    foreach ($m in 'Shift', 'Ctrl', 'Alt', 'Win') { if (Test-AutoModifierDown $m) { $down += $m } }
    return $down
}

function Clear-AutoModifiers {
    if (-not $script:AutoReady) { return }
    foreach ($vk in @($script:AutoHeld.Keys)) { [void][SnipAuto.Native]::Key([int]$vk, $true) }
    $script:AutoHeld = @{}
}

# Chords like 'Ctrl+A', 'Delete', 'X', 'Ctrl+V', 'Esc'.
function Send-AutoKeys([string[]]$Chords) {
    foreach ($chord in $Chords) {
        Assert-AutoContinue
        $parts = $chord -split '\+'
        $keyName = $parts[-1]
        $vk = if ($script:AutoVk.ContainsKey($keyName)) { $script:AutoVk[$keyName] } elseif ($keyName.Length -eq 1) { [int][char]$keyName.ToUpper() } else { throw "unknown key '$keyName'" }
        try {
            if ($parts.Count -gt 1) {
                foreach ($m in $parts[0..($parts.Count - 2)]) {
                    $mvk = $script:AutoVk[$m]; [void][SnipAuto.Native]::Key($mvk, $false); $script:AutoHeld[$mvk] = $true
                }
            }
            [void][SnipAuto.Native]::Key($vk, $false); Invoke-AutoSleep 30; [void][SnipAuto.Native]::Key($vk, $true)
        } finally { Clear-AutoModifiers }
        Invoke-AutoSleep 80
    }
}

function Send-AutoText([string]$Text) {
    Assert-AutoContinue
    if (-not [SnipAuto.Native]::Unicode($Text)) { throw 'typing was blocked (SendInput)' }
    Invoke-AutoSleep 100
}

# ---------------------------------------------------------------- clipboard

function Get-AutoClipboard {
    for ($i = 0; $i -lt 5; $i++) {
        try { $t = Get-Clipboard -Raw -ErrorAction Stop; if ($null -eq $t) { return '' }; return [string]$t } catch { Start-Sleep -Milliseconds 60 }
    }
    return $null
}

function Set-AutoClipboard([string]$Text) {
    for ($i = 0; $i -lt 5; $i++) {
        try { Set-Clipboard -Value $Text -ErrorAction Stop; return } catch { Start-Sleep -Milliseconds 60 }
    }
    throw 'could not set the clipboard'
}

function Get-AutoClipboardSeq { return [SnipAuto.Native]::ClipboardSeq() }

function Wait-AutoWinRt($Op, [type]$ResultType, [int]$TimeoutMs = 3000) {
    $asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
        $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
    } | Select-Object -First 1
    $task = $asTask.MakeGenericMethod($ResultType).Invoke($null, @($Op))
    if (-not $task.Wait($TimeoutMs)) { throw 'timed out' }
    return $task.Result
}

# Windows clipboard history (Win+V) via WinRT. Windows PowerShell 5.1 only (PowerShell 7 has no WinRT projection).
function Get-AutoClipboardHistory {
    if ($PSVersionTable.PSEdition -eq 'Core') { return @{ Available = $false; Reason = 'needs Windows PowerShell 5.1 (run the .bat launcher)'; Texts = @() } }
    try {
        Add-Type -AssemblyName System.Runtime.WindowsRuntime
        $null = [Windows.ApplicationModel.DataTransfer.Clipboard, Windows.ApplicationModel.DataTransfer, ContentType = WindowsRuntime]
        if (-not [Windows.ApplicationModel.DataTransfer.Clipboard]::IsHistoryEnabled()) { return @{ Available = $false; Reason = 'clipboard history is turned off (Win+V > Turn on)'; Texts = @() } }
        $res = Wait-AutoWinRt ([Windows.ApplicationModel.DataTransfer.Clipboard]::GetHistoryItemsAsync()) ([Windows.ApplicationModel.DataTransfer.ClipboardHistoryItemsResult])
        if ([string]$res.Status -ne 'Success') { return @{ Available = $false; Reason = "history status $($res.Status)"; Texts = @() } }
        $texts = @()
        $n = 0
        foreach ($item in $res.Items) {
            if ($n++ -ge 25) { break }
            $content = $item.Content
            if ($content.Contains([Windows.ApplicationModel.DataTransfer.StandardDataFormats]::Text)) {
                $texts += [string](Wait-AutoWinRt ($content.GetTextAsync()) ([string]))
            }
        }
        return @{ Available = $true; Reason = ''; Texts = $texts }
    } catch {
        return @{ Available = $false; Reason = "could not read history: $($_.Exception.Message)"; Texts = @() }
    }
}

# ---------------------------------------------------------------- UI Automation and window text

function Get-AutoUiaElements([long]$Hwnd) {
    try {
        Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
        [SnipAuto.Native]::EnsureDpi()
        $root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
        $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        $out = @()
        foreach ($e in $all) {
            $c = $e.Current
            $r = $c.BoundingRectangle
            $rect = $null
            if (-not $r.IsEmpty -and -not [double]::IsInfinity($r.Left)) { $rect = @{ L = [int]$r.Left; T = [int]$r.Top; R = [int]$r.Right; B = [int]$r.Bottom } }
            $out += @{ Name = $c.Name; Type = ($c.ControlType.ProgrammaticName -replace '^ControlType\.', ''); Rect = $rect }
            if ($out.Count -ge 200) { break }
        }
        if ($out.Count -eq 0) { return $null }
        return $out
    } catch { return $null }
}

# Text of the edit control in a Notepad-like window: classic Edit / RichEdit via WM_GETTEXT, else UI Automation.
function Get-AutoEditText([long]$Hwnd) {
    try {
        $kids = @([SnipAuto.Native]::ListChildren($Hwnd) | Where-Object { $_.Visible -and @('Edit', 'RichEditD2DPT', 'RICHEDIT50W') -contains $_.ClassName })
        if ($kids.Count -gt 0) {
            $gui = Get-AutoGuiInfo $Hwnd
            $pick = $kids[0]
            if ($gui) { foreach ($k in $kids) { if ($k.Hwnd -eq $gui.FocusHwnd) { $pick = $k } } }
            $t = [SnipAuto.Native]::GetText($pick.Hwnd, 2000)
            if ($null -ne $t) { return @{ Ok = $true; Text = $t; Via = "WM_GETTEXT $($pick.ClassName)" } }
        }
    } catch { }
    try {
        Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
        $root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
        foreach ($ct in @([System.Windows.Automation.ControlType]::Document, [System.Windows.Automation.ControlType]::Edit)) {
            $cond = New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::ControlTypeProperty), $ct
            $el = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
            if (-not $el) { continue }
            $p = $null
            if ($el.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern, [ref]$p)) { return @{ Ok = $true; Text = $p.DocumentRange.GetText(-1); Via = 'UIA TextPattern' } }
            if ($el.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$p)) { return @{ Ok = $true; Text = $p.Current.Value; Via = 'UIA ValuePattern' } }
        }
    } catch { }
    return @{ Ok = $false; Text = $null; Via = ''; Reason = 'no readable edit control' }
}

# ---------------------------------------------------------------- screen pixels

function Get-AutoScreenPixels($Rect, [string]$SavePath = '') {
    [SnipAuto.Native]::EnsureDpi()
    $w = [int]($Rect.R - $Rect.L); $h = [int]($Rect.B - $Rect.T)
    if ($w -le 0 -or $h -le 0) { throw 'empty capture rectangle' }
    $bmp = New-Object System.Drawing.Bitmap $w, $h, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen([int]$Rect.L, [int]$Rect.T, 0, 0, (New-Object System.Drawing.Size $w, $h))
        $g.Dispose()
        if ($SavePath) {
            $dir = Split-Path -Parent $SavePath
            if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
            $bmp.Save($SavePath, [System.Drawing.Imaging.ImageFormat]::Png)
        }
        $data = $bmp.LockBits((New-Object System.Drawing.Rectangle 0, 0, $w, $h), [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $px = New-Object int[] ($w * $h)
        [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $px, 0, $px.Length)
        $bmp.UnlockBits($data)
        return @{ W = $w; H = $h; Px = $px }
    } finally { $bmp.Dispose() }
}

# ---------------------------------------------------------------- registry toggles (restored in finally, and after a crash)

function Save-AutoRestoreFile {
    if (-not $script:AutoRestoreFile) { return }
    if ($script:AutoRegOriginals.Count -eq 0) { if (Test-Path $script:AutoRestoreFile) { Remove-Item $script:AutoRestoreFile -Force }; return }
    $list = @($script:AutoRegOriginals.Values)
    [IO.File]::WriteAllText($script:AutoRestoreFile, (ConvertTo-Json -InputObject $list -Depth 4))
}

function Get-AutoRegistryDword([string]$Key, [string]$Name) {
    try { return [int](Get-ItemProperty -Path $script:AutoRegPaths[$Key] -Name $Name -ErrorAction Stop).$Name } catch { return $null }
}

# Changes an HKCU DWORD. The first change of each value records its original (also on disk) for Restore-AutoRegistry.
function Set-AutoRegistryDword([string]$Key, [string]$Name, [int]$Value) {
    $id = "$Key|$Name"
    if (-not $script:AutoRegOriginals.Contains($id)) {
        $old = Get-AutoRegistryDword $Key $Name
        $script:AutoRegOriginals[$id] = @{ Key = $Key; Name = $Name; Existed = ($null -ne $old); Value = $old }
        Save-AutoRestoreFile
    }
    $path = $script:AutoRegPaths[$Key]
    if (-not (Test-Path $path)) { New-Item -Path $path -Force | Out-Null }
    New-ItemProperty -Path $path -Name $Name -Value $Value -PropertyType DWord -Force | Out-Null
}

function Send-AutoSettingChange([string]$Area = 'ImmersiveColorSet') {
    [void][SnipAuto.Native]::BroadcastSettingChange($Area, 3000)
}

# Puts back every value changed by Set-AutoRegistryDword (idempotent) and tells running apps.
function Restore-AutoRegistry {
    if ($script:AutoRegOriginals.Count -eq 0) { return }
    foreach ($o in @($script:AutoRegOriginals.Values)) {
        $path = $script:AutoRegPaths[$o.Key]
        try {
            if ($o.Existed) { New-ItemProperty -Path $path -Name $o.Name -Value ([int]$o.Value) -PropertyType DWord -Force | Out-Null }
            else { Remove-ItemProperty -Path $path -Name $o.Name -ErrorAction SilentlyContinue }
        } catch { }
    }
    $script:AutoRegOriginals = [ordered]@{}
    Save-AutoRestoreFile
    if ($script:AutoReady) { Send-AutoSettingChange 'ImmersiveColorSet' }
}

# Called at wizard start: a previous run that died mid-toggle left auto-restore.json behind.
function Restore-AutoRegistryFromFile([string]$StateDir) {
    $file = Join-Path $StateDir 'auto-restore.json'
    if (-not (Test-Path $file)) { return $false }
    $script:AutoRestoreFile = $file
    foreach ($o in @(Get-Content -Raw -Path $file | ConvertFrom-Json)) {
        $script:AutoRegOriginals["$($o.Key)|$($o.Name)"] = @{ Key = $o.Key; Name = $o.Name; Existed = [bool]$o.Existed; Value = $o.Value }
    }
    [void](Initialize-AutoPrimitives $StateDir)
    Restore-AutoRegistry
    return $true
}

# ---------------------------------------------------------------- processes and app output

# An empty text file under the run folder for Notepad to open (unique name = window-title marker).
function New-AutoScratchFile($Ctx, [string]$Prefix) {
    $dir = Join-Path $Ctx.RunDir 'auto'
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
    $file = Join-Path $dir ('{0}-{1}.txt' -f $Prefix, (Get-Date -Format 'HHmmssfff'))
    [IO.File]::WriteAllText($file, '')
    return $file
}

function Start-AutoProcess([string]$File, [string[]]$Arguments = @(), [switch]$Elevated) {
    $splat = @{ FilePath = $File; PassThru = $true }
    if ($Arguments.Count -gt 0) { $splat.ArgumentList = $Arguments }
    if ($Elevated) { $splat.Verb = 'RunAs' }
    try { $p = Start-Process @splat; return @{ Ok = $true; Pid = $p.Id } }
    catch { return @{ Ok = $false; Reason = $_.Exception.Message } }
}

function Get-AutoAppLines($Ctx) {
    if (-not $Ctx.App) { return @() }
    return @((Read-LogTail $Ctx.App.logOut 3000) + (Read-LogTail $Ctx.App.logErr 3000))
}

function Test-AutoAppRunning($Ctx) { return ($Ctx.App -and $Ctx.App.proc -and -not $Ctx.App.proc.HasExited) }

# Relaunches the app under test with the same settings (the engine's Start-App) and updates the context.
function Restart-AutoApp($Ctx) {
    Start-App $Ctx.Spike $Ctx.App.norm
    $Ctx.App = $script:App
}

# Loader error boxes ("<exe> - Entry Point Not Found") are titled after the executable in every language.
function Find-AutoLoaderDialog([string]$Exe) {
    foreach ($w in @(Get-AutoWindows)) {
        if ($w.Title -like "$Exe.exe - *" -or $w.Title -like '*Entry Point Not Found*' -or $w.Title -like '*Ordinal Not Found*') { return $w.Title }
    }
    return $null
}

# End of the wizard: put the scratch Notepad back where it was.
function Exit-AutoSession {
    try { Restore-AutoRegistry } catch { }
    try { Clear-AutoModifiers } catch { }
    $np = $script:AutoState.Notepad
    if ($np -and $np.OrigRect -and $script:AutoReady -and [SnipAuto.Native]::Alive($np.Hwnd)) {
        try { Set-AutoWindowRect $np.Hwnd $np.OrigRect } catch { }
    }
}
