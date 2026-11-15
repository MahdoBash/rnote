//! Clip native HWNDs to the visible GTK content when the GPU cannot composite alpha.
//! CSS alone cannot hide the black backing surface on affected Windows drivers.
use glib::translate::*;
use gtk4::{glib, prelude::*};
use std::{cell::Cell, ffi::c_void, rc::Rc};

type Handle = *mut c_void;
#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}

#[link(name = "gtk-4")]
unsafe extern "C" {
    fn gdk_win32_surface_get_handle(surface: *mut gtk4::gdk::ffi::GdkSurface) -> Handle;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowRect(window: Handle, rect: *mut Rect) -> i32;
    fn GetClientRect(window: Handle, rect: *mut Rect) -> i32;
    fn ClientToScreen(window: Handle, point: *mut Point) -> i32;
    fn SetWindowRgn(window: Handle, region: Handle, redraw: i32) -> i32;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateRoundRectRgn(
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        width: i32,
        height: i32,
    ) -> Handle;
    fn DeleteObject(object: Handle) -> i32;
}

pub(crate) fn install() {
    // Installed once during GTK startup, before any application window is created.
    // A realization hook also covers lazy menus, dialogs and future windows.
    unsafe {
        let signal = glib::gobject_ffi::g_signal_lookup(
            c"realize".as_ptr(),
            gtk4::Widget::static_type().into_glib(),
        );
        glib::gobject_ffi::g_signal_add_emission_hook(
            signal,
            0,
            Some(realized),
            std::ptr::null_mut(),
            None,
        );
        let mapped_signal = glib::gobject_ffi::g_signal_lookup(c"map".as_ptr(), gtk4::Widget::static_type().into_glib());
        glib::gobject_ffi::g_signal_add_emission_hook(mapped_signal, 0, Some(mapped), std::ptr::null_mut(), None);

    }
}

unsafe extern "C" fn realized(
    _: *mut glib::gobject_ffi::GSignalInvocationHint,
    count: u32,
    values: *const glib::gobject_ffi::GValue,
    _: *mut c_void,
) -> glib::ffi::gboolean {
    if count == 0 {
        return 1;
    }
    // The first signal argument is the live widget; retain it only for this call.
    let widget: gtk4::Widget = unsafe {
        from_glib_none(glib::gobject_ffi::g_value_get_object(values) as *mut gtk4::ffi::GtkWidget)
    };
    if !widget.is::<gtk4::Window>() && !widget.is::<gtk4::Popover>() {
        return 1;
    }
    if let Some(popover) = widget.downcast_ref::<gtk4::Popover>() {
        // A rounded native region is rectangular; remove the external arrow rather
        // than leaving transparent pixels in a separate triangular surface area.
        popover.set_has_arrow(false);
    }
    if let Some(surface) = widget.native().and_then(|native| native.surface()) {
        let last = Rc::new(Cell::new(None));
        let weak = widget.downgrade();
        surface.connect_layout(move |surface, _, _| {
            if let Some(widget) = weak.upgrade() {
                clip(&widget, surface, &last);
            }
        });
    }
    1
}

// Initial surface layout may happen inside realize, before our layout handler
// is connected. Mapping is a separate, guaranteed initial clipping point.
unsafe extern "C" fn mapped(
    _: *mut glib::gobject_ffi::GSignalInvocationHint,
    count: u32,
    values: *const glib::gobject_ffi::GValue,
    _: *mut c_void,
) -> glib::ffi::gboolean {
    if count == 0 { return 1; }
    let widget: gtk4::Widget = unsafe {
        from_glib_none(glib::gobject_ffi::g_value_get_object(values) as *mut gtk4::ffi::GtkWidget)
    };
    if !widget.is::<gtk4::Window>() && !widget.is::<gtk4::Popover>() { return 1; }
    let last = Cell::new(None);
    let frames = Cell::new(0);
    widget.add_tick_callback(move |widget, _| {
        if let Some(native) = widget.dynamic_cast_ref::<gtk4::Native>() {
            if let Some(surface) = native.surface() { clip(widget, &surface, &last); }
        }
        // Tick callbacks precede allocation. The second frame also covers a
        // widget that had no allocated content in its first frame.
        frames.set(frames.get() + 1);
        if frames.get() >= 2 { glib::ControlFlow::Break } else { glib::ControlFlow::Continue }
    });
    1
}

fn clip(
    widget: &gtk4::Widget,
    surface: &gtk4::gdk::Surface,
    last: &Cell<Option<(i32, i32, i32, i32, i32)>>,
) {
    let Some(native) = widget.native() else {
        return;
    };
    let (tx, ty) = native.surface_transform();
    let mut bounds =
        gtk4::graphene::Rect::new(0.0, 0.0, widget.width() as f32, widget.height() as f32);
    if widget.is::<gtk4::Popover>() {
        // Use GTK's actual content allocation, excluding all external shadow space.
        let mut child = widget.first_child();
        while let Some(current) = child {
            if current.css_name() == "contents" {
                if let Some(rect) = current.compute_bounds(widget) {
                    bounds = rect;
                }
                break;
            }
            child = current.next_sibling();
        }
    }
    if bounds.width() <= 0.0 || bounds.height() <= 0.0 || surface.width() <= 0 {
        return;
    }
    let square = widget
        .downcast_ref::<gtk4::Window>()
        .is_some_and(|w| w.is_maximized() || w.is_fullscreen());
    // HWND regions use physical window coordinates, not logical client coordinates.
    unsafe {
        let window = gdk_win32_surface_get_handle(surface.to_glib_none().0);
        if window.is_null() {
            return;
        }
        let mut outer = Rect::default();
        let mut client = Rect::default();
        let mut origin = Point::default();
        if GetWindowRect(window, &mut outer) == 0
            || GetClientRect(window, &mut client) == 0
            || ClientToScreen(window, &mut origin) == 0
        {
            return;
        }
        let scale = (client.right - client.left) as f64 / surface.width() as f64;
        let x = origin.x - outer.left + ((bounds.x() as f64 + tx) * scale).round() as i32;
        let y = origin.y - outer.top + ((bounds.y() as f64 + ty) * scale).round() as i32;
        let right = x + (bounds.width() as f64 * scale).round() as i32;
        let bottom = y + (bounds.height() as f64 * scale).round() as i32;
        let diameter = if square {
            0
        } else {
            (24.0 * scale).round() as i32
        };
        let key = (x, y, right, bottom, diameter);
        if last.get() == Some(key) {
            return;
        }
        let region = CreateRoundRectRgn(x, y, right + 1, bottom + 1, diameter, diameter);
        if region.is_null() {
            return;
        }
        let result = SetWindowRgn(window, region, 1);
        if std::env::var_os("RNOTE_TRACE_WINDOWS_REGIONS").is_some() {
            eprintln!("Native region: hwnd={window:?}, bounds={key:?}, result={result}");
        }
        if result == 0 {
            DeleteObject(region);
        } else {
            // On success Windows owns the region. Do not delete it.
            last.set(Some(key));
        }
    }
}
