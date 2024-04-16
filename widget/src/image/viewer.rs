//! Zoom and pan on an image.
use crate::core::event::{self, Event};
use crate::core::image;
use crate::core::layout;
use crate::core::mouse;
use crate::core::renderer;
use crate::core::widget::tree::{self, Tree};
use crate::core::{
    Clipboard, Element, Layout, Length, Pixels, Point, Rectangle, Shell, Size,
    Vector, Widget,
};
use iced_renderer::core::ContentFit;
use iced_renderer::core::RotationLayout;

use std::hash::Hash;

/// A frame that displays an image with the ability to zoom in/out and pan.
#[allow(missing_debug_implementations)]
pub struct Viewer<Handle> {
    padding: f32,
    width: Length,
    height: Length,
    min_scale: f32,
    max_scale: f32,
    scale_step: f32,
    handle: Handle,
    filter_method: image::FilterMethod,
    content_fit: ContentFit,
    rotation: f32,
    rotation_layout: RotationLayout,
}

impl<Handle> Viewer<Handle> {
    /// Creates a new [`Viewer`] with the given [`State`].
    pub fn new(handle: Handle) -> Self {
        Viewer {
            handle,
            padding: 0.0,
            width: Length::Shrink,
            height: Length::Shrink,
            min_scale: 0.25,
            max_scale: 10.0,
            scale_step: 0.10,
            filter_method: image::FilterMethod::default(),
            content_fit: ContentFit::Contain,
            rotation: 0.0,
            rotation_layout: RotationLayout::Change,
        }
    }

    /// Rotates the [`Viewer`] by the given angle in radians.
    pub fn rotation(mut self, degrees: f32) -> Self {
        self.rotation = degrees;
        self
    }

    /// Sets the [`RotationLayout`] of the [`Viewer`].
    pub fn rotation_layout(mut self, rotation_layout: RotationLayout) -> Self {
        self.rotation_layout = rotation_layout;
        self
    }

    /// Sets the [`image::FilterMethod`] of the [`Viewer`].
    pub fn filter_method(mut self, filter_method: image::FilterMethod) -> Self {
        self.filter_method = filter_method;
        self
    }

    /// Sets the [`iced_renderer::core::ContentFit`] of the [`Viewer`].
    pub fn content_fit(mut self, content_fit: ContentFit) -> Self {
        self.content_fit = content_fit;
        self
    }

    /// Sets the padding of the [`Viewer`].
    pub fn padding(mut self, padding: impl Into<Pixels>) -> Self {
        self.padding = padding.into().0;
        self
    }

    /// Sets the width of the [`Viewer`].
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`Viewer`].
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the max scale applied to the image of the [`Viewer`].
    ///
    /// Default is `10.0`
    pub fn max_scale(mut self, max_scale: f32) -> Self {
        self.max_scale = max_scale;
        self
    }

    /// Sets the min scale applied to the image of the [`Viewer`].
    ///
    /// Default is `0.25`
    pub fn min_scale(mut self, min_scale: f32) -> Self {
        self.min_scale = min_scale;
        self
    }

    /// Sets the percentage the image of the [`Viewer`] will be scaled by
    /// when zoomed in / out.
    ///
    /// Default is `0.10`
    pub fn scale_step(mut self, scale_step: f32) -> Self {
        self.scale_step = scale_step;
        self
    }
}

impl<Message, Theme, Renderer, Handle> Widget<Message, Theme, Renderer>
    for Viewer<Handle>
where
    Renderer: image::Renderer<Handle = Handle>,
    Handle: Clone + Hash,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &self,
        _tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let image_size = {
            let Size { width, height } = renderer.measure_image(&self.handle);
            Size::new(width as f32, height as f32)
        };
        let rotated_size = self.rotation_layout.apply_to_size(image_size, self.rotation);
        let raw_size = limits.resolve(self.width, self.height, rotated_size);
        let full_size = self.content_fit.fit(rotated_size, raw_size);

        let final_size = Size {
            width: match self.width {
                Length::Shrink => f32::min(raw_size.width, full_size.width),
                _ => raw_size.width,
            },
            height: match self.height {
                Length::Shrink => f32::min(raw_size.height, full_size.height),
                _ => raw_size.height,
            },
        };

        layout::Node::new(final_size)
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> event::Status {
        let bounds = layout.bounds();

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(cursor_position) = cursor.position_over(bounds) else {
                    return event::Status::Ignored;
                };

                match delta {
                    mouse::ScrollDelta::Lines { y, .. }
                    | mouse::ScrollDelta::Pixels { y, .. } => {
                        let state = tree.state.downcast_mut::<State>();
                        let previous_scale = state.scale;

                        if y < 0.0 && previous_scale > self.min_scale
                            || y > 0.0 && previous_scale < self.max_scale
                        {
                            state.scale = (if y > 0.0 {
                                state.scale * (1.0 + self.scale_step)
                            } else {
                                state.scale / (1.0 + self.scale_step)
                            })
                            .clamp(self.min_scale, self.max_scale);

                            let image_size = image_size(
                                renderer,
                                &self.handle,
                                state,
                                bounds.size(),
                                self.content_fit,
                                self.rotation,
                                self.rotation_layout,
                            );

                            let factor = state.scale / previous_scale - 1.0;

                            let cursor_to_center =
                                cursor_position - bounds.center();

                            let adjustment = cursor_to_center * factor
                                + state.current_offset * factor;

                            state.current_offset = Vector::new(
                                if image_size.width > bounds.width {
                                    state.current_offset.x + adjustment.x
                                } else {
                                    0.0
                                },
                                if image_size.height > bounds.height {
                                    state.current_offset.y + adjustment.y
                                } else {
                                    0.0
                                },
                            );
                        }
                    }
                }

                event::Status::Captured
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(cursor_position) = cursor.position() else {
                    return event::Status::Ignored;
                };

                let state = tree.state.downcast_mut::<State>();

                state.cursor_grabbed_at = Some(cursor_position);
                state.starting_offset = state.current_offset;

                event::Status::Captured
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let state = tree.state.downcast_mut::<State>();

                if state.cursor_grabbed_at.is_some() {
                    state.cursor_grabbed_at = None;

                    event::Status::Captured
                } else {
                    event::Status::Ignored
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let state = tree.state.downcast_mut::<State>();

                if let Some(origin) = state.cursor_grabbed_at {
                    let image_size = image_size(
                        renderer,
                        &self.handle,
                        state,
                        bounds.size(),
                        self.content_fit,
                        self.rotation,
                        self.rotation_layout,
                    );
                    let hidden_width = (image_size.width - bounds.width / 2.0)
                        .max(0.0)
                        .round();

                    let hidden_height = (image_size.height
                        - bounds.height / 2.0)
                        .max(0.0)
                        .round();

                    let delta = position - origin;

                    let x = if bounds.width < image_size.width {
                        (state.starting_offset.x - delta.x)
                            .clamp(-hidden_width, hidden_width)
                    } else {
                        0.0
                    };

                    let y = if bounds.height < image_size.height {
                        (state.starting_offset.y - delta.y)
                            .clamp(-hidden_height, hidden_height)
                    } else {
                        0.0
                    };

                    state.current_offset = Vector::new(x, y);

                    event::Status::Captured
                } else {
                    event::Status::Ignored
                }
            }
            _ => event::Status::Ignored,
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let is_mouse_over = cursor.is_over(bounds);

        if state.is_cursor_grabbed() {
            mouse::Interaction::Grabbing
        } else if is_mouse_over {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::Idle
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let img_size = renderer.measure_image(&self.handle);
        let img_size = Size::new(img_size.width as f32, img_size.height as f32);
        let rotated_size = self.rotation_layout.apply_to_size(img_size, self.rotation);
    
        let bounds = layout.bounds();

        let adjusted_fit = image_size(
            renderer,
            &self.handle,
            state,
            bounds.size(),
            self.content_fit,
            self.rotation,
            self.rotation_layout,
        );
        let scale = Size::new(
            adjusted_fit.width / rotated_size.width,
            adjusted_fit.height / rotated_size.height,
        );

        let translation = {
            let image_top_left = Vector::new(
                (bounds.width - img_size.width).max(0.0) / 2.0,
                (bounds.height - img_size.height).max(0.0) / 2.0,
            );

            image_top_left - state.offset(bounds, img_size)
        };
        let drawing_bounds = match self.content_fit {
            // TODO: `none` window resizing doesn't work as it should
            ContentFit::None => Rectangle {
                width: img_size.width,
                height: img_size.height,
                // ..bounds
                x: bounds.position().x + (rotated_size.width - img_size.width) / 2.0,
                y: bounds.position().y + (rotated_size.height - img_size.height) / 2.0,
            },
            _ => Rectangle {
                width: img_size.width,
                height: img_size.height,
                x: bounds.center_x() - img_size.width / 2.0,
                y: bounds.center_y() - img_size.height / 2.0,
            }
        };
    
        let render = |renderer: &mut Renderer| {
            renderer.with_translation(translation, |renderer| {
                renderer.draw_image(
                    self.handle.clone(),
                    self.filter_method,
                    drawing_bounds,
                    self.rotation,
                    scale,
                );
            });
        };
        
        renderer.with_layer(bounds, render);
    }
}

/// The local state of a [`Viewer`].
#[derive(Debug, Clone, Copy)]
pub struct State {
    scale: f32,
    starting_offset: Vector,
    current_offset: Vector,
    cursor_grabbed_at: Option<Point>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            scale: 1.0,
            starting_offset: Vector::default(),
            current_offset: Vector::default(),
            cursor_grabbed_at: None,
        }
    }
}

impl State {
    /// Creates a new [`State`].
    pub fn new() -> Self {
        State::default()
    }

    /// Returns the current offset of the [`State`], given the bounds
    /// of the [`Viewer`] and its image.
    fn offset(&self, bounds: Rectangle, image_size: Size) -> Vector {
        let hidden_width =
            (image_size.width - bounds.width / 2.0).max(0.0).round();

        let hidden_height =
            (image_size.height - bounds.height / 2.0).max(0.0).round();

        Vector::new(
            self.current_offset.x.clamp(-hidden_width, hidden_width),
            self.current_offset.y.clamp(-hidden_height, hidden_height),
        )
    }

    /// Returns if the cursor is currently grabbed by the [`Viewer`].
    pub fn is_cursor_grabbed(&self) -> bool {
        self.cursor_grabbed_at.is_some()
    }
}

impl<'a, Message, Theme, Renderer, Handle> From<Viewer<Handle>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: 'a + image::Renderer<Handle = Handle>,
    Message: 'a,
    Handle: Clone + Hash + 'a,
{
    fn from(viewer: Viewer<Handle>) -> Element<'a, Message, Theme, Renderer> {
        Element::new(viewer)
    }
}

/// Returns the bounds of the underlying image, given the bounds of
/// the [`Viewer`]. Scaling will be applied and original aspect ratio
/// will be respected.
pub fn image_size<Renderer>(
    renderer: &Renderer,
    handle: &<Renderer as image::Renderer>::Handle,
    state: &State,
    bounds: Size,
    content_fit: ContentFit,
    rotation: f32,
    rotation_layout: RotationLayout,
) -> Size
where
    Renderer: image::Renderer,
{
    let size = renderer.measure_image(handle);
    let size = Size::new(size.width as f32, size.height as f32);
    let rotated_size = rotation_layout.apply_to_size(size, rotation);
    let size = content_fit.fit(rotated_size, bounds);

    Size::new(size.width * state.scale, size.height * state.scale)
}
