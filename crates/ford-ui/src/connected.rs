//! The Connected Vehicle tab (the Electron app's `manual/connected-vehicle/`): what the vehicle reports to Ford through
//! its modem (crate `ford`'s connected.rs), in three parts along the top:
//!
//!   Home       the latest status (odometer, fuel, oil life, 12V battery, tire pressures), the DTCs and alerts it's
//!              reporting now, Recent DTCs (each DTC in the history once: how often and when, with the TSBs / GSBs /
//!              SSMs Ford lists for it), and the health history (each report with DTCs or alerts; "Show older" asks
//!              for 60 days more at a time, back a year). Tick DTCs to search them together in Diagnostics.
//!   Over-the-air updates   the vehicle's OTA settings, what's lined up next, updates in process and finished ones
//!              (their modules, software, release notes and every step Ford recorded)
//!   Software   the vehicle's software subscriptions (connectivity, BlueCruise…)
//!
//! OASIS shows the status too ([`oasis_card`]) and checks warranty against its odometer. The status is read with
//! the values OASIS's page sets, so OASIS is read first when it hasn't been. Nothing is kept on disk.

use std::collections::HashSet;

use chrono::{Local, NaiveDate, Utc};
use ford::connected::{
    Articles, CvArticle, CvStatus, CvVars, Descriptions, Dtc, Modules, Ota, OtaUpdate, Reading, Report, StepClass, Subscriptions, Tire,
    When, articles_for, dist, merge_reports, module_of,
};
use iced::widget::{Column, Row, Space, button, checkbox, column, container, horizontal_space, row, text, tooltip};
use iced::{Alignment, Color, Element, Length, Task};
use ui_kit::scroll::scrollable;
use ui_kit::{anim, pill, style};

use crate::{Env, FordError, Out};

/// Days of history asked for at a time (as Ford's page asks), and how far back "Show older" goes
const HISTORY_DAYS: i64 = 60;
const HISTORY_MAX: i64 = 365;
/// Diagnostics searches up to 20 DTCs at once
const DIAG_MAX: usize = 20;

const GREEN: [u8; 3] = [0x15, 0x80, 0x3d];
const RED: [u8; 3] = [0xdc, 0x26, 0x26];
const AMBER: [u8; 3] = [0xd9, 0x77, 0x06];
const GRAY: [u8; 3] = [0x5d, 0x66, 0x70];
const BLUE: [u8; 3] = [0x1a, 0x5f, 0xb4];

/// The tab's parts, along its top
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Part {
    #[default]
    Home,
    Ota,
    Software,
}

impl Part {
    pub const ALL: [Part; 3] = [Part::Home, Part::Ota, Part::Software];

    pub fn label(self) -> &'static str {
        match self {
            Part::Home => "Home",
            Part::Ota => "Over-the-air updates",
            Part::Software => "Software",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConnectedView {
    pub vin: String,
    pub part: Part,
    pub status: Option<CvStatus>,
    pub loading: bool,
    pub error: Option<FordError>,
    /// What OASIS's page set (once OASIS is read)
    vars: Option<CvVars>,
    /// The health reports so far (newest first), and from when
    pub reports: Vec<Report>,
    pub oldest: Option<NaiveDate>,
    pub history_loading: bool,
    pub history_error: Option<String>,
    /// Every report shown (else only those with DTCs or alerts)
    pub every_report: bool,
    pub modules: Modules,
    modules_asked: bool,
    pub descs: Descriptions,
    /// DTCs already asked about ("code|ft|sb")
    asked: HashSet<String>,
    describing: bool,
    /// Ford's TSBs / GSBs / SSMs by DTC (none: still asking), or why they couldn't be had
    pub articles: Option<Articles>,
    pub articles_failed: String,
    articles_loading: bool,
    /// DTCs ticked to search together in Diagnostics ("U2100:00"), in the order ticked
    pub picked: Vec<String>,
    pub ota: Option<Result<Ota, FordError>>,
    pub ota_loading: bool,
    pub subs: Option<Result<Subscriptions, FordError>>,
    pub subs_loading: bool,
    /// Updates whose release notes / steps are showing (their IDs)
    pub open_updates: HashSet<String>,
    /// "All values Ford sent" showing
    pub everything: bool,
    /// Changes with each fresh read, so answers to an older one are let go
    round: u64,
    /// The tab has been shown (until then only the status is read, for OASIS)
    opened: bool,
}

#[derive(Debug, Clone)]
pub enum ConnectedMsg {
    Part(Part),
    Refresh,
    Loaded(u64, Result<CvStatus, FordError>),
    Older,
    History(u64, NaiveDate, Result<Vec<Report>, FordError>),
    EveryReport(bool),
    Modules(u64, Result<Modules, FordError>),
    Described(u64, Result<Descriptions, FordError>),
    Articles(u64, Result<Articles, FordError>),
    Pick(String, bool),
    PickAll(bool),
    ClearPicked,
    /// Search DTCs in Diagnostics ("U2100:00 B1081")
    Search(String),
    SearchPicked,
    OpenArticle(CvArticle),
    Copy(String, String),
    OtaLoaded(u64, Result<Ota, FordError>),
    SubsLoaded(u64, Result<Subscriptions, FordError>),
    ToggleUpdate(String),
    Everything(bool),
    SignIn,
    /// (from OASIS) show this tab
    ShowTab,
}

/// One DTC in Recent DTCs: each DTC in the history once
struct Recent {
    dtc: Dtc,
    module: String,
    module_desc: String,
    count: usize,
    now: bool,
    last: When,
    last_odo: Option<f64>,
    first: When,
}

impl ConnectedView {
    pub fn new(vin: impl Into<String>) -> Self {
        Self {
            vin: vin.into(),
            ..Default::default()
        }
    }

    /// What OASIS's page set (read with OASIS): used for the status request
    pub fn set_vars(&mut self, vars: CvVars) {
        self.vars = Some(vars);
    }

    pub fn has_vars(&self) -> bool {
        self.vars.is_some()
    }

    /// The odometer the vehicle last reported, in miles, and when (for OASIS's warranty)
    pub fn odometer(&self) -> Option<(f64, When)> {
        let s = self.status.as_ref()?;
        Some((s.odometer_miles()?, s.odometer_when))
    }

    /// Reads the status, unless it's being read or was read already (unless `force`): everything else follows
    pub fn load(&mut self, env: &Env, force: bool) -> Task<ConnectedMsg> {
        if self.loading || env.sample || self.vin.trim().is_empty() || (self.status.is_some() && !force) {
            return Task::none();
        }
        if force || self.status.is_none() {
            self.round += 1;
        }
        self.loading = true;
        let (ford, vin, vars, round) = (env.ford.clone(), self.vin.clone(), self.vars.clone(), self.round);
        Task::perform(
            async move { ford.connected_vehicle(&vin, vars).await.map_err(FordError::from) },
            move |r| ConnectedMsg::Loaded(round, r),
        )
    }

    /// What the part showing needs (its own request), once the status is in
    fn load_part(&mut self, env: &Env, force: bool) -> Task<ConnectedMsg> {
        let Some(ctx) = self.status.as_ref().map(|s| s.ctx.clone()) else {
            return Task::none();
        };
        let (ford, round) = (env.ford.clone(), self.round);
        match self.part {
            Part::Home => Task::none(),
            Part::Ota if !self.ota_loading && (force || !matches!(self.ota, Some(Ok(_)))) => {
                self.ota_loading = true;
                Task::perform(async move { ford.cv_ota(&ctx).await.map_err(FordError::from) }, move |r| {
                    ConnectedMsg::OtaLoaded(round, r)
                })
            }
            Part::Software if !self.subs_loading && (force || !matches!(self.subs, Some(Ok(_)))) => {
                self.subs_loading = true;
                Task::perform(
                    async move { ford.cv_subscriptions(&ctx).await.map_err(FordError::from) },
                    move |r| ConnectedMsg::SubsLoaded(round, r),
                )
            }
            _ => Task::none(),
        }
    }

    /// Home's other requests: module names, TSBs by DTC, what new DTCs mean, older history when none came
    fn load_home(&mut self, env: &Env) -> Task<ConnectedMsg> {
        let Some(ctx) = self.status.as_ref().map(|s| s.ctx.clone()) else {
            return Task::none();
        };
        let round = self.round;
        let mut tasks = Vec::new();
        if !self.modules_asked {
            self.modules_asked = true;
            let (ford, ctx) = (env.ford.clone(), ctx.clone());
            tasks.push(Task::perform(
                async move { ford.cv_modules(&ctx).await.map_err(FordError::from) },
                move |r| ConnectedMsg::Modules(round, r),
            ));
        }
        if self.articles.is_none() && !self.articles_loading {
            self.articles_loading = true;
            let (ford, ctx) = (env.ford.clone(), ctx.clone());
            tasks.push(Task::perform(
                async move { ford.cv_articles(&ctx).await.map_err(FordError::from) },
                move |r| ConnectedMsg::Articles(round, r),
            ));
        }
        tasks.push(self.describe_new(env));
        if self.oldest.is_none() && !self.history_loading {
            tasks.push(self.older(env));
        }
        Task::batch(tasks)
    }

    /// Every DTC now and in the history
    fn all_dtcs(&self) -> impl Iterator<Item = &Dtc> {
        let now = self.status.as_ref().and_then(|s| s.now.as_ref());
        now.into_iter()
            .flat_map(|r| r.dtcs.iter())
            .chain(self.reports.iter().flat_map(|r| r.dtcs.iter()))
    }

    /// Asks Ford what DTCs not asked about yet mean
    fn describe_new(&mut self, env: &Env) -> Task<ConnectedMsg> {
        if self.describing {
            return Task::none();
        }
        let Some(ctx) = self.status.as_ref().map(|s| s.ctx.clone()) else {
            return Task::none();
        };
        let mut want: Vec<Dtc> = Vec::new();
        let mut asked = std::mem::take(&mut self.asked);
        for d in self.all_dtcs() {
            if asked.insert(format!("{}|{}|{}", d.code, d.ft, d.sb)) {
                want.push(d.clone());
            }
        }
        self.asked = asked;
        if want.is_empty() {
            return Task::none();
        }
        self.describing = true;
        let (ford, round) = (env.ford.clone(), self.round);
        Task::perform(
            async move { ford.cv_describe(&ctx, &want).await.map_err(FordError::from) },
            move |r| ConnectedMsg::Described(round, r),
        )
    }

    /// 60 days more of history, before the oldest so far
    fn older(&mut self, env: &Env) -> Task<ConnectedMsg> {
        let Some(ctx) = self.status.as_ref().map(|s| s.ctx.clone()) else {
            return Task::none();
        };
        if self.history_loading {
            return Task::none();
        }
        let today = Local::now().date_naive();
        let to = match self.oldest {
            Some(o) => o - chrono::Duration::days(1),
            None => today,
        };
        let from = to - chrono::Duration::days(HISTORY_DAYS);
        self.history_loading = true;
        self.history_error = None;
        let (ford, round) = (env.ford.clone(), self.round);
        Task::perform(
            async move { ford.cv_history(&ctx, from, to).await.map_err(FordError::from) },
            move |r| ConnectedMsg::History(round, from, r),
        )
    }

    /// History can go further back
    fn can_older(&self) -> bool {
        let today = Local::now().date_naive();
        self.oldest.is_none_or(|o| (today - o).num_days() < HISTORY_MAX)
    }

    /// Shows a part (reading what it needs)
    pub fn show(&mut self, part: Part, env: &Env) -> Task<ConnectedMsg> {
        self.part = part;
        self.opened = true;
        if self.status.is_none() {
            return self.load(env, false);
        }
        let home = self.load_home(env);
        let part = self.load_part(env, false);
        Task::batch([home, part])
    }

    pub fn update(&mut self, msg: ConnectedMsg, env: &Env) -> (Task<ConnectedMsg>, Vec<Out>) {
        let mut outs = Vec::new();
        let task = match msg {
            ConnectedMsg::Part(p) => self.show(p, env),
            ConnectedMsg::Refresh => {
                // a fresh read: what came from the last one is asked again
                self.reports.clear();
                self.oldest = None;
                self.history_error = None;
                self.articles = None;
                self.articles_failed.clear();
                self.ota = None;
                self.subs = None;
                self.asked.clear();
                self.describing = false;
                self.articles_loading = false;
                self.history_loading = false;
                self.ota_loading = false;
                self.subs_loading = false;
                self.loading = false;
                self.load(env, true)
            }
            ConnectedMsg::Loaded(round, r) if round == self.round => {
                self.loading = false;
                match r {
                    Ok(st) => {
                        self.reports = st.history.clone();
                        self.oldest = st.history_from;
                        self.status = Some(st);
                        self.error = None;
                        // (read for OASIS only: the rest once the tab's shown)
                        if !self.opened {
                            return (Task::none(), outs);
                        }
                        let home = self.load_home(env);
                        let part = self.load_part(env, false);
                        Task::batch([home, part])
                    }
                    Err(e) => {
                        self.error = Some(e);
                        Task::none()
                    }
                }
            }
            ConnectedMsg::Older => self.older(env),
            ConnectedMsg::History(round, from, r) if round == self.round => {
                self.history_loading = false;
                match r {
                    Ok(list) => {
                        self.oldest = Some(from);
                        merge_reports(&mut self.reports, list);
                        self.describe_new(env)
                    }
                    Err(FordError::SignedOut) => {
                        self.history_error = Some("Sign in to PTS to see more history.".into());
                        Task::none()
                    }
                    Err(e) => {
                        self.history_error = Some(format!("Couldn't get older history ({}).", e.text()));
                        Task::none()
                    }
                }
            }
            ConnectedMsg::EveryReport(on) => {
                self.every_report = on;
                Task::none()
            }
            ConnectedMsg::Modules(round, r) if round == self.round => {
                if let Ok(m) = r {
                    self.modules = m;
                }
                Task::none()
            }
            ConnectedMsg::Described(round, r) if round == self.round => {
                self.describing = false;
                match r {
                    Ok(d) => {
                        self.descs.merge(d);
                        // any that came in meanwhile
                        self.describe_new(env)
                    }
                    Err(_) => Task::none(),
                }
            }
            ConnectedMsg::Articles(round, r) if round == self.round => {
                self.articles_loading = false;
                match r {
                    Ok(a) => self.articles = Some(a),
                    Err(e) => {
                        self.articles = Some(Articles::new());
                        self.articles_failed = match e {
                            FordError::SignedOut => "sign in to PTS".into(),
                            e => e.text(),
                        };
                    }
                }
                Task::none()
            }
            ConnectedMsg::Pick(key, on) => {
                self.picked.retain(|k| *k != key);
                if on {
                    self.picked.push(key);
                }
                Task::none()
            }
            ConnectedMsg::PickAll(on) => {
                let keys: Vec<String> = self.recent().into_iter().map(|r| r.dtc.key()).collect();
                self.picked.retain(|k| !keys.contains(k));
                if on {
                    self.picked.extend(keys);
                }
                Task::none()
            }
            ConnectedMsg::ClearPicked => {
                self.picked.clear();
                Task::none()
            }
            ConnectedMsg::Search(q) => {
                outs.push(Out::SearchDiagnostics(q));
                Task::none()
            }
            ConnectedMsg::SearchPicked => {
                let q: Vec<String> = self.picked.iter().take(DIAG_MAX).cloned().collect();
                if !q.is_empty() {
                    outs.push(Out::SearchDiagnostics(q.join(" ")));
                }
                Task::none()
            }
            ConnectedMsg::OpenArticle(a) => {
                outs.push(Out::OpenBulletin {
                    kind: a.kind,
                    id: a.id,
                    title: a.title,
                    url: a.url,
                });
                Task::none()
            }
            ConnectedMsg::Copy(t, what) => {
                outs.push(Out::Copy(t, what));
                Task::none()
            }
            ConnectedMsg::OtaLoaded(round, r) if round == self.round => {
                self.ota_loading = false;
                self.ota = Some(r);
                Task::none()
            }
            ConnectedMsg::SubsLoaded(round, r) if round == self.round => {
                self.subs_loading = false;
                self.subs = Some(r);
                Task::none()
            }
            ConnectedMsg::ToggleUpdate(id) => {
                if !self.open_updates.remove(&id) {
                    self.open_updates.insert(id);
                }
                Task::none()
            }
            ConnectedMsg::Everything(on) => {
                self.everything = on;
                Task::none()
            }
            ConnectedMsg::SignIn => {
                outs.push(Out::SignIn);
                Task::none()
            }
            ConnectedMsg::ShowTab => {
                outs.push(Out::ShowConnected);
                Task::none()
            }
            // an answer to a read since replaced
            _ => Task::none(),
        };
        (task, outs)
    }

    /// Each DTC in the status and the history once (by its module and key), newest first
    fn recent(&self) -> Vec<Recent> {
        let mut out: Vec<Recent> = Vec::new();
        let mut add = |d: &Dtc, when: When, odo: Option<f64>, now: bool| {
            let (module, module_desc) = module_of(&self.modules, d);
            let key = d.key();
            let r = match out.iter_mut().position(|r| r.module == module && r.dtc.key() == key) {
                Some(i) => &mut out[i],
                None => {
                    out.push(Recent {
                        dtc: d.clone(),
                        module,
                        module_desc,
                        count: 0,
                        now: false,
                        last: None,
                        last_odo: None,
                        first: None,
                    });
                    out.last_mut().expect("just added")
                }
            };
            r.count += 1;
            r.now |= now;
            if when.is_some() && (r.last.is_none() || when > r.last) {
                r.last = when;
                r.last_odo = odo;
            }
            if when.is_some() && (r.first.is_none() || when < r.first) {
                r.first = when;
            }
            if r.dtc.desc.is_empty() && !d.desc.is_empty() {
                r.dtc.desc = d.desc.clone();
            }
        };
        if let Some(now) = self.status.as_ref().and_then(|s| s.now.as_ref()) {
            for d in &now.dtcs {
                add(d, now.when, None, true);
            }
        }
        for ev in &self.reports {
            for d in &ev.dtcs {
                add(d, ev.when, ev.odometer_km, false);
            }
        }
        out.sort_by(|a, b| b.last.cmp(&a.last));
        out
    }

    fn miles(&self) -> bool {
        self.status.as_ref().is_none_or(CvStatus::miles)
    }
}

// =====================================================================================================================
// Small pieces
// =====================================================================================================================

/// "Nov 10, 2026, 7:00 AM" (the computer's time)
pub fn fmt_time(t: When) -> String {
    t.map(|d| d.with_timezone(&Local).format("%b %-d, %Y, %-I:%M %p").to_string())
        .unwrap_or_default()
}

/// "Nov 10, 2026"
pub fn fmt_date(t: When) -> String {
    t.map(|d| d.with_timezone(&Local).format("%b %-d, %Y").to_string())
        .unwrap_or_else(|| "—".into())
}

fn h2<'a>(t: impl text::IntoFragment<'a>) -> Element<'a, ConnectedMsg> {
    text(t).size(17).font(style::BOLD).into()
}

fn h3<'a>(t: impl text::IntoFragment<'a>) -> Element<'a, ConnectedMsg> {
    text(t).size(14).font(style::BOLD).into()
}

fn card<'a>(content: impl Into<Element<'a, ConnectedMsg>>) -> Element<'a, ConnectedMsg> {
    container(content).padding(14).width(Length::Fill).style(style::card).into()
}

/// A card of a fixed width (they wrap side by side)
fn small_card<'a>(content: impl Into<Element<'a, ConnectedMsg>>) -> Element<'a, ConnectedMsg> {
    container(content).padding(14).width(340).style(style::card).into()
}

fn msg_card<'a>(t: impl text::IntoFragment<'a>, good: bool) -> Element<'a, ConnectedMsg> {
    let mark: Element<'a, ConnectedMsg> = if good { pill("OK", GREEN) } else { Space::with_width(0).into() };
    card(row![mark, text(t)].spacing(8).align_y(Alignment::Center))
}

fn busy<'a>(t: impl text::IntoFragment<'a>) -> Element<'a, ConnectedMsg> {
    card(
        row![anim::dot(Color::from_rgb8(BLUE[0], BLUE[1], BLUE[2]), true), text(t)]
            .spacing(8)
            .align_y(Alignment::Center),
    )
}

fn tip<'a>(content: impl Into<Element<'a, ConnectedMsg>>, t: String) -> Element<'a, ConnectedMsg> {
    if t.is_empty() {
        return content.into();
    }
    tooltip(
        content,
        container(text(t).size(12)).padding([6, 10]).style(style::floating_card),
        tooltip::Position::Top,
    )
    .gap(4.0)
    .into()
}

/// A label / value line; the value copies itself when clicked
fn reading_row(r: &Reading) -> Element<'_, ConnectedMsg> {
    let value: Element<'_, ConnectedMsg> = match &r.value {
        None => text("—").style(style::muted).into(),
        Some(v) => {
            let t = text(v.as_str()).size(14).font(style::BOLD);
            let t = if r.warn { t.style(text::danger) } else { t };
            button(t)
                .padding(0)
                .style(button::text)
                .on_press(ConnectedMsg::Copy(v.clone(), r.label.clone()))
                .into()
        }
    };
    let mut line = row![text(r.label.as_str()).size(13).width(150).style(style::muted), value]
        .spacing(10)
        .align_y(Alignment::Center);
    if r.when.is_some() {
        line = line
            .push(horizontal_space())
            .push(text(fmt_time(r.when)).size(11).style(style::muted));
    }
    line.into()
}

/// A failed read: sign in, or try again
fn failed<'a>(e: &FordError, what: &str, retry: ConnectedMsg) -> Element<'a, ConnectedMsg> {
    let (title, why, button_) = match e {
        FordError::SignedOut => (
            "Sign in to PTS".to_string(),
            "Connected vehicle data needs your PTS sign-in.".to_string(),
            ("Sign in to PTS", ConnectedMsg::SignIn),
        ),
        e => (format!("Couldn't load {what}"), e.text(), ("Try again", retry)),
    };
    card(
        column![
            text(title).size(18).font(style::BOLD),
            text(why).style(style::muted),
            button(text(button_.0)).padding([6, 14]).style(button::primary).on_press(button_.1),
        ]
        .spacing(8),
    )
}

/// An article's chip (opens it in the TSBs tab)
fn article_chip(a: &CvArticle, short: bool) -> Element<'_, ConnectedMsg> {
    let color = match a.kind.as_str() {
        "SSM" => AMBER,
        "GSB" => GRAY,
        _ => BLUE,
    };
    let mut r = row![pill(a.kind.as_str(), color), text(a.id.as_str()).size(13).font(style::BOLD)]
        .spacing(6)
        .align_y(Alignment::Center);
    if !short && !a.title.is_empty() {
        r = r.push(text(a.title.as_str()).size(12));
    }
    let b = button(r)
        .padding([2, 6])
        .style(style::ghost)
        .on_press(ConnectedMsg::OpenArticle(a.clone()));
    tip(b, format!("{} {}: {}\nOpen it in the TSBs tab", a.kind, a.id, a.title))
}

// =====================================================================================================================
// The tab
// =====================================================================================================================

pub fn view(cv: &ConnectedView, sample: bool) -> Element<'_, ConnectedMsg> {
    // the parts along the top, and Refresh
    let parts = anim::tabs(
        Part::ALL.iter().position(|p| *p == cv.part).unwrap_or(0),
        Part::ALL
            .iter()
            .map(|&p| {
                button(text(p.label()).size(13))
                    .padding([4, 10])
                    .style(style::tab(cv.part == p))
                    .on_press(ConnectedMsg::Part(p))
                    .into()
            })
            .collect(),
    );
    let busy_now = cv.loading || cv.ota_loading || cv.subs_loading;
    let refresh = button(text(if busy_now { "Refreshing…" } else { "Refresh" }).size(13))
        .padding([5, 12])
        .style(style::ghost)
        .on_press_maybe((!cv.loading).then_some(ConnectedMsg::Refresh));
    let bar = container(row![parts, horizontal_space(), refresh].spacing(8).align_y(Alignment::Center)).padding([0, 16]);

    let body: Element<'_, ConnectedMsg> = if sample {
        card(text("Connected vehicle data shows for your real ROs, once you're signed in to Xtime.").style(style::muted))
    } else if let Some(st) = &cv.status {
        match cv.part {
            Part::Home => home(cv, st),
            Part::Ota => ota(cv, st),
            Part::Software => software(cv),
        }
    } else if cv.loading {
        busy("Asking Ford for the vehicle’s latest data…")
    } else if let Some(e) = &cv.error {
        failed(e, "connected vehicle data", ConnectedMsg::Refresh)
    } else {
        card(
            column![
                text("Connected Vehicle").size(18).font(style::BOLD),
                text("What the vehicle reports to Ford through its modem.").style(style::muted),
                button(text("Load"))
                    .padding([6, 14])
                    .style(button::primary)
                    .on_press(ConnectedMsg::Refresh),
            ]
            .spacing(8),
        )
    };
    let mut body = column![body].spacing(18);
    if cv.status.is_some() {
        if let Some(e) = &cv.error {
            body = body.push(text(format!("Couldn't refresh ({})", e.text())).size(13).style(text::danger));
        }
    }
    let scrolled = scrollable(container(body).padding(16).width(Length::Fill))
        .height(Length::Fill)
        .remember(format!("cv:{}:{}", cv.vin, cv.part.label()));
    let mut page = column![container(bar).padding([10, 0]), scrolled];
    if cv.part == Part::Home && !cv.picked.is_empty() && !sample {
        page = page.push(pick_bar(cv));
    }
    page.into()
}

/// The DTCs ticked: search them together in Diagnostics
fn pick_bar(cv: &ConnectedView) -> Element<'_, ConnectedMsg> {
    let n = cv.picked.len();
    let mut r = row![
        text(format!("{n} DTC{} picked", if n == 1 { "" } else { "s" })).font(style::BOLD),
        text(cv.picked.join(", ")).size(13).font(style::MONO).width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if n > DIAG_MAX {
        r = r.push(
            text(format!("Diagnostics searches the first {DIAG_MAX}"))
                .size(12)
                .style(style::muted),
        );
    }
    r = r
        .push(
            button(text("Search in Diagnostics").size(13))
                .padding([5, 12])
                .style(button::primary)
                .on_press(ConnectedMsg::SearchPicked),
        )
        .push(
            button(text("Copy").size(13))
                .padding([5, 10])
                .style(style::ghost)
                .on_press(ConnectedMsg::Copy(cv.picked.join(", "), "DTCs".into())),
        )
        .push(
            button(text("Clear").size(13))
                .padding([5, 10])
                .style(style::ghost)
                .on_press(ConnectedMsg::ClearPicked),
        );
    container(r).padding([10, 16]).width(Length::Fill).style(style::panel).into()
}

// ---------- Home ----------

fn status_notes(st: &CvStatus) -> Vec<Element<'_, ConnectedMsg>> {
    let mut out: Vec<Element<'_, ConnectedMsg>> = st.notes.iter().map(|n| msg_card(n.as_str(), false)).collect();
    if !st.has_data() {
        out.push(msg_card("Ford has no connected vehicle data for this vehicle.", false));
    }
    out
}

fn home<'a>(cv: &'a ConnectedView, st: &'a CvStatus) -> Element<'a, ConnectedMsg> {
    let mut head = row![h2("Connected vehicle status")].spacing(10).align_y(Alignment::Center);
    if st.latest.is_some() {
        head = head.push(text(format!("Last update {}", fmt_time(st.latest))).size(13).style(style::muted));
    }
    let mut c = Column::new().spacing(14).push(head);
    for n in status_notes(st) {
        c = c.push(n);
    }
    let mut cards: Vec<Element<'a, ConnectedMsg>> = Vec::new();
    if !st.readings.is_empty() {
        cards.push(small_card(
            column![
                h3("Vehicle status"),
                Column::with_children(st.readings.iter().map(reading_row)).spacing(8)
            ]
            .spacing(10),
        ));
    }
    if let Some(t) = &st.tires {
        cards.push(tire_card(t));
    }
    if let Some(now) = &st.now {
        let mut rows = Column::new().spacing(8);
        if now.when.is_some() {
            rows = rows.push(text(format!("Reported {}", fmt_time(now.when))).size(12).style(style::muted));
        }
        for d in &now.dtcs {
            rows = rows.push(dtc_row(cv, d, true));
        }
        for a in &now.alerts {
            rows = rows.push(alert_row(a));
        }
        if !now.notable() {
            rows = rows.push(
                row![pill("OK", GREEN), text("No codes or alerts reported")]
                    .spacing(8)
                    .align_y(Alignment::Center),
            );
        }
        cards.push(
            container(column![h3("DTCs & alerts now"), rows].spacing(10))
                .padding(14)
                .width(Length::Fill)
                .style(style::card)
                .into(),
        );
    }
    if !cards.is_empty() {
        c = c.push(Row::with_children(cards).spacing(12).wrap());
    }
    c = c.push(recent(cv)).push(history(cv));
    // everything Ford sent, in case something above is missing
    if !st.everything.is_empty() {
        let toggle = checkbox(format!("All values Ford sent ({})", st.everything.len()), cv.everything)
            .text_size(13)
            .on_toggle(ConnectedMsg::Everything);
        c = c.push(toggle);
        if cv.everything {
            c = c.push(card(Column::with_children(st.everything.iter().map(reading_row)).spacing(6)));
        }
    }
    c.into()
}

fn tire_card<'a>(t: &'a ford::connected::Tires) -> Element<'a, ConnectedMsg> {
    let tire = |x: &Tire| -> Element<'a, ConnectedMsg> {
        let low = x.low;
        let v = x.value.map(|v| v.to_string()).unwrap_or_else(|| "—".into());
        let b = container(
            column![
                text(v).size(20).font(style::BOLD),
                text(x.label.clone()).size(11).style(style::muted)
            ]
            .align_x(Alignment::Center)
            .spacing(1),
        )
        .padding([6, 10])
        .width(110)
        .align_x(Alignment::Center)
        .style(move |th: &iced::Theme| {
            let mut s = style::card(th);
            s.border.color = if low {
                Color::from_rgb8(RED[0], RED[1], RED[2])
            } else {
                style::colors(th).line
            };
            s.border.width = if low { 2.0 } else { 1.0 };
            s
        });
        let why = [x.status.clone(), fmt_time(x.when)]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        tip(b, why)
    };
    let front: Element<'a, ConnectedMsg> = Row::with_children(t.front.iter().map(&tire)).spacing(40).into();
    let rear: Element<'a, ConnectedMsg> = Row::with_children(t.rear.iter().map(&tire)).spacing(40).into();
    // the vehicle from above, between its axles
    let body = container(Space::new(70, 54)).style(|th: &iced::Theme| container::Style {
        background: Some(style::colors(th).hover.into()),
        border: iced::Border {
            color: style::colors(th).line,
            width: 1.0,
            radius: 14.0.into(),
        },
        ..Default::default()
    });
    let mut c = column![
        h3(format!("Tire pressure ({})", t.unit)),
        column![front, body, rear].spacing(8).align_x(Alignment::Center),
    ]
    .spacing(10)
    .width(Length::Fill)
    .align_x(Alignment::Center);
    if t.front_recommended.is_some() || t.rear_recommended.is_some() {
        let f = |v: Option<i64>| v.map(|v| v.to_string()).unwrap_or_else(|| "—".into());
        c = c.push(
            text(format!(
                "Recommended: front {} · rear {}",
                f(t.front_recommended),
                f(t.rear_recommended)
            ))
            .size(12)
            .style(style::muted),
        );
    }
    if let Some(sys) = &t.system {
        c = c.push(text(format!("System: {sys}")).size(12).style(text::danger));
    }
    container(c).padding(14).width(300).style(style::card).into()
}

/// One DTC: tick, module, the code (searches it), what it means, its TSBs, Copy
fn dtc_row<'a>(cv: &'a ConnectedView, d: &'a Dtc, with_pick: bool) -> Element<'a, ConnectedMsg> {
    let (module, module_desc) = module_of(&cv.modules, d);
    let key = d.key();
    let mut r = Row::new().spacing(10).align_y(Alignment::Center);
    if with_pick {
        let k = key.clone();
        r = r.push(checkbox("", cv.picked.contains(&key)).on_toggle(move |on| ConnectedMsg::Pick(k.clone(), on)));
    }
    r = r.push(tip(text(module.clone()).size(13).width(48).font(style::MONO), module_desc));
    let detail = cv.descs.detail(d);
    let code = button(text(d.full()).size(14).font(style::MONO))
        .padding([2, 6])
        .style(style::ghost)
        .on_press(ConnectedMsg::Search(key));
    let why = if detail.is_empty() {
        "Search this DTC in Diagnostics".to_string()
    } else {
        format!("{detail}\n\nSearch this DTC in Diagnostics")
    };
    r = r.push(tip(code, why));
    let title = cv.descs.title(d);
    r = r.push(text(title).size(13).width(Length::Fill));
    if let Some(arts) = &cv.articles {
        let list = articles_for(arts, d, &module);
        if !list.is_empty() {
            r = r.push(Row::with_children(list.into_iter().map(|a| article_chip(a, true))).spacing(4));
        }
    }
    r.push(
        button(text("Copy").size(12))
            .padding([2, 8])
            .style(style::ghost)
            .on_press(ConnectedMsg::Copy(d.full(), "DTC".into())),
    )
    .into()
}

fn alert_row(a: &ford::connected::Alert) -> Element<'_, ConnectedMsg> {
    let color = if a.clear {
        GREEN
    } else if a.urgent {
        RED
    } else {
        AMBER
    };
    let mut r = row![
        pill(a.kind.as_str(), color),
        text(if a.what.is_empty() { "(no description)" } else { a.what.as_str() }).size(13)
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if !a.action.is_empty() {
        r = r.push(text(a.action.as_str()).size(12).style(style::muted));
    }
    r.into()
}

/// Recent DTCs: every DTC in the history once
fn recent(cv: &ConnectedView) -> Element<'_, ConnectedMsg> {
    let list = cv.recent();
    let mut head = row![h2("Recent DTCs")].spacing(10).align_y(Alignment::Center);
    if let Some(o) = cv.oldest {
        let days = (Local::now().date_naive() - o).num_days();
        head = head.push(text(format!("last {days} days")).size(13).style(style::muted));
    }
    if cv.articles.is_none() {
        head = head.push(text("· checking TSBs…").size(13).style(style::muted));
    } else if !cv.articles_failed.is_empty() {
        head = head.push(
            text(format!("· couldn't check TSBs ({})", cv.articles_failed))
                .size(13)
                .style(style::muted),
        );
    }
    let c = column![head].spacing(10);
    if list.is_empty() {
        return c
            .push(if cv.history_loading {
                busy("Loading the history…")
            } else {
                msg_card("No DTCs reported in this time.", true)
            })
            .into();
    }
    let all_on = list.iter().all(|r| cv.picked.contains(&r.dtc.key()));
    let head_row = row![
        checkbox("", all_on).on_toggle(ConnectedMsg::PickAll),
        text("Module").size(12).width(56).style(style::muted),
        text("DTC").size(12).width(Length::FillPortion(4)).style(style::muted),
        text("Seen").size(12).width(70).style(style::muted),
        text("Last seen").size(12).width(120).style(style::muted),
        text("First seen").size(12).width(110).style(style::muted),
        text("TSBs / GSBs / SSMs")
            .size(12)
            .width(Length::FillPortion(3))
            .style(style::muted),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let mut rows = Column::new().spacing(0).push(container(head_row).padding([4, 8]));
    for (i, r) in list.into_iter().enumerate() {
        let key = r.dtc.key();
        let detail = cv.descs.detail(&r.dtc);
        let title = cv.descs.title(&r.dtc);
        let k = key.clone();
        let code = button(text(key.clone()).size(14).font(style::MONO))
            .padding([2, 6])
            .style(style::ghost)
            .on_press(ConnectedMsg::Search(key.clone()));
        let why = if detail.is_empty() {
            "Search this DTC in Diagnostics".to_string()
        } else {
            format!("{detail}\n\nSearch this DTC in Diagnostics")
        };
        let mut dtc = column![tip(code, why)].spacing(2);
        if !title.is_empty() {
            dtc = dtc.push(text(title).size(12));
        }
        let mut seen = row![text(format!("{}×", r.count)).size(13)].spacing(6).align_y(Alignment::Center);
        if r.now {
            seen = seen.push(pill("now", RED));
        }
        let mut last = column![text(fmt_date(r.last)).size(13)].spacing(1);
        if let Some(o) = r.last_odo {
            last = last.push(text(dist(o, cv.miles())).size(11).style(style::muted));
        }
        let arts: Element<'_, ConnectedMsg> = match &cv.articles {
            Some(a) => {
                let list = articles_for(a, &r.dtc, &r.module);
                if list.is_empty() {
                    text("None").size(12).style(style::muted).into()
                } else {
                    Column::with_children(list.into_iter().map(|a| article_chip(a, false)))
                        .spacing(4)
                        .into()
                }
            }
            None => Space::with_width(0).into(),
        };
        let line = row![
            checkbox("", cv.picked.contains(&key)).on_toggle(move |on| ConnectedMsg::Pick(k.clone(), on)),
            tip(text(r.module).size(13).width(56).font(style::MONO), r.module_desc),
            container(dtc).width(Length::FillPortion(4)),
            container(seen).width(70),
            container(last).width(120),
            text(fmt_date(r.first)).size(13).width(110),
            container(arts).width(Length::FillPortion(3)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        rows = rows.push(
            container(line)
                .padding([6, 8])
                .width(Length::Fill)
                .style(move |t: &iced::Theme| container::Style {
                    background: (i % 2 == 1).then(|| style::colors(t).hover.into()),
                    ..Default::default()
                }),
        );
    }
    c.push(container(rows).padding(6).width(Length::Fill).style(style::card)).into()
}

/// The health history: each report (with DTCs or alerts, or all of them), newest first
fn history(cv: &ConnectedView) -> Element<'_, ConnectedMsg> {
    let head = row![
        h2("Health history (DTCs & alerts)"),
        horizontal_space(),
        checkbox("Show every report", cv.every_report)
            .text_size(13)
            .on_toggle(ConnectedMsg::EveryReport),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let mut c = column![head].spacing(10);
    if let Some(o) = cv.oldest {
        let days = (Local::now().date_naive() - o).num_days();
        c = c.push(
            text(format!("Reports from the last {days} days · {} in all", cv.reports.len()))
                .size(12)
                .style(style::muted),
        );
    }
    let shown: Vec<&Report> = cv.reports.iter().filter(|r| cv.every_report || r.notable()).collect();
    if shown.is_empty() {
        if !cv.history_loading {
            c = c.push(msg_card(
                if cv.reports.is_empty() {
                    "No health reports in this time."
                } else {
                    "No DTCs or alerts in these reports."
                },
                !cv.reports.is_empty(),
            ));
        }
    } else {
        let miles = cv.miles();
        let list = Column::with_children(shown.into_iter().map(|ev| {
            let mut top = row![text(fmt_time(ev.when)).size(13).font(style::BOLD)]
                .spacing(12)
                .align_y(Alignment::Center);
            if let Some(o) = ev.odometer_km {
                top = top.push(text(dist(o, miles)).size(12).style(style::muted));
            }
            if ev.volts.is_some() || ev.soc.is_some() {
                let parts: Vec<String> = [ev.volts.map(|v| format!("{v:.1} V")), ev.soc.map(|s| format!("{}%", s.floor()))]
                    .into_iter()
                    .flatten()
                    .collect();
                top = top.push(text(format!("12V {}", parts.join(" · "))).size(12).style(style::muted));
            }
            if ev.ota_change {
                top = top.push(pill("Software changed", BLUE));
            }
            let mut e = column![top].spacing(6);
            for d in &ev.dtcs {
                e = e.push(dtc_row(cv, d, true));
            }
            for a in &ev.alerts {
                e = e.push(alert_row(a));
            }
            container(e).padding([8, 4]).width(Length::Fill).into()
        }))
        .spacing(4);
        c = c.push(card(list));
    }
    if let Some(e) = &cv.history_error {
        c = c.push(msg_card(e.as_str(), false));
    }
    if cv.history_loading {
        c = c.push(busy("Asking Ford for older history…"));
    } else if cv.can_older() {
        c = c.push(
            button(text(format!("Show older ({HISTORY_DAYS} days more)")).size(13))
                .padding([6, 14])
                .style(style::ghost)
                .on_press(ConnectedMsg::Older),
        );
    }
    c.into()
}

// ---------- Over-the-air updates ----------

fn class_color(c: StepClass) -> [u8; 3] {
    match c {
        StepClass::Ok => GREEN,
        StepClass::Bad => RED,
        StepClass::Gone => GRAY,
        StepClass::Pending => AMBER,
    }
}

fn fact<'a>(k: &'a str, v: String) -> Option<Element<'a, ConnectedMsg>> {
    (!v.is_empty()).then(|| {
        row![text(k).size(12).width(120).style(style::muted), text(v).size(13)]
            .spacing(8)
            .into()
    })
}

fn yes_no(v: Option<bool>) -> String {
    match v {
        Some(true) => "Yes".into(),
        Some(false) => "No".into(),
        None => String::new(),
    }
}

fn update_card<'a>(cv: &'a ConnectedView, u: &'a OtaUpdate) -> Element<'a, ConnectedMsg> {
    let mut head = row![
        pill(
            if u.status.is_empty() { "Status unknown" } else { u.status.as_str() },
            class_color(u.class)
        ),
        text(fmt_time(u.when)).size(13),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if !u.kind.is_empty() {
        head = head.push(pill(u.kind.as_str(), GRAY));
    }
    let facts: Vec<Element<'a, ConnectedMsg>> = [
        fact("Software ID", u.id.clone()),
        fact("Group", u.groups.clone()),
        fact("Created", fmt_time(u.created)),
        fact("Expires", fmt_time(u.expires)),
        fact("Vehicle inhibit", yes_no(u.inhibit)),
        fact("Wi-Fi required", yes_no(u.wifi)),
        fact("Ford Power-Up", u.power_up.clone()),
        fact("Recall", u.recall.clone()),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut c = column![head, Column::with_children(facts).spacing(4)].spacing(10);
    if !u.modules.is_empty() {
        c = c.push(
            Column::with_children(u.modules.iter().map(|(m, sw)| {
                row![
                    text(if m.is_empty() { "—" } else { m.as_str() })
                        .size(13)
                        .font(style::BOLD)
                        .width(80),
                    text(sw.as_str()).size(12).font(style::MONO)
                ]
                .spacing(8)
                .into()
            }))
            .spacing(3),
        );
    }
    let open = cv.open_updates.contains(&u.id);
    if u.notes.is_some() || u.steps.len() > 1 {
        let label = match (&u.notes, open) {
            (_, true) => "Hide details".to_string(),
            (Some(n), false) if !n.header.is_empty() => format!("Release notes: {} · every step ({})", n.header, u.steps.len()),
            (Some(_), false) => format!("Release notes · every step ({})", u.steps.len()),
            (None, false) => format!("Every step ({})", u.steps.len()),
        };
        c = c.push(
            button(text(label).size(12))
                .padding([3, 10])
                .style(style::ghost)
                .on_press(ConnectedMsg::ToggleUpdate(u.id.clone())),
        );
    }
    if open {
        if let Some(n) = &u.notes {
            let mut notes = Column::new().spacing(6);
            if !n.header.is_empty() {
                notes = notes.push(text(n.header.as_str()).font(style::BOLD));
            }
            if !n.text.is_empty() {
                notes = notes.push(text(n.text.as_str()).size(13));
            }
            for (h, t) in &n.groups {
                if !h.is_empty() {
                    notes = notes.push(text(h.as_str()).size(13).font(style::BOLD));
                }
                if !t.is_empty() {
                    notes = notes.push(text(t.as_str()).size(13));
                }
            }
            if !n.disclaimer.is_empty() {
                notes = notes.push(text(n.disclaimer.as_str()).size(12).style(style::muted));
            }
            c = c.push(container(notes).padding(10).width(Length::Fill).style(style::panel));
        }
        if u.steps.len() > 1 {
            c = c.push(
                Column::with_children(u.steps.iter().map(|s| {
                    let mut r = row![
                        text(fmt_time(s.when)).size(12).width(170),
                        pill(if s.text.is_empty() { "—" } else { s.text.as_str() }, class_color(s.class)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center);
                    if !s.code.is_empty() {
                        r = r.push(text(s.code.as_str()).size(11).font(style::MONO).style(style::muted));
                    }
                    if s.count > 1 {
                        r = r.push(
                            text(format!("×{} since {}", s.count, fmt_time(s.since)))
                                .size(11)
                                .style(style::muted),
                        );
                    }
                    r.into()
                }))
                .spacing(4),
            );
        }
    }
    card(c)
}

fn ota<'a>(cv: &'a ConnectedView, st: &'a CvStatus) -> Element<'a, ConnectedMsg> {
    let res = match (&cv.ota, cv.ota_loading) {
        (_, true) | (None, false) => return busy("Asking Ford for the vehicle’s over-the-air updates…"),
        (Some(Err(e)), _) => return failed(e, "over-the-air updates", ConnectedMsg::Part(Part::Ota)),
        (Some(Ok(o)), _) => o,
    };
    let newest = res.updates.first();
    let mut latest = column![h3("Latest status")].spacing(8);
    match newest {
        Some(u) => {
            let rows = [
                (
                    "Status",
                    if u.status.is_empty() {
                        "Unknown".to_string()
                    } else {
                        u.status.clone()
                    },
                ),
                ("Date / time", fmt_time(u.when)),
                ("Update ID", u.id.clone()),
                ("Release notes", u.notes.as_ref().map(|n| n.header.clone()).unwrap_or_default()),
            ];
            for (k, v) in rows {
                latest = latest.push(reading_row_owned(k, v));
            }
        }
        None => {
            latest = latest.push(
                text(if res.updates_failed.is_empty() {
                    "No OTA history for this VIN.".to_string()
                } else {
                    format!("Couldn't get OTA history ({}).", res.updates_failed)
                })
                .size(13)
                .style(style::muted),
            );
        }
    }
    let mut top: Vec<Element<'a, ConnectedMsg>> = vec![small_card(latest)];
    if !st.ota_settings.is_empty() {
        top.push(small_card(
            column![
                h3("Vehicle OTA settings"),
                Column::with_children(st.ota_settings.iter().map(reading_row)).spacing(8)
            ]
            .spacing(8),
        ));
    }
    // what's lined up next
    let seq: Element<'a, ConnectedMsg> = if res.sequence.is_empty() {
        text(if res.sequence_failed.is_empty() {
            "No upcoming updates lined up for this vehicle.".to_string()
        } else {
            format!("Couldn't get the update sequence ({}).", res.sequence_failed)
        })
        .size(13)
        .style(style::muted)
        .into()
    } else {
        Column::with_children(res.sequence.iter().map(|q| {
            row![
                text(q.sequence.as_str()).size(13).width(30),
                text(q.modules.as_str()).size(13).width(Length::FillPortion(2)),
                text(q.software.as_str()).size(12).font(style::MONO).width(Length::FillPortion(2)),
                if q.qualified {
                    pill("Qualified", GREEN)
                } else {
                    pill("Pending qualification", AMBER)
                },
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        }))
        .spacing(6)
        .into()
    };
    top.push(small_card(column![h3("Upcoming updates sequence"), seq].spacing(8)));
    let in_process: Vec<&OtaUpdate> = res.updates.iter().filter(|u| u.in_process).collect();
    let done: Vec<&OtaUpdate> = res.updates.iter().filter(|u| !u.in_process).collect();
    let section = |title: String, list: Vec<&'a OtaUpdate>, empty: String| -> Element<'a, ConnectedMsg> {
        let mut c = column![h2(title)].spacing(10);
        if list.is_empty() {
            c = c.push(msg_card(empty, false));
        } else {
            for u in list {
                c = c.push(update_card(cv, u));
            }
        }
        c.into()
    };
    let count = |n: usize| if n == 0 { String::new() } else { format!(" ({n})") };
    column![
        h2("Over-the-air updates"),
        Row::with_children(top).spacing(12).wrap(),
        section(
            format!("Currently in process{}", count(in_process.len())),
            in_process,
            "No updates in process.".into()
        ),
        section(
            format!("History{}", count(done.len())),
            done,
            if res.updates_failed.is_empty() {
                "No OTA update attempts for this vehicle in the last 180 days.".into()
            } else {
                "OTA history couldn’t be loaded.".into()
            }
        ),
    ]
    .spacing(16)
    .into()
}

fn reading_row_owned<'a>(k: &'a str, v: String) -> Element<'a, ConnectedMsg> {
    row![
        text(k).size(13).width(150).style(style::muted),
        if v.is_empty() {
            text("—").style(style::muted)
        } else {
            text(v).size(14).font(style::BOLD)
        }
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

// ---------- Software ----------

fn software(cv: &ConnectedView) -> Element<'_, ConnectedMsg> {
    let res = match (&cv.subs, cv.subs_loading) {
        (_, true) | (None, false) => return busy("Asking Ford for the vehicle’s software subscriptions…"),
        (Some(Err(e)), _) => return failed(e, "software subscriptions", ConnectedMsg::Part(Part::Software)),
        (Some(Ok(s)), _) => s,
    };
    let mut head = row![h2("Vehicle software subscriptions")].spacing(10).align_y(Alignment::Center);
    if res.asked.is_some() {
        head = head.push(text(format!("As of {}", fmt_time(res.asked))).size(13).style(style::muted));
    }
    let mut c = column![head].spacing(12);
    for n in &res.notes {
        c = c.push(msg_card(n.as_str(), false));
    }
    if res.products.is_empty() {
        c = c.push(msg_card(
            if res.none {
                "Ford has no subscription information for this vehicle."
            } else {
                "No software subscriptions for this vehicle."
            },
            false,
        ));
    } else {
        let head_row = row![
            text("Subscription").size(12).width(Length::FillPortion(3)).style(style::muted),
            text("Current status").size(12).width(Length::FillPortion(3)).style(style::muted),
            text("Type").size(12).width(Length::FillPortion(1)).style(style::muted),
            text("Expiration date").size(12).width(Length::FillPortion(2)).style(style::muted),
        ]
        .spacing(10);
        let mut rows = Column::new().push(container(head_row).padding([4, 8]));
        for (i, p) in res.products.iter().enumerate() {
            let color = if p.active {
                if p.expiring_soon { AMBER } else { GREEN }
            } else if p.trial {
                AMBER
            } else {
                RED
            };
            let mut status = row![pill(p.status.as_str(), color)].spacing(6).align_y(Alignment::Center);
            if p.expiring_soon {
                status = status.push(text("expiring soon").size(12).style(style::muted));
            }
            if p.renew_anytime {
                status = status.push(text("renew anytime").size(12).style(style::muted));
            }
            let line = row![
                text(p.name.as_str()).size(13).font(style::BOLD).width(Length::FillPortion(3)),
                container(status).width(Length::FillPortion(3)),
                text(p.kind.as_str()).size(13).width(Length::FillPortion(1)),
                text(fmt_date(p.expires)).size(13).width(Length::FillPortion(2)),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            rows = rows.push(
                container(line)
                    .padding([7, 8])
                    .width(Length::Fill)
                    .style(move |t: &iced::Theme| container::Style {
                        background: (i % 2 == 1).then(|| style::colors(t).hover.into()),
                        ..Default::default()
                    }),
            );
        }
        c = c.push(container(rows).padding(6).width(Length::Fill).style(style::card));
    }
    c.push(
        text("Customers start or renew subscriptions in the Ford app or the vehicle’s screen; dealers can activate them through the Dealer Member Portal.")
            .size(12)
            .style(style::muted),
    )
    .into()
}

// =====================================================================================================================
// On the OASIS tab
// =====================================================================================================================

/// The vehicle's connected vehicle status, as OASIS shows it (its odometer is also used for warranty coverage)
pub fn oasis_card(cv: &ConnectedView) -> Element<'_, ConnectedMsg> {
    let head = row![
        h2("Connected vehicle data"),
        horizontal_space(),
        button(text(if cv.loading { "Refreshing…" } else { "↻" }).size(13))
            .padding([3, 10])
            .style(style::ghost)
            .on_press_maybe((!cv.loading).then_some(ConnectedMsg::Refresh)),
        button(text("Open Connected Vehicle").size(13))
            .padding([3, 10])
            .style(style::ghost)
            .on_press(ConnectedMsg::ShowTab),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let mut c = column![head].spacing(10);
    let Some(st) = &cv.status else {
        return c
            .push(match (&cv.error, cv.loading) {
                (_, true) => busy("Asking the vehicle’s latest data…"),
                (Some(FordError::SignedOut), _) => msg_card("Sign in to PTS to see connected vehicle data.", false),
                (Some(e), _) => msg_card(format!("Couldn't get connected vehicle data ({}).", e.text()), false),
                (None, false) => msg_card("Connected vehicle data loads with OASIS.", false),
            })
            .into();
    };
    for n in status_notes(st) {
        c = c.push(n);
    }
    let mut cards: Vec<Element<'_, ConnectedMsg>> = Vec::new();
    if !st.readings.is_empty() {
        let mut rows = Column::with_children(st.readings.iter().map(reading_row)).spacing(8);
        if let Some(t) = &st.tires {
            let line: Vec<String> = t
                .front
                .iter()
                .chain(&t.rear)
                .map(|x| x.value.map(|v| v.to_string()).unwrap_or_else(|| "—".into()))
                .collect();
            let mut r = row![
                text(format!("Tires ({})", t.unit)).size(13).width(150).style(style::muted),
                text(line.join(" · ")).size(14).font(style::BOLD),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            if t.front.iter().chain(&t.rear).any(|x| x.low) || t.system.is_some() {
                r = r.push(pill("Low", RED));
            }
            rows = rows.push(r);
        }
        cards.push(small_card(column![h3("Vehicle status"), rows].spacing(10)));
    }
    if let Some(now) = &st.now {
        let mut rows = Column::new().spacing(6);
        if now.when.is_some() {
            rows = rows.push(text(format!("Reported {}", fmt_time(now.when))).size(12).style(style::muted));
        }
        for d in &now.dtcs {
            let (m, _) = module_of(&cv.modules, d);
            let title = cv.descs.title(d);
            rows = rows.push(
                row![
                    text(m).size(13).width(48).font(style::MONO),
                    button(text(d.full()).size(13).font(style::MONO))
                        .padding([1, 6])
                        .style(style::ghost)
                        .on_press(ConnectedMsg::Copy(d.full(), "DTC".into())),
                    text(title).size(12),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        for a in &now.alerts {
            rows = rows.push(alert_row(a));
        }
        if !now.notable() {
            rows = rows.push(
                row![pill("OK", GREEN), text("None reported").size(13)]
                    .spacing(8)
                    .align_y(Alignment::Center),
            );
        }
        cards.push(small_card(column![h3("DTCs & health alerts"), rows].spacing(10)));
    }
    if !cards.is_empty() {
        c = c.push(Row::with_children(cards).spacing(12).wrap());
    }
    c.into()
}

/// Today (for the warranty's dates)
pub fn today() -> NaiveDate {
    Utc::now().with_timezone(&Local).date_naive()
}
