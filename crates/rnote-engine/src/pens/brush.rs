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
                if element.pos.distance(hold.anchor) * zoom > 3.0 {
                    hold.anchor = element.pos;
                    hold.moved_at = now;
                    hold.attempted = false;
                    if let Some(original) = hold.original.take() {
                        if let Some(Stroke::BrushStroke(stroke)) =
                            engine_view.store.get_stroke_mut(*current_stroke_key)
                        {
                            stroke.replace_path(original);
                        }
                        engine_view.store.regenerate_rendering_for_stroke(
                            *current_stroke_key,
                            engine_view.camera.viewport(),
                            engine_view.camera.image_scale(),
                        );
                        widget_flags.redraw = true;
                    }
                }
                if hold.original.is_none()
                    && hold.points.len() <= 10000
                    && hold
                        .points
                        .last()
                        .is_none_or(|p| p.pos.distance(element.pos) * zoom > 0.5)
                {
                    hold.points.push(*element);
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

    pub(crate) fn recognize_hold(
        &mut self,
        key: StrokeKey,
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
            || hold.moved_at.elapsed() < Duration::from_millis(600)
        {
            return flags;
        }
        hold.attempted = true;
        let zoom = engine_view.camera.total_zoom();
        let points: Vec<_> = hold
            .points
            .iter()
            .map(|p| recognize::Point(p.pos.x * zoom, p.pos.y * zoom))
            .collect();
        let Some(clean) = recognize::recognize(&points) else {
            return flags;
        };
        let pressure =
            hold.points.iter().map(|p| p.pressure).sum::<f64>() / hold.points.len() as f64;
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
