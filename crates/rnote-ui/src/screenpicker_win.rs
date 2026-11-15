//! One-shot Windows eyedropper. Capture and consume the selecting click so it
//! cannot draw on the canvas or activate a control beneath the cursor.
use std::ffi::c_void;

type Handle = *mut c_void;
type SubclassProc = unsafe extern "system" fn(Handle, u32, usize, isize, usize, usize) -> isize;
const PICKER_ID: usize = 0x524e504b;
#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetCursorPos(point: *mut Point) -> i32;
    fn SetCapture(window: Handle) -> Handle;
    fn GetCapture() -> Handle;
    fn ReleaseCapture() -> i32;
    fn GetDC(window: Handle) -> Handle;
    fn ReleaseDC(window: Handle, dc: Handle) -> i32;
    fn LoadCursorW(instance: Handle, name: *const u16) -> Handle;
    fn SetCursor(cursor: Handle) -> Handle;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn GetPixel(dc: Handle, x: i32, y: i32) -> u32;
}
#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(
        window: Handle,
        callback: Option<SubclassProc>,
        id: usize,
        data: usize,
    ) -> i32;
    fn RemoveWindowSubclass(window: Handle, callback: Option<SubclassProc>, id: usize) -> i32;
    fn DefSubclassProc(window: Handle, message: u32, w: usize, l: isize) -> isize;
}

struct Picker {
    complete: Option<Box<dyn FnOnce(Option<[u8; 3]>)>>,
    selected: Option<[u8; 3]>,
    crosshair: Handle,
    previous_cursor: Handle,
}

pub(crate) fn pick(window: Handle, complete: impl FnOnce(Option<[u8; 3]>) + 'static) -> bool {
    if window.is_null() {
        return false;
    }
    unsafe {
        let crosshair = LoadCursorW(std::ptr::null_mut(), 32515usize as *const u16);
        let picker = Box::new(Picker {
            complete: Some(Box::new(complete)),
            selected: None,
            crosshair,
            previous_cursor: std::ptr::null_mut(),
        });
        let data = Box::into_raw(picker);
        if SetWindowSubclass(window, Some(messages), PICKER_ID, data as usize) == 0 {
            drop(Box::from_raw(data));
            return false;
        }
        SetCapture(window);
        if GetCapture() != window {
            finish(window, data, None);
            return true;
        }
        if !crosshair.is_null() {
            (*data).previous_cursor = SetCursor(crosshair);
        }
    }
    true
}

fn colorref(value: u32) -> Option<[u8; 3]> {
    (value != u32::MAX).then_some([value as u8, (value >> 8) as u8, (value >> 16) as u8])
}

unsafe fn sample(point: Point) -> Option<[u8; 3]> {
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        if dc.is_null() {
            return None;
        }
        let color = GetPixel(dc, point.x, point.y);
        ReleaseDC(std::ptr::null_mut(), dc);
        colorref(color)
    }
}

unsafe fn finish(window: Handle, data: *mut Picker, color: Option<[u8; 3]>) {
    unsafe {
        // Unhook before releasing capture, which sends a reentrant message.
        RemoveWindowSubclass(window, Some(messages), PICKER_ID);
        let mut picker = Box::from_raw(data);
        if GetCapture() == window {
            ReleaseCapture();
        }
        if !picker.previous_cursor.is_null() {
            SetCursor(picker.previous_cursor);
        }
        if let Some(complete) = picker.complete.take() {
            complete(color);
        }
    }
}

unsafe extern "system" fn messages(
    window: Handle,
    message: u32,
    w: usize,
    l: isize,
    _: usize,
    data: usize,
) -> isize {
    let data = data as *mut Picker;
    unsafe {
        match message {
            0x0200 | 0x0020 => {
                // WM_MOUSEMOVE / WM_SETCURSOR
                if !(*data).crosshair.is_null() {
                    SetCursor((*data).crosshair);
                }
                1
            }
            0x0201 => {
                // WM_LBUTTONDOWN: sample before anything under the click can change.
                let mut point = Point::default();
                if GetCursorPos(&mut point) != 0 {
                    (*data).selected = sample(point);
                }
                0
            }
            0x0202 => {
                let selected = (*data).selected;
                finish(window, data, selected);
                0
            }
            0x0246 => {
                // WM_POINTERDOWN, screen coordinates (pen/touch inside the app).
                (*data).selected = sample(Point {
                    x: l as i16 as i32,
                    y: (l >> 16) as i16 as i32,
                });
                0
            }
            0x0247 => {
                let selected = (*data).selected;
                finish(window, data, selected);
                0
            }
            0x0245 | 0x0203 | 0x0204 | 0x020a | 0x020e => 0,
            0x0205 | 0x001f | 0x0215 => {
                finish(window, data, None);
                0
            } // right release, cancel, capture lost
            0x0100 if w == 27 => {
                finish(window, data, None);
                0
            } // Escape
            0x0100 | 0x0101 | 0x0102 => 0,
            0x0082 => {
                finish(window, data, None);
                DefSubclassProc(window, message, w, l)
            }
            _ => DefSubclassProc(window, message, w, l),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
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
        fn ShowWindow(window: Handle, cmd: i32) -> i32;
        fn UpdateWindow(window: Handle) -> i32;
        fn SetForegroundWindow(window: Handle) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn SendMessageW(window: Handle, message: u32, w: usize, l: isize) -> isize;
    }
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmFlush() -> i32;
    }
    #[test]
    fn screen_click_samples_and_escape_cancels() {
        unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let window = CreateWindowExW(
                8,
                class.as_ptr(),
                std::ptr::null(),
                0x80000006,
                100,
                100,
                80,
                80,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            assert!(!window.is_null());
            ShowWindow(window, 5);
            UpdateWindow(window);
            SetForegroundWindow(window);
            DwmFlush();
            let mut old_cursor = Point::default();
            GetCursorPos(&mut old_cursor);
            SetCursorPos(140, 140);
            let result = Rc::new(Cell::new(None));
            let output = result.clone();
            assert!(pick(window, move |value| output.set(Some(value))));
            SendMessageW(window, 0x0201, 1, 0);
            SendMessageW(window, 0x0202, 0, 0);
            assert_eq!(result.get(), Some(Some([255, 255, 255])));
            assert_ne!(GetCapture(), window);
            result.set(None);
            let output = result.clone();
            assert!(pick(window, move |value| output.set(Some(value))));
            SendMessageW(window, 0x0100, 27, 0);
            assert_eq!(result.get(), Some(None));
            assert_ne!(GetCapture(), window);
            SetCursorPos(old_cursor.x, old_cursor.y);
            DestroyWindow(window);
        }
    }
    #[test]
    fn color_channels_and_invalid_pixel() {
        assert_eq!(colorref(0x00332211), Some([0x11, 0x22, 0x33]));
        assert_eq!(colorref(u32::MAX), None);
    }
}
