//! The Labor Times tab (the Electron app's `manual/labor-times/`): Ford's SLTS for the vehicle, redrawn in the app's
//! style. Search the way SLTS does (by section, operation number, word, part number or the alphabetic index); each
//! operation is a card with its options (warranty and customer pay time, its one-time use parts, + Add). What's added
//! goes in the tray on the right: it adds up the warranty time and the customer pay time (× 1.5), warns about Ford's
//! overlaps and combinations without their base operation, copies the op codes, lists the one-time use parts and
//! prints. The tray, the search type and the last search are kept with the vehicle (as the Electron app keeps them).
//!
//! SLTS works by posting its form back to itself: the last page it sent is kept here, and the next request posts it
//! back (crates/ford/src/labor.rs).

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use ford::labor::{
    self, CUSTOMER_PAY, Cart, Choice, Match, OpKind, Parts, Raw, RawLink, Report, Saved, SearchForm, SltsPage, TrayItem, hrs, is_word, num,
};
use iced::widget::{Column, Row, Space, button, column, container, horizontal_space, pick_list, row, stack, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Task, Theme};
use ui_kit::scroll::scrollable;
use ui_kit::{anim, pill, style};

use crate::{Env, FordError, Out};

fn body_id() -> iced::widget::scrollable::Id {
    iced::widget::scrollable::Id::new("labor-body")
}

fn input_id() -> text_input::Id {
    text_input::Id::new("labor-q")
}

/// An operation's card, to glide to
fn op_name(code: &str) -> String {
    format!("labor-op:{code}")
}

/// Space left above an operation glided to
const MARGIN: f32 = 12.0;

/// What shows under the search
#[derive(Debug, Clone, Default)]
enum Body {
    #[default]
    Hint,
    Report(Report),
    /// Not a report (an index, a message, nothing found): what SLTS showed
    Raw(Raw),
    /// Word Search's matches
    Matches,
    Nothing(String),
}

/// One operation's one-time use parts, as the parts window and the printout show them
#[derive(Debug, Clone)]
pub struct PartsBlock {
    pub code: String,
    /// "Compressor - A/C · 2.0L GTDI"
    pub name: String,
    pub parts: Parts,
}

/// What's printed: the tray (with its parts) or one parts list
#[derive(Debug, Clone)]
pub struct LaborPrint {
    pub title: String,
    /// "2023 Ford F-150 · 1FT… · RO 820367"
    pub vehicle: String,
    /// The operations (none for a parts list on its own)
    pub tray: Vec<TrayItem>,
    pub warnings: Vec<String>,
    pub parts: Vec<PartsBlock>,
}

/// The one-time use parts window: which options, and whether it's coming in
#[derive(Debug, Clone)]
struct PartsWindow {
    title: String,
    list: Vec<(String, String, Option<Cart>)>,
    phase: anim::Phase,
}

fn cart_key(c: &Cart) -> String {
    format!("{}|{}", c.op_num, c.qual)
}

/// Where a search came from (what's kept, and what shows after)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Source {
    Search,
    /// A Word Search match opened
    Match,
    /// A link in SLTS's page
    Follow,
}

#[derive(Debug, Clone, Default)]
pub struct LaborView {
    pub vin: String,
    /// "2023 Ford F-150 · 1FT… · RO 820367" (for printing)
    pub vehicle: String,
    /// What's kept with the vehicle (the tray, the last search)
    pub saved: Saved,
    /// SLTS's last page (its form is posted back next)
    page: Option<SltsPage>,
    form: SearchForm,
    /// The search type picked, the pick list's choice, what's typed
    kind: String,
    pick: String,
    q: String,
    opened: bool,
    /// What it's waiting on ("Asking SLTS…")
    busy: Option<String>,
    body: Body,
    /// What went wrong, and what to try again
    error: Option<(FordError, Box<LaborMsg>)>,
    /// The last Word Search: its words and matches, and whether "Back to the matches" shows
    matches: Option<(String, Vec<Match>)>,
    back: bool,
    /// Operations whose overlaps are showing
    overlaps: HashSet<String>,
    /// The operation searched for (glided to, outlined)
    flash: Option<String>,
    parts: Option<PartsWindow>,
    parts_cache: HashMap<String, Parts>,
    printing: bool,
    /// Where the results are scrolled to (kept while other tabs show)
    offset: f32,
    /// Answers to an older request are passed over
    token: u64,
}

#[derive(Debug, Clone)]
pub enum LaborMsg {
    /// The tab shows: SLTS opens (once)
    Open,
    Opened(u64, Result<SltsPage, FordError>),
    Kind(Choice),
    KindSet(u64, String, Result<SltsPage, FordError>),
    Pick(Choice),
    Query(String),
    Search,
    Searched(u64, Source, String, Result<SltsPage, FordError>),
    Words(u64, String, Result<(SltsPage, Vec<Match>, String), FordError>),
    OpenMatch(usize),
    BackToMatches,
    /// One of SLTS's own links (an index entry): posted back
    Follow(String, String),
    /// An operation number in SLTS's page: searched for
    FindOp(String),
    OpenUrl(String),
    /// Puts an option in the tray, or takes it out (the report's operation and option)
    Add(usize, usize),
    Remove(usize),
    Clear,
    Copy(String),
    CopyAll,
    Overlaps(String),
    /// One option's one-time use parts (its operation and option), or the tray's
    PartsFor(usize, usize),
    TrayParts,
    PartsLoaded(String, Parts),
    CloseParts,
    PrintParts,
    Print,
    PrintReady(Vec<(String, Parts)>),
    Scrolled(f32),
    /// Where the glide to an operation ended up
    Glided(Option<f32>),
    SignIn,
    Retry,
}

fn op_kind_value(form: &SearchForm) -> String {
    form.types
        .iter()
        .find(|c| c.value.to_ascii_lowercase().starts_with("operation"))
        .map(|c| c.value.clone())
        .unwrap_or_else(|| "Operation".into())
}

impl LaborView {
    pub fn new(vin: impl Into<String>, vehicle: impl Into<String>) -> Self {
        Self {
            vin: vin.into(),
            vehicle: vehicle.into(),
            ..Default::default()
        }
    }

    /// The tab shows: SLTS opens the first time
    pub fn load(&mut self, env: &Env) -> Task<LaborMsg> {
        self.update(LaborMsg::Open, env).0
    }

    /// Back where the results were
    pub fn restore(&self) -> Task<LaborMsg> {
        iced::widget::scrollable::scroll_to(body_id(), iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: self.offset })
    }

    fn started(&mut self, what: &str) -> u64 {
        self.busy = Some(what.into());
        self.error = None;
        self.token += 1;
        self.token
    }

    fn failed(&mut self, e: FordError, retry: LaborMsg) {
        self.busy = None;
        self.error = Some((e, Box::new(retry)));
    }

    fn in_tray(&self, code: &str, qual: &str) -> bool {
        self.saved.tray.iter().any(|t| t.code == code && t.qual == qual)
    }

    /// Whether the typing box shows (Word Search always types)
    fn shows_text(&self) -> bool {
        is_word(&self.kind) || self.form.shows_text()
    }

    fn shows_pick(&self) -> bool {
        !is_word(&self.kind) && self.form.pick.is_some()
    }

    fn placeholder(&self) -> String {
        if is_word(&self.kind) {
            "Words in the operation name, e.g. compressor, water pump…".into()
        } else {
            self.form.placeholder()
        }
    }

    /// A new page from SLTS: its form read
    fn set_page(&mut self, page: SltsPage) {
        self.form = labor::read_form(&page.doc());
        self.page = Some(page);
        if let Some((list, picked)) = &self.form.pick {
            if !list.iter().any(|c| c.value == self.pick) {
                self.pick = if list.iter().any(|c| c.value == self.saved.pick) {
                    self.saved.pick.clone()
                } else {
                    picked.clone()
                };
            }
        }
    }

    /// SLTS's answer shown: its report (the operation asked for glided to), or what else it showed
    fn show(&mut self, env: &Env, q: &str) -> Task<LaborMsg> {
        let Some(page) = &self.page else { return Task::none() };
        let doc = page.doc();
        self.flash = None;
        self.offset = 0.0;
        let top = iced::widget::scrollable::snap_to(body_id(), iced::widget::scrollable::RelativeOffset::START);
        match labor::parse_report(&doc).filter(|r| !r.ops.is_empty()) {
            Some(r) => {
                let want: String = q.to_uppercase().split_whitespace().collect();
                let hit = r.find(&want).map(|i| {
                    // an option's code: its operation's card (a combination's, a supplement's, under its own)
                    r.ops[i].code.clone()
                });
                self.body = Body::Report(r);
                match hit {
                    Some(code) => {
                        self.flash = Some(code.clone());
                        let part = iced::advanced::widget::Id::new(op_name(&code));
                        Task::batch([
                            top,
                            ui_kit::scroll::glide_to(body_id(), ui_kit::scroll::Target::Part(part), MARGIN).map(LaborMsg::Glided),
                        ])
                    }
                    None => top,
                }
            }
            None => {
                let raw = labor::read_raw(&doc);
                let words: String = raw
                    .lines
                    .iter()
                    .flat_map(|l| l.iter().map(|p| p.text.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(300)
                    .collect();
                env.ford.log(&format!(
                    "   labor: {} search gave no operation report; page text: {}",
                    self.form.current,
                    if words.is_empty() { "(none)" } else { &words }
                ));
                self.body = if raw.lines.is_empty() && raw.messages.is_empty() {
                    Body::Nothing("SLTS found nothing for that search.".into())
                } else {
                    Body::Raw(raw)
                };
                top
            }
        }
    }

    fn toggle(&mut self, item: TrayItem) {
        match self.saved.tray.iter().position(|t| t.code == item.code && t.qual == item.qual) {
            Some(i) => {
                self.saved.tray.remove(i);
            }
            None => self.saved.tray.push(item),
        }
    }

    /// Fetches the parts lists not had yet
    fn fetch_parts(&self, env: &Env, carts: Vec<Cart>) -> Task<LaborMsg> {
        let mut seen = HashSet::new();
        let tasks: Vec<Task<LaborMsg>> = carts
            .into_iter()
            .filter(|c| !self.parts_cache.contains_key(&cart_key(c)) && seen.insert(cart_key(c)))
            .map(|c| {
                let (ford, vin, page) = (env.ford.clone(), self.vin.clone(), self.page.clone());
                let key = cart_key(&c);
                Task::perform(async move { ford.slts_parts(&vin, page, &c).await }, move |p| {
                    LaborMsg::PartsLoaded(key.clone(), p)
                })
            })
            .collect();
        Task::batch(tasks)
    }

    fn blocks(&self, list: &[(String, String, Option<Cart>)]) -> Vec<PartsBlock> {
        list.iter()
            .map(|(code, name, cart)| PartsBlock {
                code: code.clone(),
                name: name.clone(),
                parts: match cart {
                    Some(c) => self.parts_cache.get(&cart_key(c)).cloned().unwrap_or_default(),
                    None => Parts {
                        note: "No one-time use parts listed.".into(),
                        ..Default::default()
                    },
                },
            })
            .collect()
    }

    fn tray_list(&self) -> Vec<(String, String, Option<Cart>)> {
        self.saved
            .tray
            .iter()
            .map(|t| (t.code.clone(), join_name(&t.name, &t.qual), t.cart.clone()))
            .collect()
    }

    fn print_tray(&self) -> LaborPrint {
        LaborPrint {
            title: "Labor operations".into(),
            vehicle: self.vehicle.clone(),
            tray: self.saved.tray.clone(),
            warnings: labor::tray_warnings(&self.saved.tray),
            parts: self.blocks(&self.tray_list()),
        }
    }

    pub fn update(&mut self, msg: LaborMsg, env: &Env) -> (Task<LaborMsg>, Vec<Out>) {
        let mut outs = Vec::new();
        let busy = self.busy.is_some();
        let task = match msg {
            LaborMsg::Open => {
                if self.opened || busy {
                    return (Task::none(), outs);
                }
                let token = self.started("Opening SLTS…");
                let (ford, vin, kind) = (env.ford.clone(), self.vin.clone(), self.saved.kind.clone());
                Task::perform(
                    async move {
                        let page = ford.slts_start(&vin).await?;
                        // the search type it was left on
                        let current = labor::read_form(&page.doc()).current;
                        if !kind.is_empty() && !is_word(&kind) && kind != current {
                            let over = [("DropDownList_SearchType".to_string(), kind)];
                            return ford
                                .slts_post_back(&vin, Some(page), &over, "DropDownList_SearchType", "", false)
                                .await;
                        }
                        Ok(page)
                    },
                    move |r: ford::Result<SltsPage>| LaborMsg::Opened(token, r.map_err(FordError::from)),
                )
            }
            LaborMsg::Opened(token, r) => {
                if token != self.token {
                    return (Task::none(), outs);
                }
                match r {
                    Ok(page) => {
                        self.busy = None;
                        self.opened = true;
                        self.set_page(page);
                        self.kind = if is_word(&self.saved.kind) {
                            self.saved.kind.clone()
                        } else if !self.form.current.is_empty() {
                            self.form.current.clone()
                        } else {
                            self.form.types.first().map(|c| c.value.clone()).unwrap_or_default()
                        };
                        self.q = if self.saved.q.is_empty() {
                            self.form.text_value.clone()
                        } else {
                            self.saved.q.clone()
                        };
                        self.body = Body::Hint;
                        // (no focus taken: it may have opened in the background, while something else is typed in)
                        Task::none()
                    }
                    Err(e) => {
                        self.failed(e, LaborMsg::Open);
                        Task::none()
                    }
                }
            }
            LaborMsg::Kind(c) => {
                if busy {
                    return (Task::none(), outs);
                }
                if is_word(&c.value) {
                    self.kind = c.value.clone();
                    self.saved.kind = c.value;
                    self.saved.pick.clear();
                    self.body = Body::Hint;
                    self.back = false;
                    return (text_input::focus(input_id()), outs);
                }
                let token = self.started("Opening SLTS…");
                let (ford, vin, page) = (env.ford.clone(), self.vin.clone(), self.page.clone());
                let value = c.value.clone();
                Task::perform(
                    async move {
                        let over = [("DropDownList_SearchType".to_string(), value)];
                        ford.slts_post_back(&vin, page, &over, "DropDownList_SearchType", "", false).await
                    },
                    move |r| LaborMsg::KindSet(token, c.value.clone(), r.map_err(FordError::from)),
                )
            }
            LaborMsg::KindSet(token, kind, r) => {
                if token != self.token {
                    return (Task::none(), outs);
                }
                match r {
                    Ok(page) => {
                        self.busy = None;
                        self.pick.clear();
                        self.saved.pick.clear();
                        self.set_page(page);
                        self.kind = kind.clone();
                        self.saved.kind = kind;
                        self.body = Body::Hint;
                        self.back = false;
                        if self.shows_text() {
                            text_input::focus(input_id())
                        } else {
                            Task::none()
                        }
                    }
                    Err(e) => {
                        let label = self.form.types.iter().find(|c| c.value == kind).cloned();
                        let retry = LaborMsg::Kind(label.unwrap_or(Choice {
                            value: kind,
                            label: String::new(),
                        }));
                        self.failed(e, retry);
                        Task::none()
                    }
                }
            }
            LaborMsg::Pick(c) => {
                self.pick = c.value;
                return self.update(LaborMsg::Search, env);
            }
            LaborMsg::Query(q) => {
                self.q = q;
                Task::none()
            }
            LaborMsg::Search => {
                if busy {
                    return (Task::none(), outs);
                }
                let q = self.q.trim().to_string();
                if self.shows_text() && q.is_empty() {
                    return (text_input::focus(input_id()), outs);
                }
                if is_word(&self.kind) {
                    let token = self.started("Asking SLTS…");
                    let (ford, vin, page) = (env.ford.clone(), self.vin.clone(), self.page.clone());
                    let words = q.clone();
                    return (
                        Task::perform(async move { ford.slts_word_search(&vin, page, &words).await }, move |r| {
                            LaborMsg::Words(token, q.clone(), r.map_err(FordError::from))
                        }),
                        outs,
                    );
                }
                let token = self.started("Asking SLTS…");
                let (ford, vin, page) = (env.ford.clone(), self.vin.clone(), self.page.clone());
                let (kind, pick, typed) = (self.kind.clone(), self.pick.clone(), q.clone());
                Task::perform(
                    async move {
                        // (SLTS opened again first, after something went wrong)
                        let mut page = match page {
                            Some(p) => p,
                            None => ford.slts_start(&vin).await?,
                        };
                        if labor::read_form(&page.doc()).current != kind {
                            let over = [("DropDownList_SearchType".to_string(), kind.clone())];
                            page = ford
                                .slts_post_back(&vin, Some(page), &over, "DropDownList_SearchType", "", false)
                                .await?;
                        }
                        let form = labor::read_form(&page.doc());
                        let mut over = vec![("DropDownList_SearchType".to_string(), kind.clone())];
                        if let Some((list, picked)) = &form.pick {
                            let v = if list.iter().any(|c| c.value == pick) {
                                pick
                            } else {
                                picked.clone()
                            };
                            over.push(("DropDownList_SearchFor".into(), v));
                        }
                        if form.shows_text() {
                            // SLTS's operation search takes the base number (19700); a full code (19700A4) is looked up
                            // and glided to
                            let v = if kind.to_ascii_lowercase().starts_with("operation") {
                                let code: String = typed.to_uppercase().split_whitespace().collect();
                                labor::base_number(&code)
                            } else {
                                typed
                            };
                            over.push((form.text_name(), v));
                        }
                        ford.slts_post_back(&vin, Some(page), &over, "", "", true).await
                    },
                    move |r| LaborMsg::Searched(token, Source::Search, q.clone(), r.map_err(FordError::from)),
                )
            }
            LaborMsg::Searched(token, from, q, r) => {
                if token != self.token {
                    return (Task::none(), outs);
                }
                match r {
                    Ok(page) => {
                        self.busy = None;
                        self.set_page(page);
                        match from {
                            Source::Search => {
                                self.saved.kind = self.kind.clone();
                                self.saved.pick = if self.form.pick.is_some() {
                                    self.pick.clone()
                                } else {
                                    String::new()
                                };
                                self.saved.q = if self.form.shows_text() { q.clone() } else { String::new() };
                                self.back = false;
                            }
                            Source::Match => self.back = self.matches.is_some(),
                            Source::Follow => self.back = false,
                        }
                        self.show(env, &q)
                    }
                    Err(e) => {
                        let retry = match from {
                            Source::Search => LaborMsg::Search,
                            // (opened again from the matches)
                            Source::Match | Source::Follow => LaborMsg::Search,
                        };
                        self.failed(e, retry);
                        Task::none()
                    }
                }
            }
            LaborMsg::Words(token, q, r) => {
                if token != self.token {
                    return (Task::none(), outs);
                }
                match r {
                    Ok((page, list, said)) => {
                        self.busy = None;
                        self.opened = true;
                        self.set_page(page);
                        self.saved.kind = self.kind.clone();
                        self.saved.pick.clear();
                        self.saved.q = q.clone();
                        self.back = false;
                        self.flash = None;
                        if list.is_empty() {
                            if !said.is_empty() {
                                env.ford.log(&format!(
                                    "   labor: word search page said: {}",
                                    said.chars().take(300).collect::<String>()
                                ));
                            }
                            self.matches = None;
                            self.body = Body::Nothing(format!("SLTS found no operations named “{q}”."));
                        } else {
                            self.matches = Some((q, list));
                            self.body = Body::Matches;
                        }
                        iced::widget::scrollable::snap_to(body_id(), iced::widget::scrollable::RelativeOffset::START)
                    }
                    Err(e) => {
                        self.failed(e, LaborMsg::Search);
                        Task::none()
                    }
                }
            }
            LaborMsg::OpenMatch(i) => {
                let Some(m) = self.matches.as_ref().and_then(|(_, l)| l.get(i)).cloned() else {
                    return (Task::none(), outs);
                };
                let code = m.code();
                if busy || code.is_empty() {
                    return (Task::none(), outs);
                }
                // searched by the operation's base number (18124), as the Operation Number search does, then glided to
                let token = self.started("Asking SLTS…");
                let (ford, vin, page) = (env.ford.clone(), self.vin.clone(), self.page.clone());
                let op = op_kind_value(&self.form);
                let c = code.clone();
                Task::perform(
                    async move {
                        let mut page = match page {
                            Some(p) => p,
                            None => ford.slts_start(&vin).await?,
                        };
                        if labor::read_form(&page.doc()).current != op {
                            let over = [("DropDownList_SearchType".to_string(), op.clone())];
                            page = ford
                                .slts_post_back(&vin, Some(page), &over, "DropDownList_SearchType", "", false)
                                .await?;
                        }
                        let field = labor::read_form(&page.doc()).text_name();
                        let over = [("DropDownList_SearchType".to_string(), op), (field, labor::base_number(&c))];
                        ford.slts_post_back(&vin, Some(page), &over, "", "", true).await
                    },
                    move |r| LaborMsg::Searched(token, Source::Match, code.clone(), r.map_err(FordError::from)),
                )
            }
            LaborMsg::BackToMatches => {
                if self.matches.is_some() {
                    self.body = Body::Matches;
                    self.back = false;
                    self.flash = None;
                }
                iced::widget::scrollable::snap_to(body_id(), iced::widget::scrollable::RelativeOffset::START)
            }
            LaborMsg::Follow(target, arg) => {
                if busy {
                    return (Task::none(), outs);
                }
                let token = self.started("Asking SLTS…");
                let (ford, vin, page) = (env.ford.clone(), self.vin.clone(), self.page.clone());
                Task::perform(
                    async move { ford.slts_post_back(&vin, page, &[], &target, &arg, false).await },
                    move |r| LaborMsg::Searched(token, Source::Follow, String::new(), r.map_err(FordError::from)),
                )
            }
            LaborMsg::FindOp(code) => {
                self.kind = op_kind_value(&self.form);
                self.q = code;
                return self.update(LaborMsg::Search, env);
            }
            LaborMsg::OpenUrl(url) => {
                let full = if url.get(..4).is_some_and(|h| h.eq_ignore_ascii_case("http")) {
                    url
                } else {
                    labor::slts_url(&url).unwrap_or(url)
                };
                let _ = crate::open_external(&full);
                Task::none()
            }
            LaborMsg::Add(o, x) => {
                if let Body::Report(r) = &self.body {
                    if let Some(item) = r
                        .ops
                        .get(o)
                        .and_then(|op| op.options.get(x).map(|opt| labor::tray_item(r, op, opt)))
                    {
                        self.toggle(item);
                    }
                }
                Task::none()
            }
            LaborMsg::Remove(i) => {
                if i < self.saved.tray.len() {
                    self.saved.tray.remove(i);
                }
                Task::none()
            }
            LaborMsg::Clear => {
                self.saved.tray.clear();
                Task::none()
            }
            LaborMsg::Copy(code) => {
                outs.push(Out::Copy(code, "Op code".into()));
                Task::none()
            }
            LaborMsg::CopyAll => {
                let codes = self.saved.tray.iter().map(|t| t.code.as_str()).collect::<Vec<_>>().join(", ");
                if !codes.is_empty() {
                    outs.push(Out::Copy(codes, "Op codes".into()));
                }
                Task::none()
            }
            LaborMsg::Overlaps(code) => {
                if !self.overlaps.remove(&code) {
                    self.overlaps.insert(code);
                }
                Task::none()
            }
            LaborMsg::PartsFor(o, x) => {
                let Body::Report(r) = &self.body else { return (Task::none(), outs) };
                let Some((op, opt)) = r.ops.get(o).and_then(|op| op.options.get(x).map(|opt| (op, opt))) else {
                    return (Task::none(), outs);
                };
                let list = vec![(opt.code.clone(), join_name(&op.name, &opt.qual), opt.cart.clone())];
                let carts = opt.cart.clone().into_iter().collect();
                self.parts = Some(PartsWindow {
                    title: format!("One-time use parts · {}", opt.code),
                    list,
                    phase: anim::Phase::Entering(Instant::now()),
                });
                self.fetch_parts(env, carts)
            }
            LaborMsg::TrayParts => {
                if self.saved.tray.is_empty() {
                    return (Task::none(), outs);
                }
                let carts = self.saved.tray.iter().filter_map(|t| t.cart.clone()).collect();
                self.parts = Some(PartsWindow {
                    title: "One-time use parts · selected operations".into(),
                    list: self.tray_list(),
                    phase: anim::Phase::Entering(Instant::now()),
                });
                self.fetch_parts(env, carts)
            }
            LaborMsg::PartsLoaded(key, p) => {
                self.parts_cache.insert(key, p);
                Task::none()
            }
            LaborMsg::CloseParts => {
                self.parts = None;
                Task::none()
            }
            LaborMsg::PrintParts => {
                if let Some(w) = &self.parts {
                    outs.push(Out::Print(LaborPrint {
                        title: w.title.clone(),
                        vehicle: self.vehicle.clone(),
                        tray: Vec::new(),
                        warnings: Vec::new(),
                        parts: self.blocks(&w.list),
                    }));
                }
                Task::none()
            }
            LaborMsg::Print => {
                if self.saved.tray.is_empty() || self.printing {
                    return (Task::none(), outs);
                }
                let missing: Vec<Cart> = self
                    .saved
                    .tray
                    .iter()
                    .filter_map(|t| t.cart.clone())
                    .filter(|c| !self.parts_cache.contains_key(&cart_key(c)))
                    .collect();
                if missing.is_empty() {
                    outs.push(Out::Print(self.print_tray()));
                    return (Task::none(), outs);
                }
                // the one-time use parts first
                self.printing = true;
                let (ford, vin, page) = (env.ford.clone(), self.vin.clone(), self.page.clone());
                Task::perform(
                    async move {
                        let mut got = Vec::new();
                        for c in missing {
                            if got.iter().any(|(k, _): &(String, Parts)| *k == cart_key(&c)) {
                                continue;
                            }
                            got.push((cart_key(&c), ford.slts_parts(&vin, page.clone(), &c).await));
                        }
                        got
                    },
                    LaborMsg::PrintReady,
                )
            }
            LaborMsg::PrintReady(got) => {
                self.printing = false;
                self.parts_cache.extend(got);
                outs.push(Out::Print(self.print_tray()));
                Task::none()
            }
            LaborMsg::Scrolled(y) => {
                self.offset = y;
                Task::none()
            }
            LaborMsg::Glided(y) => {
                if let Some(y) = y {
                    self.offset = y;
                }
                Task::none()
            }
            LaborMsg::SignIn => {
                outs.push(Out::SignIn);
                Task::none()
            }
            LaborMsg::Retry => {
                let Some((_, retry)) = self.error.take() else {
                    return (Task::none(), outs);
                };
                // SLTS from the start (its page may be what went wrong)
                self.page = None;
                if matches!(*retry, LaborMsg::Open) {
                    self.opened = false;
                }
                return self.update(*retry, env);
            }
        };
        (task, outs)
    }
}

fn join_name(name: &str, qual: &str) -> String {
    [name, qual]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ")
}

// ---------- the view ----------

const AMBER: Color = Color::from_rgb(0.85, 0.47, 0.02);
const GREEN: Color = Color::from_rgb(0.09, 0.64, 0.29);

/// A combination's or supplement's card, inside its operation's
fn inner_card(theme: &Theme) -> container::Style {
    let c = style::colors(theme);
    container::Style {
        background: Some(theme.palette().background.into()),
        border: Border {
            color: c.line,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

/// Ford's caution (the tray's warnings)
fn caution(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color { a: 0.12, ..AMBER })),
        border: Border {
            color: Color { a: 0.6, ..AMBER },
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

/// A thin box around the options' table
fn table_box(theme: &Theme) -> container::Style {
    let c = style::colors(theme);
    container::Style {
        border: Border {
            color: c.line,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

fn table_head(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(style::colors(theme).hover.into()),
        ..Default::default()
    }
}

/// A link that's only its text (SLTS's notes and index entries)
fn link_button(theme: &Theme, status: button::Status) -> button::Style {
    let c = style::colors(theme);
    button::Style {
        background: None,
        text_color: if matches!(status, button::Status::Hovered) {
            theme.palette().primary
        } else {
            c.accent
        },
        border: Border::default(),
        ..Default::default()
    }
}

/// The op code: copied when clicked
fn code_button<'a>(code: &'a str, size: f32) -> Element<'a, LaborMsg> {
    button(text(code).font(style::MONO).size(size))
        .padding(0)
        .style(|theme: &Theme, status| button::Style {
            background: None,
            text_color: if matches!(status, button::Status::Hovered) {
                theme.palette().primary
            } else {
                theme.palette().text
            },
            ..Default::default()
        })
        .on_press(LaborMsg::Copy(code.to_string()))
        .into()
}

fn small<'a>(label: &'a str, msg: Option<LaborMsg>) -> Element<'a, LaborMsg> {
    button(text(label).size(12))
        .padding([3, 9])
        .style(style::ghost)
        .on_press_maybe(msg)
        .into()
}

fn card<'a>(content: impl Into<Element<'a, LaborMsg>>) -> Element<'a, LaborMsg> {
    container(content).padding(18).width(Length::Fill).style(style::card).into()
}

/// The option rows' columns
const W_CODE: f32 = 104.0;
const W_WARRANTY: f32 = 70.0;
const W_CP: f32 = 90.0;
const W_PARTS: f32 = 58.0;
const W_ADD: f32 = 80.0;

fn option_rows<'a>(v: &'a LaborView, oi: usize, op: &'a labor::Op) -> Element<'a, LaborMsg> {
    let head = |s: &'a str| text(s).size(11).font(style::BOLD).style(style::muted);
    let mut rows = Column::new().push(
        container(
            row![
                head("OPTION").width(Length::Fill),
                head("OP CODE").width(W_CODE),
                head("WARRANTY").width(W_WARRANTY).align_x(Alignment::End),
                head("CUSTOMER PAY").width(W_CP).align_x(Alignment::End),
                Space::with_width(W_PARTS + W_ADD + 12.0),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([5, 10])
        .style(table_head),
    );
    for (xi, opt) in op.options.iter().enumerate() {
        let w = num(&opt.time);
        let on = v.in_tray(&opt.code, &opt.qual);
        let add = if on {
            button(text("✓ Added").size(12).color(GREEN)).padding([3, 9]).style(style::ghost)
        } else {
            button(text("+ Add").size(12).font(style::BOLD))
                .padding([3, 9])
                .style(button::primary)
        }
        .on_press(LaborMsg::Add(oi, xi));
        let parts: Element<'a, LaborMsg> = if opt.cart.is_some() {
            small("Parts", Some(LaborMsg::PartsFor(oi, xi)))
        } else {
            Space::with_width(0).into()
        };
        let row = row![
            text(if opt.qual.is_empty() { "All" } else { opt.qual.as_str() })
                .size(13)
                .font(style::BOLD)
                .width(Length::Fill),
            container(code_button(&opt.code, 13.0)).width(W_CODE),
            text(
                w.map(hrs)
                    .unwrap_or_else(|| if opt.time.is_empty() { "—".into() } else { opt.time.clone() })
            )
            .size(13)
            .font(style::BOLD)
            .width(W_WARRANTY)
            .align_x(Alignment::End),
            text(w.map(|n| hrs(n * CUSTOMER_PAY)).unwrap_or_else(|| "—".into()))
                .size(13)
                .style(style::muted)
                .width(W_CP)
                .align_x(Alignment::End),
            container(parts).width(W_PARTS).align_x(Alignment::End),
            container(add).width(W_ADD).align_x(Alignment::End),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        rows = rows.push(container(row).padding([4, 10]));
    }
    container(rows).style(table_box).width(Length::Fill).into()
}

fn op_card<'a>(v: &'a LaborView, r: &'a Report, i: usize, nested: bool) -> Element<'a, LaborMsg> {
    let op = &r.ops[i];
    let mut head = Row::new().spacing(8).align_y(Alignment::Center);
    match op.kind {
        OpKind::Combo => head = head.push(pill("COMBINATION", [0x1a, 0x5f, 0xb4])),
        OpKind::Supp => head = head.push(pill("SUPPLEMENT", [0xd9, 0x77, 0x06])),
        OpKind::Base => {}
    }
    head = head.push(text(op.name.as_str()).size(14).font(style::BOLD));
    for p in &op.parts {
        head = head.push(pill(p.as_str(), [0x5d, 0x66, 0x70]));
    }
    let mut c = column![head.wrap()].spacing(6);
    if !op.desc.is_empty() {
        c = c.push(text(op.desc.join(" ")).size(12.5).style(style::muted));
    }
    if op.options.is_empty() {
        c = c.push(text("No time listed for this vehicle.").size(12.5).style(style::muted));
    } else {
        c = c.push(option_rows(v, i, op));
    }
    for n in &op.notes {
        c = c.push(if n.url.is_empty() {
            Element::from(text(n.text.as_str()).size(12))
        } else {
            button(text(n.text.as_str()).size(12))
                .padding(0)
                .style(link_button)
                .on_press(LaborMsg::OpenUrl(n.url.clone()))
                .into()
        });
    }
    if !op.overlaps.is_empty() {
        let open = v.overlaps.contains(&op.code);
        c = c.push(
            button(
                text(format!("{} Overlaps (don't claim together)", if open { "▾" } else { "▸" }))
                    .size(12)
                    .font(style::BOLD)
                    .color(AMBER),
            )
            .padding(0)
            .style(link_button)
            .on_press(LaborMsg::Overlaps(op.code.clone())),
        );
        if open {
            let mut list = Column::new().spacing(3).padding([0, 14]);
            for o in &op.overlaps {
                list = list.push(text(o.as_str()).size(12).style(style::muted));
            }
            c = c.push(list);
        }
    }
    let kids = r.children(i);
    if !kids.is_empty() {
        let mut col = Column::new().spacing(8);
        for k in kids {
            col = col.push(op_card(v, r, k, true));
        }
        c = c.push(container(col).padding(iced::Padding {
            top: 4.0,
            right: 0.0,
            bottom: 0.0,
            left: 12.0,
        }));
    }
    let flash = v.flash.as_deref() == Some(op.code.as_str());
    container(c)
        .padding(if nested { [8, 12] } else { [10, 14] })
        .width(Length::Fill)
        .style(move |t: &Theme| {
            if flash {
                style::card_highlight(t)
            } else if nested {
                inner_card(t)
            } else {
                style::card(t)
            }
        })
        .id(iced::widget::container::Id::new(op_name(&op.code)))
        .into()
}

fn report<'a>(v: &'a LaborView, r: &'a Report) -> Element<'a, LaborMsg> {
    let mut c = Column::new().spacing(10);
    if !r.title.is_empty() {
        c = c.push(text(r.title.as_str()).size(13).font(style::BOLD).style(style::muted));
    }
    let mut group = "";
    for i in r.roots() {
        let g = r.ops[i].group.as_str();
        if !g.is_empty() && g != group {
            group = g;
            c = c.push(container(text(g.to_uppercase()).size(12).font(style::BOLD).style(style::muted)).padding([8, 0]));
        }
        c = c.push(op_card(v, r, i, false));
    }
    c.into()
}

fn raw<'a>(raw: &'a Raw) -> Element<'a, LaborMsg> {
    let mut c = Column::new().spacing(10);
    if !raw.messages.is_empty() {
        c = c.push(
            container(text(raw.messages.join(" ")).size(13))
                .padding([10, 14])
                .width(Length::Fill)
                .style(caution),
        );
    }
    if raw.lines.is_empty() {
        c = c.push(card(text("SLTS found nothing for that search.").style(style::muted)));
        return c.into();
    }
    let mut lines = Column::new().spacing(4);
    for line in &raw.lines {
        let mut r = Row::new().align_y(Alignment::Center);
        for p in line {
            r = r.push(match &p.link {
                None => Element::from(text(p.text.as_str()).size(13)),
                Some(l) => {
                    let msg = match l {
                        RawLink::Search(code) => LaborMsg::FindOp(code.clone()),
                        RawLink::PostBack(t, a) => LaborMsg::Follow(t.clone(), a.clone()),
                        RawLink::Url(u) => LaborMsg::OpenUrl(u.clone()),
                    };
                    button(text(p.text.as_str()).size(13))
                        .padding(0)
                        .style(link_button)
                        .on_press(msg)
                        .into()
                }
            });
        }
        lines = lines.push(r.wrap());
    }
    c = c.push(container(lines).padding([12, 16]).width(Length::Fill).style(style::card));
    c.into()
}

fn matches_list<'a>(v: &'a LaborView) -> Element<'a, LaborMsg> {
    let Some((q, list)) = &v.matches else {
        return card(text("No matches.").style(style::muted));
    };
    let mut c = column![
        text(format!(
            "{} operation{} matching “{q}”: pick one to see its times",
            list.len(),
            if list.len() == 1 { "" } else { "s" }
        ))
        .size(13)
        .font(style::BOLD)
        .style(style::muted)
    ]
    .spacing(6)
    .max_width(760);
    for (i, m) in list.iter().enumerate() {
        let (code, name) = m.split();
        let mut r = Row::new().spacing(10).align_y(Alignment::Center);
        if let Some(code) = code {
            r = r.push(text(code).font(style::MONO).size(13));
        }
        r = r.push(text(name).size(13.5).width(Length::Fill));
        r = r.push(text("View ›").size(12).font(style::BOLD).style(text::primary));
        c = c.push(
            button(r)
                .width(Length::Fill)
                .padding([9, 14])
                .style(style::toggle(false))
                .on_press(LaborMsg::OpenMatch(i)),
        );
    }
    c.into()
}

fn hint<'a>() -> Element<'a, LaborMsg> {
    card(
        column![
            text("Search the way SLTS does: by section, operation number (e.g. 19700), word, part number or the alphabetic index.")
                .size(14),
            text(format!(
                "Press + Add on an option to put it in your list on the right: it adds up the warranty time and the customer \
                 pay time (warranty × {CUSTOMER_PAY}), checks Ford's overlaps, and lists the one-time use parts."
            ))
            .style(style::muted),
        ]
        .spacing(8),
    )
}

fn tray<'a>(v: &'a LaborView) -> Element<'a, LaborMsg> {
    let t = &v.saved.tray;
    let mut head = row![
        text("SELECTED OPERATIONS").size(12).font(style::BOLD).style(style::muted),
        horizontal_space()
    ]
    .align_y(Alignment::Center);
    if !t.is_empty() {
        head = head.push(small("Clear", Some(LaborMsg::Clear)));
    }
    let mut c = column![head].spacing(8);
    if t.is_empty() {
        c = c.push(text("Press + Add on an option to add it up here.").size(12.5).style(style::muted));
        return container(scrollable(c.padding([12, 14]))).width(330).style(style::card).into();
    }
    let mut items = Column::new().spacing(6);
    for (i, item) in t.iter().enumerate() {
        let time = num(&item.time)
            .map(hrs)
            .unwrap_or_else(|| if item.time.is_empty() { "—".into() } else { item.time.clone() });
        items = items.push(
            column![
                row![
                    code_button(&item.code, 13.0),
                    horizontal_space(),
                    text(time).size(13).font(style::BOLD),
                    button(text("✕").size(11))
                        .padding([0, 4])
                        .style(link_button)
                        .on_press(LaborMsg::Remove(i)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text(join_name(&item.name, &item.qual)).size(12).style(style::muted),
            ]
            .spacing(2),
        );
    }
    c = c.push(items);
    let (w, actual) = labor::tray_total(t);
    let mut totals = column![
        iced::widget::horizontal_rule(2),
        row![
            text("Warranty").size(13),
            horizontal_space(),
            text(format!("{} hrs", hrs(w))).size(16).font(style::BOLD)
        ]
        .align_y(Alignment::Center),
        row![
            text(format!("Customer pay (×{CUSTOMER_PAY})")).size(13),
            horizontal_space(),
            text(format!("{} hrs", hrs(w * CUSTOMER_PAY)))
                .size(16)
                .font(style::BOLD)
                .style(text::primary)
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(4);
    if actual > 0 {
        totals = totals.push(
            text(format!(
                "{actual} operation{} without a set time (actual time) not included.",
                if actual == 1 { "" } else { "s" }
            ))
            .size(11.5)
            .style(style::muted),
        );
    }
    c = c.push(totals);
    let warns = labor::tray_warnings(t);
    if !warns.is_empty() {
        let mut w = Column::new().spacing(4);
        for x in warns {
            w = w.push(text(x).size(12));
        }
        c = c.push(container(w).padding([6, 10]).width(Length::Fill).style(caution));
    }
    c = c.push(
        row![
            small("Copy op codes", Some(LaborMsg::CopyAll)),
            small("One-time use parts", Some(LaborMsg::TrayParts)),
            button(text(if v.printing { "Getting parts…" } else { "Print" }).size(12).font(style::BOLD))
                .padding([3, 12])
                .style(button::primary)
                .on_press_maybe((!v.printing).then_some(LaborMsg::Print)),
        ]
        .spacing(6)
        .wrap(),
    );
    container(scrollable(c.padding([12, 14]))).width(330).style(style::card).into()
}

fn parts_table<'a>(p: &'a Parts, loading: bool) -> Element<'a, LaborMsg> {
    if p.rows.is_empty() {
        let note = if loading {
            "Asking SLTS…"
        } else if p.note.is_empty() {
            "No one-time use parts listed."
        } else {
            p.note.as_str()
        };
        return text(note).size(12.5).style(style::muted).into();
    }
    let cols = p.head.len().max(p.rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
    let portion = |j: usize| if j == 1 && cols > 2 { 3 } else { 1 };
    let mut t = Column::new();
    if !p.head.is_empty() {
        let mut r = Row::new().spacing(8);
        for j in 0..cols {
            r = r.push(
                text(p.head.get(j).map(|h| h.to_uppercase()).unwrap_or_default())
                    .size(11)
                    .font(style::BOLD)
                    .style(style::muted)
                    .width(Length::FillPortion(portion(j))),
            );
        }
        t = t.push(container(r).padding([5, 8]).style(table_head));
    }
    for row in &p.rows {
        let mut r = Row::new().spacing(8);
        for j in 0..cols {
            r = r.push(
                text(row.get(j).cloned().unwrap_or_default())
                    .size(12.5)
                    .width(Length::FillPortion(portion(j))),
            );
        }
        t = t.push(container(r).padding([5, 8]));
    }
    container(t).style(table_box).width(Length::Fill).into()
}

fn parts_window<'a>(v: &'a LaborView, w: &'a PartsWindow) -> Element<'a, LaborMsg> {
    let loading = |c: &Option<Cart>| c.as_ref().is_some_and(|c| !v.parts_cache.contains_key(&cart_key(c)));
    let any_loading = w.list.iter().any(|(_, _, c)| loading(c));
    let head = row![
        text(w.title.as_str()).size(15).font(style::BOLD),
        horizontal_space(),
        small("Print", (!any_loading).then_some(LaborMsg::PrintParts)),
        small("✕", Some(LaborMsg::CloseParts)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let mut blocks = Column::new().spacing(14);
    for (code, name, cart) in &w.list {
        let parts: Element<'a, LaborMsg> = match cart {
            Some(c) => match v.parts_cache.get(&cart_key(c)) {
                Some(p) => parts_table(p, false),
                None => text("Asking SLTS…").size(12.5).style(style::muted).into(),
            },
            None => text("No one-time use parts listed.").size(12.5).style(style::muted).into(),
        };
        blocks = blocks.push(
            column![
                row![
                    text(code.as_str()).font(style::MONO).size(13),
                    text(name.as_str()).size(13).font(style::BOLD)
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                parts
            ]
            .spacing(6),
        );
    }
    container(column![head, scrollable(blocks.padding([0, 4])).height(Length::Shrink)].spacing(12))
        .padding([14, 18])
        .width(760)
        .max_height(560)
        .style(style::floating_card)
        .into()
}

pub fn view(v: &LaborView) -> Element<'_, LaborMsg> {
    // the search (mirrors SLTS's own)
    let picked_kind = v.form.types.iter().find(|c| c.value == v.kind).cloned();
    let mut bar = Row::new().spacing(10).align_y(Alignment::Center).push(
        pick_list(v.form.types.as_slice(), picked_kind, LaborMsg::Kind)
            .placeholder("Search type")
            .padding(8)
            .text_size(13)
            .width(200),
    );
    if v.shows_pick() {
        if let Some((list, _)) = &v.form.pick {
            let picked = list.iter().find(|c| c.value == v.pick).cloned();
            bar = bar.push(
                pick_list(list.as_slice(), picked, LaborMsg::Pick)
                    .placeholder(if v.form.label.is_empty() { "Choose" } else { v.form.label.as_str() })
                    .padding(8)
                    .text_size(13)
                    .width(Length::Fill),
            );
        }
    }
    if v.shows_text() {
        bar = bar.push(
            text_input(&v.placeholder(), &v.q)
                .id(input_id())
                .on_input(LaborMsg::Query)
                .on_submit(LaborMsg::Search)
                .padding(8)
                .size(14)
                .width(Length::Fill),
        );
    } else if !v.shows_pick() {
        bar = bar.push(horizontal_space());
    }
    bar = bar.push(
        button(text("Search").font(style::BOLD))
            .padding([8, 18])
            .style(button::primary)
            .on_press_maybe((v.busy.is_none() && v.opened).then_some(LaborMsg::Search)),
    );

    // the results
    let mut body = Column::new().spacing(10);
    if v.back && v.busy.is_none() && v.error.is_none() {
        body = body.push(small("‹ Back to the matches", Some(LaborMsg::BackToMatches)));
    }
    if let Some(what) = &v.busy {
        body = body.push(card(
            row![
                anim::dot(Color::from_rgb8(0x1a, 0x5f, 0xb4), true),
                text(what.as_str()).style(style::muted)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        ));
    } else if let Some((e, _)) = &v.error {
        let signed_out = *e == FordError::SignedOut;
        let mut actions = row![].spacing(8);
        actions = actions.push(if signed_out {
            button(text("Sign in to PTS"))
                .padding([6, 14])
                .style(button::primary)
                .on_press(LaborMsg::SignIn)
        } else {
            button(text("Try again"))
                .padding([6, 14])
                .style(button::primary)
                .on_press(LaborMsg::Retry)
        });
        if signed_out {
            actions = actions.push(
                button(text("Try again"))
                    .padding([6, 14])
                    .style(style::ghost)
                    .on_press(LaborMsg::Retry),
            );
        }
        body = body.push(card(
            column![
                text(if signed_out { "Sign in to PTS" } else { "Couldn't get labor times" })
                    .size(16)
                    .font(style::BOLD),
                text(if signed_out {
                    "Labor times need your PTS login.".to_string()
                } else {
                    e.text()
                })
                .style(style::muted),
                actions,
            ]
            .spacing(10),
        ));
    } else {
        body = body.push(match &v.body {
            Body::Hint => hint(),
            Body::Report(r) => report(v, r),
            Body::Raw(r) => raw(r),
            Body::Matches => matches_list(v),
            Body::Nothing(m) => card(text(m.as_str()).style(style::muted)),
        });
    }

    let page = column![
        container(bar).padding([12, 16]).width(Length::Fill).style(style::panel),
        row![
            scrollable(
                container(body.padding(iced::Padding {
                    top: 0.0,
                    right: 6.0,
                    bottom: 40.0,
                    left: 0.0
                }))
                .width(Length::Fill)
            )
            .id(body_id())
            .on_scroll(|vp| LaborMsg::Scrolled(vp.absolute_offset().y))
            .height(Length::Fill)
            .width(Length::Fill),
            container(tray(v)).height(Length::Fill),
        ]
        .spacing(16)
        .padding([12, 16])
        .height(Length::Fill),
    ]
    .height(Length::Fill);

    // (always in a stack, the parts window or nothing over it: the page keeps its place in the widget tree, so where
    // it's scrolled to isn't lost when the window opens or closes)
    let over: Element<'_, LaborMsg> = match &v.parts {
        Some(w) => anim::modal(parts_window(v, w), w.phase, LaborMsg::CloseParts).into(),
        None => Space::new(0, 0).into(),
    };
    stack![page, over].into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(join_name("Compressor", "2.0L"), "Compressor · 2.0L");
        assert_eq!(join_name("Compressor", ""), "Compressor");
        let f = SearchForm {
            types: vec![Choice {
                value: "OperationNumber".into(),
                label: "Operation Number".into(),
            }],
            ..Default::default()
        };
        assert_eq!(op_kind_value(&f), "OperationNumber");
        assert_eq!(op_kind_value(&SearchForm::default()), "Operation");
    }
}
