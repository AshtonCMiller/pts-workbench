//! A menu that drops down under something while the mouse is over it (the Service Manual's Diagnostics tab: its
//! Diagnostics Home, Vehicle History and Vehicle Software). It unrolls with the app's little bounce, stays while the
//! mouse moves from the anchor onto it, and goes a moment after the mouse leaves both, or once something in it (or
//! the anchor itself) is clicked.
//!
//! `on_open` hears when it opens and closes: the app hides a web page that would sit on top of it (crates/webpane).
//! Until the app says it's [`HoverMenu::ready`] (the page is out of the way), the menu waits, then unrolls.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget, tree};
use iced::advanced::{Clipboard, Renderer as _, Shell, mouse, overlay};
use iced::event::{self, Event};
use iced::{Element, Length, Point, Rectangle, Renderer, Size, Theme, Vector, window};

use crate::anim::{ease_out, ease_out_back};

/// How long it takes to unroll
const OPEN: Duration = Duration::from_millis(240);
/// How long it waits after the mouse leaves before closing (time to cross onto it)
const LINGER: Duration = Duration::from_millis(160);

pub struct HoverMenu<'a, Message> {
    anchor: Element<'a, Message>,
    menu: Element<'a, Message>,
    on_open: Option<Box<dyn Fn(bool) -> Message + 'a>>,
    ready: bool,
}

/// `anchor` with `menu` dropping down under it while the mouse is over either
pub fn hover_menu<'a, Message>(anchor: impl Into<Element<'a, Message>>, menu: impl Into<Element<'a, Message>>) -> HoverMenu<'a, Message> {
    HoverMenu {
        anchor: anchor.into(),
        menu: menu.into(),
        on_open: None,
        ready: true,
    }
}

impl<'a, Message> HoverMenu<'a, Message> {
    /// Hear when it opens (`true`) and closes
    pub fn on_open(mut self, f: impl Fn(bool) -> Message + 'a) -> Self {
        self.on_open = Some(Box::new(f));
        self
    }

    /// Whether it can show yet (`false`: a web page that would cover it is still being moved out of the way; it
    /// shows, and unrolls, once this is `true`)
    pub fn ready(mut self, ready: bool) -> Self {
        self.ready = ready;
        self
    }
}

#[derive(Default)]
struct State {
    over_anchor: bool,
    over_menu: bool,
    open: bool,
    /// When it opened (for unrolling)
    since: Option<Instant>,
    /// It's been drawn since it opened (it unrolls from then)
    drawn: bool,
    /// When the mouse left both (it closes a moment later)
    left: Option<Instant>,
    /// Clicked: stays closed until the mouse leaves the anchor
    shut: bool,
}

impl State {
    fn set_open<Message>(&mut self, open: bool, on_open: Option<&dyn Fn(bool) -> Message>, shell: &mut Shell<'_, Message>) {
        if self.open == open {
            return;
        }
        self.open = open;
        self.left = None;
        self.drawn = false;
        if open {
            self.since = Some(Instant::now());
            shell.request_redraw(window::RedrawRequest::NextFrame);
        } else {
            self.over_menu = false;
        }
        if let Some(f) = on_open {
            shell.publish(f(open));
        }
    }

    /// After the mouse moved: open while over either, else close a moment later
    fn settle<Message>(&mut self, on_open: Option<&dyn Fn(bool) -> Message>, shell: &mut Shell<'_, Message>) {
        if !self.over_anchor {
            self.shut = false;
        }
        if (self.over_anchor && !self.shut) || (self.open && self.over_menu) {
            self.left = None;
            self.set_open(true, on_open, shell);
        } else if self.open && self.left.is_none() {
            self.left = Some(Instant::now());
            shell.request_redraw(window::RedrawRequest::NextFrame);
        }
    }

    /// Time's up since the mouse left
    fn tick<Message>(&mut self, on_open: Option<&dyn Fn(bool) -> Message>, shell: &mut Shell<'_, Message>) {
        match self.left {
            Some(t) if t.elapsed() >= LINGER => self.set_open(false, on_open, shell),
            Some(_) => shell.request_redraw(window::RedrawRequest::NextFrame),
            None => {}
        }
        if self.since.is_some_and(|s| s.elapsed() < OPEN) && self.open {
            shell.request_redraw(window::RedrawRequest::NextFrame);
        }
    }
}

impl<Message> Widget<Message, Theme, Renderer> for HoverMenu<'_, Message> {
    fn size(&self) -> Size<Length> {
        self.anchor.as_widget().size()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.anchor), Tree::new(&self.menu)]
    }

    fn diff(&self, tree: &mut Tree) {
        if tree.children.len() == 2 {
            tree.children[0].diff(self.anchor.as_widget());
            tree.children[1].diff(self.menu.as_widget());
        } else {
            tree.children = self.children();
        }
    }

    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let child = self.anchor.as_widget().layout(&mut tree.children[0], renderer, limits);
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
            self.anchor
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
        let on_open = self.on_open.as_deref();
        let state = tree.state.downcast_mut::<State>();
        match &event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                state.over_anchor = cursor.is_over(layout.bounds());
                state.settle(on_open, shell);
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                state.over_anchor = false;
                state.over_menu = false;
                state.settle(on_open, shell);
            }
            // the anchor clicked: it does its own thing, and the menu goes
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if cursor.is_over(layout.bounds()) => {
                state.shut = true;
                state.set_open(false, on_open, shell);
            }
            Event::Window(window::Event::RedrawRequested(_)) => state.tick(on_open, shell),
            _ => {}
        }
        let Some(child) = layout.children().next() else {
            return event::Status::Ignored;
        };
        self.anchor
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
                self.anchor
                    .as_widget()
                    .mouse_interaction(&tree.children[0], child, cursor, viewport, renderer)
            })
            .unwrap_or_default()
    }

    fn operate(&self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn widget::Operation) {
        if let Some(child) = layout.children().next() {
            self.anchor.as_widget().operate(&mut tree.children[0], child, renderer, operation);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let Tree { state, children, .. } = tree;
        let state = state.downcast_mut::<State>();
        if !state.open || !self.ready {
            return None;
        }
        // (it unrolls from when it can first be seen)
        if !state.drawn {
            state.drawn = true;
            state.since = Some(Instant::now());
        }
        let b = layout.bounds();
        let anchor = Rectangle {
            x: b.x + translation.x,
            y: b.y + translation.y,
            ..b
        };
        Some(overlay::Element::new(Box::new(MenuOverlay {
            menu: &mut self.menu,
            tree: &mut children[1],
            state,
            anchor,
            on_open: self.on_open.as_deref(),
        })))
    }
}

impl<'a, Message: 'a> From<HoverMenu<'a, Message>> for Element<'a, Message> {
    fn from(m: HoverMenu<'a, Message>) -> Self {
        Element::new(m)
    }
}

struct MenuOverlay<'a, 'b, Message> {
    menu: &'b mut Element<'a, Message>,
    tree: &'b mut Tree,
    state: &'b mut State,
    /// The anchor, where it is in the window
    anchor: Rectangle,
    on_open: Option<&'b (dyn Fn(bool) -> Message + 'a)>,
}

impl<Message> overlay::Overlay<Message, Theme, Renderer> for MenuOverlay<'_, '_, Message> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let top = self.anchor.y + self.anchor.height;
        let limits = layout::Limits::new(Size::ZERO, Size::new(bounds.width, (bounds.height - top).max(0.0)));
        let node = self.menu.as_widget().layout(self.tree, renderer, &limits);
        // under the anchor's left edge, kept inside the window
        let x = self.anchor.x.min((bounds.width - node.size().width).max(0.0));
        node.move_to(Point::new(x, top))
    }

    fn draw(&self, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor) {
        let bounds = layout.bounds();
        let t = self
            .state
            .since
            .map(|s| (s.elapsed().as_secs_f32() / OPEN.as_secs_f32()).clamp(0.0, 1.0))
            .unwrap_or(1.0);
        // unrolling: the clip grows downwards while the menu settles into place (a touch past, then back)
        let clip = Rectangle {
            height: bounds.height * ease_out(t) + 12.0,
            x: bounds.x - 24.0,
            width: bounds.width + 48.0,
            ..bounds
        };
        let drop = (1.0 - ease_out_back(t)) * -8.0;
        renderer.with_layer(clip, |r| {
            r.with_translation(Vector::new(0.0, drop), |r| {
                self.menu.as_widget().draw(&*self.tree, r, theme, style, layout, cursor, &bounds);
            });
        });
    }

    fn on_event(
        &mut self,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) -> event::Status {
        let bounds = layout.bounds();
        let over = cursor.is_over(bounds);
        let clicked = matches!(event, Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))) && over;
        match &event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                self.state.over_menu = over;
                // (the anchor is told about the mouse too: when it's over the menu, the anchor hears it's nowhere)
                self.state.over_anchor = cursor.is_over(self.anchor);
                self.state.settle(self.on_open, shell);
            }
            Event::Window(window::Event::RedrawRequested(_)) => self.state.tick(self.on_open, shell),
            _ => {}
        }
        let status = self
            .menu
            .as_widget_mut()
            .on_event(self.tree, event, layout, cursor, renderer, clipboard, shell, &bounds);
        // something in it picked: it goes
        if clicked {
            self.state.shut = self.state.over_anchor;
            self.state.set_open(false, self.on_open, shell);
        }
        status
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.menu
            .as_widget()
            .mouse_interaction(&*self.tree, layout, cursor, viewport, renderer)
    }

    fn is_over(&self, layout: Layout<'_>, _renderer: &Renderer, cursor_position: Point) -> bool {
        layout.bounds().contains(cursor_position)
    }
}
