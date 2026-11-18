//! Websites inside the app's window: Xtime's own dashboard and RO page, and PTS, for whoever still works in them.
//!
//! iced can't draw a web page, so each page is a real web view (WebView2 on Windows, through wry) placed over the
//! window, where the app leaves room for it: [`hole`] is that room. It says where it is whenever it moves or changes
//! size; the app then calls [`show`] with that place, and every other page is hidden. Pages stay open while hidden
//! (going back to one finds it as you left it) until [`close`].
//!
//! A web view sits on top of everything iced draws, so the app hides them all ([`hide_all`]) while one of its own
//! cards (Settings…) is open.
//!
//! Each page opens signed in: the app's sign-in cookies are handed to it first. If a site asks to sign in anyway,
//! its saved login is filled in (the sign-in window's saved logins: crates/signin/src/logins.rs), and a new one is
//! saved once the sign-in works. Their own profile folder is the app's (`pages`), not the sign-in window's.
//!
//! Windows only: elsewhere (developing on Linux) the room says the pages show on Windows.

use std::path::PathBuf;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse, overlay};
use iced::event::{self, Event};
use iced::{Element, Length, Rectangle, Renderer, Size, Task, Theme, Vector, window};
use workbench_net::SiteCookie;

#[cfg(windows)]
#[path = "../../signin/src/logins.rs"]
mod logins;

#[cfg(windows)]
mod win;

/// A page to show: which one (the app's name for it: "xtime-ro:820400"), its address, and the sign-ins to hand it
/// when it opens
#[derive(Debug, Clone)]
pub struct Page {
    pub id: String,
    pub url: String,
    pub cookies: Vec<SiteCookie>,
    /// The web views' own profile folder
    pub data_dir: PathBuf,
}

/// Shows the page in this place (opened the first time, else moved there); every other page is hidden. Its error
/// (couldn't open a web view here) comes back.
pub fn show(page: Page, at: Rectangle) -> Task<Result<(), String>> {
    // (on the window's own thread, with its handle: web views are made and moved there)
    window::get_latest().and_then(move |id| {
        let page = page.clone();
        window::run_with_handle(id, move |_handle| {
            #[cfg(windows)]
            {
                win::show(&_handle, &page, at)
            }
            #[cfg(not(windows))]
            {
                let _ = (&page, at);
                Err("Xtime's and PTS's own pages show inside the app on Windows.".to_string())
            }
        })
    })
}

/// Hides every page (a card of the app's is over them, or no page is shown)
pub fn hide_all() -> Task<()> {
    window::get_latest().and_then(|id| {
        window::run_with_handle(id, |_| {
            #[cfg(windows)]
            win::hide_all();
        })
    })
}

/// Closes these pages (their RO closed)
pub fn close(ids: Vec<String>) -> Task<()> {
    window::get_latest().and_then(move |id| {
        let ids = ids.clone();
        window::run_with_handle(id, move |_| {
            #[cfg(windows)]
            win::close(&ids);
            #[cfg(not(windows))]
            let _ = ids;
        })
    })
}

/// Reloads a page (it's gone wrong, or to see it again from the start)
pub fn reload(page: String) -> Task<()> {
    window::get_latest().and_then(move |id| {
        let page = page.clone();
        window::run_with_handle(id, move |_| {
            #[cfg(windows)]
            win::reload(&page);
            #[cfg(not(windows))]
            let _ = page;
        })
    })
}

// ---------- the room left for a page ----------

/// Where a page goes: fills the space it's given, shows `content` until the page is over it (or when it can't be),
/// and says where it is (`on_place`) when that changes
pub fn hole<'a, Message: 'a>(
    id: impl Into<String>,
    content: impl Into<Element<'a, Message>>,
    on_place: impl Fn(String, Rectangle) -> Message + 'a,
) -> Element<'a, Message> {
    Element::new(Hole {
        id: id.into(),
        content: content.into(),
        on_place: Box::new(on_place),
    })
}

struct Hole<'a, Message> {
    id: String,
    content: Element<'a, Message>,
    on_place: Box<dyn Fn(String, Rectangle) -> Message + 'a>,
}

/// Where it said it was last (and for which page)
#[derive(Default)]
struct Said(Option<(String, Rectangle)>);

impl<Message> Widget<Message, Theme, Renderer> for Hole<'_, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Said>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Said::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let size = limits.width(Length::Fill).height(Length::Fill).max();
        let child = self
            .content
            .as_widget()
            .layout(&mut tree.children[0], renderer, &layout::Limits::new(Size::ZERO, size));
        layout::Node::with_children(size, vec![child])
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
            self.content
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
        if let Event::Window(window::Event::RedrawRequested(_)) = &event {
            let at = layout.bounds();
            let said = tree.state.downcast_mut::<Said>();
            let now = Some((self.id.clone(), at));
            if said.0 != now {
                said.0 = now;
                shell.publish((self.on_place)(self.id.clone(), at));
            }
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
