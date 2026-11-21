mod recognize;

// Imports
use super::PenBehaviour;
use super::PenStyle;
use super::pensconfig::brushconfig::BrushStyle;
use crate::engine::{EngineTask, EngineView, EngineViewMut};
use crate::store::StrokeKey;
use crate::strokes::BrushStroke;
use crate::strokes::Stroke;
use crate::tasks::{PeriodicTaskHandle, PeriodicTaskResult};
use crate::{DrawableOnDoc, WidgetFlags};
use p2d::bounding_volume::{Aabb, BoundingVolume};
use p2d::math::Vector2;
use piet::RenderContext;
use rnote_compose::Constraints;
use rnote_compose::Style;
use rnote_compose::builders::buildable::{Buildable, BuilderCreator, BuilderProgress};
use rnote_compose::builders::{
    PenPathBuilderType, PenPathCurvedBuilder, PenPathModeledBuilder, PenPathSimpleBuilder,
};
use rnote_compose::eventresult::{EventPropagation, EventResult};
use rnote_compose::penevent::{PenEvent, PenProgress};
use rnote_compose::penpath::{Element, PenPath, Segment};
use std::time::{Duration, Instant};

#[derive(Debug)]
enum BrushState {
    Idle,
    Drawing {
        path_builder: Box<dyn Buildable<Emit = Segment>>,
        current_stroke_key: StrokeKey,
        preview_style: Style,
    },
}

#[derive(Debug)]
pub struct Brush {
    state: BrushState,
    hold: Option<ShapeHold>,
    hold_task: Option<PeriodicTaskHandle>,
}

#[derive(Debug)]
struct ShapeHold {
    points: Vec<Element>,
    /// Samples through the last intentional movement. Stationary digitizer noise
    /// must not add loops or length to the outline sent to the recognizer.
    moving_points: usize,
    anchor: Vector2,
    moved_at: Instant,
    attempted: bool,
    original: Option<PenPath>,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            state: BrushState::Idle,
            hold: None,
            hold_task: None,
        }
    }
}

impl PenBehaviour for Brush {
    fn init(&mut self, _engine_view: &EngineView) -> WidgetFlags {
        WidgetFlags::default()
    }

    fn deinit(&mut self) -> WidgetFlags {
        self.hold_task = None;
        self.hold = None;
        WidgetFlags::default()
    }

    fn style(&self) -> PenStyle {
        PenStyle::Brush
    }

    fn update_state(&mut self, _engine_view: &mut EngineViewMut) -> WidgetFlags {
        WidgetFlags::default()
    }

    fn handle_event(
        &mut self,
        event: PenEvent,
        now: Instant,
        engine_view: &mut EngineViewMut,
    ) -> (EventResult<PenProgress>, WidgetFlags) {
        let mut widget_flags = WidgetFlags::default();

        // A release can arrive after the hold deadline but before the next timer tick.
        if let (PenEvent::Up { element, .. }, BrushState::Drawing { current_stroke_key, .. }, Some(hold)) =
            (&event, &self.state, &self.hold)
            && element.pos.distance(hold.anchor) * engine_view.camera.total_zoom() <= Self::RESUME_RADIUS
        {
            widget_flags |= self.recognize_hold_at(*current_stroke_key, now, engine_view);
        }

        // Preserve the builder while showing the recognized path. Moving again
        // restores the freehand stroke and continues it; releasing commits once.
        if let (
            BrushState::Drawing {
                current_stroke_key, ..
            },
            Some(hold),
        ) = (&self.state, self.hold.as_mut())
        {
            if let PenEvent::Down { element, .. } = &event {
                let zoom = engine_view.camera.total_zoom();
                let radius = if hold.original.is_some() { Self::RESUME_RADIUS } else { Self::HOLD_RADIUS };
                let moved = element.pos.distance(hold.anchor) * zoom > radius;
                if moved {
                    hold.anchor = element.pos;
                    hold.moved_at = now;
                    hold.attempted = false;
                    if let Some(original) = hold.original.take() {
                        let preview_style = Self::get_preview_style(&engine_view.as_im());
                        if let Some(Stroke::BrushStroke(stroke)) =
                            engine_view.store.get_stroke_mut(*current_stroke_key)
                        {
                            stroke.replace_path(original);
                            stroke.style = preview_style;
                        }
                        engine_view.store.update_geometry_for_stroke(*current_stroke_key);
                        engine_view.store.regenerate_rendering_for_stroke(
                            *current_stroke_key,
                            engine_view.camera.viewport(),
                            engine_view.camera.image_scale(),
                        );
                        widget_flags.redraw = true;
                    }
                }
                if hold.original.is_none()
                    && hold
                        .points
                        .last()
                        .is_none_or(|p| p.pos.distance(element.pos) * zoom > 0.5)
                {
                    if hold.points.len() == 8192 {
                        // Keep a bounded history even on high-rate tablets, retaining both
                        // the first sample and the end of the moving part of the trace.
                        let mut index = 0;
                        hold.points.retain(|_| {
                            index += 1;
                            index % 2 == 1
                        });
                        hold.moving_points = hold.moving_points.div_ceil(2);
                    }
                    hold.points.push(*element);
                }
                if moved {
                    hold.moving_points = hold.points.len();
                }
            }
            if hold.original.is_some() {
                if matches!(event, PenEvent::Up { .. } | PenEvent::Cancel) {
                    let key = *current_stroke_key;
                    engine_view.store.update_geometry_for_stroke(key);
                    engine_view.store.regenerate_rendering_for_stroke(
                        key,
                        engine_view.camera.viewport(),
                        engine_view.camera.image_scale(),
                    );
                    widget_flags |= engine_view
                        .document
                        .resize_autoexpand(engine_view.store, engine_view.camera);
                    widget_flags |= engine_view.store.record(now);
                    widget_flags.store_modified = true;
                    widget_flags.redraw = true;
                    self.state = BrushState::Idle;
                    self.hold = None;
                    self.hold_task = None;
                    return (
                        EventResult {
                            handled: true,
                            propagate: EventPropagation::Stop,
                            progress: PenProgress::Finished,
                        },
                        widget_flags,
                    );
                }
                return (
                    EventResult {
                        handled: true,
                        propagate: EventPropagation::Stop,
                        progress: PenProgress::InProgress,
                    },
                    widget_flags,
                );
            }
        }

        let event_result = match (&mut self.state, event) {
            (BrushState::Idle, PenEvent::Down { element, .. }) => {
                if !element.filter_by_bounds(
                    engine_view
                        .document
                        .bounds()
                        .loosened(Self::INPUT_OVERSHOOT),
                ) {
                    #[cfg(feature = "ui")]
                    {
                        if engine_view.config.pens_config.brush_config.style == BrushStyle::Marker {
                            play_marker_sound(engine_view);
                        } else {
                            trigger_brush_sound(engine_view);
                        }
                    }

                    engine_view
                        .config
                        .pens_config
                        .brush_config
                        .new_style_seeds();

                    let preview_style = Self::get_preview_style(&engine_view.as_im());
                    let brushstroke =
                        Stroke::BrushStroke(BrushStroke::new(element, preview_style.clone()));

                    let current_stroke_key = engine_view.store.insert_stroke(
                        brushstroke,
                        Some(
                            engine_view
                                .config
                                .pens_config
                                .brush_config
                                .layer_for_current_options(),
                        ),
                    );

                    engine_view.store.regenerate_rendering_for_stroke(
                        current_stroke_key,
                        engine_view.camera.viewport(),
                        engine_view.camera.image_scale(),
                    );

                    self.hold = Some(ShapeHold {
                        points: vec![element],
                        moving_points: 1,
                        anchor: element.pos,
                        moved_at: now,
                        attempted: false,
                        original: None,
                    });
                    let tx = engine_view.tasks_tx.clone();
                    self.hold_task = Some(PeriodicTaskHandle::new(
                        move || {
                            tx.send(EngineTask::RecognizeBrushHold(current_stroke_key));
                            PeriodicTaskResult::Continue
                        },
                        Duration::from_millis(100),
                    ));

                    self.state = BrushState::Drawing {
                        path_builder: new_builder(
                            engine_view.config.pens_config.brush_config.builder_type,
                            element,
                            now,
                        ),
                        current_stroke_key,
                        preview_style,
                    };

                    EventResult {
                        handled: true,
                        propagate: EventPropagation::Stop,
                        progress: PenProgress::InProgress,
                    }
                } else {
                    EventResult {
                        handled: false,
                        propagate: EventPropagation::Proceed,
                        progress: PenProgress::Idle,
                    }
                }
            }
            (BrushState::Idle, _) => EventResult {
                handled: false,
                propagate: EventPropagation::Proceed,
                progress: PenProgress::Idle,
            },
            (
                BrushState::Drawing {
                    current_stroke_key, ..
                },
                PenEvent::Cancel,
            ) => {
                if let Some(Stroke::BrushStroke(brushstroke)) =
                    engine_view.store.get_stroke_mut(*current_stroke_key)
                {
                    brushstroke.style = engine_view
                        .config
                        .pens_config
                        .brush_config
                        .style_for_current_options();
                }

                // Finish up the last stroke
                engine_view
                    .store
                    .update_geometry_for_stroke(*current_stroke_key);
                engine_view.store.regenerate_rendering_for_stroke_threaded(
                    engine_view.tasks_tx.clone(),
                    *current_stroke_key,
                    engine_view.camera.viewport(),
                    engine_view.camera.image_scale(),
                );
                widget_flags |= engine_view
                    .document
                    .resize_autoexpand(engine_view.store, engine_view.camera);

                self.state = BrushState::Idle;

                widget_flags |= engine_view.store.record(Instant::now());
                widget_flags.store_modified = true;

                EventResult {
                    handled: true,
                    propagate: EventPropagation::Stop,
                    progress: PenProgress::Finished,
                }
            }
            (
                BrushState::Drawing {
                    path_builder,
                    current_stroke_key,
                    ..
                },
                pen_event,
            ) => {
                let builder_result =
                    path_builder.handle_event(pen_event, now, Constraints::default());
                let handled = builder_result.handled;
                let propagate = builder_result.propagate;

                let progress = match builder_result.progress {
                    BuilderProgress::InProgress => {
                        #[cfg(feature = "ui")]
                        {
                            if engine_view.config.pens_config.brush_config.style
                                != BrushStyle::Marker
                            {
                                trigger_brush_sound(engine_view);
                            }
                        }

                        PenProgress::InProgress
                    }
                    BuilderProgress::EmitContinue(segments) => {
                        #[cfg(feature = "ui")]
                        {
                            if engine_view.config.pens_config.brush_config.style
                                != BrushStyle::Marker
                            {
                                trigger_brush_sound(engine_view);
                            }
                        }

                        let n_segments = segments.len();

                        if n_segments != 0 {
                            if let Some(Stroke::BrushStroke(brushstroke)) =
                                engine_view.store.get_stroke_mut(*current_stroke_key)
                            {
                                brushstroke.extend_w_segments(segments);
                                widget_flags.store_modified = true;
                            }

                            engine_view.store.append_rendering_last_segments(
                                engine_view.tasks_tx.clone(),
                                *current_stroke_key,
                                n_segments,
                                engine_view.camera.viewport(),
                                engine_view.camera.image_scale(),
                            );
                        }

                        PenProgress::InProgress
                    }
                    BuilderProgress::Finished(segments) => {
                        let n_segments = segments.len();

                        if n_segments != 0 {
                            if let Some(Stroke::BrushStroke(brushstroke)) =
                                engine_view.store.get_stroke_mut(*current_stroke_key)
                            {
                                brushstroke.extend_w_segments(segments);
                                widget_flags.store_modified = true;
                            }

                            engine_view.store.append_rendering_last_segments(
                                engine_view.tasks_tx.clone(),
                                *current_stroke_key,
                                n_segments,
                                engine_view.camera.viewport(),
                                engine_view.camera.image_scale(),
                            );
                        }

                        if let Some(Stroke::BrushStroke(brushstroke)) =
                            engine_view.store.get_stroke_mut(*current_stroke_key)
                        {
                            brushstroke.style = engine_view
                                .config
                                .pens_config
                                .brush_config
                                .style_for_current_options();
                        }

                        // Finish up the last stroke
                        engine_view
                            .store
                            .update_geometry_for_stroke(*current_stroke_key);
                        engine_view.store.regenerate_rendering_for_stroke_threaded(
                            engine_view.tasks_tx.clone(),
                            *current_stroke_key,
                            engine_view.camera.viewport(),
                            engine_view.camera.image_scale(),
                        );
                        widget_flags |= engine_view
                            .document
                            .resize_autoexpand(engine_view.store, engine_view.camera);

                        self.state = BrushState::Idle;

                        widget_flags |= engine_view.store.record(Instant::now());
                        widget_flags.store_modified = true;

                        PenProgress::Finished
                    }
                };

                EventResult {
                    handled,
                    propagate,
                    progress,
                }
            }
        };

        if matches!(self.state, BrushState::Idle) {
            self.hold = None;
            self.hold_task = None;
        }
        (event_result, widget_flags)
    }
}

impl DrawableOnDoc for Brush {
    fn bounds_on_doc(&self, engine_view: &EngineView) -> Option<Aabb> {
        let style = engine_view
            .config
            .pens_config
            .brush_config
            .style_for_current_options();

        match &self.state {
            BrushState::Idle => None,
            BrushState::Drawing { path_builder, .. } => {
                path_builder.bounds(&style, engine_view.camera.zoom())
            }
        }
    }

    fn draw_on_doc(
        &self,
        cx: &mut piet_cairo::CairoRenderContext,
        engine_view: &EngineView,
    ) -> anyhow::Result<()> {
        cx.save().map_err(|e| anyhow::anyhow!("{e:?}"))?;

        if self
            .hold
            .as_ref()
            .is_some_and(|hold| hold.original.is_some())
        {
            cx.restore().map_err(|e| anyhow::anyhow!("{e:?}"))?;
            return Ok(());
        }
        match &self.state {
            BrushState::Idle => {}
            BrushState::Drawing {
                path_builder,
                preview_style,
                ..
            } => {
                match engine_view.config.pens_config.brush_config.style {
                    BrushStyle::Marker => {
                        // Don't draw the marker, as the pen would render on top of other strokes, while the stroke itself would render underneath them.
                    }
                    BrushStyle::Solid | BrushStyle::Textured => {
                        path_builder.draw_styled(
                            cx,
                            preview_style,
                            engine_view.camera.total_zoom(),
                        );
                    }
                }
            }
        }

        cx.restore().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        Ok(())
    }
}

impl Brush {
    const INPUT_OVERSHOOT: f64 = 30.0;
    // Logical screen pixels, independent of canvas zoom.
    const HOLD_RADIUS: f64 = 6.0;
    const RESUME_RADIUS: f64 = 14.0;
    const HOLD_DELAY: Duration = Duration::from_millis(450);

    pub(crate) fn recognize_hold(
        &mut self,
        key: StrokeKey,
        engine_view: &mut EngineViewMut,
    ) -> WidgetFlags {
        self.recognize_hold_at(key, Instant::now(), engine_view)
    }

    fn recognize_hold_at(
        &mut self,
        key: StrokeKey,
        now: Instant,
        engine_view: &mut EngineViewMut,
    ) -> WidgetFlags {
        let mut flags = WidgetFlags::default();
        let BrushState::Drawing {
            current_stroke_key, ..
        } = &self.state
        else {
            return flags;
        };
        if *current_stroke_key != key {
            return flags;
        }
        let Some(hold) = self.hold.as_mut() else {
            return flags;
        };
        if hold.original.is_some()
            || hold.attempted
            || now.saturating_duration_since(hold.moved_at) < Self::HOLD_DELAY
        {
            return flags;
        }
        hold.attempted = true;
        let zoom = engine_view.camera.total_zoom();
        let moving = &hold.points[..hold.moving_points];
        let points: Vec<_> = moving
            .iter()
            .chain(hold.points.last())
            .map(|p| recognize::Point(p.pos.x * zoom, p.pos.y * zoom))
            .collect();
        let Some(clean) = recognize::recognize(&points) else {
            return flags;
        };
        let pressure =
            moving.iter().map(|p| p.pressure).sum::<f64>() / moving.len() as f64;
        let element =
            |p: recognize::Point| Element::new(Vector2::new(p.0 / zoom, p.1 / zoom), pressure);
        let path = PenPath {
            start: element(clean[0]),
            segments: clean[1..]
                .iter()
                .map(|p| Segment::LineTo { end: element(*p) })
                .collect(),
        };
        if let Some(Stroke::BrushStroke(stroke)) = engine_view.store.get_stroke_mut(key) {
            hold.original = Some(stroke.path.clone());
            stroke.replace_path(path);
            stroke.style = engine_view
                .config
                .pens_config
                .brush_config
                .style_for_current_options();
        }
        engine_view.store.update_geometry_for_stroke(key);
        engine_view.store.regenerate_rendering_for_stroke(
            key,
            engine_view.camera.viewport(),
            engine_view.camera.image_scale(),
        );
        flags.redraw = true;
        flags.store_modified = true;
        flags
    }

    fn get_preview_style(engine_view: &EngineView) -> Style {
        let mut style = engine_view
            .config
            .pens_config
            .brush_config
            .style_for_current_options();

        if let Some(mut stroke_color) = style.stroke_color() {
            stroke_color.a = 1.0;
            style.set_stroke_color(stroke_color);
        }

        style
    }
}

#[cfg(feature = "ui")]
fn play_marker_sound(engine_view: &mut EngineViewMut) {
    if let Some(audioplayer) = engine_view.audioplayer {
        audioplayer.play_random_marker_sound();
    }
}

#[cfg(feature = "ui")]
fn trigger_brush_sound(engine_view: &mut EngineViewMut) {
    if let Some(audioplayer) = engine_view.audioplayer.as_mut() {
        audioplayer.trigger_random_brush_sound();
    }
}

fn new_builder(
    builder_type: PenPathBuilderType,
    element: Element,
    now: Instant,
) -> Box<dyn Buildable<Emit = Segment>> {
    match builder_type {
        PenPathBuilderType::Simple => Box::new(PenPathSimpleBuilder::start(element, now)),
        PenPathBuilderType::Curved => Box::new(PenPathCurvedBuilder::start(element, now)),
        PenPathBuilderType::Modeled => Box::new(PenPathModeledBuilder::start(element, now)),
    }
}

#[cfg(test)]
mod held_shape_render_tests {
    use super::*;
    use crate::Image;
    use rnote_compose::style::{Composer, textured::TexturedOptions};

    fn contact(engine: &mut crate::Engine, pos: (f64,f64), release: bool, time: Instant) {
        let element = Element::new(Vector2::new(pos.0,pos.1), if release { 0.0 } else { 0.75 });
        let modifier_keys = Default::default();
        let event = if release {
            PenEvent::Up {element,modifier_keys}
        } else {
            PenEvent::Down {element,modifier_keys}
        };
        engine.handle_pen_event(event, None, time);
    }

    fn draw_rectangle(engine: &mut crate::Engine, start: Instant) -> StrokeKey {
        let corners = [(49.,50.), (146.,52.), (144.,120.), (48.,118.), (51.,52.)];
        for (i,p) in corners.into_iter().enumerate() {
            contact(engine,p,false,start+Duration::from_millis(i as u64*15));
        }
        engine.store.stroke_keys_unordered()[0]
    }

    fn is_closed_rectangle(engine: &crate::Engine, key: StrokeKey) -> bool {
        let Some(Stroke::BrushStroke(s)) = engine.store.get_stroke_ref(key) else { return false };
        let path = &s.path;
        path.segments.len() == 4 && path.segments.iter().all(|s| matches!(s, Segment::LineTo { .. }))
            && path.start.pos == path.segments[3].end().pos
    }

    #[test]
    fn preview_survives_pen_jitter_and_lift_and_undoes_as_one_stroke() {
        for style in [BrushStyle::Solid, BrushStyle::Marker, BrushStyle::Textured] {
            let mut engine = crate::Engine::default();
            engine.config.write().pens_config.brush_config.style = style;
            let start = Instant::now() - Duration::from_millis(1000);
            let key = draw_rectangle(&mut engine,start);
            for i in 0..90 {
                let phase = i as f64;
                contact(&mut engine,(51.+3.4*phase.sin(),52.+2.8*phase.cos()),false,
                    start+Duration::from_millis(100+i*9));
            }
            engine.handle_engine_task(EngineTask::RecognizeBrushHold(key));
            assert!(is_closed_rectangle(&engine,key), "preview for {style:?}");
            contact(&mut engine,(58.,55.),false,Instant::now());
            contact(&mut engine,(58.,55.),true,Instant::now());
            assert!(is_closed_rectangle(&engine,key), "after lift for {style:?}");
            assert_eq!(engine.store.stroke_keys_unordered().len(),1);
            engine.undo(Instant::now());
            assert!(engine.store.stroke_keys_unordered().is_empty());
            engine.redo(Instant::now());
            assert!(is_closed_rectangle(&engine,key));
        }
    }

    #[test]
    fn release_checks_the_deadline_even_without_a_timer_tick() {
        for (pause, expected) in [(150,false),(460,true)] {
            let mut engine = crate::Engine::default();
            let start = Instant::now();
            let key = draw_rectangle(&mut engine,start);
            contact(&mut engine,(52.,53.),true,start+Duration::from_millis(60+pause));
            assert_eq!(is_closed_rectangle(&engine,key),expected);
        }
    }

    #[test]
    fn moving_past_the_preview_restores_the_freehand_path() {
        let mut engine = crate::Engine::default();
        let key = draw_rectangle(&mut engine,Instant::now()-Duration::from_secs(1));
        engine.handle_engine_task(EngineTask::RecognizeBrushHold(key));
        assert!(is_closed_rectangle(&engine,key));
        contact(&mut engine,(86.,79.),false,Instant::now());
        contact(&mut engine,(100.,85.),true,Instant::now());
        assert!(!is_closed_rectangle(&engine,key));
        assert_eq!(engine.store.stroke_keys_unordered().len(),1);
    }

    #[test]
    fn recognized_rectangle_renders_all_four_sides() {
        use recognize::Point;
        // Feed a rectangle with rounded-off corners and a small closure gap,
        // then rasterize the exact path representation used when holding the pen.
        let corners = [
            Point(18., 18.),
            Point(104., 20.),
            Point(102., 82.),
            Point(17., 80.),
            Point(19., 21.),
        ];
        let trace: Vec<_> = corners
            .windows(2)
            .flat_map(|w| {
                (0..24).map(move |i| {
                    let f = i as f64 / 24.;
                    Point(
                        w[0].0 + f * (w[1].0 - w[0].0),
                        w[0].1 + f * (w[1].1 - w[0].1),
                    )
                })
            })
            .chain(corners.last().copied())
            .collect();
        let outline = recognize::recognize(&trace).unwrap();
        assert_eq!(outline.len(), 5);
        assert_eq!(outline.first(), outline.last());
        let element = |point: Point| Element::new(Vector2::new(point.0, point.1), 0.8);
        let path = PenPath::new_w_segments(
            element(outline[0]),
            outline[1..]
                .iter()
                .map(|&p| Segment::LineTo { end: element(p) }),
        );
        for style in [
            Style::default(),
            Style::Textured(TexturedOptions {
                seed: Some(1),
                ..Default::default()
            }),
        ] {
            let image = Image::gen_with_piet(
                |cx| {
                    path.draw_composed(cx, &style);
                    Ok(())
                },
                Aabb::new(Vector2::ZERO, Vector2::new(124., 104.)),
                1.,
            )
            .unwrap();
            let stride = image.pixel_width as usize;
            for edge in outline.windows(2) {
                let x = ((edge[0].0 + edge[1].0) * 0.5).round() as usize + 1;
                let y = ((edge[0].1 + edge[1].1) * 0.5).round() as usize + 1;
                assert!(
                    (y - 4..=y + 4).any(|row| (x - 4..=x + 4)
                        .any(|col| image.data[(row * stride + col) * 4 + 3] > 0)),
                    "Missing rectangle edge at ({x},{y}) with {style:?}"
                );
            }
        }
    }
}
