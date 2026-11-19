//! A wiring page drawn: Ford's drawing (made ready by `ford::wiring::prepare`) read with resvg, and shown on a sheet
//! you drag about and zoom into (the wheel, double-click, or the zoom buttons), smoothly.
//!
//! Drawing a whole page sharply at every zoom would take too long (and too much memory), so it's drawn twice:
//!   * once, the whole page at a moderate size ([`render_page`]): shown straight away at any zoom, while moving
//!   * the part on screen, sharp, at the zoom it's at ([`render_view`]): drawn in the background a moment after it
//!     stops moving, and shown over the first
//!
//! In the dark appearance, Ford's black-on-white is drawn light-on-dark; the wires keep their colors.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use iced::advanced::image::{Image, Renderer as _};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Renderer as _, Shell, mouse};
use iced::event::{self, Event};
use iced::widget::image::Handle;
use iced::{Background, Border, Color, Element, Length, Point, Rectangle, Renderer, Shadow, Size, Theme, Vector, window};
use resvg::{tiny_skia, usvg};

use ford::wiring::{Link, Prepared};

/// A drawing, read and ready: its size, and where its links and texts are on it
pub struct Drawing {
    tree: usvg::Tree,
    pub width: f32,
    pub height: f32,
    pub links: Vec<(Rectangle, Link)>,
    pub texts: Vec<(Rectangle, String)>,
}

impl std::fmt::Debug for Drawing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Drawing({}×{}, {} links)", self.width, self.height, self.links.len())
    }
}

/// The computer's fonts (looked up once: it takes a moment)
fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

fn rect(r: usvg::Rect) -> Rectangle {
    Rectangle {
        x: r.x(),
        y: r.y(),
        width: r.width(),
        height: r.height(),
    }
}

fn union(a: Rectangle, b: Rectangle) -> Rectangle {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    Rectangle {
        x,
        y,
        width: (a.x + a.width).max(b.x + b.width) - x,
        height: (a.y + a.height).max(b.y + b.height) - y,
    }
}

/// Reads a prepared drawing (slow-ish: on a background thread)
pub fn build(p: &Prepared) -> Result<Drawing, String> {
    let mut opt = usvg::Options::default();
    opt.fontdb = fonts();
    let tree = usvg::Tree::from_str(&p.svg, &opt).map_err(|e| format!("Couldn't read the drawing ({e})"))?;
    let size = tree.size();
    let found = |id: &str| tree.node_by_id(id).map(|n| rect(n.abs_bounding_box()));
    let texts: Vec<(Rectangle, String)> = p
        .texts
        .iter()
        .filter(|(_, t)| !t.is_empty())
        .filter_map(|(id, t)| Some((found(id)?, t.clone())))
        .collect();
    let links = p
        .links
        .iter()
        .filter_map(|(id, text_ids, link)| {
            // the link's texts (always there), else its group
            let from_texts = text_ids.iter().filter_map(|t| found(t)).reduce(union);
            let r = from_texts.or_else(|| found(id))?;
            Some((r, link.clone()))
        })
        .collect();
    Ok(Drawing {
        tree,
        width: size.width(),
        height: size.height(),
        links,
        texts,
    })
}

/// Where the sheet is: `x`, `y` the drawing's point at the top left, `z` how many pixels to its unit
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for View {
    fn default() -> Self {
        View { x: 0.0, y: 0.0, z: 1.0 }
    }
}

pub const MIN_ZOOM: f32 = 0.2;
pub const MAX_ZOOM: f32 = 12.0;

impl View {
    /// The whole page, centred in `area`
    pub fn fit(page: Size, area: Size) -> View {
        if page.width <= 0.0 || page.height <= 0.0 || area.width <= 0.0 || area.height <= 0.0 {
            return View::default();
        }
        let z = (area.width / page.width).min(area.height / page.height) * 0.97;
        View {
            x: -(area.width / z - page.width) / 2.0,
            y: -(area.height / z - page.height) / 2.0,
            z,
        }
    }

    /// Zoomed by `factor`, keeping the drawing's point under `at` (in the area) where it is
    pub fn zoomed(self, at: Point, factor: f32) -> View {
        let z = (self.z * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let (px, py) = (at.x / self.z + self.x, at.y / self.z + self.y);
        View {
            x: px - at.x / z,
            y: py - at.y / z,
            z,
        }
    }

    /// A box on the drawing in the middle of `area`, zoomed in to at least `min_z`
    pub fn centered(r: Rectangle, area: Size, min_z: f32) -> View {
        let fit = (area.width / (r.width * 3.0).max(1.0)).min(area.height / (r.height * 3.0).max(1.0));
        let z = fit.clamp(min_z, 4.0).clamp(MIN_ZOOM, MAX_ZOOM);
        let c = r.center();
        View {
            x: c.x - area.width / z / 2.0,
            y: c.y - area.height / z / 2.0,
            z,
        }
    }

    /// The drawing's point at `p` (in the area)
    pub fn point(self, p: Point) -> Point {
        Point::new(p.x / self.z + self.x, p.y / self.z + self.y)
    }

    /// A box on the drawing, on screen (in the area)
    pub fn on_screen(self, r: Rectangle) -> Rectangle {
        Rectangle {
            x: (r.x - self.x) * self.z,
            y: (r.y - self.y) * self.z,
            width: r.width * self.z,
            height: r.height * self.z,
        }
    }

    fn mix(a: View, b: View, t: f32) -> View {
        View {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
            z: a.z + (b.z - a.z) * t,
        }
    }

    fn near(self, o: View) -> bool {
        (self.x - o.x).abs() < 0.01 && (self.y - o.y).abs() < 0.01 && (self.z - o.z).abs() < 0.0001
    }
}

// ---------- drawing it ----------

/// Ford's black, white and gray drawn in the dark appearance's colors (its blue lighter); the wires keep theirs
fn darken(px: &mut [u8]) {
    const INK: [f32; 3] = [0xe6 as f32, 0xe8 as f32, 0xeb as f32];
    const PAPER: [f32; 3] = [0x1c as f32, 0x20 as f32, 0x26 as f32];
    for p in px.chunks_exact_mut(4) {
        let (r, g, b) = (p[0], p[1], p[2]);
        let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
        if hi - lo < 24 {
            // gray: white paper → dark, black ink → light
            let t = (u16::from(r) + u16::from(g) + u16::from(b)) as f32 / (3.0 * 255.0);
            for i in 0..3 {
                p[i] = (INK[i] + (PAPER[i] - INK[i]) * t).round() as u8;
            }
        } else if b > 180 && r < 90 && g < 90 {
            // Ford's blue (page numbers, connector names)
            p[0] = 0x6c;
            p[1] = 0xa8;
            p[2] = 0xff;
        }
    }
}

fn draw(d: &Drawing, view: View, w: u32, h: u32, dark: bool) -> Option<Handle> {
    let mut pm = tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    pm.fill(tiny_skia::Color::WHITE);
    let t = tiny_skia::Transform::from_row(view.z, 0.0, 0.0, view.z, -view.x * view.z, -view.y * view.z);
    resvg::render(&d.tree, t, &mut pm.as_mut());
    let mut px = pm.take();
    if dark {
        darken(&mut px);
    }
    Some(Handle::from_rgba(w.max(1), h.max(1), px))
}

/// The whole page, at most `longest` pixels along its longer side
pub fn render_page(d: &Drawing, longest: f32, dark: bool) -> Option<Handle> {
    let z = longest / d.width.max(d.height).max(1.0);
    draw(
        d,
        View { x: 0.0, y: 0.0, z },
        (d.width * z).ceil() as u32,
        (d.height * z).ceil() as u32,
        dark,
    )
}

/// What's on screen (`size`, in pixels), sharp
pub fn render_view(d: &Drawing, view: View, size: Size, dark: bool) -> Option<Handle> {
    draw(d, view, size.width.ceil() as u32, size.height.ceil() as u32, dark)
}

// ---------- the widget ----------

/// How long a zoom (or a jump to an item) takes
const GLIDE: Duration = Duration::from_millis(240);

/// The sheet: `base` the whole page (any size), `sharp` the part on screen drawn for a view and a size
pub struct Paper<'a, Message> {
    page: Size,
    view: View,
    base: Option<&'a Handle>,
    sharp: Option<(&'a Handle, View, Size)>,
    /// Boxes to mark on the drawing (found words, the item looked for), and whether it's the current one
    marks: &'a [(Rectangle, bool)],
    /// Where the links are (the pointer shows them)
    links: Vec<Rectangle>,
    dark: bool,
    on_view: Box<dyn Fn(View) -> Message + 'a>,
    on_click: Box<dyn Fn(Point) -> Message + 'a>,
    on_size: Box<dyn Fn(Size) -> Message + 'a>,
}

#[allow(clippy::too_many_arguments)]
pub fn paper<'a, Message>(
    page: Size,
    view: View,
    base: Option<&'a Handle>,
    sharp: Option<(&'a Handle, View, Size)>,
    marks: &'a [(Rectangle, bool)],
    links: Vec<Rectangle>,
    dark: bool,
    on_view: impl Fn(View) -> Message + 'a,
    on_click: impl Fn(Point) -> Message + 'a,
    on_size: impl Fn(Size) -> Message + 'a,
) -> Paper<'a, Message> {
    Paper {
        page,
        view,
        base,
        sharp,
        marks,
        links,
        dark,
        on_view: Box::new(on_view),
        on_click: Box::new(on_click),
        on_size: Box::new(on_size),
    }
}

#[derive(Default)]
struct State {
    /// Dragging: where it started (on screen), the view then, and whether it has moved yet
    drag: Option<(Point, View, bool)>,
    /// Gliding from one view to another (since)
    glide: Option<(View, View, Instant)>,
    /// The view last asked for
    target: Option<View>,
    size: Size,
    last_click: Option<(Instant, Point)>,
}

fn ease(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

impl State {
    /// The view showing now (part way through a glide)
    fn shown(&self, asked: View) -> View {
        match self.glide {
            Some((from, to, at)) => {
                let t = (at.elapsed().as_secs_f32() / GLIDE.as_secs_f32()).clamp(0.0, 1.0);
                View::mix(from, to, ease(t))
            }
            None => asked,
        }
    }
}

impl<Message> Widget<Message, Theme, Renderer> for Paper<'_, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn layout(&self, _tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> event::Status {
        let st = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        if st.size != bounds.size() {
            st.size = bounds.size();
            shell.publish((self.on_size)(bounds.size()));
        }
        // a new view asked for (zoom buttons, an item to show): glides there
        if st.target != Some(self.view) {
            let from = st.shown(st.target.unwrap_or(self.view));
            st.glide = (st.target.is_some() && st.drag.is_none() && !from.near(self.view)).then(|| (from, self.view, Instant::now()));
            st.target = Some(self.view);
        }
        if st.glide.is_some_and(|(_, _, at)| at.elapsed() >= GLIDE) {
            st.glide = None;
        }
        if st.glide.is_some() {
            shell.request_redraw(window::RedrawRequest::NextFrame);
        }
        let shown = st.shown(self.view);
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(p) = cursor.position_over(bounds) {
                    st.drag = Some((p, shown, false));
                    st.glide = None;
                    return event::Status::Captured;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some((start, v0, moved)) = st.drag.as_mut() {
                    let d = position - *start;
                    if !*moved && d.x.abs() + d.y.abs() > 4.0 {
                        *moved = true;
                    }
                    if *moved {
                        let v = View {
                            x: v0.x - d.x / v0.z,
                            y: v0.y - d.y / v0.z,
                            z: v0.z,
                        };
                        st.target = Some(v);
                        shell.publish((self.on_view)(v));
                    }
                    return event::Status::Captured;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some((start, v0, moved)) = st.drag.take() {
                    if !moved {
                        let local = Point::new(start.x - bounds.x, start.y - bounds.y);
                        let double = st
                            .last_click
                            .is_some_and(|(at, p)| at.elapsed() < Duration::from_millis(380) && p.distance(start) < 8.0);
                        if double {
                            st.last_click = None;
                            shell.publish((self.on_view)(v0.zoomed(local, 2.0)));
                        } else {
                            st.last_click = Some((Instant::now(), start));
                            shell.publish((self.on_click)(v0.point(local)));
                        }
                    }
                    return event::Status::Captured;
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(p) = cursor.position_over(bounds) {
                    let steps = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => y,
                        mouse::ScrollDelta::Pixels { y, .. } => y / 60.0,
                    };
                    if steps != 0.0 {
                        let local = Point::new(p.x - bounds.x, p.y - bounds.y);
                        // from where it's gliding to, so quick turns of the wheel add up
                        let from = st.target.unwrap_or(shown);
                        shell.publish((self.on_view)(from.zoomed(local, 1.25f32.powf(steps))));
                    }
                    return event::Status::Captured;
                }
            }
            _ => {}
        }
        event::Status::Ignored
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let st = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let v = st.shown(self.view);
        let accent = theme.extended_palette().primary.base.color;
        renderer.with_layer(bounds, |r| {
            let sheet = v.on_screen(Rectangle::new(Point::ORIGIN, self.page)) + Vector::new(bounds.x, bounds.y);
            // the sheet, with a soft shadow under it
            r.fill_quad(
                Quad {
                    bounds: sheet,
                    border: Border::default(),
                    shadow: Shadow {
                        color: Color { a: 0.28, ..Color::BLACK },
                        offset: Vector::new(0.0, 4.0),
                        blur_radius: 18.0,
                    },
                },
                Background::Color(if self.dark {
                    Color::from_rgb8(0x1c, 0x20, 0x26)
                } else {
                    Color::WHITE
                }),
            );
            if let Some(base) = self.base {
                r.draw_image(Image::new(base.clone()), sheet);
            }
            // the sharp part on screen, when it's for where the sheet is now
            if let Some((h, sv, size)) = self.sharp {
                if st.glide.is_none() && sv.near(v) && size == bounds.size() {
                    r.draw_image(Image::new(h.clone()), bounds);
                }
            }
            for (m, current) in self.marks {
                let b = v.on_screen(*m) + Vector::new(bounds.x, bounds.y);
                let pad = 3.0;
                r.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: b.x - pad,
                            y: b.y - pad,
                            width: b.width + pad * 2.0,
                            height: b.height + pad * 2.0,
                        },
                        border: Border {
                            color: if *current { accent } else { Color { a: 0.6, ..accent } },
                            width: if *current { 2.5 } else { 1.5 },
                            radius: 4.0.into(),
                        },
                        shadow: Shadow::default(),
                    },
                    Background::Color(Color {
                        a: if *current { 0.28 } else { 0.14 },
                        ..accent
                    }),
                );
            }
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let st = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        if st.drag.is_some_and(|(_, _, moved)| moved) {
            return mouse::Interaction::Grabbing;
        }
        let Some(p) = cursor.position_over(bounds) else {
            return mouse::Interaction::Idle;
        };
        let at = st.shown(self.view).point(Point::new(p.x - bounds.x, p.y - bounds.y));
        let near = |l: &Rectangle| at.x >= l.x - 2.0 && at.x <= l.x + l.width + 2.0 && at.y >= l.y - 2.0 && at.y <= l.y + l.height + 2.0;
        if self.links.iter().any(near) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::Grab
        }
    }
}

impl<'a, Message: 'a> From<Paper<'a, Message>> for Element<'a, Message> {
    fn from(p: Paper<'a, Message>) -> Self {
        Element::new(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zooms_around_the_pointer_and_fits() {
        let v = View { x: 10.0, y: 20.0, z: 2.0 };
        let at = Point::new(100.0, 50.0);
        let z = v.zoomed(at, 2.0);
        // the drawing's point under the pointer stays put
        assert_eq!(v.point(at), z.point(at));
        assert_eq!(z.z, 4.0);
        let f = View::fit(Size::new(1100.0, 850.0), Size::new(1200.0, 850.0));
        let sheet = f.on_screen(Rectangle::new(Point::ORIGIN, Size::new(1100.0, 850.0)));
        assert!((sheet.center().x - 600.0).abs() < 0.5 && (sheet.center().y - 425.0).abs() < 0.5);
        let c = View::centered(
            Rectangle::new(Point::new(500.0, 400.0), Size::new(20.0, 10.0)),
            Size::new(1000.0, 800.0),
            2.0,
        );
        let b = c.on_screen(Rectangle::new(Point::new(500.0, 400.0), Size::new(20.0, 10.0)));
        assert!((b.center().x - 500.0).abs() < 0.5 && (b.center().y - 400.0).abs() < 0.5);
    }

    #[test]
    fn dark_keeps_the_wires() {
        let mut px = vec![255, 255, 255, 255, 0, 0, 0, 255, 220, 30, 30, 255, 0, 0, 255, 255];
        darken(&mut px);
        assert_eq!(&px[0..3], &[0x1c, 0x20, 0x26]); // paper
        assert_eq!(&px[4..7], &[0xe6, 0xe8, 0xeb]); // ink
        assert_eq!(&px[8..11], &[220, 30, 30]); // a red wire
        assert_eq!(&px[12..15], &[0x6c, 0xa8, 0xff]); // Ford's blue
    }
}
