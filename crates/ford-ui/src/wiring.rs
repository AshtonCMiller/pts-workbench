//! The Wiring tab (the Electron app's `manual/wiring/`): the vehicle's wiring book from Ford, drawn in the app.
//!
//!   * The contents: Ford's list of diagrams, gray (or left out with "Only this vehicle") when they don't apply to
//!     this VIN; a filter box, by number or by name. Enter (or Search) searches Ford's wiring for components,
//!     connectors, splices, grounds and diagrams.
//!   * A page: the drawing on a sheet you drag about and zoom (the wheel, double-click, − Fit +), its pages (‹ ›),
//!     Find on this page. A blue page number opens that page; a connector, splice or ground shows where it is (the
//!     location views) and its connector face (Ford's page, in your browser). Back goes to the page before.
//!
//! The connector list, the location index and connector faces are Ford's own pages: they open in your browser.
//! The drawing itself: paper.rs.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ford::wiring::{self, Cell, Hit, HitKind, Link, Location, PageInfo, Toc};
use iced::widget::image::Handle;
use iced::widget::{Column, button, checkbox, column, container, horizontal_space, pick_list, row, stack, text, text_input};
use iced::{Alignment, Element, Length, Point, Rectangle, Size, Task};
use ui_kit::scroll::scrollable;
use ui_kit::{anim, pill, style};

use crate::paper::{self, Drawing, View};
use crate::{Env, FordError, Out};

/// The page showing: its diagram, page, the item to mark on it, and its title
#[derive(Debug, Clone, PartialEq, Default)]
pub struct At {
    pub cell: String,
    pub page: u32,
    pub item: String,
    pub title: String,
}

/// A connector, splice or ground clicked: where it is
#[derive(Debug, Clone)]
pub struct Pop {
    pub name: String,
    pub face: String,
    pub places: Option<Result<Vec<Location>, FordError>>,
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub q: String,
    pub busy: bool,
    pub results: Option<Result<Vec<Hit>, FordError>>,
    pub kind: Option<HitKind>,
}

#[derive(Debug, Clone, Default)]
pub struct WiringView {
    vin: String,
    pub toc: Option<Toc>,
    pub loading: bool,
    pub error: Option<FordError>,
    pub filter: String,
    pub by_name: bool,
    /// Only what applies to this VIN
    pub only: bool,
    pub search: Option<SearchState>,
    /// The page showing (none: the contents), and the ones before it (Back)
    pub at: Option<At>,
    back: Vec<At>,
    /// Each diagram's pages, once read
    pages: HashMap<String, Vec<PageInfo>>,
    /// The drawings read (a few kept, so Back is quick)
    drawings: HashMap<(String, u32), Arc<Drawing>>,
    pub page_loading: bool,
    pub page_error: Option<FordError>,
    /// The page's drawing, the whole page drawn, and the part on screen drawn sharp (for a view and a size)
    drawing: Option<Arc<Drawing>>,
    base: Option<Handle>,
    sharp: Option<(Handle, View, Size)>,
    pub view: View,
    size: Size,
    /// Counts views: only the latest one's sharp drawing is wanted
    seq: u64,
    /// Marked on the page (the item looked for, found words), and the current one
    marks: Vec<(Rectangle, bool)>,
    pub find: String,
    found: Vec<Rectangle>,
    find_at: usize,
    pub pop: Option<Pop>,
    dark: bool,
}

#[derive(Debug, Clone)]
pub enum WiringMsg {
    Load,
    TocLoaded(Result<Toc, FordError>),
    Filter(String),
    ByName(bool),
    Only(bool),
    /// Search Ford's wiring for what's in the filter box; its answer; a kind picked; back to the diagrams
    Search,
    Searched(String, Result<Vec<Hit>, FordError>),
    SearchKind(Option<HitKind>),
    CloseSearch,
    OpenHit(usize),
    /// A diagram from the contents (its number)
    OpenCell(String),
    /// A page (from a link, a search result, the location views, Diagnostics)
    Open(At),
    Contents,
    Back,
    PagesLoaded(String, Result<Vec<PageInfo>, FordError>),
    PageLoaded(String, u32, Result<Arc<Drawing>, FordError>),
    BaseDrawn(String, u32, Option<Handle>),
    PickPage(u32),
    PrevPage,
    NextPage,
    ZoomIn,
    ZoomOut,
    Fit,
    /// The sheet moved or zoomed; its size; drawing it sharp (when it's still that view); drawn
    View(View),
    Size(Size),
    DrawDue(u64),
    Drawn(u64, Option<Handle>, View, Size),
    /// Clicked on the drawing (its point)
    Click(Point),
    Find(String),
    FindNext,
    ClosePop,
    Located(String, Result<Vec<Location>, FordError>),
    /// Ford's page for what's showing (in your browser); a connector face; its address, worked out
    FordPage,
    Face(String),
    OpenUrl(Option<String>),
    /// The page again (after a problem)
    Retry,
    SignIn,
    SetUp,
}

/// A page's label in the page list
#[derive(Debug, Clone, PartialEq)]
pub struct PageChoice {
    pub page: u32,
    pub applies: bool,
}

impl std::fmt::Display for PageChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.applies {
            write!(f, "Page {}", self.page)
        } else {
            write!(f, "Page {} (not this VIN)", self.page)
        }
    }
}

fn err(e: ford::Error) -> FordError {
    FordError::from(e)
}

impl WiringView {
    pub fn new(vin: impl Into<String>) -> Self {
        WiringView {
            vin: vin.into(),
            only: true,
            ..Default::default()
        }
    }

    pub fn features(&self) -> Option<&wiring::Features> {
        self.toc.as_ref().and_then(|t| t.features.as_ref())
    }

    /// Reads the contents (unless they're read, or being read)
    pub fn load(&mut self, env: &Env, force: bool) -> Task<WiringMsg> {
        self.dark = env.dark;
        if env.sample || self.loading || (self.toc.is_some() && !force) {
            return Task::none();
        }
        self.loading = true;
        self.error = None;
        let (ford, vin) = (env.ford.clone(), self.vin.clone());
        Task::perform(async move { ford.wiring_toc(&vin).await.map_err(err) }, WiringMsg::TocLoaded)
    }

    fn title_of(&self, cell: &str) -> String {
        self.toc
            .as_ref()
            .and_then(|t| t.cells.iter().find(|c| c.number == wiring::cell_no(cell)))
            .map(|c| c.title.clone())
            .unwrap_or_default()
    }

    /// The pages to step through: this vehicle's (unless "Only this vehicle" is off), and the one showing
    pub fn page_choices(&self) -> Vec<PageChoice> {
        let Some(at) = &self.at else { return Vec::new() };
        let f = self.features();
        let mut list: Vec<PageChoice> = self
            .pages
            .get(&at.cell)
            .into_iter()
            .flatten()
            .map(|p| PageChoice {
                page: p.page,
                applies: wiring::page_applies(p, f),
            })
            .filter(|p| p.applies || !self.only || f.is_none() || p.page == at.page)
            .collect();
        if !list.iter().any(|p| p.page == at.page) {
            list.push(PageChoice {
                page: at.page,
                applies: true,
            });
        }
        list.sort_by_key(|p| p.page);
        list
    }

    fn open(&mut self, env: &Env, at: At, push: bool) -> Task<WiringMsg> {
        let at = At {
            cell: wiring::cell_no(&at.cell),
            page: at.page.max(1),
            title: if at.title.is_empty() {
                self.title_of(&at.cell)
            } else {
                at.title.clone()
            },
            ..at
        };
        if at.cell.is_empty() {
            return Task::none();
        }
        if push {
            if let Some(now) = self
                .at
                .take()
                .filter(|n| !(n.cell == at.cell && n.page == at.page && n.item == at.item))
            {
                self.back.push(now);
                if self.back.len() > 50 {
                    self.back.remove(0);
                }
            }
        }
        self.at = Some(at.clone());
        self.pop = None;
        self.search = None;
        self.page_error = None;
        self.marks.clear();
        self.found.clear();
        let mut tasks = Vec::new();
        if !self.pages.contains_key(&at.cell) {
            let (ford, vin, cell) = (env.ford.clone(), self.vin.clone(), at.cell.clone());
            tasks.push(Task::perform(
                async move {
                    let r = ford.wiring_pages(&vin, &cell).await.map_err(err);
                    (cell, r)
                },
                |(c, r)| WiringMsg::PagesLoaded(c, r),
            ));
        }
        let key = (at.cell.clone(), at.page);
        match self.drawings.get(&key).cloned() {
            Some(d) => tasks.push(self.show(env, d)),
            None => {
                self.page_loading = true;
                let (ford, vin, cell, page) = (env.ford.clone(), self.vin.clone(), at.cell.clone(), at.page);
                tasks.push(Task::perform(
                    async move {
                        let r = match ford.wiring_page(&vin, &cell, page).await {
                            Ok(p) => tokio::task::spawn_blocking(move || paper::build(&p))
                                .await
                                .map_err(|_| FordError::Problem("Couldn't draw the diagram".into()))
                                .and_then(|r| r.map_err(FordError::Problem))
                                .map(Arc::new),
                            Err(e) => Err(err(e)),
                        };
                        (cell, page, r)
                    },
                    |(c, p, r)| WiringMsg::PageLoaded(c, p, r),
                ));
            }
        }
        Task::batch(tasks)
    }

    /// A drawing read: shown (fitted, or at its item), and drawn
    fn show(&mut self, env: &Env, d: Arc<Drawing>) -> Task<WiringMsg> {
        self.dark = env.dark;
        self.page_loading = false;
        self.drawing = Some(d.clone());
        self.base = None;
        self.sharp = None;
        let page = Size::new(d.width, d.height);
        self.view = View::fit(page, self.size);
        // the item looked for, marked and zoomed to
        let item = self.at.as_ref().map(|a| a.item.trim().to_uppercase()).unwrap_or_default();
        if !item.is_empty() {
            let hits: Vec<Rectangle> = d
                .texts
                .iter()
                .filter(|(_, t)| {
                    let t = t.to_uppercase();
                    t == item || t.starts_with(&format!("{item} ")) || t.split_whitespace().any(|w| w == item)
                })
                .map(|(r, _)| *r)
                .collect();
            if let Some(first) = hits.first() {
                self.view = View::centered(*first, self.size, 2.2);
            }
            self.marks = hits.iter().enumerate().map(|(i, r)| (*r, i == 0)).collect();
        }
        if !self.find.trim().is_empty() {
            self.find_words();
        }
        let key = self.at.as_ref().map(|a| (a.cell.clone(), a.page)).unwrap_or_default();
        let dark = self.dark;
        let base = Task::perform(
            async move {
                let h = tokio::task::spawn_blocking(move || paper::render_page(&d, 2600.0, dark))
                    .await
                    .ok()
                    .flatten();
                (key, h)
            },
            |((c, p), h)| WiringMsg::BaseDrawn(c, p, h),
        );
        Task::batch([base, self.draw_soon()])
    }

    /// The part on screen drawn sharp, a moment after the sheet stops moving
    fn draw_soon(&mut self) -> Task<WiringMsg> {
        self.seq += 1;
        let seq = self.seq;
        Task::perform(tokio::time::sleep(Duration::from_millis(140)), move |_| WiringMsg::DrawDue(seq))
    }

    fn find_words(&mut self) {
        let q = wiring::clean(&self.find).to_uppercase();
        self.found.clear();
        self.find_at = 0;
        if let (Some(d), false) = (&self.drawing, q.is_empty()) {
            let mut hits: Vec<Rectangle> = d
                .texts
                .iter()
                .filter(|(_, t)| t.to_uppercase().contains(&q))
                .map(|(r, _)| *r)
                .collect();
            hits.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
            self.found = hits;
        }
        self.mark_found();
    }

    fn mark_found(&mut self) {
        self.marks = self.found.iter().enumerate().map(|(i, r)| (*r, i == self.find_at)).collect();
    }

    pub fn update(&mut self, msg: WiringMsg, env: &Env) -> (Task<WiringMsg>, Vec<Out>) {
        let mut outs = Vec::new();
        let task = match msg {
            WiringMsg::Load => self.load(env, true),
            WiringMsg::SetUp => {
                if self.error == Some(FordError::NotSetUp) {
                    self.load(env, true)
                } else {
                    Task::none()
                }
            }
            WiringMsg::TocLoaded(r) => {
                self.loading = false;
                match r {
                    Ok(t) => self.toc = Some(t),
                    Err(e) => {
                        if e == FordError::NotSetUp {
                            outs.push(Out::SetUpPts(self.vin.clone()));
                        }
                        self.error = Some(e);
                    }
                }
                Task::none()
            }
            WiringMsg::Filter(q) => {
                self.filter = q;
                Task::none()
            }
            WiringMsg::ByName(b) => {
                self.by_name = b;
                Task::none()
            }
            WiringMsg::Only(on) => {
                self.only = on;
                Task::none()
            }
            WiringMsg::Search => {
                let q = self.filter.trim().to_string();
                if q.chars().count() < 2 || env.sample {
                    return (Task::none(), outs);
                }
                if self
                    .search
                    .as_ref()
                    .is_some_and(|s| s.q == q && (s.busy || matches!(s.results, Some(Ok(_)))))
                {
                    return (Task::none(), outs);
                }
                self.search = Some(SearchState {
                    q: q.clone(),
                    busy: true,
                    ..Default::default()
                });
                let (ford, vin) = (env.ford.clone(), self.vin.clone());
                Task::perform(
                    async move {
                        let r = ford.wiring_search(&vin, &q).await.map_err(err);
                        (q, r)
                    },
                    |(q, r)| WiringMsg::Searched(q, r),
                )
            }
            WiringMsg::Searched(q, r) => {
                if let Some(s) = self.search.as_mut().filter(|s| s.q == q) {
                    s.busy = false;
                    if r.as_ref().err() == Some(&FordError::SignedOut) {
                        outs.push(Out::SignIn);
                    }
                    s.results = Some(r);
                }
                Task::none()
            }
            WiringMsg::SearchKind(k) => {
                if let Some(s) = self.search.as_mut() {
                    s.kind = k;
                }
                Task::none()
            }
            WiringMsg::CloseSearch => {
                self.search = None;
                Task::none()
            }
            WiringMsg::OpenHit(i) => {
                let hit = self
                    .search
                    .as_ref()
                    .and_then(|s| s.results.as_ref())
                    .and_then(|r| r.as_ref().ok())
                    .and_then(|l| self.shown_hits(l).get(i).map(|h| (*h).clone()));
                match hit {
                    Some(h) if h.cell.is_empty() => {
                        // a connector face or the connector list: Ford's page
                        let _ = crate::open_external(&h.url);
                        Task::none()
                    }
                    Some(h) => {
                        self.back.clear();
                        let title = if h.kind == HitKind::Diagram {
                            wiring::clean(h.title.trim_start_matches(|c: char| c.is_ascii_digit() || c == ' '))
                        } else {
                            String::new()
                        };
                        self.open(
                            env,
                            At {
                                cell: h.cell,
                                page: h.page,
                                item: h.item,
                                title,
                            },
                            false,
                        )
                    }
                    None => Task::none(),
                }
            }
            WiringMsg::OpenCell(n) => {
                let cell = self.toc.as_ref().and_then(|t| t.cells.iter().find(|c| c.number == n)).cloned();
                match cell {
                    // the connector list and location index are Ford's own pages
                    Some(c) if !c.drawn() => self.ford_page(env, &c.kind, &c.number, 1, "", &c.title),
                    Some(c) => {
                        self.back.clear();
                        self.open(
                            env,
                            At {
                                cell: c.number,
                                page: 1,
                                item: String::new(),
                                title: c.title,
                            },
                            false,
                        )
                    }
                    None => Task::none(),
                }
            }
            WiringMsg::Open(at) => self.open(env, at, true),
            WiringMsg::Contents => {
                if let Some(now) = self.at.take() {
                    self.back.push(now);
                }
                self.pop = None;
                if self.toc.is_none() { self.load(env, false) } else { Task::none() }
            }
            WiringMsg::Back => match self.back.pop() {
                Some(prev) => self.open(env, prev, false),
                None => {
                    self.at = None;
                    Task::none()
                }
            },
            WiringMsg::PagesLoaded(cell, r) => {
                if let Ok(list) = r {
                    self.pages.insert(cell, list);
                }
                Task::none()
            }
            WiringMsg::PageLoaded(cell, page, r) => {
                let showing = self.at.as_ref().is_some_and(|a| a.cell == cell && a.page == page);
                match r {
                    Ok(d) => {
                        if self.drawings.len() > 12 {
                            self.drawings.clear();
                        }
                        self.drawings.insert((cell, page), d.clone());
                        if showing {
                            return (self.show(env, d), outs);
                        }
                    }
                    Err(e) if showing => {
                        self.page_loading = false;
                        if e == FordError::SignedOut {
                            outs.push(Out::SignIn);
                        }
                        self.page_error = Some(e);
                    }
                    Err(_) => {}
                }
                Task::none()
            }
            WiringMsg::BaseDrawn(cell, page, h) => {
                if self.at.as_ref().is_some_and(|a| a.cell == cell && a.page == page) {
                    self.base = h;
                }
                Task::none()
            }
            WiringMsg::PickPage(p) => self.step_to(env, Some(p)),
            WiringMsg::PrevPage => self.step(env, -1),
            WiringMsg::NextPage => self.step(env, 1),
            WiringMsg::ZoomIn => self.zoom(1.5),
            WiringMsg::ZoomOut => self.zoom(1.0 / 1.5),
            WiringMsg::Fit => {
                if let Some(d) = &self.drawing {
                    self.view = View::fit(Size::new(d.width, d.height), self.size);
                }
                self.draw_soon()
            }
            WiringMsg::View(v) => {
                self.view = v;
                self.draw_soon()
            }
            WiringMsg::Size(s) => {
                let first = self.size.width <= 0.0;
                self.size = s;
                // (the first size: the page fitted to it)
                if first {
                    if let Some(d) = &self.drawing {
                        self.view = View::fit(Size::new(d.width, d.height), s);
                    }
                }
                self.draw_soon()
            }
            WiringMsg::DrawDue(seq) => {
                let (Some(d), true) = (self.drawing.clone(), seq == self.seq) else {
                    return (Task::none(), outs);
                };
                if self.size.width < 2.0 || self.size.height < 2.0 {
                    return (Task::none(), outs);
                }
                let (view, size, dark) = (self.view, self.size, self.dark);
                Task::perform(
                    async move {
                        let h = tokio::task::spawn_blocking(move || paper::render_view(&d, view, size, dark))
                            .await
                            .ok()
                            .flatten();
                        (seq, h, view, size)
                    },
                    |(seq, h, v, s)| WiringMsg::Drawn(seq, h, v, s),
                )
            }
            WiringMsg::Drawn(seq, h, v, s) => {
                if seq == self.seq {
                    self.sharp = h.map(|h| (h, v, s));
                }
                Task::none()
            }
            WiringMsg::Click(p) => {
                let link = self.drawing.as_ref().and_then(|d| {
                    d.links
                        .iter()
                        .filter(|(r, _)| p.x >= r.x - 2.0 && p.x <= r.x + r.width + 2.0 && p.y >= r.y - 2.0 && p.y <= r.y + r.height + 2.0)
                        // the smallest (the most exact) under the pointer
                        .min_by(|a, b| (a.0.width * a.0.height).total_cmp(&(b.0.width * b.0.height)))
                        .map(|(_, l)| l.clone())
                });
                match link {
                    Some(Link::Page { cell, page, item, title }) => self.open(env, At { cell, page, item, title }, true),
                    Some(Link::Url(u)) => {
                        let _ = crate::open_external(&u);
                        Task::none()
                    }
                    Some(Link::Item { name, face }) => {
                        self.pop = Some(Pop {
                            name: name.clone(),
                            face,
                            places: None,
                        });
                        let (ford, vin) = (env.ford.clone(), self.vin.clone());
                        let mut tasks = vec![Task::perform(
                            async move {
                                let r = ford.wiring_locations(&vin, &name).await.map_err(err);
                                (name, r)
                            },
                            |(n, r)| WiringMsg::Located(n, r),
                        )];
                        // the location views' pages (which apply to this VIN)
                        if !self.pages.contains_key("151") {
                            let (ford, vin) = (env.ford.clone(), self.vin.clone());
                            tasks.push(Task::perform(
                                async move { ford.wiring_pages(&vin, "151").await.map_err(err) },
                                |r| WiringMsg::PagesLoaded("151".into(), r),
                            ));
                        }
                        Task::batch(tasks)
                    }
                    None => {
                        self.pop = None;
                        Task::none()
                    }
                }
            }
            WiringMsg::ClosePop => {
                self.pop = None;
                Task::none()
            }
            WiringMsg::Located(name, r) => {
                if let Some(p) = self.pop.as_mut().filter(|p| p.name == name) {
                    p.places = Some(r);
                }
                Task::none()
            }
            WiringMsg::Find(q) => {
                self.find = q;
                self.find_words();
                self.zoom_to_found()
            }
            WiringMsg::FindNext => {
                if !self.found.is_empty() {
                    self.find_at = (self.find_at + 1) % self.found.len();
                    self.mark_found();
                }
                self.zoom_to_found()
            }
            WiringMsg::FordPage => match self.at.clone() {
                Some(at) => self.ford_page(env, "page", &at.cell, at.page, &at.item, &at.title),
                None => self.ford_page(env, "TableOfContents", "", 0, "", "Wiring"),
            },
            WiringMsg::Face(face) => self.ford_page(env, "face", "150", 0, &face, ""),
            WiringMsg::OpenUrl(url) => {
                if let Some(u) = url {
                    let _ = crate::open_external(&u);
                }
                Task::none()
            }
            WiringMsg::Retry => match self.at.clone() {
                Some(at) => {
                    self.drawings.remove(&(at.cell.clone(), at.page));
                    self.open(env, at, false)
                }
                None => self.load(env, true),
            },
            WiringMsg::SignIn => {
                outs.push(Out::SignIn);
                Task::none()
            }
        };
        (task, outs)
    }

    fn zoom(&mut self, f: f32) -> Task<WiringMsg> {
        self.view = self.view.zoomed(Point::new(self.size.width / 2.0, self.size.height / 2.0), f);
        self.draw_soon()
    }

    fn zoom_to_found(&mut self) -> Task<WiringMsg> {
        match self.found.get(self.find_at) {
            Some(r) => {
                self.view = View::centered(*r, self.size, self.view.z.max(1.6));
                self.draw_soon()
            }
            None => Task::none(),
        }
    }

    fn step(&mut self, env: &Env, by: i32) -> Task<WiringMsg> {
        let Some(at) = &self.at else { return Task::none() };
        let list = self.page_choices();
        let i = list.iter().position(|p| p.page == at.page).unwrap_or(0) as i32 + by;
        let next = (i >= 0).then(|| list.get(i as usize).map(|p| p.page)).flatten();
        self.step_to(env, next)
    }

    fn step_to(&mut self, env: &Env, page: Option<u32>) -> Task<WiringMsg> {
        let (Some(at), Some(page)) = (self.at.clone(), page) else {
            return Task::none();
        };
        self.open(
            env,
            At {
                page,
                item: String::new(),
                ..at
            },
            false,
        )
    }

    /// Ford's own wiring page, in your browser (what the app doesn't draw)
    fn ford_page(&self, env: &Env, kind: &str, cell: &str, page: u32, item: &str, title: &str) -> Task<WiringMsg> {
        let (ford, vin) = (env.ford.clone(), self.vin.clone());
        let (kind, cell, item, title) = (kind.to_string(), cell.to_string(), item.to_string(), title.to_string());
        // (the book's settings are kept: this is quick)
        Task::perform(
            async move {
                ford.wiring_info(&vin)
                    .await
                    .ok()
                    .map(|info| ford::Ford::wiring_ford_url(&info, &kind, &cell, page, &item, &title))
            },
            WiringMsg::OpenUrl,
        )
    }

    /// The search's results for the kind picked (this vehicle's, unless "Only this vehicle" is off), in order
    pub fn shown_hits<'a>(&self, all: &'a [Hit]) -> Vec<&'a Hit> {
        let kind = self.search.as_ref().and_then(|s| s.kind);
        let mut list: Vec<&Hit> = all
            .iter()
            .filter(|h| h.applies || !self.only)
            .filter(|h| kind.is_none_or(|k| h.kind == k))
            .collect();
        list.sort_by_key(|h| (h.kind, !h.applies));
        list
    }
}

// ---------- the look ----------

fn card<'a>(content: impl Into<Element<'a, WiringMsg>>) -> Element<'a, WiringMsg> {
    container(content).padding(18).width(Length::Fill).style(style::card).into()
}

fn ghost<'a>(label: &'a str, msg: Option<WiringMsg>) -> Element<'a, WiringMsg> {
    button(text(label).size(13))
        .padding([4, 10])
        .style(style::ghost)
        .on_press_maybe(msg)
        .into()
}

fn problem<'a>(e: &FordError, what: &'a str, retry: WiringMsg) -> Element<'a, WiringMsg> {
    let (say, act, msg) = match e {
        FordError::SignedOut => (
            "Ford needs you to sign in to PTS first.".to_string(),
            "Sign in to PTS",
            WiringMsg::SignIn,
        ),
        FordError::NotSetUp => ("PTS is being set up for this vehicle…".to_string(), "Try again", retry),
        e => (format!("Couldn't load {what} ({})", e.text()), "Try again", retry),
    };
    card(
        column![
            text(say).style(style::muted),
            button(text(act)).padding([6, 14]).style(button::primary).on_press(msg)
        ]
        .spacing(10),
    )
}

pub fn view(w: &WiringView, sample: bool) -> Element<'_, WiringMsg> {
    if sample {
        return container(card(
            text("Wiring diagrams show for your real ROs' vehicles, once you're signed in to Xtime.").style(style::muted),
        ))
        .padding(16)
        .into();
    }
    match &w.at {
        Some(at) => page_view(w, at),
        None => contents(w),
    }
}

fn header<'a>(title: String, sub: String, right: Element<'a, WiringMsg>) -> Element<'a, WiringMsg> {
    row![
        column![text(title).size(18).font(style::BOLD), text(sub).size(12).style(style::muted)].spacing(2),
        horizontal_space(),
        right
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn contents(w: &WiringView) -> Element<'_, WiringMsg> {
    let sub = match &w.toc {
        Some(t) => format!(
            "{} · {} diagrams",
            if t.title.is_empty() { "Wiring" } else { &t.title },
            t.cells.len()
        ),
        None => "Ford's wiring book for this vehicle".into(),
    };
    let head = header(
        "Wiring Diagrams".into(),
        sub,
        row![
            ghost("Back to the page", (!w.back.is_empty()).then_some(WiringMsg::Back)),
            ghost("Ford's page", Some(WiringMsg::FordPage)),
        ]
        .spacing(6)
        .into(),
    );
    let mut bar = row![
        text_input("Filter the diagrams, or search: C1689, G100, battery monitoring sensor…", &w.filter)
            .on_input(WiringMsg::Filter)
            .on_submit(WiringMsg::Search)
            .padding([8, 12])
            .width(Length::Fill),
        button(text("Search the wiring").size(13).font(style::BOLD))
            .padding([8, 14])
            .style(button::primary)
            .on_press_maybe((w.filter.trim().chars().count() >= 2).then_some(WiringMsg::Search)),
        ui_kit::anim::tabs(
            usize::from(w.by_name),
            vec![
                button(text("By number").size(13))
                    .padding([6, 12])
                    .style(style::tab(!w.by_name))
                    .on_press(WiringMsg::ByName(false))
                    .into(),
                button(text("By name").size(13))
                    .padding([6, 12])
                    .style(style::tab(w.by_name))
                    .on_press(WiringMsg::ByName(true))
                    .into(),
            ],
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if w.features().is_some() {
        bar = bar.push(checkbox("Only this vehicle", w.only).on_toggle(WiringMsg::Only).text_size(13));
    }
    let body: Element<'_, WiringMsg> = if let Some(s) = &w.search {
        search_results(w, s)
    } else if let Some(t) = &w.toc {
        diagram_list(w, t)
    } else if let Some(e) = &w.error {
        problem(e, "the wiring book", WiringMsg::Load)
    } else {
        card(text("Loading the wiring book…").style(style::muted))
    };
    column![head, bar, body].spacing(12).padding(16).height(Length::Fill).into()
}

fn diagram_list<'a>(w: &'a WiringView, t: &'a Toc) -> Element<'a, WiringMsg> {
    let f = t.features.as_ref();
    let q = w.filter.trim().to_lowercase();
    let mut list: Vec<(&Cell, bool)> = t.cells.iter().map(|c| (c, wiring::cell_applies(c, f))).collect();
    if !q.is_empty() {
        list.retain(|(c, _)| format!("{} {} {}", c.raw, c.number, c.title).to_lowercase().contains(&q));
    }
    if w.only && f.is_some() {
        list.retain(|(_, ok)| *ok);
    }
    if w.by_name {
        list.sort_by(|a, b| a.0.title.cmp(&b.0.title));
    } else {
        list.sort_by_key(|(c, _)| c.number.parse::<u32>().unwrap_or(u32::MAX));
    }
    let grayed = f
        .map(|_| t.cells.iter().filter(|c| !wiring::cell_applies(c, f)).count())
        .unwrap_or(0);
    let count = format!(
        "{} of {} diagrams{}",
        list.len(),
        t.cells.len(),
        if grayed > 0 && !w.only {
            format!(" · gray ones ({grayed}) don't apply to this VIN")
        } else {
            String::new()
        }
    );
    let mut col = Column::new().spacing(2);
    for (c, ok) in &list {
        let tone = |t: iced::widget::Text<'a>| if *ok { t } else { t.style(style::muted) };
        let mut r = row![
            tone(text(c.number.clone()).font(style::MONO).width(48)),
            tone(text(c.title.clone()).width(Length::Fill)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        match c.kind.as_str() {
            "Connectors" => r = r.push(pill("Connector views · Ford's page", [0x5d, 0x66, 0x70])),
            "LocIndex" => r = r.push(pill("Location charts · Ford's page", [0x5d, 0x66, 0x70])),
            _ => {}
        }
        col = col.push(
            button(r)
                .width(Length::Fill)
                .padding([8, 10])
                .style(style::list_item(false))
                .on_press(WiringMsg::OpenCell(c.number.clone())),
        );
    }
    if list.is_empty() {
        col = col.push(
            column![
                text(if q.is_empty() {
                    "Nothing matches."
                } else {
                    "No diagram has this in its name."
                })
                .style(style::muted),
                ghost("Search the wiring for it ›", (!q.is_empty()).then_some(WiringMsg::Search)),
            ]
            .spacing(6),
        );
    }
    column![text(count).size(12).style(style::muted), scrollable(col).height(Length::Fill)]
        .spacing(8)
        .height(Length::Fill)
        .into()
}

fn search_results<'a>(w: &'a WiringView, s: &'a SearchState) -> Element<'a, WiringMsg> {
    let head = row![
        text(format!("Ford's wiring search: “{}”", s.q)).font(style::BOLD),
        horizontal_space(),
        ghost("‹ The diagrams", Some(WiringMsg::CloseSearch)),
    ]
    .align_y(Alignment::Center);
    let body: Element<'a, WiringMsg> = match &s.results {
        None => text("Searching Ford's wiring…").style(style::muted).into(),
        Some(Err(e)) => problem(e, "the search", WiringMsg::Search),
        Some(Ok(all)) => {
            let shown_all: Vec<&Hit> = all.iter().filter(|h| h.applies || !w.only).collect();
            let mut kinds: Vec<(HitKind, usize)> = Vec::new();
            for h in &shown_all {
                match kinds.iter_mut().find(|(k, _)| *k == h.kind) {
                    Some((_, n)) => *n += 1,
                    None => kinds.push((h.kind, 1)),
                }
            }
            kinds.sort();
            let on = |k: Option<HitKind>| s.kind == k;
            let mut tabs = row![
                button(text(format!("All {}", shown_all.len())).size(12))
                    .padding([4, 10])
                    .style(style::toggle(on(None)))
                    .on_press(WiringMsg::SearchKind(None))
            ]
            .spacing(6);
            for (k, n) in kinds {
                tabs = tabs.push(
                    button(text(format!("{} {n}", k.label())).size(12))
                        .padding([4, 10])
                        .style(style::toggle(on(Some(k))))
                        .on_press(WiringMsg::SearchKind(Some(k))),
                );
            }
            let hits = w.shown_hits(all);
            let mut col = Column::new().spacing(2);
            for (i, h) in hits.iter().enumerate() {
                let where_ = if h.cell.is_empty() {
                    "Ford's page".to_string()
                } else if h.cell == "151" {
                    format!("Location view 151-{}", h.page)
                } else {
                    let t = w.title_of(&h.cell);
                    format!(
                        "{}-{}{}",
                        h.cell,
                        h.page,
                        if t.is_empty() { String::new() } else { format!(" {t}") }
                    )
                };
                let color = match h.kind {
                    HitKind::Component => [0x1a, 0x5f, 0xb4],
                    HitKind::Connector => [0x15, 0x80, 0x3d],
                    HitKind::Splice => [0xb4, 0x53, 0x09],
                    HitKind::Ground => [0x7c, 0x3a, 0xed],
                    _ => [0x5d, 0x66, 0x70],
                };
                let r = row![
                    container(pill(h.kind.label(), color)).width(96),
                    text(h.title.clone()).width(Length::Fill),
                    text(where_).size(12).style(style::muted),
                ]
                .spacing(10)
                .align_y(Alignment::Center);
                col = col.push(
                    button(r)
                        .width(Length::Fill)
                        .padding([8, 10])
                        .style(style::list_item(false))
                        .on_press(WiringMsg::OpenHit(i)),
                );
            }
            if hits.is_empty() {
                col = col.push(text("Ford's search found nothing for this.").style(style::muted));
            }
            let more = all.len() - all.iter().filter(|h| h.applies || !w.only).count();
            if more > 0 {
                col = col.push(
                    row![
                        text(format!("{more} more for other options.")).size(12).style(style::muted),
                        ghost("Show them", Some(WiringMsg::Only(false)))
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }
            column![tabs.wrap(), scrollable(col).height(Length::Fill)]
                .spacing(10)
                .height(Length::Fill)
                .into()
        }
    };
    column![head, body].spacing(10).height(Length::Fill).into()
}

fn page_view<'a>(w: &'a WiringView, at: &'a At) -> Element<'a, WiringMsg> {
    let sub = match (&w.toc, at.item.is_empty()) {
        (Some(t), true) => t.title.clone(),
        (Some(t), false) => format!("{} · showing {}", t.title, at.item),
        (None, _) => String::new(),
    };
    let choices = w.page_choices();
    let i = choices.iter().position(|p| p.page == at.page);
    let pages = row![
        ghost("‹", i.is_some_and(|i| i > 0).then_some(WiringMsg::PrevPage)),
        pick_list(choices.clone(), choices.iter().find(|p| p.page == at.page).cloned(), |p| {
            WiringMsg::PickPage(p.page)
        })
        .text_size(13)
        .padding([4, 8]),
        ghost("›", i.is_some_and(|i| i + 1 < choices.len()).then_some(WiringMsg::NextPage)),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let zoom = row![
        ghost("−", Some(WiringMsg::ZoomOut)),
        ghost("Fit", Some(WiringMsg::Fit)),
        ghost("+", Some(WiringMsg::ZoomIn)),
    ]
    .spacing(2);
    let head = header(
        format!("{}-{}  {}", at.cell, at.page, at.title),
        sub,
        row![
            ghost("‹ Contents", Some(WiringMsg::Contents)),
            ghost("‹ Back", (!w.back.is_empty()).then_some(WiringMsg::Back)),
            pages,
            zoom,
            ghost("Ford's page", Some(WiringMsg::FordPage)),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
    );
    let find_note = if w.find.trim().is_empty() {
        String::new()
    } else if w.found.is_empty() {
        "not on this page".into()
    } else {
        format!("{} of {}", w.find_at + 1, w.found.len())
    };
    let hint = row![
        text("Drag to move · scroll to zoom · double-click to zoom in · click a blue page number or a connector")
            .size(12)
            .style(style::muted),
        horizontal_space(),
        text_input("Find on this page", &w.find)
            .on_input(WiringMsg::Find)
            .on_submit(WiringMsg::FindNext)
            .padding([5, 10])
            .size(13)
            .width(220),
        text(find_note).size(12).style(style::muted).width(110),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let sheet: Element<'a, WiringMsg> = match &w.drawing {
        Some(d) => paper::paper(
            Size::new(d.width, d.height),
            w.view,
            w.base.as_ref(),
            w.sharp.as_ref().map(|(h, v, s)| (h, *v, *s)),
            &w.marks,
            d.links.iter().map(|(r, _)| *r).collect(),
            w.dark,
            WiringMsg::View,
            WiringMsg::Click,
            WiringMsg::Size,
        )
        .into(),
        _ => paper::paper(
            Size::new(1100.0, 850.0),
            w.view,
            None,
            None,
            &[],
            Vec::new(),
            w.dark,
            WiringMsg::View,
            WiringMsg::Click,
            WiringMsg::Size,
        )
        .into(),
    };
    let mut layers = stack![container(sheet).width(Length::Fill).height(Length::Fill).style(style::panel)];
    // over the sheet: loading, a problem, or where a connector is
    if let Some(e) = &w.page_error {
        layers = layers.push(
            container(problem(e, "the diagram", WiringMsg::Retry))
                .padding(40)
                .center_x(Length::Fill),
        );
    } else if w.page_loading {
        layers = layers.push(
            container(
                container(text("Loading the diagram…").style(style::muted))
                    .padding([10, 16])
                    .style(style::floating_card),
            )
            .padding(24)
            .center_x(Length::Fill),
        );
    }
    if let Some(p) = &w.pop {
        layers = layers.push(
            container(pop_card(w, p))
                .padding(14)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right),
        );
    }
    column![head, layers.height(Length::Fill), hint]
        .spacing(10)
        .padding(16)
        .height(Length::Fill)
        .into()
}

fn pop_card<'a>(w: &'a WiringView, p: &'a Pop) -> Element<'a, WiringMsg> {
    let kind = match p.name.chars().next().map(|c| c.to_ascii_uppercase()) {
        Some('C') => "Connector",
        Some('S') => "Splice",
        Some('G') => "Ground",
        _ => "Item",
    };
    let mut col = column![
        row![
            text(p.name.clone()).size(16).font(style::BOLD),
            text(kind).size(12).style(style::muted),
            horizontal_space(),
            ghost("×", Some(WiringMsg::ClosePop))
        ]
        .spacing(8)
        .align_y(Alignment::Center)
    ]
    .spacing(8);
    if !p.face.is_empty() {
        col = col.push(
            button(text("Connector face (pin view) ›").size(13))
                .padding([5, 10])
                .style(style::ghost)
                .on_press(WiringMsg::Face(p.face.clone())),
        );
    } else if kind == "Connector" {
        col = col.push(text("No connector face for this one").size(12).style(style::muted));
    }
    let f = w.features();
    let fits = |n: u32| {
        w.pages
            .get("151")
            .and_then(|l| l.iter().find(|x| x.page == n))
            .is_none_or(|x| wiring::page_applies(x, f))
    };
    match &p.places {
        None => col = col.push(text("Finding where it is…").size(12).style(style::muted)),
        Some(Err(e)) => {
            col = col.push(
                text(format!("Couldn't find where it is ({})", e.text()))
                    .size(12)
                    .style(style::muted),
            )
        }
        Some(Ok(list)) if list.is_empty() => col = col.push(text("No location view for this one").size(12).style(style::muted)),
        Some(Ok(list)) => {
            col = col.push(text("Where it is (location views):").size(12).style(style::muted));
            for l in list {
                let label = format!(
                    "{}{}  (151-{})",
                    if l.qual.is_empty() {
                        String::new()
                    } else {
                        format!("{}, ", l.qual)
                    },
                    if l.desc.is_empty() { "Location" } else { &l.desc },
                    l.page
                );
                let label = if fits(l.page) {
                    text(label).size(13)
                } else {
                    text(label).size(13).style(style::muted)
                };
                let b = button(label)
                    .width(Length::Fill)
                    .padding([5, 10])
                    .style(style::list_item(false))
                    .on_press(WiringMsg::Open(At {
                        cell: "151".into(),
                        page: l.page,
                        item: p.name.clone(),
                        title: format!("{} location", p.name),
                    }));
                col = col.push(b);
            }
        }
    }
    anim::page(
        container(col).padding(14).width(320).style(style::floating_card),
        anim::Phase::Still,
    )
    .into()
}
