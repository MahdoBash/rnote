//! Preserve GTK's direct dragging; own only the Windows touch-release handoff.
//! Windows can deliver the final update and release in the same event batch,
//! for which GtkGestureSwipe sometimes reports no velocity. One coast owner
//! also avoids losing momentum when an infinite canvas changes its bounds.
use super::{coast::Motion, *};

#[derive(Default, Debug)]
struct State {
    contacts: Vec<gdk::EventSequence>,
    motion: Option<Motion>,
    start_offset: Vector2,
    generation: u64,
}
#[derive(Default, Debug, Clone)]
pub(super) struct Controller {
    state: Rc<RefCell<State>>,
    tick: Rc<RefCell<Option<gtk4::TickCallbackId>>>,
}
impl Controller {
    pub(super) fn cancel(&self) {
        let mut state = self.state.borrow_mut();
        state.generation = state.generation.wrapping_add(1);
        state.motion = None;
        drop(state);
        if let Some(tick) = self.tick.borrow_mut().take() {
            tick.remove();
        }
    }

    pub(super) fn install(&self, wrapper: &RnCanvasWrapper) {
        let events = gtk4::EventControllerLegacy::new();
        events.set_propagation_phase(PropagationPhase::Capture);
        let owner = self.clone();
        events.connect_event(clone!(
            #[weak]
            wrapper,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, event| {
                owner.event(&wrapper, event);
                glib::Propagation::Proceed
            }
        ));
        wrapper.scroller().add_controller(events);
        // A tab switch, minimize or closed window must not leave a ticking coast.
        let owner = self.clone();
        wrapper.connect_unmap(move |_| owner.cancel());
    }

    fn event(&self, wrapper: &RnCanvasWrapper, event: &gdk::Event) {
        use gdk::EventType::*;
        let kind = event.event_type();
        if (matches!(kind, ButtonPress) && !event.is_pointer_emulated())
            || matches!(kind, Scroll | KeyPress | TouchpadPinch | TouchCancel)
        {
            self.cancel();
        }
        if !matches!(kind, TouchBegin | TouchUpdate | TouchEnd | TouchCancel) {
            return;
        }
        let sequence = event.event_sequence();
        if kind == TouchBegin {
            self.cancel();
            let mut state = self.state.borrow_mut();
            if !state.contacts.contains(&sequence) {
                state.contacts.push(sequence);
            }
            if state.contacts.len() == 1
                && !wrapper.canvas().touch_drawing()
                && wrapper.inertial_scrolling()
            {
                state.start_offset = wrapper.canvas().engine_ref().camera.offset();
                if let Some((x, y)) = event.position() {
                    state.motion = Some(Motion::new(event.time(), [x, y]));
                }
            }
            return;
        }
        if kind == TouchUpdate {
            if let Some((x, y)) = event.position() {
                if let Some(motion) = &mut self.state.borrow_mut().motion {
                    motion.push(event.time(), [x, y]);
                }
            }
            return;
        }
        let mut state = self.state.borrow_mut();
        state.contacts.retain(|s| s != &sequence);
        let motion = state.motion.take();
        let generation = state.generation;
        let moved =
            (wrapper.canvas().engine_ref().camera.offset() - state.start_offset).length() >= 4.0;
        let eligible = kind == TouchEnd && state.contacts.is_empty() && moved && motion.is_some();
        let coast = motion.and_then(|motion| motion.release(event.time()));
        drop(state);
        if !eligible {
            return;
        }
        // Run after GTK has processed TouchEnd. Cancel its optional coast before
        // starting ours, so the two animations never fight or double the speed.
        let owner = self.clone();
        glib::idle_add_local_once(clone!(
            #[weak]
            wrapper,
            move || {
                if owner.state.borrow().generation != generation
                    || !wrapper.inertial_scrolling()
                    || wrapper.canvas().touch_drawing()
                {
                    return;
                }
                let scroller = wrapper.scroller();
                let kinetic = scroller.is_kinetic_scrolling();
                scroller.set_kinetic_scrolling(false);
                scroller.set_kinetic_scrolling(kinetic);
                let Some(coast) = coast else {
                    return;
                };
                let coast = RefCell::new(coast);
                let last_time = Cell::new(glib::monotonic_time());
                let state = Rc::downgrade(&owner.state);
                let tick_slot = Rc::downgrade(&owner.tick);
                let tick = wrapper.add_tick_callback(move |wrapper, clock| {
                    let Some(state) = state.upgrade() else {
                        return glib::ControlFlow::Break;
                    };
                    let now = clock.frame_time();
                    let dt = (now - last_time.get()).max(0) as f64 / 1_000_000.0;
                    last_time.set(now);
                    let active = wrapper
                        .root()
                        .and_downcast::<gtk4::Window>()
                        .is_some_and(|w| w.is_active());
                    let delta = if state.borrow().generation == generation
                        && wrapper.is_mapped()
                        && active
                        && wrapper.inertial_scrolling()
                        && !wrapper.canvas().touch_drawing()
                        && dt < 0.25
                    {
                        coast.borrow_mut().step(dt)
                    } else {
                        None
                    };
                    let Some([x, y]) = delta else {
                        if let Some(slot) = tick_slot.upgrade() {
                            slot.borrow_mut().take();
                        }
                        return glib::ControlFlow::Break;
                    };
                    let canvas = wrapper.canvas();
                    let before = canvas.engine_ref().camera.offset();
                    let flags = canvas
                        .engine_mut()
                        .camera_set_offset_expand(before + Vector2::new(x, y));
                    let after = canvas.engine_ref().camera.offset();
                    // Clamp through the engine; stop only the blocked axis.
                    if x.abs() > 0.001 && (after.x - before.x - x).abs() > 0.5 {
                        coast.borrow_mut().stop_axis(0);
                    }
                    if y.abs() > 0.001 && (after.y - before.y - y).abs() > 0.5 {
                        coast.borrow_mut().stop_axis(1);
                    }
                    canvas.emit_handle_widget_flags(flags);
                    glib::ControlFlow::Continue
                });
                owner.tick.replace(Some(tick));
            }
        ));
    }
}
