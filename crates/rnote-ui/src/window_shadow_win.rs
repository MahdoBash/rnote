//! A click-through, per-pixel-alpha shadow owned by the application window.
//! Windows composites it independently of GTK's Vulkan/OpenGL framebuffer.
use std::{cell::Cell, ffi::c_void};
type Handle = *mut c_void;
type Proc = unsafe extern "system" fn(Handle, u32, usize, isize, usize, usize) -> isize;
const ID: usize = 0x524e5348;
#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
struct Size {
    width: i32,
    height: i32,
}
#[repr(C)]
struct Blend {
    operation: u8,
    flags: u8,
    alpha: u8,
    format: u8,
}
#[repr(C)]
struct BitmapInfo {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bits: u16,
    compression: u32,
    image_size: u32,
    xppm: i32,
    yppm: i32,
    used: u32,
    important: u32,
    color: u32,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Handle,
        menu: Handle,
        instance: Handle,
        param: Handle,
    ) -> Handle;
    fn DestroyWindow(window: Handle) -> i32;
    fn GetWindowRect(window: Handle, rect: *mut Rect) -> i32;
    fn IsWindowVisible(window: Handle) -> i32;
    fn IsIconic(window: Handle) -> i32;
    fn IsZoomed(window: Handle) -> i32;
    fn ShowWindow(window: Handle, command: i32) -> i32;
    fn SetWindowPos(
        window: Handle,
        after: Handle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
    fn UpdateLayeredWindow(
        window: Handle,
        dest_dc: Handle,
        dest: *const Point,
        size: *const Size,
        src_dc: Handle,
        src: *const Point,
        key: u32,
        blend: *const Blend,
        flags: u32,
    ) -> i32;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateCompatibleDC(dc: Handle) -> Handle;
    fn DeleteDC(dc: Handle) -> i32;
    fn CreateDIBSection(
        dc: Handle,
        info: *const BitmapInfo,
        usage: u32,
        bits: *mut *mut c_void,
        section: Handle,
        offset: u32,
    ) -> Handle;
    fn SelectObject(dc: Handle, object: Handle) -> Handle;
    fn DeleteObject(object: Handle) -> i32;
}
#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(window: Handle, callback: Option<Proc>, id: usize, data: usize) -> i32;
    fn GetWindowSubclass(
        window: Handle,
        callback: Option<Proc>,
        id: usize,
        data: *mut usize,
    ) -> i32;
    fn RemoveWindowSubclass(window: Handle, callback: Option<Proc>, id: usize) -> i32;
    fn DefSubclassProc(window: Handle, message: u32, w: usize, l: isize) -> isize;
}
struct Shadow {
    window: Handle,
    enabled: Cell<bool>,
    bounds: Cell<[i32; 4]>,
    scale: Cell<f64>,
    rendered: Cell<Option<(i32, i32, i32)>>,
    position: Cell<Option<(i32, i32)>>,
    updating: Cell<bool>,
}

pub(crate) fn update(owner: Handle, enabled: bool, scale: f64, bounds: [i32; 4]) {
    unsafe {
        let mut data = 0usize;
        if GetWindowSubclass(owner, Some(messages), ID, &mut data) == 0 {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let title: Vec<u16> = "Rnote Shadow\0".encode_utf16().collect();
            // LAYERED | TRANSPARENT | NOACTIVATE | TOOLWINDOW. Ownership handles
            // minimization and keeps the shadow in the same stacking group.
            let window = CreateWindowExW(
                0x080800a0,
                class.as_ptr(),
                title.as_ptr(),
                0x88000000, // POPUP | DISABLED: decoration only
                0,
                0,
                0,
                0,
                owner,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if window.is_null() {
                return;
            }
            let state = Box::new(Shadow {
                window,
                enabled: Cell::new(enabled),
                bounds: Cell::new(bounds),
                scale: Cell::new(scale),
                rendered: Cell::new(None),
                position: Cell::new(None),
                updating: Cell::new(false),
            });
            data = Box::into_raw(state) as usize;
            if SetWindowSubclass(owner, Some(messages), ID, data) == 0 {
                drop(Box::from_raw(data as *mut Shadow));
                DestroyWindow(window);
                return;
            }
        }
        let state = &*(data as *mut Shadow);
        state.enabled.set(enabled);
        state.bounds.set(bounds);
        state.scale.set(scale);
        sync(owner, state);
    }
}
unsafe extern "system" fn messages(
    owner: Handle,
    message: u32,
    w: usize,
    l: isize,
    _: usize,
    data: usize,
) -> isize {
    unsafe {
        if message == 0x0082 {
            // WM_NCDESTROY
            RemoveWindowSubclass(owner, Some(messages), ID);
            let state = Box::from_raw(data as *mut Shadow);
            DestroyWindow(state.window);
            return DefSubclassProc(owner, message, w, l);
        }
        let result = DefSubclassProc(owner, message, w, l);
        if matches!(message, 0x0047 | 0x0018 | 0x0006) {
            // position, show, activate
            let mut current = 0;
            if GetWindowSubclass(owner, Some(messages), ID, &mut current) != 0 {
                sync(owner, &*(current as *mut Shadow));
            }
        }
        result
    }
}
unsafe fn sync(owner: Handle, state: &Shadow) {
    if state.updating.get() {
        return;
    }
    state.updating.set(true);
    unsafe {
        if !state.enabled.get()
            || IsWindowVisible(owner) == 0
            || IsIconic(owner) != 0
            || IsZoomed(owner) != 0
        {
            if IsWindowVisible(state.window) != 0 {
                ShowWindow(state.window, 0);
            }
        } else {
            let mut rect = Rect::default();
            if GetWindowRect(owner, &mut rect) != 0 {
                let [left, top, right, bottom] = state.bounds.get();
                rect.right = rect.left + right;
                rect.bottom = rect.top + bottom;
                rect.left += left;
                rect.top += top;
                let pad = (8.0 * state.scale.get()).ceil().max(1.0) as i32;
                let width = rect.right - rect.left + 2 * pad;
                let height = rect.bottom - rect.top + 2 * pad;
                let position = Point {
                    x: rect.left - pad,
                    y: rect.top - pad,
                };
                let key = (width, height, pad);
                if state.rendered.get() != Some(key) {
                    if render(
                        state.window,
                        &position,
                        width,
                        height,
                        pad,
                        state.scale.get(),
                    ) {
                        state.rendered.set(Some(key));
                        state.position.set(Some((position.x, position.y)));
                    }
                } else if state.position.get() != Some((position.x, position.y)) {
                    SetWindowPos(
                        state.window,
                        std::ptr::null_mut(),
                        position.x,
                        position.y,
                        width,
                        height,
                        0x0014,
                    ); // NOACTIVATE | NOZORDER
                    state.position.set(Some((position.x, position.y)));
                }
                if state.rendered.get() == Some(key) && IsWindowVisible(state.window) == 0 {
                    ShowWindow(state.window, 4);
                } // SHOWNOACTIVATE
            }
        }
    }
    state.updating.set(false);
}
fn alpha(x: f64, y: f64, width: f64, height: f64, pad: f64, scale: f64) -> u8 {
    let radius = 12.0 * scale;
    let qx = (x - width / 2.0).abs() - (width / 2.0 - pad - radius);
    let qy = (y - height / 2.0).abs() - (height / 2.0 - pad - radius);
    let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
    if distance <= 0.0 {
        0
    } else {
        (64.0 * (-distance * distance / (2.0 * (2.5 * scale).powi(2))).exp()).round() as u8
    }
}
unsafe fn render(
    window: Handle,
    position: &Point,
    width: i32,
    height: i32,
    pad: i32,
    scale: f64,
) -> bool {
    if width <= 0 || height <= 0 || width > 16384 || height > 16384 {
        return false;
    }
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {
            return false;
        }
        let info = BitmapInfo {
            size: 40,
            width,
            height: -height,
            planes: 1,
            bits: 32,
            compression: 0,
            image_size: 0,
            xppm: 0,
            yppm: 0,
            used: 0,
            important: 0,
            color: 0,
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(dc, &info, 0, &mut bits, std::ptr::null_mut(), 0);
        if bitmap.is_null() || bits.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            DeleteDC(dc);
            return false;
        }
        let pixels =
            std::slice::from_raw_parts_mut(bits as *mut u32, (width as usize) * (height as usize));
        pixels.fill(0);
        let corner = (12.0 * scale).ceil() as i32;
        for y in 0..height {
            for x in 0..width {
                if (x >= pad + corner && x < width - pad - corner && y >= pad && y < height - pad)
                    || (y >= pad + corner
                        && y < height - pad - corner
                        && x >= pad
                        && x < width - pad)
                {
                    continue;
                }
                pixels[(y * width + x) as usize] = (alpha(
                    x as f64 + 0.5,
                    y as f64 + 0.5,
                    width as f64,
                    height as f64,
                    pad as f64,
                    scale,
                ) as u32)
                    << 24;
            }
        }
        let previous = SelectObject(dc, bitmap);
        let ok = UpdateLayeredWindow(
            window,
            std::ptr::null_mut(),
            position,
            &Size { width, height },
            dc,
            &Point::default(),
            0,
            &Blend {
                operation: 0,
                flags: 0,
                alpha: 255,
                format: 1,
            },
            2,
        ) != 0;
        SelectObject(dc, previous);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ok
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn center_is_transparent_and_shadow_fades_outward() {
        assert_eq!(alpha(200.0, 150.0, 400.0, 300.0, 12.0, 1.0), 0);
        assert_eq!(alpha(12.5, 150.0, 400.0, 300.0, 12.0, 1.0), 0);
        assert!(
            alpha(11.0, 150.0, 400.0, 300.0, 12.0, 1.0)
                > alpha(4.0, 150.0, 400.0, 300.0, 12.0, 1.0)
        );
        assert_eq!(alpha(400.0, 300.0, 800.0, 600.0, 24.0, 2.0), 0);
    }
}
