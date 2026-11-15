//! Opt-in diagnostics for Windows touch regression tests. Disabled in normal use.
use super::*;

pub(super) fn install(wrapper: &RnCanvasWrapper) {
    if std::env::var_os("RNOTE_SCROLL_TRACE").is_none() {
        return;
    }
    let wrapper = wrapper.downgrade();
    glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
        let Some(wrapper) = wrapper.upgrade() else { return };
        let scroller = if std::env::var("RNOTE_SCROLL_TEST").as_deref() == Ok("reference") {
            let scroller = ScrolledWindow::new();
            let child = gtk4::DrawingArea::new();
            child.set_content_width(1200);
            child.set_content_height(50000);
            scroller.set_child(Some(&child));
            let window = gtk4::Window::builder().title("Scroll Reference")
                .default_width(1000).default_height(700).child(&scroller).build();
            window.present();
            let s = scroller.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || s.vadjustment().set_value(10000.0));
            scroller
        } else {
            if std::env::var_os("RNOTE_SCROLL_TEST").is_some() {
                let canvas = wrapper.canvas();
                canvas.set_touch_drawing(false);
                wrapper.set_inertial_scrolling(true);
                let mut engine = canvas.engine_mut();
                engine.document.config.layout = rnote_engine::document::Layout::FixedSize;
                engine.document.height = 50000.0;
                let flags = engine.camera_set_offset(Vector2::new(0.0, 10000.0));
                drop(engine);
                canvas.emit_handle_widget_flags(flags);
                canvas.queue_resize();
            }
            wrapper.scroller()
        };
        let start = Instant::now();
        let events = gtk4::EventControllerLegacy::new();
        events.set_propagation_phase(PropagationPhase::Capture);
        events.connect_event(move |_, event| {
            if matches!(event.event_type(), gdk::EventType::TouchBegin | gdk::EventType::TouchUpdate | gdk::EventType::TouchEnd | gdk::EventType::TouchCancel) {
                eprintln!("SCROLL EVENT {} {:?} {:?} {:?}", start.elapsed().as_millis(), event.event_type(), event.device().map(|d| d.source()), event.position());
            }
            glib::Propagation::Proceed
        });
        scroller.add_controller(events);
        let last = Cell::new((f64::NAN, f64::NAN, false));
        scroller.add_tick_callback(move |s, _| {
            let now = (s.hadjustment().value(), s.vadjustment().value(), s.is_kinetic_scrolling());
            if now != last.get() {
                eprintln!("SCROLL FRAME {} {:.3} {:.3} {}", start.elapsed().as_millis(), now.0, now.1, now.2);
                last.set(now);
            }
            glib::ControlFlow::Continue
        });
        eprintln!("SCROLL READY");
    });
}
