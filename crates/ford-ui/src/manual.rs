//! The Workshop Manual tab (the Electron app's `manual/viewer/`): Ford's contents on the left (folders and
//! procedures, "Only this vehicle", a search box; it slides away with Contents), the procedures opened as tabs across
//! the top, and the one picked drawn in the app's style ([`crate::document`]). Clicking a procedure in the contents
//! shows it in the tab showing; a link inside a procedure opens it in a tab of its own. Click a picture to see it
//! bigger. A procedure's pinpoint tests open over it, one step at a time ([`crate::pinpoint`]); opening one and each
//! answer are recorded to Vehicle History, as Ford's own manual does.
//!
//! (Next: jumping to a part of a procedure, PC/ED, keeping open procedures.)

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use ford::doc::{Block, Pinpoint, ProcLink, find_test};
use ford::manual::{self, PinpointReport, ProcPage, StepReport, Toc, TocKind, TocNode};
use iced::widget::scrollable::RelativeOffset;
use iced::widget::{
    Column, Row, Space, button, checkbox, column, container, horizontal_space, image, mouse_area, row, stack, text, text_input,
};
use iced::{Alignment, Color, Element, Length, Task, Theme};
use ui_kit::scroll::{self, scrollable};
use ui_kit::{anim, pill, style};

use crate::document::{self, DocMsg, Pic};
use crate::pinpoint::{self, Rec, TestMsg, TestState};
use crate::{Env, FordError, Out};

/// How wide the contents are
const TOC_W: f32 = 360.0;
/// The most pictures fetched for one procedure, and the most search results listed
const MAX_PICS: usize = 80;
const MAX_HITS: usize = 200;

/// One procedure opened
#[derive(Debug, Clone)]
pub struct ProcTab {
    pub key: String,
    pub link: ProcLink,
    pub title: String,
    /// Where it is in the contents ("Removal and Installation")
    pub section: String,
    pub loading: bool,
    pub error: Option<FordError>,
    pub page: Option<ProcPage>,
    pub pics: HashMap<String, Pic>,
    /// Its pinpoint tests: where each is (by letter); the one open over it (kept while it slides away); every step
    /// shown at once instead of one at a time
    pub tests: HashMap<String, TestState>,
    pub test: Option<String>,
    pub test_open: bool,
    pub all_steps: bool,
    /// Where it was scrolled before each jump within it (Back goes there)
    pub back: Vec<f32>,
    /// A part it was opened at: gone to again once its pictures are in (they move things down), if that's soon
    pub settle: Option<(String, Instant)>,
    /// Where it's scrolled to, and its open test (shown again there)
    pub offset: f32,
    pub test_offset: f32,
}

#[derive(Debug, Clone)]
pub struct ManualView {
    pub vin: String,
    pub toc: Option<Toc>,
    /// The contents as shown (this vehicle's only, unless all options are shown)
    pub shown: Vec<TocNode>,
    pub loading: bool,
    pub error: Option<FordError>,
    pub all_options: bool,
    /// The folders open (by their path in the contents)
    pub open: HashSet<String>,
    pub query: String,
    pub contents_open: bool,
    pub docs: Vec<ProcTab>,
    pub active: Option<String>,
    /// A picture shown big (its address)
    pub zoom: Option<String>,
    /// PTS hadn't set the vehicle up: setting it up was tried (once), and is happening now
    pub set_up_tried: bool,
    pub setting_up: bool,
    /// Where the contents are scrolled to
    pub contents_offset: f32,
}

impl Default for ManualView {
    fn default() -> Self {
        Self {
            vin: String::new(),
            toc: None,
            shown: Vec::new(),
            loading: false,
            error: None,
            all_options: false,
            open: HashSet::new(),
            query: String::new(),
            contents_open: true,
            docs: Vec::new(),
            active: None,
            zoom: None,
            set_up_tried: false,
            setting_up: false,
            contents_offset: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ManualMsg {
    Refresh,
    TocLoaded(Result<Toc, FordError>),
    /// Open or close a folder (its path)
    Toggle(String),
    Query(String),
    AllOptions(bool),
    /// Open a procedure: in the tab showing, or a new one; its title and where it is
    Open {
        link: ProcLink,
        title: String,
        section: String,
        new_tab: bool,
    },
    /// A document elsewhere (a PDF in the contents)
    OpenUrl(String),
    Loaded(String, Result<ProcPage, FordError>),
    Pic(String, String, Result<Vec<u8>, FordError>),
    Select(String),
    Close(String),
    ToggleContents,
    /// Something in a procedure (its key): a link, a picture
    Doc(String, DocMsg),
    Zoom(Option<String>),
    Opened(Result<(), String>),
    SignIn,
    /// The vehicle's PTS setup finished (or couldn't be done)
    SetUp(Result<(), String>),
    /// Something in a procedure's pinpoint test (the procedure's key)
    Test(String, TestMsg),
    /// A jump within a procedure happened: where it was before (kept for Back, when `true`)
    Jumped(String, Option<f32>, bool),
    /// Back to where the procedure showing was before its last jump
    JumpBack,
    /// Where a procedure (its key), its open test, or the contents are scrolled to, as they scroll
    Scrolled(String, f32),
    TestScrolled(String, f32),
    ContentsScrolled(f32),
}

/// How long after a procedure opens at a part it's gone to again as its pictures come in
const SETTLE: Duration = Duration::from_secs(6);
/// Space left above a part jumped to
const JUMP_MARGIN: f32 = 16.0;

fn scroll_id() -> iced::widget::scrollable::Id {
    iced::widget::scrollable::Id::new("manual-procedure")
}

fn contents_id() -> iced::widget::scrollable::Id {
    iced::widget::scrollable::Id::new("manual-contents")
}

fn to(y: f32) -> iced::widget::scrollable::AbsoluteOffset {
    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y }
}

fn is_pdf(url: &str) -> bool {
    url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase().ends_with(".pdf")
}

impl ManualView {
    pub fn new(vin: impl Into<String>) -> Self {
        Self {
            vin: vin.into(),
            ..Default::default()
        }
    }

    /// Reads the contents, unless they're being read or were read already (unless `force`)
    pub fn load(&mut self, env: &Env, force: bool) -> Task<ManualMsg> {
        if self.loading || self.vin.trim().is_empty() || (self.toc.is_some() && !force) {
            return Task::none();
        }
        self.loading = true;
        let (ford, vin) = (env.ford.clone(), self.vin.clone());
        Task::perform(async move { ford.toc(&vin).await.map_err(FordError::from) }, ManualMsg::TocLoaded)
    }

    /// Back where it was: the contents, the procedure showing, its open test (when the tab is shown again: it's
    /// drawn anew, at the top)
    pub fn restore(&self) -> Task<ManualMsg> {
        use iced::widget::scrollable::scroll_to;
        let mut tasks = vec![scroll_to(contents_id(), to(self.contents_offset))];
        if let Some(d) = self.active_tab() {
            tasks.push(scroll_to(scroll_id(), to(d.offset)));
            if d.test_open {
                tasks.push(scroll_to(pinpoint::scroll_id(), to(d.test_offset)));
            }
        }
        Task::batch(tasks)
    }

    fn reshow(&mut self) {
        self.shown = match &self.toc {
            Some(t) => manual::shown(&t.roots, self.all_options || !t.filterable),
            None => Vec::new(),
        };
    }

    fn active_tab(&self) -> Option<&ProcTab> {
        self.active.as_ref().and_then(|k| self.docs.iter().find(|d| &d.key == k))
    }

    fn load_doc(&mut self, env: &Env, key: &str) -> Task<ManualMsg> {
        let Some(d) = self.docs.iter_mut().find(|d| d.key == key) else {
            return Task::none();
        };
        if d.loading || d.page.is_some() {
            return Task::none();
        }
        d.loading = true;
        d.error = None;
        let (ford, vin, link, key) = (env.ford.clone(), self.vin.clone(), d.link.clone(), key.to_string());
        Task::perform(
            async move {
                let r = ford.procedure(&vin, &link).await.map_err(FordError::from);
                (key, r)
            },
            |(k, r)| ManualMsg::Loaded(k, r),
        )
    }

    fn load_pics(&mut self, env: &Env, key: &str) -> Task<ManualMsg> {
        let Some(d) = self.docs.iter_mut().find(|d| d.key == key) else {
            return Task::none();
        };
        let Some(page) = &d.page else { return Task::none() };
        let mut tasks = Vec::new();
        for url in Block::images(&page.blocks).into_iter().take(MAX_PICS) {
            if d.pics.contains_key(&url) {
                continue;
            }
            d.pics.insert(url.clone(), Pic::Loading);
            let (ford, key) = (env.ford.clone(), key.to_string());
            tasks.push(Task::perform(
                async move {
                    let r = ford.bytes(&url).await.map(|(b, _)| b).map_err(FordError::from);
                    (key, url, r)
                },
                |(k, u, r)| ManualMsg::Pic(k, u, r),
            ));
        }
        Task::batch(tasks)
    }

    /// Opens a procedure: in a new tab, or in place of the one showing; to the top of it
    fn open_doc(&mut self, env: &Env, link: ProcLink, title: String, section: String, new_tab: bool) -> Task<ManualMsg> {
        let key = link.key();
        let was_open = self.docs.iter().any(|d| d.key == key);
        if !was_open {
            let tab = ProcTab {
                key: key.clone(),
                link,
                title,
                section,
                loading: false,
                error: None,
                page: None,
                pics: HashMap::new(),
                tests: HashMap::new(),
                test: None,
                test_open: false,
                all_steps: false,
                back: Vec::new(),
                settle: None,
                offset: 0.0,
                test_offset: 0.0,
            };
            let at = self.active.as_ref().and_then(|k| self.docs.iter().position(|d| &d.key == k));
            match at {
                Some(i) if !new_tab => self.docs[i] = tab,
                Some(i) => self.docs.insert(i + 1, tab),
                None => self.docs.push(tab),
            }
        }
        self.active = Some(key.clone());
        self.zoom = None;
        let load = self.load_doc(env, &key);
        if was_open {
            // already open: where it was
            return Task::batch([load, self.restore()]);
        }
        Task::batch([load, iced::widget::scrollable::snap_to(scroll_id(), RelativeOffset::START)])
    }

    /// Opens a procedure from elsewhere (the Diagnostics tab) in a tab of its own: a pinpoint test's link opens at the
    /// test
    pub fn open_procedure(&mut self, env: &Env, link: ProcLink, title: String) -> Task<ManualMsg> {
        let key = link.key();
        let ppt = link.ppt.clone();
        let Some(d) = self.docs.iter_mut().find(|d| d.key == key) else {
            return self.open_doc(env, link, title, String::new(), true);
        };
        // already open: a test asked for opens over it (once it's read, if it's still being read)
        let loaded = d.page.is_some();
        if !ppt.is_empty() && !loaded {
            d.link.ppt = ppt.clone();
        }
        let shown = self.open_doc(env, link, title, String::new(), true);
        if ppt.is_empty() || !loaded {
            return shown;
        }
        let test = self.procedure_link(env, &key, &format!("ppt:{ppt}"));
        Task::batch([shown, test])
    }

    pub fn update(&mut self, msg: ManualMsg, env: &Env) -> (Task<ManualMsg>, Vec<Out>) {
        let mut outs = Vec::new();
        let task = match msg {
            ManualMsg::Refresh => {
                self.set_up_tried = false;
                self.load(env, true)
            }
            ManualMsg::TocLoaded(r) => {
                self.loading = false;
                match r {
                    Ok(t) => {
                        self.toc = Some(t);
                        self.error = None;
                        self.reshow();
                    }
                    // PTS hasn't set the vehicle up: that's done unseen, then the contents are read again
                    Err(FordError::NotSetUp) if !self.set_up_tried => {
                        self.set_up_tried = true;
                        self.setting_up = true;
                        self.error = Some(FordError::NotSetUp);
                        outs.push(Out::SetUpPts(self.vin.clone()));
                    }
                    Err(e) => self.error = Some(e),
                }
                Task::none()
            }
            ManualMsg::Toggle(path) => {
                if !self.open.remove(&path) {
                    self.open.insert(path);
                }
                Task::none()
            }
            ManualMsg::Query(q) => {
                self.query = q;
                Task::none()
            }
            ManualMsg::AllOptions(on) => {
                self.all_options = on;
                self.reshow();
                Task::none()
            }
            ManualMsg::Open {
                link,
                title,
                section,
                new_tab,
            } => self.open_doc(env, link, title, section, new_tab),
            ManualMsg::OpenUrl(url) => self.open_link(env, &url),
            ManualMsg::Loaded(key, r) => {
                let Some(d) = self.docs.iter_mut().find(|d| d.key == key) else {
                    return (Task::none(), outs);
                };
                d.loading = false;
                match r {
                    Ok(p) => {
                        if !p.title.is_empty() {
                            d.title = p.title.clone();
                        }
                        // a link to a part of it: one of its tests (a DTC chart's "GO to Pinpoint Test DH"), a step,
                        // or a place in it (Ford's anchor)
                        let want = [d.link.ppt.as_str(), d.link.hash.as_str()]
                            .into_iter()
                            .find(|x| !x.is_empty())
                            .unwrap_or("")
                            .to_string();
                        d.page = Some(p);
                        let pics = self.load_pics(env, &key);
                        if want.is_empty() {
                            pics
                        } else {
                            let go = self.procedure_link(env, &key, &format!("jump:{want}"));
                            if let Some(d) = self.docs.iter_mut().find(|d| d.key == key) {
                                d.back.clear(); // (opened there: nothing to go back to)
                                d.settle = Some((want, Instant::now()));
                            }
                            Task::batch([pics, go])
                        }
                    }
                    Err(e) => {
                        d.error = Some(e);
                        Task::none()
                    }
                }
            }
            ManualMsg::Pic(key, url, r) => {
                if let Some(d) = self.docs.iter_mut().find(|d| d.key == key) {
                    let pic = match r {
                        Ok(b) => Pic::Ready(iced::widget::image::Handle::from_bytes(b)),
                        Err(_) => Pic::Failed,
                    };
                    d.pics.insert(url, pic);
                    // opened at a part: there again once its pictures are in (they've moved it down)
                    if !d.pics.values().any(|p| matches!(p, Pic::Loading)) {
                        if let Some((part, at)) = d.settle.take() {
                            if at.elapsed() < SETTLE && self.active.as_deref() == Some(key.as_str()) {
                                return (self.jump(&key, &part, false), outs);
                            }
                        }
                    }
                }
                Task::none()
            }
            ManualMsg::Select(key) => {
                self.active = Some(key.clone());
                self.zoom = None;
                // where it was left
                Task::batch([self.load_doc(env, &key), self.restore()])
            }
            ManualMsg::Close(key) => {
                let at = self.docs.iter().position(|d| d.key == key);
                self.docs.retain(|d| d.key != key);
                if self.active.as_deref() == Some(key.as_str()) {
                    self.active = at
                        .and_then(|i| self.docs.get(i.min(self.docs.len().saturating_sub(1))))
                        .map(|d| d.key.clone());
                }
                Task::none()
            }
            ManualMsg::ToggleContents => {
                self.contents_open = !self.contents_open;
                Task::none()
            }
            ManualMsg::Doc(_, DocMsg::Picture(url)) => {
                self.zoom = Some(url);
                Task::none()
            }
            ManualMsg::Doc(key, DocMsg::Link(url)) => self.procedure_link(env, &key, &url),
            ManualMsg::Test(key, msg) => self.test_update(env, key, msg),
            ManualMsg::Jumped(key, from, remember) => {
                if let (Some(y), true) = (from, remember) {
                    if let Some(d) = self.docs.iter_mut().find(|d| d.key == key) {
                        d.back.push(y);
                        if d.back.len() > 50 {
                            d.back.remove(0);
                        }
                    }
                }
                Task::none()
            }
            ManualMsg::Scrolled(key, y) => {
                if let Some(d) = self.docs.iter_mut().find(|d| d.key == key) {
                    d.offset = y;
                }
                Task::none()
            }
            ManualMsg::TestScrolled(key, y) => {
                if let Some(d) = self.docs.iter_mut().find(|d| d.key == key) {
                    d.test_offset = y;
                }
                Task::none()
            }
            ManualMsg::ContentsScrolled(y) => {
                self.contents_offset = y;
                Task::none()
            }
            ManualMsg::JumpBack => {
                let Some(key) = self.active.clone() else {
                    return (Task::none(), outs);
                };
                let Some(y) = self.docs.iter_mut().find(|d| d.key == key).and_then(|d| d.back.pop()) else {
                    return (Task::none(), outs);
                };
                scroll::glide_to(scroll_id(), scroll::Target::Offset(y), 0.0).map(move |r| ManualMsg::Jumped(key.clone(), r, false))
            }
            ManualMsg::Zoom(z) => {
                self.zoom = z;
                Task::none()
            }
            ManualMsg::Opened(r) => {
                if let Err(e) = r {
                    if let Some(d) = self.active.clone().and_then(|k| self.docs.iter_mut().find(|d| d.key == k)) {
                        d.error = Some(FordError::Problem(format!("Couldn't open the document ({e})")));
                    }
                }
                Task::none()
            }
            ManualMsg::SignIn => {
                outs.push(Out::SignIn);
                Task::none()
            }
            ManualMsg::SetUp(r) => {
                self.setting_up = false;
                match r {
                    Ok(()) if self.toc.is_none() => {
                        self.error = None;
                        let reload = self.load(env, true);
                        // and the procedures that couldn't be read
                        let keys: Vec<String> = self.docs.iter().filter(|d| d.error.is_some()).map(|d| d.key.clone()).collect();
                        let mut tasks = vec![reload];
                        for k in keys {
                            if let Some(d) = self.docs.iter_mut().find(|d| d.key == k) {
                                d.error = None;
                            }
                            tasks.push(self.load_doc(env, &k));
                        }
                        Task::batch(tasks)
                    }
                    Ok(()) => Task::none(),
                    Err(e) if self.error == Some(FordError::NotSetUp) => {
                        self.error = Some(FordError::Problem(format!("PTS couldn't set this vehicle up ({e})")));
                        Task::none()
                    }
                    Err(_) => Task::none(),
                }
            }
        };
        (task, outs)
    }

    /// A link: another procedure (a tab of its own), a PDF (the PDF viewer), or anything else (the browser)
    fn open_link(&mut self, env: &Env, url: &str) -> Task<ManualMsg> {
        if let Some(link) = ProcLink::from_url(url) {
            let title = if link.search_number.is_empty() {
                "Procedure".to_string()
            } else {
                link.search_number.clone()
            };
            return self.open_doc(env, link, title, String::new(), true);
        }
        let full = ford::doc::resolve(ford::paths::PTS, url).unwrap_or_else(|| url.to_string());
        if is_pdf(&full) {
            let ford = env.ford.clone();
            let name = full.rsplit('/').next().unwrap_or("Document").trim_end_matches(".pdf").to_string();
            return Task::perform(
                async move {
                    let (bytes, _) = ford.bytes(&full).await.map_err(|e| FordError::from(e).text())?;
                    crate::open_file(&name, "pdf", &bytes)
                },
                ManualMsg::Opened,
            );
        }
        let _ = crate::open_external(&full);
        Task::none()
    }

    // ---------- pinpoint tests ----------

    /// A link in a procedure: one of its tests ("ppt:A"), a part of it ("jump:…": a test's step, for now), or
    /// somewhere else
    fn procedure_link(&mut self, env: &Env, key: &str, url: &str) -> Task<ManualMsg> {
        if let Some(id) = url.strip_prefix("ppt:").or_else(|| url.strip_prefix("jump:")) {
            let found = self.docs.iter().find(|d| d.key == key).and_then(|d| d.page.as_ref()).and_then(|p| {
                let name = find_test(&p.blocks, id)?;
                let t = Block::pinpoints(&p.blocks).into_iter().find(|t| t.name == name)?;
                Some((name, t.step(id).map(|s| s.id.clone()), t.has_steps(), t.anchor.clone()))
            });
            return match found {
                // a test to step through (or one of its steps)
                Some((name, step, true, _)) => self.open_test(env, key, &name, step),
                // one that's only what Ford says about it ("Diagnostic steps are not provided…"): there in the procedure
                Some((_, _, false, anchor)) if !anchor.is_empty() => self.jump(key, &anchor, true),
                // a place in the procedure
                _ => self.jump(key, id, true),
            };
        }
        self.open_link(env, url)
    }

    /// Glides the procedure to a place in it (Ford's anchor name), from over a test too; where it was is kept for
    /// Back (when `remember`)
    fn jump(&mut self, key: &str, name: &str, remember: bool) -> Task<ManualMsg> {
        if let Some(d) = self.docs.iter_mut().find(|d| d.key == key) {
            d.test_open = false; // (the procedure shows: a test slides away)
        }
        let key = key.to_string();
        let part = iced::advanced::widget::Id::new(document::anchor_name(name));
        scroll::glide_to(scroll_id(), scroll::Target::Part(part), JUMP_MARGIN).map(move |r| ManualMsg::Jumped(key.clone(), r, remember))
    }

    /// Opens a test over its procedure: at `step`, or where it was left (its first step the first time). The first
    /// time, its start is recorded to Vehicle History; a newer test's steps are fetched.
    fn open_test(&mut self, env: &Env, key: &str, name: &str, step: Option<String>) -> Task<ManualMsg> {
        let vin = self.vin.clone();
        self.zoom = None;
        let Some(d) = self.docs.iter_mut().find(|d| d.key == key) else {
            return Task::none();
        };
        let Some(page) = d.page.as_ref() else { return Task::none() };
        let Some(ppt) = Block::pinpoints(&page.blocks).into_iter().find(|t| t.name == name) else {
            return Task::none();
        };
        let first = ppt.steps.first().map(|s| s.id.clone());
        let fetch = ppt.steps.is_empty() && !ppt.spa.is_empty();
        let report = PinpointReport {
            search_number: d.link.search_number.clone(),
            proc_uid: d.link.proc_uid.clone(),
            name: name.to_string(),
            title: ppt.title.clone(),
            step: None,
        };
        let valid = |id: &str| ppt.step(id).is_some();
        let st = d.tests.entry(name.to_string()).or_default();
        match step.filter(|s| valid(s.as_str())) {
            Some(s) => st.show(s),
            None if !valid(st.step.as_str()) => {
                if let Some(f) = first {
                    st.show(f)
                }
            }
            None => {}
        }
        let mut tasks = vec![iced::widget::scrollable::snap_to(pinpoint::scroll_id(), RelativeOffset::START)];
        if !st.started {
            st.started = true;
            st.rec = Rec::Busy;
            tasks.push(report_task(env, &vin, key, report, "Test start recorded to Vehicle History".into()));
        }
        let fetch = fetch && !st.loading;
        d.test = Some(name.to_string());
        d.test_open = true;
        if fetch {
            tasks.push(self.load_steps(env, key, name));
        }
        Task::batch(tasks)
    }

    /// A newer test's steps, fetched
    fn load_steps(&mut self, env: &Env, key: &str, name: &str) -> Task<ManualMsg> {
        let vin = self.vin.clone();
        let Some(d) = self.docs.iter_mut().find(|d| d.key == key) else {
            return Task::none();
        };
        let Some(file) = d.page.as_ref().and_then(|p| {
            Block::pinpoints(&p.blocks)
                .into_iter()
                .find(|t| t.name == name)
                .map(|t| t.spa.clone())
        }) else {
            return Task::none();
        };
        let st = d.tests.entry(name.to_string()).or_default();
        st.loading = true;
        st.load_error = None;
        let (ford, link, name, key) = (env.ford.clone(), d.link.clone(), name.to_string(), key.to_string());
        Task::perform(
            async move {
                let r = ford.spa_steps(&vin, &link, &name, &file).await.map_err(FordError::from);
                (key, name, r)
            },
            |(k, n, r)| ManualMsg::Test(k, TestMsg::Steps(n, r)),
        )
    }

    /// Goes to a step by Ford's id: in the test showing (`from`: the step and answer that led there, for Back), or
    /// another test (or one of its steps)
    fn go(&mut self, env: &Env, key: &str, from: Option<(String, String)>, target: &str) -> Task<ManualMsg> {
        let Some(d) = self.docs.iter_mut().find(|d| d.key == key) else {
            return Task::none();
        };
        let Some(page) = d.page.as_ref() else { return Task::none() };
        if let Some(name) = d.test.clone().filter(|_| d.test_open) {
            let here = Block::pinpoints(&page.blocks)
                .into_iter()
                .find(|t| t.name == name)
                .is_some_and(|t| t.step(target).is_some());
            if here {
                let st = d.tests.entry(name).or_default();
                if let Some(f) = from {
                    st.history.push(f);
                }
                st.show(target.to_string());
                return iced::widget::scrollable::snap_to(pinpoint::scroll_id(), RelativeOffset::START);
            }
        }
        let url = format!("jump:{target}");
        self.procedure_link(env, key, &url)
    }

    /// An answer: kept, recorded to Vehicle History (newer tests), and then on to its step; when that depends on the
    /// vehicle, the steps to pick from are offered; an answer that ends the test stays showing
    fn answer(&mut self, env: &Env, key: &str, step: String, i: usize, jump: Option<String>) -> Task<ManualMsg> {
        let vin = self.vin.clone();
        let Some(d) = self.docs.iter_mut().find(|d| d.key == key) else {
            return Task::none();
        };
        let (Some(name), Some(page)) = (d.test.clone(), d.page.as_ref()) else {
            return Task::none();
        };
        let Some(ppt) = Block::pinpoints(&page.blocks).into_iter().find(|t| t.name == name) else {
            return Task::none();
        };
        let Some(s) = ppt.step(&step) else { return Task::none() };
        let Some(a) = s.answers.get(i) else { return Task::none() };
        let st = d.tests.entry(name.clone()).or_default();
        let again = st.answers.get(&step) == Some(&a.id) && (a.ends() || st.choosing.as_deref() == Some(step.as_str()));
        st.answers.insert(step.clone(), a.id.clone());
        let target = jump.or_else(|| (a.jumps.len() == 1).then(|| a.jumps[0].clone()));
        let choose = target.is_none() && a.jumps.len() > 1;
        st.choosing = choose.then(|| step.clone());
        let mut tasks = Vec::new();
        if ppt.legacy {
            if choose {
                st.rec = Rec::Idle;
            }
        } else if !again {
            let report = PinpointReport {
                search_number: d.link.search_number.clone(),
                proc_uid: d.link.proc_uid.clone(),
                name: name.clone(),
                title: ppt.title.clone(),
                step: Some(StepReport {
                    id: s.label.clone(),
                    title: s.title.clone(),
                    question: s.question.clone(),
                    answer: a.id.clone(),
                    outcome: if a.ends() { Block::text(&a.blocks) } else { String::new() },
                }),
            };
            let ok = if choose {
                format!("{}: {} recorded. Now pick the step that applies", s.label, a.label)
            } else {
                format!("{}: {} recorded to Vehicle History", s.label, a.label)
            };
            st.rec = Rec::Busy;
            tasks.push(report_task(env, &vin, key, report, ok));
        }
        let from = (step, a.id.clone());
        if let Some(t) = target {
            tasks.push(self.go(env, key, Some(from), &t));
        }
        Task::batch(tasks)
    }

    fn test_update(&mut self, env: &Env, key: String, msg: TestMsg) -> Task<ManualMsg> {
        let snap = || iced::widget::scrollable::snap_to(pinpoint::scroll_id(), RelativeOffset::START);
        match msg {
            TestMsg::Open(name, step) => self.open_test(env, &key, &name, step),
            TestMsg::Close => {
                if let Some(d) = self.docs.iter_mut().find(|d| d.key == key) {
                    d.test_open = false;
                }
                self.zoom = None;
                Task::none()
            }
            TestMsg::Answer { step, answer, jump } => self.answer(env, &key, step, answer, jump),
            TestMsg::Go(target) => {
                // (Back returns to the step it was clicked from)
                let from = self
                    .docs
                    .iter()
                    .find(|d| d.key == key)
                    .and_then(|d| d.tests.get(d.test.as_ref()?))
                    .map(|st| (st.step.clone(), String::new()))
                    .filter(|(s, _)| !s.is_empty());
                self.go(env, &key, from, &target)
            }
            TestMsg::Back | TestMsg::Restart => {
                let back = matches!(msg, TestMsg::Back);
                let Some(d) = self.docs.iter_mut().find(|d| d.key == key) else {
                    return Task::none();
                };
                let (Some(name), Some(page)) = (d.test.clone(), d.page.as_ref()) else {
                    return Task::none();
                };
                let first = Block::pinpoints(&page.blocks)
                    .into_iter()
                    .find(|t| t.name == name)
                    .and_then(|t| t.steps.first())
                    .map(|s| s.id.clone());
                let st = d.tests.entry(name).or_default();
                if back {
                    if let Some((prev, _)) = st.history.pop() {
                        st.show(prev);
                    }
                } else {
                    st.history.clear();
                    st.answers.clear();
                    if let Some(f) = first {
                        st.show(f);
                    }
                }
                snap()
            }
            TestMsg::AllSteps(on) => {
                if let Some(d) = self.docs.iter_mut().find(|d| d.key == key) {
                    d.all_steps = on;
                }
                snap()
            }
            TestMsg::Retry => match self.docs.iter().find(|d| d.key == key).and_then(|d| d.test.clone()) {
                Some(name) => self.load_steps(env, &key, &name),
                None => Task::none(),
            },
            TestMsg::Reported(name, r, ok) => {
                if let Some(st) = self.docs.iter_mut().find(|d| d.key == key).and_then(|d| d.tests.get_mut(&name)) {
                    st.rec = match r {
                        Ok(()) => Rec::Done(ok),
                        Err(FordError::SignedOut) => Rec::Failed("Not recorded: sign in to PTS".into()),
                        Err(e) => Rec::Failed(format!("Not recorded ({})", e.text())),
                    };
                }
                Task::none()
            }
            TestMsg::Steps(name, r) => {
                let Some(d) = self.docs.iter_mut().find(|d| d.key == key) else {
                    return Task::none();
                };
                let mut first = None;
                let failed = match r {
                    Ok((intro, steps)) => {
                        if let Some(p) = d.page.as_mut().and_then(|p| Block::pinpoint_mut(&mut p.blocks, &name)) {
                            if p.intro.is_empty() {
                                p.intro = intro;
                            }
                            p.steps = steps;
                            first = p.steps.first().map(|s| s.id.clone());
                        }
                        None
                    }
                    Err(e) => Some(e),
                };
                let st = d.tests.entry(name).or_default();
                st.loading = false;
                st.load_error = failed;
                if st.step.is_empty() {
                    if let Some(f) = first {
                        st.show(f);
                    }
                }
                self.load_pics(env, &key)
            }
            TestMsg::Doc(DocMsg::Picture(u)) => {
                self.zoom = Some(u);
                Task::none()
            }
            TestMsg::Doc(DocMsg::Link(u)) => match u.strip_prefix("jump:") {
                Some(t) => self.go(env, &key, None, t),
                None => self.procedure_link(env, &key, &u),
            },
            TestMsg::Scrolled(y) => {
                if let Some(d) = self.docs.iter_mut().find(|d| d.key == key) {
                    d.test_offset = y;
                }
                Task::none()
            }
        }
    }
}

/// Records to Vehicle History, then says how that went
fn report_task(env: &Env, vin: &str, key: &str, report: PinpointReport, ok: String) -> Task<ManualMsg> {
    let (ford, vin, key) = (env.ford.clone(), vin.to_string(), key.to_string());
    let name = report.name.clone();
    Task::perform(
        async move { ford.report_pinpoint(&vin, &report).await.map_err(FordError::from) },
        move |r| ManualMsg::Test(key.clone(), TestMsg::Reported(name.clone(), r, ok.clone())),
    )
}

/// Before a test has been opened (it always has a state once it is)
static NO_TEST: LazyLock<TestState> = LazyLock::new(TestState::default);

// ---------- the view ----------

/// "303-05B Accessory Drive" → its section number and the rest
fn section_code(title: &str) -> Option<(&str, &str)> {
    let (code, rest) = title.split_once(' ')?;
    let b = code.as_bytes();
    let ok = b.len() >= 6
        && b[..3].iter().all(u8::is_ascii_digit)
        && b[3] == b'-'
        && b[4..6].iter().all(u8::is_ascii_digit)
        && b[6..].iter().all(u8::is_ascii_alphanumeric)
        && b.len() <= 8;
    ok.then_some((code, rest))
}

/// The usual sub-sections in their own colours, so they're easy to pick out
fn kind_color(title: &str) -> Option<Color> {
    let t = title.to_lowercase();
    let rgb = if t.starts_with("description and operation") {
        [0x1a, 0x5f, 0xb4]
    } else if t.starts_with("diagnosis and testing") {
        [0xd9, 0x77, 0x06]
    } else if t.starts_with("removal and installation") {
        [0x15, 0x80, 0x3d]
    } else if t.starts_with("general procedures") {
        [0x7a, 0x5a, 0xf5]
    } else if t.starts_with("disassembly") || t.starts_with("assembly") || t.starts_with("installation") {
        [0x0e, 0x74, 0x90]
    } else if t.starts_with("specifications") {
        [0xb4, 0x53, 0x09]
    } else {
        return None;
    };
    Some(Color::from_rgb8(rgb[0], rgb[1], rgb[2]))
}

fn muted_if(na: bool) -> impl Fn(&Theme) -> text::Style {
    move |theme: &Theme| text::Style {
        color: na.then(|| style::colors(theme).muted),
    }
}

/// The contents' rows: folders (open ones with what's in them) and procedures
fn tree_rows<'a>(m: &'a ManualView, nodes: &'a [TocNode], depth: u16, parent: &str, out: &mut Vec<Element<'a, ManualMsg>>) {
    let active = m.active.as_deref();
    for n in nodes {
        let path = format!("{parent} / {}", n.title);
        let open = m.open.contains(&path);
        let indent = Space::with_width(depth as f32 * 14.0);
        match &n.kind {
            TocKind::Folder => {
                let mark = text(if open { "–" } else { "+" }).size(13).width(14).style(style::muted);
                let mut label = Row::new().spacing(6).align_y(Alignment::Center);
                match section_code(&n.title) {
                    Some((code, rest)) => {
                        label = label
                            .push(pill(code, [0x5d, 0x66, 0x70]))
                            .push(text(rest).size(13).style(muted_if(n.na)));
                    }
                    None => {
                        let t = text(n.title.as_str()).size(13);
                        label = label.push(match kind_color(&n.title) {
                            Some(c) if !n.na => t.color(c),
                            _ => t.style(muted_if(n.na)),
                        });
                    }
                }
                out.push(
                    button(row![indent, mark, label].spacing(4).align_y(Alignment::Center))
                        .width(Length::Fill)
                        .padding([4, 6])
                        .style(style::list_item(false))
                        .on_press(ManualMsg::Toggle(path.clone()))
                        .into(),
                );
                if open {
                    tree_rows(m, &n.children, depth + 1, &path, out);
                }
            }
            TocKind::Doc(link) => {
                let parts = !n.children.is_empty();
                let mark: Element<'a, ManualMsg> = if parts {
                    button(text(if open { "–" } else { "+" }).size(13).style(style::muted))
                        .width(14)
                        .padding(0)
                        .style(button::text)
                        .on_press(ManualMsg::Toggle(path.clone()))
                        .into()
                } else {
                    Space::with_width(14).into()
                };
                let on = active == Some(link.key().as_str());
                out.push(
                    button(
                        row![indent, mark, text(n.title.as_str()).size(13).style(muted_if(n.na))]
                            .spacing(4)
                            .align_y(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .padding([4, 6])
                    .style(style::list_item(on))
                    .on_press(ManualMsg::Open {
                        link: link.clone(),
                        title: n.title.clone(),
                        section: parent.rsplit(" / ").next().unwrap_or("").to_string(),
                        new_tab: false,
                    })
                    .into(),
                );
                if parts && open {
                    tree_rows(m, &n.children, depth + 1, &path, out);
                }
            }
            TocKind::Link(url) => out.push(
                button(row![indent, Space::with_width(14), text(n.title.as_str()).size(13).style(text::primary)].spacing(4))
                    .width(Length::Fill)
                    .padding([4, 6])
                    .style(style::list_item(false))
                    .on_press(ManualMsg::OpenUrl(url.clone()))
                    .into(),
            ),
        }
    }
}

/// Every procedure (and link) with where it is, for searching
fn leaves<'a>(nodes: &'a [TocNode], path: &mut Vec<&'a str>, out: &mut Vec<(&'a TocNode, String)>) {
    for n in nodes {
        if !matches!(n.kind, TocKind::Folder) {
            out.push((n, path.join(" › ")));
        }
        path.push(n.title.as_str());
        leaves(&n.children, path, out);
        path.pop();
    }
}

fn search_rows<'a>(m: &'a ManualView, out: &mut Vec<Element<'a, ManualMsg>>) {
    let words: Vec<String> = m.query.to_lowercase().split_whitespace().map(str::to_string).collect();
    let mut all = Vec::new();
    leaves(&m.shown, &mut Vec::new(), &mut all);
    let hits: Vec<(&TocNode, String)> = all
        .into_iter()
        .filter(|(n, path)| {
            let hay = format!("{path} {}", n.title).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .take(MAX_HITS)
        .collect();
    if hits.is_empty() {
        out.push(
            container(text("Nothing in the contents matches that.").size(13).style(style::muted))
                .padding(10)
                .into(),
        );
        return;
    }
    for (n, path) in hits {
        let msg = match &n.kind {
            TocKind::Doc(link) => ManualMsg::Open {
                link: link.clone(),
                title: n.title.clone(),
                section: path.rsplit(" › ").next().unwrap_or("").to_string(),
                new_tab: false,
            },
            TocKind::Link(u) => ManualMsg::OpenUrl(u.clone()),
            TocKind::Folder => continue,
        };
        let short_path: String = path
            .rsplit(" › ")
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" › ");
        out.push(
            button(
                column![
                    text(n.title.as_str()).size(13).style(muted_if(n.na)),
                    text(short_path).size(11).style(style::muted)
                ]
                .spacing(1),
            )
            .width(Length::Fill)
            .padding([5, 8])
            .style(style::list_item(false))
            .on_press(msg)
            .into(),
        );
    }
}

fn contents<'a>(m: &'a ManualView) -> Element<'a, ManualMsg> {
    let mut head = column![
        row![
            text("Contents").size(15).font(style::BOLD),
            horizontal_space(),
            text(format!("{} entries", manual::count(&m.shown))).size(12).style(style::muted),
        ]
        .align_y(Alignment::Center)
    ]
    .spacing(8);
    head = head.push(text_input("Search the contents…", &m.query).on_input(ManualMsg::Query).padding(7));
    if let Some(t) = &m.toc {
        if t.filterable {
            let hidden = manual::count(&t.roots).saturating_sub(manual::count(&manual::shown(&t.roots, false)));
            head = head.push(
                row![
                    checkbox("Only this vehicle", !m.all_options)
                        .text_size(13)
                        .size(15)
                        .on_toggle(|on| ManualMsg::AllOptions(!on)),
                    horizontal_space(),
                    text(if m.all_options {
                        format!("{hidden} don't apply (gray)")
                    } else {
                        format!("{hidden} hidden")
                    })
                    .size(11)
                    .style(style::muted),
                ]
                .align_y(Alignment::Center),
            );
        }
    }
    let mut rows: Vec<Element<'a, ManualMsg>> = Vec::new();
    match (&m.toc, &m.error) {
        (Some(_), _) if !m.query.trim().is_empty() => search_rows(m, &mut rows),
        (Some(_), _) => tree_rows(m, &m.shown, 0, "", &mut rows),
        (None, Some(FordError::SignedOut)) => rows.push(
            column![
                text("The manual needs your PTS sign-in.").size(13).style(style::muted),
                button(text("Sign in to PTS"))
                    .padding([6, 14])
                    .style(button::primary)
                    .on_press(ManualMsg::SignIn)
            ]
            .spacing(8)
            .into(),
        ),
        (None, Some(FordError::NotSetUp)) if m.setting_up => {
            rows.push(text("Setting this vehicle up in PTS…").size(13).style(style::muted).into())
        }
        (None, Some(e)) => rows.push(
            column![
                text(format!("Couldn't load the contents ({})", e.text()))
                    .size(13)
                    .style(style::muted),
                button(text("Try again"))
                    .padding([6, 14])
                    .style(button::primary)
                    .on_press(ManualMsg::Refresh)
            ]
            .spacing(8)
            .into(),
        ),
        (None, None) => rows.push(
            text(if m.loading { "Loading the contents…" } else { "" })
                .size(13)
                .style(style::muted)
                .into(),
        ),
    }
    container(
        column![
            head,
            scrollable(Column::with_children(rows).spacing(1).padding([0, 4]))
                .id(contents_id())
                .on_scroll(|v| ManualMsg::ContentsScrolled(v.absolute_offset().y))
                .height(Length::Fill)
        ]
        .spacing(10),
    )
    .padding(10)
    .width(TOC_W)
    .height(Length::Fill)
    .style(style::panel)
    .into()
}

fn procedure<'a>(m: &'a ManualView) -> Element<'a, ManualMsg> {
    let note = |t: String, action: Option<(&'a str, ManualMsg)>| -> Element<'a, ManualMsg> {
        let mut c = column![text(t).style(style::muted)].spacing(10);
        if let Some((label, msg)) = action {
            c = c.push(button(text(label)).padding([6, 14]).style(button::primary).on_press(msg));
        }
        container(container(c).padding(18).width(Length::Fill).style(style::card))
            .padding(16)
            .into()
    };
    let Some(d) = m.active_tab() else {
        return note(
            "Pick a procedure from the contents on the left, or search them. Links inside a procedure open in tabs of their own.".into(),
            None,
        );
    };
    let body: Element<'a, ManualMsg> = match (&d.page, &d.error) {
        (None, Some(FordError::SignedOut)) => note(
            "The manual needs your PTS sign-in.".into(),
            Some(("Sign in to PTS", ManualMsg::SignIn)),
        ),
        (None, Some(e)) => note(
            format!("Couldn't load this procedure ({})", e.text()),
            Some(("Try again", ManualMsg::Select(d.key.clone()))),
        ),
        (None, None) => note(format!("Loading {}…", d.title), None),
        (Some(p), _) => {
            let key = d.key.clone();
            let mut head = Column::new().spacing(4);
            if !p.section.is_empty() {
                head = head.push(text(p.section.as_str()).size(12).style(style::muted));
            }
            head = head.push(
                text(if p.title.is_empty() { d.title.as_str() } else { p.title.as_str() })
                    .size(22)
                    .font(style::BOLD),
            );
            if !p.rev.is_empty() {
                head = head.push(text(p.rev.as_str()).size(11).style(style::muted));
            }
            let k = key.clone();
            let doc = document::view(&p.blocks, &d.pics).map(move |msg| ManualMsg::Doc(k.clone(), msg));
            let k = key.clone();
            let base = scrollable(
                container(column![head, doc].spacing(16).max_width(1000))
                    .padding(20)
                    .width(Length::Fill),
            )
            .id(scroll_id())
            .on_scroll(move |v| ManualMsg::Scrolled(k.clone(), v.absolute_offset().y))
            .height(Length::Fill);
            // a pinpoint test, over the procedure (still drawn while it slides away)
            let test: Option<&Pinpoint> = d
                .test
                .as_ref()
                .and_then(|n| Block::pinpoints(&p.blocks).into_iter().find(|t| &t.name == n));
            let top: Element<'a, ManualMsg> = match test {
                Some(t) => {
                    let st = d.tests.get(&t.name).unwrap_or(&*NO_TEST);
                    pinpoint::view(t, st, d.all_steps, &d.pics).map(move |m| match m {
                        TestMsg::Scrolled(y) => ManualMsg::TestScrolled(key.clone(), y),
                        m => ManualMsg::Test(key.clone(), m),
                    })
                }
                None => Space::new(Length::Fill, Length::Fill).into(),
            };
            anim::push(base, top, d.test_open && test.is_some()).into()
        }
    };
    body
}

/// The procedures open, as tabs (each with its ×), and Contents to slide the contents away
fn tabs_bar<'a>(m: &'a ManualView) -> Element<'a, ManualMsg> {
    let mut bar = Row::new().spacing(4).align_y(Alignment::Center).push(
        button(text("Contents").size(13))
            .padding([5, 10])
            .style(style::toggle(m.contents_open))
            .on_press(ManualMsg::ToggleContents),
    );
    for d in &m.docs {
        let on = m.active.as_deref() == Some(d.key.as_str());
        let title: String = if d.title.chars().count() > 34 {
            format!("{}…", d.title.chars().take(33).collect::<String>())
        } else {
            d.title.clone()
        };
        bar = bar.push(
            row![
                button(text(title).size(13))
                    .padding([5, 4])
                    .style(style::tab(on))
                    .on_press(ManualMsg::Select(d.key.clone())),
                button(text("×").size(13))
                    .padding([5, 6])
                    .style(style::tab(on))
                    .on_press(ManualMsg::Close(d.key.clone())),
            ]
            .align_y(Alignment::Center),
        );
    }
    let tabs = scrollable(bar.padding([0, 0]))
        .direction(iced::widget::scrollable::Direction::Horizontal(
            iced::widget::scrollable::Scrollbar::new().width(4).scroller_width(4),
        ))
        .width(Length::Fill);
    // back to where the procedure was before a jump within it
    let back = m.active_tab().filter(|d| !d.back.is_empty() && !d.test_open).map(|_| {
        button(text("‹ Back").size(13))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(ManualMsg::JumpBack)
    });
    let mut r = row![tabs].spacing(8).align_y(Alignment::Center);
    if let Some(b) = back {
        r = r.push(b);
    }
    container(r).padding([8, 10]).width(Length::Fill).into()
}

pub fn view<'a>(m: &'a ManualView) -> Element<'a, ManualMsg> {
    let main = row![
        anim::side(contents(m), m.contents_open, anim::Edge::Left, TOC_W),
        column![tabs_bar(m), procedure(m)].width(Length::Fill).height(Length::Fill),
    ]
    .height(Length::Fill);
    // a picture shown big, over everything (click anywhere to close it)
    let big = m.zoom.as_ref().and_then(|u| match m.active_tab()?.pics.get(u)? {
        Pic::Ready(h) => Some(h.clone()),
        _ => None,
    });
    match big {
        Some(h) => stack![
            main,
            mouse_area(
                container(
                    image(h)
                        .content_fit(iced::ContentFit::Contain)
                        .width(Length::Fill)
                        .height(Length::Fill)
                )
                .padding(24)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_: &Theme| container::Style {
                    background: Some(Color { a: 0.82, ..Color::BLACK }.into()),
                    ..Default::default()
                })
            )
            .on_press(ManualMsg::Zoom(None)),
        ]
        .into(),
        None => main.into(),
    }
}
