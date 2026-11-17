use eframe::egui;
use moe_frame::diagnostics::WindowObservation;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{io, mem};
use windows_sys::{
    Wdk::System::SystemServices::RtlGetVersion,
    Win32::{
        Foundation::{GetLastError, SetLastError, HWND, RECT},
        System::SystemInformation::OSVERSIONINFOW,
        UI::{
            HiDpi::{
                AreDpiAwarenessContextsEqual, GetAwarenessFromDpiAwarenessContext, GetDpiForWindow,
                GetWindowDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                DPI_AWARENESS_INVALID,
            },
            Input::KeyboardAndMouse::GetFocus,
            WindowsAndMessaging::{
                GetClientRect, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, GWL_EXSTYLE,
                GWL_STYLE, WINDOW_LONG_PTR_INDEX, WS_EX_TOPMOST,
            },
        },
    },
};

pub fn foreground() -> Option<usize> {
    // SAFETY: This query takes no handle and does not change focus.
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_null()).then_some(hwnd as usize)
}

/// GUI-subsystem processes have no console, so a startup failure needs an actual dialog.
pub fn report_startup_error(message: &str) {
    let text: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let caption: Vec<u16> = "MoeFrame window validation"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: Both strings are NUL-terminated and outlive the call; no window handle is required.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
        );
    }
}
pub fn system_version() -> io::Result<String> {
    let mut version = OSVERSIONINFOW::default();
    version.dwOSVersionInfoSize = mem::size_of::<OSVERSIONINFOW>() as u32;
    // SAFETY: The initialized output struct has the size required by RtlGetVersion.
    let status = unsafe { RtlGetVersion(&mut version) };
    if status < 0 {
        return Err(io::Error::other(format!(
            "RtlGetVersion NTSTATUS=0x{:08x}",
            status as u32
        )));
    }
    Ok(format!(
        "Windows NT {}.{}.{}; architecture={}",
        version.dwMajorVersion,
        version.dwMinorVersion,
        version.dwBuildNumber,
        std::env::consts::ARCH
    ))
}

pub fn observe(ctx: &egui::Context, frame: &eframe::Frame) -> WindowObservation {
    let size = ctx.viewport_rect().size();
    let mut sample = WindowObservation {
        logical_client: Some([size.x, size.y]),
        pixels_per_point: Some(ctx.pixels_per_point()),
        zoom_factor: Some(ctx.zoom_factor()),
        egui_maximized: ctx.input(|input| input.viewport().maximized),
        ..Default::default()
    };
    if let Some(window) = frame.winit_window() {
        let physical = window.inner_size();
        let scale = window.scale_factor();
        let logical = physical.to_logical::<f64>(scale);
        sample.physical_client = Some([physical.width, physical.height]);
        sample.system_logical_client = Some([logical.width, logical.height]);
        sample.native_scale_factor = Some(scale);
        sample.maximized = Some(window.is_maximized());
        sample.fullscreen = Some(window.fullscreen().is_some());
        sample.winit_focus = Some(window.has_focus());
        sample.visible = window.is_visible();
    }
    let handle = match frame.window_handle() {
        Ok(handle) => handle,
        Err(error) => {
            sample.error = Some(format!("window handle unavailable: {error}"));
            return sample;
        }
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        sample.error = Some("not a Win32 window handle".to_owned());
        return sample;
    };
    let hwnd = win32.hwnd.get() as HWND;
    sample.hwnd = Some(hwnd as usize);
    let mut errors = Vec::new();
    // SAFETY: This HWND is borrowed from the live Frame on its UI thread. It is never retained.
    // SAFETY: read-only Win32 queries against an HWND borrowed from the live Frame on its owning
    // (UI) thread. The handle is not stored past this call, no message pump runs here, and every
    // call only fills caller-owned OUT parameters or reads a LONG_PTR, so no aliasing or reentrancy
    // hazard exists.
    unsafe {
        let mut client = RECT::default();
        if GetClientRect(hwnd, &mut client) != 0 {
            sample.win32_client = Some([client.right - client.left, client.bottom - client.top]);
        } else {
            errors.push(format!("GetClientRect: {}", io::Error::last_os_error()));
        }
        let mut outer = RECT::default();
        if GetWindowRect(hwnd, &mut outer) != 0 {
            sample.window_rect = Some([outer.left, outer.top, outer.right, outer.bottom]);
        } else {
            errors.push(format!("GetWindowRect: {}", io::Error::last_os_error()));
        }
        let dpi = GetDpiForWindow(hwnd);
        if dpi != 0 {
            sample.dpi = Some(dpi);
        } else {
            errors.push("GetDpiForWindow returned 0 (unavailable)".to_owned());
        }
        let awareness_context = GetWindowDpiAwarenessContext(hwnd);
        if !awareness_context.is_null() {
            let awareness = GetAwarenessFromDpiAwarenessContext(awareness_context);
            if awareness != DPI_AWARENESS_INVALID {
                sample.dpi_awareness = Some(awareness);
            } else {
                errors.push("DPI awareness unavailable".to_owned());
            }
            sample.dpi_awareness_v2 = Some(
                AreDpiAwarenessContextsEqual(
                    awareness_context,
                    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                ) != 0,
            );
        } else {
            errors.push("window DPI awareness context unavailable".to_owned());
        }
        sample.foreground_hwnd = foreground();
        sample.foreground = sample
            .foreground_hwnd
            .map(|foreground| foreground == hwnd as usize);
        // GetFocus only sees this thread's queue, so it cannot prove focus held by another process.
        sample.keyboard_focus = sample
            .foreground
            .map(|foreground| foreground && GetFocus() == hwnd);
        match window_long(hwnd, GWL_STYLE) {
            Ok(style) => {
                sample.window_style = Some(style);
                // winit keeps WS_CAPTION/WS_THICKFRAME for undecorated windows, so style bits cannot prove framelessness.
            }
            Err(error) => errors.push(format!("GWL_STYLE: {error}")),
        }
        match window_long(hwnd, GWL_EXSTYLE) {
            Ok(style) => {
                sample.window_ex_style = Some(style);
                sample.topmost = Some(style & WS_EX_TOPMOST as isize != 0);
            }
            Err(error) => errors.push(format!("GWL_EXSTYLE: {error}")),
        }
    }
    if !errors.is_empty() {
        sample.error = Some(errors.join("; "));
    }
    sample
}

unsafe fn window_long(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> io::Result<isize> {
    // SAFETY: Caller guarantees the borrowed HWND is live on this UI thread. These are queries.
    unsafe {
        SetLastError(0);
        let value = GetWindowLongPtrW(hwnd, index);
        let error = GetLastError();
        if value == 0 && error != 0 {
            Err(io::Error::from_raw_os_error(error as i32))
        } else {
            Ok(value)
        }
    }
}
