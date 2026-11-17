//! Smooth scrolling: a scrollable whose mouse wheel glides to where it's going instead of jumping there.
//!
//! [`scrollable`] is iced's own scrollable with one change. A turn of the mouse wheel isn't passed on at once: the
//! distance is kept, and every frame a share of what's left is passed on (fast at first, easing off at the end), so a
//! notch glides over about a fifth of a second and quick turns add up. A touchpad already scrolls smoothly, so its
//! scrolling is passed straight on, gently: some send pixels; Windows' precision touchpads send small parts of a
//! notch, many a second, which are told apart from a wheel's whole notches. Everything else is iced's scrollable.
//!
//! Used just like iced's: `scrollable(content).height(Length::Fill)`.
//!
//! [`glide_to`] glides one (by its id) to a part of what's in it (a container's id) or back to where it was, the same
//! smooth way: a jump within a procedure that doesn't lose your place.

use std::any::Any;
use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::operation::{Operation, Outcome};
use iced::advanced::widget::{self, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse, overlay};
use iced::event::{self, Event};
use iced::widget::scrollable::Scrollable;
use iced::{Element, Length, Point, Rectangle, Renderer, Size, Task, Theme, Vector, window};

/// How far one notch of the wheel goes (iced on its own goes 60 pixels a line)
const LINE: f32 = 120.0;
/// How far a touchpad's notch goes (its small parts of one are passed on at once): gentler than a wheel's, as a
/// touchpad sends many of them
const PAD_LINE: f32 = 36.0;
/// How quickly what's left of a scroll is used up: about 63% of it every this many seconds
const EASE: f32 = 0.075;

/// A scrollable that glides (see the module's notes). Builder: `.height()`, `.width()`, `.direction()`, `.id()`,
/// `.on_scroll()`.
pub struct Smooth<'a, Message> {
    inner: Scrollable<'a, Message, Theme, Renderer>,
    id: Option<widget::Id>,
}

pub fn scrollable<'a, Message: 'a>(content: impl Into<Element<'a, Message>>) -> Smooth<'a, Message> {
    Smooth {
        inner: Scrollable::new(content),
        id: None,
    }
}

impl<'a, Message: 'a> Smooth<'a, Message> {
    pub fn height(mut self, h: impl Into<Length>) -> Self {
        self.inner = self.inner.height(h);
        self
    }
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.inner = self.inner.width(w);
        self
    }
    pub fn direction(mut self, d: impl Into<iced::widget::scrollable::Direction>) -> Self {
        self.inner = self.inner.direction(d);
        self
    }
    /// Says where it's scrolled to, as it scrolls (to keep its place)
    pub fn on_scroll(mut self, f: impl Fn(iced::widget::scrollable::Viewport) -> Message + 'a) -> Self {
        self.inner = self.inner.on_scroll(f);
        self
    }
    pub fn id(mut self, id: iced::widget::scrollable::Id) -> Self {
        self.id = Some(id.clone().into());
        self.inner = self.inner.id(id);
        self
    }
}

/// The widget itself: iced's scrollable inside, fed the wheel a little at a time
struct Glide<'a, Message> {
    inner: Element<'a, Message>,
    /// Its scrollable's id ([`glide_to`] finds it by it)
    id: Option<widget::Id>,
}

#[derive(Default)]
struct State {
    /// Scrolling still to pass on (pixels, in iced's wheel direction)
    left: Vector,
    /// Where the wheel was turned (the scrolling goes to what's under it)
    at: Option<Point>,
    /// The last frame (to know how much time passed)
    last: Option<Instant>,
}

impl State {
    fn gliding(&self) -> bool {
        self.left.x.abs() > 0.0 || self.left.y.abs() > 0.0
    }
}

impl<'a, Message: 'a> From<Smooth<'a, Message>> for Element<'a, Message> {
    fn from(s: Smooth<'a, Message>) -> Self {
        Element::new(Glide {
            inner: s.inner.into(),
            id: s.id,
        })
    }
}

impl<'a, Message> Widget<Message, Theme, Renderer> for Glide<'a, Message> {
    fn size(&self) -> Size<Length> {
        self.inner.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.inner.as_widget().size_hint()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.inner)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.inner));
    }

    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let child = self.inner.as_widget().layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(child.size(), vec![child])
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if let Some(child) = layout.children().next() {
            self.inner
                .as_widget()
                .draw(&tree.children[0], renderer, theme, style, child, cursor, viewport);
        }
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> event::Status {
        let Some(child) = layout.children().next() else {
            return event::Status::Ignored;
        };
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();

        match &event {
            // a notch of the mouse wheel over it: kept, to be passed on a little at a time
            // a touchpad's part of a notch (not a whole one): passed on at once, gently (it's smooth already)
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x, y },
            }) if cursor.is_over(bounds) && (x.fract().abs() > 1e-3 || y.fract().abs() > 1e-3) => {
                let pad = Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels {
                        x: x * PAD_LINE,
                        y: y * PAD_LINE,
                    },
                });
                return self.inner.as_widget_mut().on_event(
                    &mut tree.children[0],
                    pad,
                    child,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x, y },
            }) if cursor.is_over(bounds) => {
                if !state.gliding() {
                    state.last = Some(Instant::now());
                }
                state.left = state.left + Vector::new(x * LINE, y * LINE);
                state.at = cursor.position();
                shell.request_redraw(window::RedrawRequest::NextFrame);
                return event::Status::Captured;
            }
            // each frame: pass on a share of what's left
            Event::Window(window::Event::RedrawRequested(_)) if state.gliding() => {
                let now = Instant::now();
                let dt = state.last.map(|l| (now - l).as_secs_f32()).unwrap_or(1.0 / 60.0).clamp(0.0, 0.1);
                state.last = Some(now);
                let share = 1.0 - (-dt / EASE).exp();
                let mut step = state.left * share;
                // nearly there: the rest at once
                if (state.left.x - step.x).abs() < 0.5 && (state.left.y - step.y).abs() < 0.5 {
                    step = state.left;
                }
                state.left = state.left - step;
                let at = state.at.filter(|p| bounds.contains(*p)).unwrap_or(bounds.center());
                let wheel = Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels { x: step.x, y: step.y },
                });
                let gliding = state.gliding();
                let _ = self.inner.as_widget_mut().on_event(
                    &mut tree.children[0],
                    wheel,
                    child,
                    mouse::Cursor::Available(at),
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
                if gliding {
                    shell.request_redraw(window::RedrawRequest::NextFrame);
                }
            }
            _ => {}
        }
        self.inner
            .as_widget_mut()
            .on_event(&mut tree.children[0], event, child, cursor, renderer, clipboard, shell, viewport)
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        layout
            .children()
            .next()
            .map(|child| {
                self.inner
                    .as_widget()
                    .mouse_interaction(&tree.children[0], child, cursor, viewport, renderer)
            })
            .unwrap_or_default()
    }

    fn operate(&self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn widget::Operation) {
        // (its glide, for glide_to)
        operation.custom(tree.state.downcast_mut::<State>(), self.id.as_ref());
        if let Some(child) = layout.children().next() {
            self.inner.as_widget().operate(&mut tree.children[0], child, renderer, operation);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let child = layout.children().next()?;
        self.inner
            .as_widget_mut()
            .overlay(&mut tree.children[0], child, renderer, translation)
    }
}

/// Where [`glide_to`] goes: to a container (its id: the part's top, a little below the scrollable's top), or to a
/// scroll position (back to where it was)
#[derive(Debug, Clone)]
pub enum Target {
    Part(widget::Id),
    Offset(f32),
}

/// Glides the scrollable `scroll` (made with [`scrollable`] and its `.id()`) to `target`, `margin` pixels of space
/// above it. Gives where it was (to go back to), or nothing when the scrollable or the part isn't there.
pub fn glide_to(scroll: iced::widget::scrollable::Id, target: Target, margin: f32) -> Task<Option<f32>> {
    iced::advanced::widget::operate(Find {
        scroll: scroll.into(),
        target,
        margin,
        inside: false,
        at: None,
        found: None,
    })
}

/// First: where the scrollable is scrolled to and where the part is
struct Find {
    scroll: widget::Id,
    target: Target,
    margin: f32,
    /// Going through the scrollable's content
    inside: bool,
    /// The scrollable: how far it's scrolled, and where it is
    at: Option<(Vector, Rectangle)>,
    /// The part's top, from the top of what shows
    found: Option<f32>,
}

impl Operation<Option<f32>> for Find {
    fn scrollable(
        &mut self,
        _state: &mut dyn widget::operation::Scrollable,
        id: Option<&widget::Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: Vector,
    ) {
        if id == Some(&self.scroll) && self.at.is_none() {
            self.at = Some((translation, bounds));
        }
    }

    fn container(
        &mut self,
        id: Option<&widget::Id>,
        bounds: Rectangle,
        operate_on_children: &mut dyn FnMut(&mut dyn Operation<Option<f32>>),
    ) {
        if self.found.is_some() {
            return;
        }
        if id == Some(&self.scroll) && !self.inside && self.at.is_some() {
            self.inside = true;
            operate_on_children(self);
            self.inside = false;
            return;
        }
        if let (Target::Part(want), true, Some((translation, view))) = (&self.target, self.inside, self.at) {
            if id == Some(want) {
                // (what's in a scrollable is laid out as if it weren't scrolled)
                self.found = Some(bounds.y - translation.y - view.y);
                return;
            }
        }
        operate_on_children(self);
    }

    fn finish(&self) -> Outcome<Option<f32>> {
        let Some((translation, _)) = self.at else {
            return Outcome::Some(None);
        };
        let by = match (&self.target, self.found) {
            (Target::Part(_), Some(y)) => y - self.margin,
            (Target::Part(_), None) => return Outcome::Some(None),
            (Target::Offset(y), _) => y - translation.y,
        };
        Outcome::Chain(Box::new(Nudge {
            scroll: self.scroll.clone(),
            by,
            from: translation.y,
        }))
    }
}

/// Then: the scrollable's glide given the distance (it glides there over the next frames)
struct Nudge {
    scroll: widget::Id,
    by: f32,
    from: f32,
}

impl Operation<Option<f32>> for Nudge {
    fn container(
        &mut self,
        _id: Option<&widget::Id>,
        _bounds: Rectangle,
        operate_on_children: &mut dyn FnMut(&mut dyn Operation<Option<f32>>),
    ) {
        operate_on_children(self);
    }

    fn custom(&mut self, state: &mut dyn Any, id: Option<&widget::Id>) {
        if id != Some(&self.scroll) {
            return;
        }
        if let Some(s) = state.downcast_mut::<State>() {
            // (in the wheel's direction: up is positive)
            s.left = Vector::new(0.0, -self.by);
            s.at = None;
            s.last = Some(Instant::now());
        }
    }

    fn finish(&self) -> Outcome<Option<f32>> {
        Outcome::Some(Some(self.from))
    }
}
