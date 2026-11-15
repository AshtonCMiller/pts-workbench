//! Motion: the app's smooth, slightly bouncy feel (as in the Electron app).
//!
//! * [`tabs`]: a row of tabs whose blue rounded highlight slides to the selected tab, overshooting a touch and
//!   settling. The highlight is drawn here, behind the tab buttons (their own style only colors the text).
//! * [`page`]: a page that slides down a little while fading out ([`Phase::Leaving`]), then fades in while sliding up
//!   ([`Phase::Entering`]). The app keeps showing the old page while it leaves (see `App::outer` / `App::inner`).
//!
//! * [`side`]: a side panel (My ROs, Chat) that slides in and out of its edge, taking its space with it.
//! * [`modal`]: a card over everything (Settings, Feedback) on a frosted wash, sliding up in and down out.
//!
//! They run themselves: while moving they ask iced for the next frame; when still, they cost nothing.

use std::cell::Cell;
use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::{self, Tree, Widget, tree};
use iced::advanced::{Clipboard, Renderer as _, Shell, mouse, overlay};
use iced::event::{self, Event};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Rectangle, Renderer, Size, Theme, Vector, window};

use super::style;

/// How long the tab highlight takes to slide
pub const SLIDE: Duration = Duration::from_millis(340);
/// How long a page takes to leave (the app waits this long before showing the next one)
pub const LEAVE: Duration = Duration::from_millis(130);
/// How long a page takes to come in
pub const ENTER: Duration = Duration::from_millis(260);

/// 0 → 1 over `length` since `start`
fn progress(start: Instant, length: Duration) -> f32 {
    (start.elapsed().as_secs_f32() / length.as_secs_f32()).clamp(0.0, 1.0)
}

/// Ease out with a small overshoot (the "bounce"): goes a little past 1, then settles back
pub fn ease_out_back(t: f32) -> f32 {
    const C1: f32 = 1.2; // overshoot (1.70158 is the classic, livelier one)
    const C3: f32 = C1 + 1.0;
    let u = t - 1.0;
    1.0 + C3 * u * u * u + C1 * u * u
}

pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_in(t: f32) -> f32 {
    t * t
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_rect(a: Rectangle, b: Rectangle, t: f32) -> Rectangle {
    Rectangle {
        x: lerp(a.x, b.x, t),
        y: lerp(a.y, b.y, t),
        width: lerp(a.width, b.width, t).max(0.0),
        height: lerp(a.height, b.height, t).max(0.0),
    }
}

// =====================================================================================================================
// Tabs with a sliding highlight
// =====================================================================================================================

pub struct Tabs<'a, Message> {
    children: Vec<Element<'a, Message>>,
    active: usize,
    spacing: f32,
    radius: f32,
}

/// A row of tab buttons; `active`: which one is selected (its highlight slides there when it changes)
pub fn tabs<'a, Message>(active: usize, children: Vec<Element<'a, Message>>) -> Tabs<'a, Message> {
    Tabs {
        children,
        active,
        spacing: 4.0,
        radius: 6.0,
    }
}

impl<Message> Tabs<'_, Message> {
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }
}

/// Where the highlight was drawn last, and the slide in progress
struct TabsState {
    active: usize,
    /// Where the slide starts from (where the highlight was when the selection changed)
    from: Option<Rectangle>,
    start: Option<Instant>,
    /// Where the highlight was last drawn (set while drawing)
    last: Cell<Option<Rectangle>>,
}

impl<Message> Tabs<'_, Message> {
    fn sliding(state: &TabsState) -> bool {
        state.start.is_some_and(|s| s.elapsed() < SLIDE)
    }
}

impl<'a, Message> Widget<Message, Theme, Renderer> for Tabs<'a, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<TabsState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(TabsState {
            active: self.active,
            from: None,
            start: None,
            last: Cell::new(None),
        })
    }

    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
        let state = tree.state.downcast_mut::<TabsState>();
        if state.active != self.active {
            // slide from wherever the highlight is now (even mid-slide)
            state.from = state.last.get();
            state.start = Some(Instant::now());
            state.active = self.active;
        }
    }

    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::flex::resolve(
            layout::flex::Axis::Horizontal,
            renderer,
            limits,
            Length::Shrink,
            Length::Shrink,
            Padding::ZERO,
            self.spacing,
            Alignment::Center,
            &self.children,
            &mut tree.children,
        )
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
        let state = tree.state.downcast_ref::<TabsState>();
        if let Some(target) = layout.children().nth(self.active).map(|l| l.bounds()) {
            let rect = match (state.from, state.start) {
                (Some(from), Some(start)) => lerp_rect(from, target, ease_out_back(progress(start, SLIDE))),
                _ => target,
            };
            state.last.set(Some(rect));
            renderer.fill_quad(
                Quad {
                    bounds: rect,
                    border: Border {
                        radius: self.radius.into(),
                        ..Border::default()
                    },
                    ..Quad::default()
                },
                Background::Color(style::colors(theme).accent),
            );
        }
        for ((child, state), layout) in self.children.iter().zip(&tree.children).zip(layout.children()) {
            child.as_widget().draw(state, renderer, theme, style, layout, cursor, viewport);
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
        if Self::sliding(tree.state.downcast_ref::<TabsState>()) {
            shell.request_redraw(window::RedrawRequest::NextFrame);
        }
        self.children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget_mut()
                    .on_event(state, event.clone(), layout, cursor, renderer, clipboard, shell, viewport)
            })
            .fold(event::Status::Ignored, event::Status::merge)
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| child.as_widget().mouse_interaction(state, layout, cursor, viewport, renderer))
            .max()
            .unwrap_or_default()
    }

    fn operate(&self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn widget::Operation) {
        operation.container(None, layout.bounds(), &mut |operation| {
            for ((child, state), layout) in self.children.iter().zip(&mut tree.children).zip(layout.children()) {
                child.as_widget().operate(state, layout, renderer, operation);
            }
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        overlay::from_children(&mut self.children, tree, layout, renderer, translation)
    }
}

impl<'a, Message: 'a> From<Tabs<'a, Message>> for Element<'a, Message> {
    fn from(t: Tabs<'a, Message>) -> Self {
        Element::new(t)
    }
}

// =====================================================================================================================
// Pages that slide and fade
// =====================================================================================================================

/// Where a page is in its transition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Still,
    /// Sliding down and fading out (since)
    Leaving(Instant),
    /// Fading in and sliding up (since)
    Entering(Instant),
}

impl Phase {
    /// How far down the page is drawn, and how much of the window's background covers it (0 = none, 1 = all)
    fn look(self) -> (f32, f32) {
        match self {
            Phase::Still => (0.0, 0.0),
            Phase::Leaving(s) => {
                let t = ease_in(progress(s, LEAVE));
                (10.0 * t, t)
            }
            Phase::Entering(s) => {
                let t = ease_out(progress(s, ENTER));
                (14.0 * (1.0 - t), 1.0 - t)
            }
        }
    }

    fn moving(self) -> bool {
        match self {
            Phase::Still => false,
            Phase::Leaving(s) => s.elapsed() < LEAVE,
            Phase::Entering(s) => s.elapsed() < ENTER,
        }
    }

    pub fn is_leaving(self) -> bool {
        matches!(self, Phase::Leaving(_))
    }
}

pub struct Page<'a, Message> {
    content: Element<'a, Message>,
    phase: Phase,
    on_card: bool,
}

/// A page in its transition
pub fn page<'a, Message>(content: impl Into<Element<'a, Message>>, phase: Phase) -> Page<'a, Message> {
    Page {
        content: content.into(),
        phase,
        on_card: false,
    }
}

impl<Message> Page<'_, Message> {
    /// A page inside a card: it fades into the card's color rather than the window's
    pub fn on_card(mut self) -> Self {
        self.on_card = true;
        self
    }
}

impl<'a, Message> Widget<Message, Theme, Renderer> for Page<'a, Message> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let child = self.content.as_widget().layout(&mut tree.children[0], renderer, limits);
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
        let Some(child) = layout.children().next() else { return };
        let (down, cover) = self.phase.look();
        let bounds = layout.bounds();
        if down == 0.0 && cover == 0.0 {
            self.content
                .as_widget()
                .draw(&tree.children[0], renderer, theme, style, child, cursor, viewport);
            return;
        }
        renderer.with_layer(bounds, |r| {
            r.with_translation(Vector::new(0.0, down), |r| {
                self.content
                    .as_widget()
                    .draw(&tree.children[0], r, theme, style, child, cursor, viewport);
            });
            // fading: the background it sits on (the window's, or a card's) drawn over it, more or less see-through
            let bg = if self.on_card {
                style::colors(theme).panel
            } else {
                theme.palette().background
            };
            r.fill_quad(Quad { bounds, ..Quad::default() }, Background::Color(Color { a: cover, ..bg }));
        });
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
        if self.phase.moving() {
            shell.request_redraw(window::RedrawRequest::NextFrame);
        }
        let Some(child) = layout.children().next() else {
            return event::Status::Ignored;
        };
        self.content
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
                self.content
                    .as_widget()
                    .mouse_interaction(&tree.children[0], child, cursor, viewport, renderer)
            })
            .unwrap_or_default()
    }

    fn operate(&self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn widget::Operation) {
        if let Some(child) = layout.children().next() {
            self.content.as_widget().operate(&mut tree.children[0], child, renderer, operation);
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
        self.content
            .as_widget_mut()
            .overlay(&mut tree.children[0], child, renderer, translation)
    }
}

impl<'a, Message: 'a> From<Page<'a, Message>> for Element<'a, Message> {
    fn from(p: Page<'a, Message>) -> Self {
        Element::new(p)
    }
}

// =====================================================================================================================
// Side panels that slide in and out (My ROs on the left, Chat on the right)
// =====================================================================================================================

/// How long a side panel takes to slide in or out
pub const SIDE: Duration = Duration::from_millis(320);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
}

pub struct Side<'a, Message> {
    content: Element<'a, Message>,
    open: bool,
    edge: Edge,
    width: f32,
}

/// A panel `width` wide at the window's `edge`: slides out when `open` turns false, and back in (with the bounce)
/// when it turns true. Shown or not, the space it takes moves with it.
pub fn side<'a, Message>(content: impl Into<Element<'a, Message>>, open: bool, edge: Edge, width: f32) -> Side<'a, Message> {
    Side {
        content: content.into(),
        open,
        edge,
        width,
    }
}

struct SideState {
    open: bool,
    start: Option<Instant>,
}

impl SideState {
    /// How much of the panel shows: 0 (gone) to 1 (all of it; a little over while it bounces in)
    fn amount(&self) -> f32 {
        match self.start {
            Some(s) => {
                let t = progress(s, SIDE);
                if self.open { ease_out_back(t) } else { 1.0 - ease_out(t) }
            }
            None => {
                if self.open {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    fn moving(&self) -> bool {
        self.start.is_some_and(|s| s.elapsed() < SIDE)
    }
}

impl<'a, Message> Widget<Message, Theme, Renderer> for Side<'a, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Fill)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SideState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SideState {
            open: self.open,
            start: None,
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
        let state = tree.state.downcast_mut::<SideState>();
        if state.open != self.open {
            state.open = self.open;
            state.start = Some(Instant::now());
        }
    }

    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let shown = (self.width * tree.state.downcast_ref::<SideState>().amount()).max(0.0);
        let height = limits.max().height;
        // the panel is laid out whole, and slides: on the left it moves out to the left, on the right off to the right
        let child = self.content.as_widget().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::new(self.width, 0.0), Size::new(self.width, height)),
        );
        let x = match self.edge {
            Edge::Left => shown - self.width,
            Edge::Right => 0.0,
        };
        layout::Node::with_children(Size::new(shown, height), vec![child.move_to(iced::Point::new(x, 0.0))])
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
        let bounds = layout.bounds();
        let Some(child) = layout.children().next() else { return };
        if bounds.width < 0.5 {
            return;
        }
        renderer.with_layer(bounds, |r| {
            self.content
                .as_widget()
                .draw(&tree.children[0], r, theme, style, child, cursor, viewport);
        });
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
        if tree.state.downcast_ref::<SideState>().moving() {
            // its width changes every frame: lay the window out again, and draw the next frame
            shell.invalidate_layout();
            shell.request_redraw(window::RedrawRequest::NextFrame);
        }
        let Some(child) = layout.children().next() else {
            return event::Status::Ignored;
        };
        if layout.bounds().width < 0.5 {
            return event::Status::Ignored;
        }
        self.content
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
        match layout.children().next() {
            Some(child) if layout.bounds().width >= 0.5 => {
                self.content
                    .as_widget()
                    .mouse_interaction(&tree.children[0], child, cursor, viewport, renderer)
            }
            _ => mouse::Interaction::default(),
        }
    }

    fn operate(&self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn widget::Operation) {
        if let Some(child) = layout.children().next() {
            self.content.as_widget().operate(&mut tree.children[0], child, renderer, operation);
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
        self.content
            .as_widget_mut()
            .overlay(&mut tree.children[0], child, renderer, translation)
    }
}

impl<'a, Message: 'a> From<Side<'a, Message>> for Element<'a, Message> {
    fn from(s: Side<'a, Message>) -> Self {
        Element::new(s)
    }
}

// =====================================================================================================================
// Cards over everything (Settings, Feedback)
// =====================================================================================================================

/// How long a card takes to come in, and to go
pub const MODAL_IN: Duration = Duration::from_millis(320);
pub const MODAL_OUT: Duration = Duration::from_millis(180);
/// How much the frosted wash over the page hides it (0 none, 1 all)
const FROST: f32 = 0.62;

pub struct Modal<'a, Message> {
    card: Element<'a, Message>,
    phase: Phase,
    on_close: Message,
}

/// A card over the whole window: a frosted wash fades in over the page and the card slides up into the middle (with
/// the bounce); [`Phase::Leaving`] reverses it. Clicking outside the card, or Esc, sends `on_close`. While it's up,
/// nothing behind it can be clicked.
pub fn modal<'a, Message>(card: impl Into<Element<'a, Message>>, phase: Phase, on_close: Message) -> Modal<'a, Message> {
    Modal {
        card: card.into(),
        phase,
        on_close,
    }
}

impl Phase {
    /// The card's distance below its place, how much frost shows (0 to 1), and whether the card shows at all
    fn modal_look(self) -> (f32, f32, bool) {
        match self {
            Phase::Still => (0.0, 1.0, true),
            Phase::Entering(s) => {
                let t = progress(s, MODAL_IN);
                (28.0 * (1.0 - ease_out_back(t)), ease_out(t), true)
            }
            Phase::Leaving(s) => {
                let t = progress(s, MODAL_OUT);
                (18.0 * ease_in(t), 1.0 - t, t < 1.0)
            }
        }
    }

    fn modal_moving(self) -> bool {
        match self {
            Phase::Still => false,
            Phase::Entering(s) => s.elapsed() < MODAL_IN,
            Phase::Leaving(s) => s.elapsed() < MODAL_OUT,
        }
    }
}

impl<'a, Message: Clone> Widget<Message, Theme, Renderer> for Modal<'a, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.card)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.card));
    }

    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let full = limits.max();
        // the card gets the window less a margin, and sits in the middle
        let margin = 32.0;
        let room = Size::new((full.width - 2.0 * margin).max(0.0), (full.height - 2.0 * margin).max(0.0));
        let card = self
            .card
            .as_widget()
            .layout(&mut tree.children[0], renderer, &layout::Limits::new(Size::ZERO, room));
        let size = card.size();
        let at = iced::Point::new(
            ((full.width - size.width) / 2.0).max(0.0),
            ((full.height - size.height) / 2.0).max(0.0),
        );
        layout::Node::with_children(full, vec![card.move_to(at)])
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
        let (down, frost, card_shows) = self.phase.modal_look();
        let bounds = layout.bounds();
        renderer.with_layer(bounds, |r| {
            let bg = theme.palette().background;
            r.fill_quad(
                Quad { bounds, ..Quad::default() },
                Background::Color(Color {
                    a: FROST * frost.clamp(0.0, 1.0),
                    ..bg
                }),
            );
        });
        if let (true, Some(card)) = (card_shows, layout.children().next()) {
            renderer.with_layer(bounds, |r| {
                r.with_translation(Vector::new(0.0, down), |r| {
                    self.card
                        .as_widget()
                        .draw(&tree.children[0], r, theme, style, card, cursor, viewport);
                });
            });
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
        if self.phase.modal_moving() {
            shell.request_redraw(window::RedrawRequest::NextFrame);
        }
        let Some(card) = layout.children().next() else {
            return event::Status::Ignored;
        };
        let status = self.card.as_widget_mut().on_event(
            &mut tree.children[0],
            event.clone(),
            card,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if status == event::Status::Captured {
            return status;
        }
        match &event {
            // Esc closes it
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) => {
                shell.publish(self.on_close.clone());
                event::Status::Captured
            }
            // a click outside the card closes it
            Event::Mouse(mouse::Event::ButtonPressed(_)) if !cursor.is_over(card.bounds()) => {
                shell.publish(self.on_close.clone());
                event::Status::Captured
            }
            // nothing behind it gets the mouse
            Event::Mouse(_) | Event::Touch(_) => event::Status::Captured,
            _ => event::Status::Ignored,
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let over_card = layout
            .children()
            .next()
            .map(|card| {
                self.card
                    .as_widget()
                    .mouse_interaction(&tree.children[0], card, cursor, viewport, renderer)
            })
            .unwrap_or_default();
        // something other than None, so what's behind doesn't show its own pointer
        if over_card == mouse::Interaction::None {
            mouse::Interaction::Idle
        } else {
            over_card
        }
    }

    fn operate(&self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn widget::Operation) {
        if let Some(card) = layout.children().next() {
            self.card.as_widget().operate(&mut tree.children[0], card, renderer, operation);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let card = layout.children().next()?;
        self.card
            .as_widget_mut()
            .overlay(&mut tree.children[0], card, renderer, translation)
    }
}

impl<'a, Message: Clone + 'a> From<Modal<'a, Message>> for Element<'a, Message> {
    fn from(m: Modal<'a, Message>) -> Self {
        Element::new(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bounce_overshoots_then_settles() {
        assert_eq!(ease_out_back(0.0), 0.0);
        assert!((ease_out_back(1.0) - 1.0).abs() < 1e-6);
        let peak = (1..100).map(|i| ease_out_back(i as f32 / 100.0)).fold(0.0, f32::max);
        assert!(peak > 1.02 && peak < 1.15, "overshoot {peak}");
    }

    #[test]
    fn pages_end_where_they_belong() {
        assert_eq!(Phase::Still.look(), (0.0, 0.0));
        let long_ago = Instant::now() - Duration::from_secs(5);
        assert_eq!(Phase::Entering(long_ago).look(), (0.0, 0.0)); // fully in
        assert_eq!(Phase::Leaving(long_ago).look(), (10.0, 1.0)); // fully out: covered until the next page comes
        assert!(!Phase::Entering(long_ago).moving());
        assert_eq!(Phase::Entering(long_ago).modal_look(), (0.0, 1.0, true)); // a card fully in
        assert!(!Phase::Leaving(long_ago).modal_look().2); // a card gone
    }

    #[test]
    fn side_panels_end_open_or_shut() {
        let long_ago = Instant::now() - Duration::from_secs(5);
        assert!(
            (SideState {
                open: true,
                start: Some(long_ago)
            }
            .amount()
                - 1.0)
                .abs()
                < 1e-6
        );
        assert!(
            SideState {
                open: false,
                start: Some(long_ago)
            }
            .amount()
            .abs()
                < 1e-6
        );
        assert_eq!(SideState { open: false, start: None }.amount(), 0.0);
    }
}
