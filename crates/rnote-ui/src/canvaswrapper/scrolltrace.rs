//! Opt-in diagnostics for Windows touch regression tests. Disabled in normal use.
use super::*;

// Report the geometry a save would persist. A zero is a freehand/open stroke.
fn closed_brush_sides(canvas: &RnCanvas) -> Vec<usize> {
    use rnote_compose::penpath::Segment;
    use rnote_engine::strokes::Stroke;
    canvas.engine_ref().take_snapshot().stroke_components.values().filter_map(|stroke| {
        let Stroke::BrushStroke(brush) = stroke.as_ref() else { return None };
        let path = &brush.path;
        let closed = path.segments.last().is_some_and(|s| s.end().pos == path.start.pos);
        let straight = path.segments.iter().all(|s| matches!(s, Segment::LineTo { .. }));
        Some(if closed && straight { path.segments.len() } else { 0 })
    }).collect()
}

pub(super) fn install(wrapper: &RnCanvasWrapper) {
    if std::env::var_os("RNOTE_SCROLL_TRACE").is_none() {
        return;
    }
    let wrapper = wrapper.downgrade();
    glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
        let Some(wrapper) = wrapper.upgrade() else {
            return;
        };
        let mode = std::env::var("RNOTE_SCROLL_TEST").unwrap_or_default();
        let reference = mode == "reference";
        let scroller = if reference {
            let scroller = ScrolledWindow::new();
            let child = gtk4::DrawingArea::new();
            child.set_content_width(1200);
            child.set_content_height(50000);
            scroller.set_child(Some(&child));
            let window = gtk4::Window::builder()
                .title("Scroll Reference")
                .default_width(1000)
                .default_height(700)
                .child(&scroller)
                .build();
            window.present();
            #[cfg(target_os = "windows")]
            if let Some(handle) = crate::windows_surfaces::window_handle(&window) {
                eprintln!("SCROLL WINDOW {}", handle as usize);
            }
            // Keep the independent reference window alive for the external probe.
            glib::timeout_add_local_once(std::time::Duration::from_secs(120), move || {
                window.close()
            });
            let s = scroller.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
                s.vadjustment().set_value(10000.0)
            });
            scroller
        } else {
            if std::env::var_os("RNOTE_SCROLL_TEST").is_some() {
                let canvas = wrapper.canvas();
                canvas.set_touch_drawing(mode == "drawing" || mode == "brush-hold");
                wrapper.set_inertial_scrolling(mode != "disabled");
                let mut engine = canvas.engine_mut();
                engine.document.config.layout = if mode == "expanding" {
                    rnote_engine::document::Layout::Infinite
                } else {
                    rnote_engine::document::Layout::FixedSize
                };
                engine.document.height = 50000.0;
                let flags = engine.camera_set_offset_expand(Vector2::new(0.0, 10000.0));
                drop(engine);
                canvas.emit_handle_widget_flags(flags);
                canvas.queue_resize();
            }
            wrapper.scroller()
        };
        if mode == "rotation" {
            // Observe the real gesture handlers after they have applied the view.
            let pivot = Rc::new(Cell::new(None::<Vector2>));
            let canvas = wrapper.canvas();
            wrapper.imp().canvas_zoom_gesture.connect_begin(glib::clone!(
                #[strong] pivot, #[weak] canvas, move |gesture, _| {
                    pivot.set(gesture.bounding_box_center().map(|(x, y)|
                        canvas.engine_ref().camera.transform().inverse().transform_point2(Vector2::new(x, y))));
                }
            ));
            wrapper.imp().canvas_rotate_gesture.connect_angle_changed(glib::clone!(
                #[strong] pivot, #[weak] canvas, move |gesture, _, _| {
                    if let (Some(anchor), Some((x, y))) = (pivot.get(), gesture.bounding_box_center()) {
                        let engine = canvas.engine_ref();
                        let drift = engine.camera.transform().transform_point2(anchor).distance(Vector2::new(x, y));
                        eprintln!("ROTATION SAMPLE {:.3} {:.3} {:.3}", engine.camera.rotation().to_degrees(),
                            engine.camera.total_zoom(), drift);
                    }
                }
            ));
            let turns = Cell::new(0u32);
            wrapper.imp().canvas_zoom_gesture.connect_end(glib::clone!(
                #[weak] canvas, move |_, _| {
                    let engine = canvas.engine_ref();
                    eprintln!("ROTATION END {:.3} {:.3}", engine.camera.rotation().to_degrees(), engine.camera.total_zoom());
                    let turn = turns.get() + 1;
                    turns.set(turn);
                    if turn == 15 || turn == 17 {
                        // Activate the same persistent action as the Canvas menu between gestures.
                        let weak = canvas.downgrade();
                        glib::idle_add_local_once(move || {
                            if let Some(canvas) = weak.upgrade() {
                                canvas.activate_action("win.rotation-locked", None).expect("rotation lock action");
                            }
                        });
                    }
                }
            ));
        }
        let start = Instant::now();
        let events = gtk4::EventControllerLegacy::new();
        events.set_propagation_phase(PropagationPhase::Capture);
        let test_canvas = wrapper.canvas().downgrade();
        let expansion_armed = Cell::new(false);
        let trace_shapes = mode == "brush-hold";
        events.connect_event(move |_, event| {
            if trace_shapes && matches!(event.event_type(), gdk::EventType::TouchEnd | gdk::EventType::ButtonRelease) {
                let weak = test_canvas.clone();
                glib::idle_add_local_once(move || {
                    if let Some(canvas) = weak.upgrade() {
                        eprintln!("BRUSH LIFT {:?}", closed_brush_sides(&canvas));
                    }
                });
            }
            if mode == "expanding" && event.event_type() == gdk::EventType::TouchEnd && !expansion_armed.replace(true) {
                if let Some(canvas) = test_canvas.upgrade() {
                    // Put the growth threshold 20 px beyond the release position:
                    // the coast must cross it without losing its velocity.
                    let mut engine = canvas.engine_mut();
                    engine.document.height = engine.camera.viewport().maxs.y
                        + 2.0 * engine.document.config.format.height() + 20.0 - engine.document.y;
                    eprintln!("SCROLL ARMED_HEIGHT {:.3}", engine.document.height);
                    drop(engine);
                    canvas.queue_resize();
                }
            }
            if matches!(
                event.event_type(),
                gdk::EventType::TouchBegin
                    | gdk::EventType::TouchUpdate
                    | gdk::EventType::TouchEnd
                    | gdk::EventType::TouchCancel
            ) {
                eprintln!(
                    "SCROLL EVENT {} {:?} {:?} {:?}",
                    start.elapsed().as_millis(),
                    event.event_type(),
                    event.device().map(|d| d.source()),
                    event.position()
                );
            }
            glib::Propagation::Proceed
        });
        scroller.add_controller(events);
        let canvas = (!reference).then(|| wrapper.canvas().downgrade());
        let last = Cell::new((f64::NAN, f64::NAN, false));
        let last_height = Cell::new(f64::NAN);
        let last_paths = RefCell::new(Vec::new());
        scroller.add_tick_callback(move |s, _| {
            if let Some(c) = canvas.as_ref().and_then(|c| c.upgrade()) {
                if trace_shapes {
                    let paths = closed_brush_sides(&c);
                    if *last_paths.borrow() != paths {
                        eprintln!("BRUSH PREVIEW {paths:?}");
                        last_paths.replace(paths);
                    }
                }
                let height = c.engine_ref().document.height;
                if height != last_height.replace(height) { eprintln!("SCROLL HEIGHT {:.3}", height); }
            }
            let position = canvas
                .as_ref()
                .and_then(|c| c.upgrade())
                .map(|c| c.engine_ref().camera.offset())
                .unwrap_or_else(|| Vector2::new(s.hadjustment().value(), s.vadjustment().value()));
            let now = (position.x, position.y, s.is_kinetic_scrolling());
            if now != last.get() {
                eprintln!(
                    "SCROLL FRAME {} {:.3} {:.3} {}",
                    start.elapsed().as_millis(),
                    now.0,
                    now.1,
                    now.2
                );
                last.set(now);
            }
            glib::ControlFlow::Continue
        });
        eprintln!("SCROLL READY");
    });
}
