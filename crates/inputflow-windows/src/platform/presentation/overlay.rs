use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::{io, slice};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::core::w;

use super::session::{Brush, Rect, Segment};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Display {
    pub id: String,
    pub primary: bool,
    pub bounds: Rect,
}

pub(super) fn displays() -> Vec<Display> {
    let mut result = Vec::<Display>::new();
    // SAFETY: Enumeration is synchronous and the pointer references this live vector.
    unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(add_display),
            &mut result as *mut Vec<Display> as LPARAM,
        )
    };
    result.sort_by(|a, b| b.primary.cmp(&a.primary).then_with(|| a.id.cmp(&b.id)));
    result
}

unsafe extern "system" fn add_display(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> i32 {
    // SAFETY: The callback receives the Vec passed by displays().
    let target = unsafe { &mut *(data as *mut Vec<Display>) };
    let mut details = MONITORINFOEXW::default();
    details.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: GetMonitorInfoW writes the expanded structure according to cbSize.
    if unsafe { GetMonitorInfoW(monitor, &mut details.monitorInfo) } != 0 {
        let id_len = details
            .szDevice
            .iter()
            .position(|&ch| ch == 0)
            .unwrap_or(details.szDevice.len());
        let bounds = details.monitorInfo.rcMonitor;
        target.push(Display {
            id: String::from_utf16_lossy(&details.szDevice[..id_len]),
            primary: details.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            bounds: Rect {
                left: bounds.left,
                top: bounds.top,
                right: bounds.right,
                bottom: bounds.bottom,
            },
        });
    }
    1
}

const CLASS: windows_sys::core::PCWSTR = w!("InputFlowPresentationOverlay");
const MAX_PIXELS: usize = 16 * 1024 * 1024;

pub(in crate::platform) struct Overlay {
    window: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u32,
    bounds: Rect,
    width: i32,
    height: i32,
    first: bool,
}

impl Overlay {
    pub fn new(bounds: Rect) -> io::Result<Self> {
        let (width, height) = (bounds.right - bounds.left, bounds.bottom - bounds.top);
        if width <= 0
            || height <= 0
            || (width as usize).saturating_mul(height as usize) > MAX_PIXELS
        {
            return Err(io::Error::other(
                "display cannot fit the bounded overlay buffer",
            ));
        }
        let mut overlay = Self {
            window: null_mut(),
            dc: null_mut(),
            bitmap: null_mut(),
            previous: null_mut(),
            bits: null_mut(),
            bounds,
            width,
            height,
            first: true,
        };
        // SAFETY: Window/DIB resources belong to the current message thread and
        // their signatures use initialized structures.
        unsafe {
            let module = GetModuleHandleW(null());
            let definition = WNDCLASSW {
                hInstance: module,
                lpfnWndProc: Some(window_proc),
                lpszClassName: CLASS,
                ..Default::default()
            };
            // Existing registrations of this class are reusable within the process.
            RegisterClassW(&definition);
            overlay.window = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST,
                CLASS,
                w!("InputFlow Presentation Pointer"),
                WS_POPUP,
                bounds.left,
                bounds.top,
                width,
                height,
                null_mut(),
                null_mut(),
                module,
                null(),
            );
            if overlay.window.is_null() {
                return Err(io::Error::last_os_error());
            }
            overlay.dc = CreateCompatibleDC(null_mut());
            if overlay.dc.is_null() {
                return Err(io::Error::last_os_error());
            }
            let header = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = null_mut();
            overlay.bitmap = CreateDIBSection(
                overlay.dc,
                &header,
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            );
            if overlay.bitmap.is_null() || bits.is_null() {
                return Err(io::Error::last_os_error());
            }
            overlay.bits = bits.cast();
            overlay.previous = SelectObject(overlay.dc, overlay.bitmap);
            if overlay.previous.is_null() {
                return Err(io::Error::last_os_error());
            }
            slice::from_raw_parts_mut(overlay.bits, (width * height) as usize).fill(0);
        }
        Ok(overlay)
    }

    pub(super) fn render(
        &mut self,
        dirty: Rect,
        ink: &[Segment],
        pointer: Option<((i32, i32), Brush)>,
    ) -> io::Result<()> {
        // SAFETY: The DIB's bound buffer is owned by self and writable on this thread.
        let pixels =
            unsafe { slice::from_raw_parts_mut(self.bits, (self.width * self.height) as usize) };
        let mut canvas = Canvas {
            pixels,
            width: self.width as usize,
            clip: dirty,
        };
        canvas.fill_rect(dirty, 0);
        for segment in ink
            .iter()
            .filter(|segment| segment.bounds().intersect(dirty).is_some())
        {
            canvas.segment(*segment);
        }
        if let Some((position, brush)) = pointer {
            let (color, radius, inner) = brush.pointer();
            canvas.circle(position, radius, inner, color);
        }
        self.present(dirty)
    }

    pub fn identify(&mut self, number: usize) -> io::Result<()> {
        let clip = Rect {
            left: 0,
            top: 0,
            right: self.width,
            bottom: self.height,
        };
        // SAFETY: The DIB's bound buffer is owned by self and writable on this thread.
        let pixels =
            unsafe { slice::from_raw_parts_mut(self.bits, (self.width * self.height) as usize) };
        let mut canvas = Canvas {
            pixels,
            width: self.width as usize,
            clip,
        };
        canvas.fill_rect(clip, 0xd0101010);
        let digits = number.to_string();
        for (column, digit) in digits.bytes().enumerate() {
            let x = (self.width - digits.len() as i32 * 26) / 2 + column as i32 * 26;
            let y = (self.height - 38) / 2;
            canvas.digit(digit - b'0', x, y);
        }
        self.present(clip)
    }

    pub(in crate::platform) fn panel<A>(
        &mut self,
        geometry: &inputflow_touchpad::panel::Geometry<A>,
        surface: &inputflow_touchpad::panel::Surface,
        highlight: Option<usize>,
    ) -> io::Result<()> {
        let clip = self.panel_background();
        let pixel = |[x, y]: [f64; 2]| POINT {
            x: 6 + (x * f64::from(self.width - 12)).round() as i32,
            y: 6 + (y * f64::from(self.height - 12)).round() as i32,
        };
        // SAFETY: The worker owns the DC, selected bitmap and all GDI handles below.
        unsafe {
            let old_pen = SelectObject(self.dc, GetStockObject(NULL_PEN));
            for (i, region) in geometry.regions.iter().enumerate() {
                let color = if highlight == Some(i) {
                    0x00b95a18
                } else if region.action.is_none() {
                    0x0029211d
                } else {
                    0x00473831
                };
                let brush = CreateSolidBrush(color);
                let old_brush = SelectObject(self.dc, brush);
                for face in &region.faces {
                    let points: Vec<_> = face.iter().copied().map(pixel).collect();
                    Polygon(self.dc, points.as_ptr(), points.len() as i32);
                }
                SelectObject(self.dc, old_brush);
                DeleteObject(brush);
            }
            SelectObject(self.dc, old_pen);
            let gap = (geometry.gap[0] * f64::from(self.width - 12)
                + geometry.gap[1] * f64::from(self.height - 12))
            .ceil() as i32;
            let pen = CreatePen(PS_SOLID, gap.max(1), 0x00201713);
            let old = SelectObject(self.dc, pen);
            for [a, b] in surface.seams().iter().copied() {
                let (a, b) = (pixel(a), pixel(b));
                MoveToEx(self.dc, a.x, a.y, null_mut());
                LineTo(self.dc, b.x, b.y);
            }
            SelectObject(self.dc, old);
            DeleteObject(pen);
            for region in &geometry.regions {
                let union = CreateRectRgn(0, 0, 0, 0);
                let mut label_face = None;
                let mut area = 0;
                for face in &region.faces {
                    let points: Vec<_> = face.iter().copied().map(pixel).collect();
                    let part = CreatePolygonRgn(points.as_ptr(), points.len() as i32, WINDING);
                    CombineRgn(union, union, part, RGN_OR);
                    DeleteObject(part);
                    let width = points.iter().map(|point| point.x).max().unwrap_or_default()
                        - points.iter().map(|point| point.x).min().unwrap_or_default();
                    let height = points.iter().map(|point| point.y).max().unwrap_or_default()
                        - points.iter().map(|point| point.y).min().unwrap_or_default();
                    if width * height > area {
                        area = width * height;
                        label_face = Some((points, width, height));
                    }
                }
                if let Some((points, width, height)) = label_face {
                    let x = points.iter().map(|point| point.x).sum::<i32>() / points.len() as i32;
                    let y = points.iter().map(|point| point.y).sum::<i32>() / points.len() as i32;
                    SelectClipRgn(self.dc, union);
                    self.draw_label(
                        &region.name,
                        RECT {
                            left: x - width / 2,
                            right: x + width / 2,
                            top: y - height / 2,
                            bottom: y + height / 2,
                        },
                    );
                    SelectClipRgn(self.dc, null_mut());
                }
                DeleteObject(union);
            }
        }
        self.finish_panel(clip)
    }

    pub(in crate::platform) fn message(&mut self, message: &str) -> io::Result<()> {
        let clip = self.panel_background();
        // SAFETY: The HWND, DC and DIB belong to this worker.
        unsafe {
            self.draw_label(
                message,
                RECT {
                    left: 6,
                    top: 0,
                    right: self.width - 6,
                    bottom: self.height,
                },
            );
        }
        self.finish_panel(clip)
    }

    unsafe fn draw_label(&self, label: &str, mut bounds: RECT) {
        let text: Vec<_> = label.encode_utf16().collect();
        // SAFETY: The synchronous GDI calls read live text, bounds and a valid memory DC.
        unsafe {
            let old = SelectObject(self.dc, GetStockObject(DEFAULT_GUI_FONT));
            SetTextColor(self.dc, 0x00ffffff);
            SetBkMode(self.dc, TRANSPARENT as i32);
            DrawTextW(
                self.dc,
                text.as_ptr(),
                text.len() as i32,
                &mut bounds,
                DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            SelectObject(self.dc, old);
        }
    }

    fn panel_background(&self) -> Rect {
        let clip = Rect {
            left: 0,
            top: 0,
            right: self.width,
            bottom: self.height,
        };
        // SAFETY: The DIB is valid for the bounded pixel count and is worker-owned.
        let pixels =
            unsafe { slice::from_raw_parts_mut(self.bits, (self.width * self.height) as usize) };
        pixels.fill(0xff131720);
        clip
    }

    fn finish_panel(&mut self, clip: Rect) -> io::Result<()> {
        // SAFETY: Flush GDI before accessing the worker-owned DIB bytes. GDI does
        // not preserve the alpha byte of drawn pixels.
        unsafe {
            GdiFlush();
            for pixel in slice::from_raw_parts_mut(self.bits, (self.width * self.height) as usize) {
                *pixel |= 0xff000000;
            }
        }
        self.present(clip)
    }

    fn present(&mut self, dirty: Rect) -> io::Result<()> {
        let to = POINT {
            x: self.bounds.left,
            y: self.bounds.top,
        };
        let from = POINT::default();
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let dirty = RECT {
            left: dirty.left,
            top: dirty.top,
            right: dirty.right,
            bottom: dirty.bottom,
        };
        let update = UPDATELAYEREDWINDOWINFO {
            cbSize: size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
            hdcDst: null_mut(),
            pptDst: &to,
            psize: &size,
            hdcSrc: self.dc,
            pptSrc: &from,
            crKey: 0,
            pblend: &blend,
            dwFlags: ULW_ALPHA,
            prcDirty: if self.first { null() } else { &dirty },
        };
        // SAFETY: All buffers are live through the synchronous update. Showing
        // the topmost tool window with SWP_NOACTIVATE preserves foreground focus.
        unsafe {
            if UpdateLayeredWindowIndirect(self.window, &update) == 0 {
                return Err(io::Error::last_os_error());
            }
            if self.first {
                if SetWindowPos(
                    self.window,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
                self.first = false;
            }
        }
        Ok(())
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        // SAFETY: Only initialized resources owned by the window thread are freed.
        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.dc, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
            if !self.window.is_null() {
                DestroyWindow(self.window);
            }
        }
    }
}

unsafe extern "system" fn window_proc(window: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        // SAFETY: Delegating unhandled messages is the normal window procedure contract.
        _ => unsafe { DefWindowProcW(window, msg, wp, lp) },
    }
}

struct Canvas<'a> {
    pixels: &'a mut [u32],
    width: usize,
    clip: Rect,
}

impl Canvas<'_> {
    fn digit(&mut self, digit: u8, x: i32, y: i32) {
        const MASKS: [u8; 10] = [0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x07, 0x7f, 0x6f];
        let mask = MASKS[usize::from(digit)];
        for (i, (left, top, right, bottom)) in [
            (3, 0, 20, 4),
            (19, 3, 23, 19),
            (19, 19, 23, 35),
            (3, 34, 20, 38),
            (0, 19, 4, 35),
            (0, 3, 4, 19),
            (3, 17, 20, 21),
        ]
        .into_iter()
        .enumerate()
        {
            if mask & (1 << i) != 0 {
                self.fill_rect(
                    Rect {
                        left: x + left,
                        top: y + top,
                        right: x + right,
                        bottom: y + bottom,
                    },
                    0xffffffff,
                );
            }
        }
    }

    fn fill_rect(&mut self, bounds: Rect, color: u32) {
        let Some(bounds) = bounds.intersect(self.clip) else {
            return;
        };
        for y in bounds.top..bounds.bottom {
            let row = y as usize * self.width;
            self.pixels[row + bounds.left as usize..row + bounds.right as usize].fill(color);
        }
    }

    fn circle(&mut self, (x, y): (i32, i32), radius: i32, inner: i32, color: u32) {
        let Some(bounds) = Rect::around((x, y), radius).intersect(self.clip) else {
            return;
        };
        let color = 0xff000000 | color;
        for py in bounds.top..bounds.bottom {
            let row = py as usize * self.width;
            for px in bounds.left..bounds.right {
                let distance = (px - x).pow(2) + (py - y).pow(2);
                if distance <= radius * radius && distance >= inner * inner {
                    self.pixels[row + px as usize] = color;
                }
            }
        }
    }

    fn segment(&mut self, segment: Segment) {
        let (mut x, mut y) = segment.from;
        let (end_x, end_y) = segment.to;
        let dx = (end_x - x).abs();
        let dy = -(end_y - y).abs();
        let sx = (end_x - x).signum();
        let sy = (end_y - y).signum();
        let mut error = dx + dy;
        loop {
            self.circle((x, y), segment.radius, 0, segment.color);
            if (x, y) == (end_x, end_y) {
                break;
            }
            let doubled = 2 * error;
            if doubled >= dy {
                error += dy;
                x += sx;
            }
            if doubled <= dx {
                error += dx;
                y += sy;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "opens a small non-activating overlay on the interactive desktop"]
    fn interactive_overlay_updates_and_preserves_foreground() {
        use windows_sys::Win32::UI::HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
        };
        // SAFETY: Set the awareness for the current window-owning test thread.
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let display = displays().into_iter().next().expect("interactive display");
        let bounds = Rect {
            left: display.bounds.left + 20,
            top: display.bounds.top + 20,
            right: display.bounds.left + 140,
            bottom: display.bounds.top + 100,
        };
        // SAFETY: Treat the foreground handle only as an identity.
        let foreground = unsafe { GetForegroundWindow() };
        assert!(!foreground.is_null());
        let mut overlay = Overlay::new(bounds).unwrap();
        let brush = Brush::Ring {
            color: 0xff3333,
            radius: 10,
            width: 3,
        };
        overlay
            .render(
                Rect {
                    left: 0,
                    top: 0,
                    right: 120,
                    bottom: 80,
                },
                &[],
                Some(((20, 20), brush)),
            )
            .unwrap();
        overlay
            .render(
                Rect {
                    left: 9,
                    top: 9,
                    right: 45,
                    bottom: 45,
                },
                &[],
                Some(((32, 32), brush)),
            )
            .unwrap();
        // SAFETY: Both handles come from live window APIs.
        assert_ne!(unsafe { IsWindowVisible(overlay.window) }, 0);
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        drop(overlay);
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    }
}
