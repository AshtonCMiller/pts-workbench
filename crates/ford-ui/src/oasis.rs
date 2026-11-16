//! The OASIS tab (the Electron app's `manual/oasis/`): Ford's OASIS for the vehicle's VIN, redrawn in the app's look.
//!
//!   the top     the vehicle, its VIN (click to copy), when OASIS made it, and what to look at further down
//!               (open field service actions, warning messages)
//!   Vehicle information   Ford's fields in cards (Vehicle, Powertrain, Paint…); click a value to copy it
//!   Field service actions the open recalls, each with Copy FSA; click one (or one of Ford's notices) to open it
//!                         in a Service Manual tab of its own (its dealer bulletin, instructions…)
//!   Warning messages
//!   Warranty coverage     each coverage checked against the warranty start and the odometer (typed in here)
//!
//! Read when the tab is first shown, and again with Refresh. (The vehicle's picture, its connected vehicle data and
//! Retrieve SVC come with later stages.)

use chrono::NaiveDate;
use ford::oasis::{self, Coverage, Field, Oasis, Standing};
use iced::widget::{Column, Row, Space, button, column, container, horizontal_space, row, text, text_input};
use iced::{Alignment, Color, Element, Length, Task};
use ui_kit::scroll::scrollable;
use ui_kit::{pill, style};

use crate::{Env, FordError, Out};

/// OASIS's own address (FSA documents' addresses are read from its page)
const OASIS_PAGE: &str = "https://www.fordtechservice.dealerconnection.com/oasis/Index";

#[derive(Debug, Clone, Default)]
pub struct OasisView {
    pub vin: String,
    pub data: Option<Oasis>,
    pub loading: bool,
    pub error: Option<FordError>,
    /// When it was read (seconds since 1970)
    pub loaded_at: Option<i64>,
    /// The odometer typed in (for the warranty's mileage)
    pub odometer: String,
    /// The vehicle is being opened in PTS (to set it up), and whether that's been tried already
    pub setting_up: bool,
    pub set_up_tried: bool,
}

#[derive(Debug, Clone)]
pub enum OasisMsg {
    Refresh,
    Loaded(Result<Oasis, FordError>),
    Odometer(String),
    Copy(String, String),
    SignIn,
    /// The vehicle was opened in PTS (to set it up): how it went
    SetUp(Result<(), String>),
    /// Open an FSA (its place in the list), or one of Ford's notices, in a tab of its own
    OpenFsa(usize),
    OpenNotice(usize),
}

impl OasisView {
    pub fn new(vin: impl Into<String>) -> Self {
        Self {
            vin: vin.into(),
            ..Default::default()
        }
    }

    /// Reads OASIS, unless it's being read or was read already (unless `force`)
    pub fn load(&mut self, env: &Env, force: bool) -> Task<OasisMsg> {
        if self.loading || env.sample || self.vin.trim().is_empty() || (self.data.is_some() && !force) {
            return Task::none();
        }
        self.loading = true;
        let (ford, vin) = (env.ford.clone(), self.vin.clone());
        Task::perform(async move { ford.oasis(&vin).await.map_err(FordError::from) }, OasisMsg::Loaded)
    }

    pub fn update(&mut self, msg: OasisMsg, env: &Env) -> (Task<OasisMsg>, Vec<Out>) {
        let mut outs = Vec::new();
        let task = match msg {
            OasisMsg::Refresh => {
                // asked again: setting up may be tried again too
                self.set_up_tried = false;
                self.load(env, true)
            }
            OasisMsg::Loaded(r) => {
                self.loading = false;
                match r {
                    Ok(o) => {
                        self.data = Some(o);
                        self.error = None;
                        self.loaded_at = Some(crate::now());
                    }
                    // PTS hasn't shown this vehicle yet: open it there once (unseen), then try again
                    Err(FordError::NotSetUp) if !self.set_up_tried => {
                        self.set_up_tried = true;
                        self.setting_up = true;
                        self.error = None;
                        outs.push(Out::SetUpPts(self.vin.clone()));
                    }
                    Err(e) => self.error = Some(e),
                }
                Task::none()
            }
            OasisMsg::SetUp(r) => {
                self.setting_up = false;
                match r {
                    Ok(()) => self.load(env, true),
                    Err(e) => {
                        self.error = Some(FordError::Problem(format!("Couldn't open this vehicle in PTS to set it up ({e})")));
                        Task::none()
                    }
                }
            }
            OasisMsg::Odometer(s) => {
                self.odometer = s.chars().filter(|c| c.is_ascii_digit() || *c == ',').take(9).collect();
                Task::none()
            }
            OasisMsg::Copy(t, what) => {
                outs.push(Out::Copy(t, what));
                Task::none()
            }
            OasisMsg::SignIn => {
                outs.push(Out::SignIn);
                Task::none()
            }
            OasisMsg::OpenFsa(i) => {
                let item = self.data.as_ref().and_then(|d| d.fsa.items.get(i)).cloned();
                if let Some((f, url)) = item.and_then(|f| ford::doc::resolve(OASIS_PAGE, &f.url).map(|u| (f, u))) {
                    outs.push(Out::OpenDoc {
                        kind: "FSA".into(),
                        id: f.number,
                        title: f.description,
                        url,
                    });
                }
                Task::none()
            }
            OasisMsg::OpenNotice(i) => {
                let notice = self.data.as_ref().and_then(|d| d.fsa.notices.get(i)).cloned();
                if let Some((t, url)) = notice.and_then(|(t, u)| ford::doc::resolve(OASIS_PAGE, &u).map(|u| (t, u))) {
                    outs.push(Out::OpenDoc {
                        kind: "Notice".into(),
                        id: String::new(),
                        title: t,
                        url,
                    });
                }
                Task::none()
            }
        };
        (task, outs)
    }

    fn odometer_miles(&self) -> Option<f64> {
        let n: String = self.odometer.chars().filter(char::is_ascii_digit).collect();
        n.parse().ok()
    }
}

// ---------- the view ----------

fn h2<'a>(t: &'a str) -> Element<'a, OasisMsg> {
    text(t).size(17).font(style::BOLD).into()
}

fn card<'a>(content: impl Into<Element<'a, OasisMsg>>) -> Element<'a, OasisMsg> {
    container(content).padding(14).width(Length::Fill).style(style::card).into()
}

/// A short message in a card (nothing found, all clear…), with a coloured mark
fn note<'a>(t: impl text::IntoFragment<'a>, good: bool) -> Element<'a, OasisMsg> {
    let mark: Element<'a, OasisMsg> = if good {
        pill("OK", [0x15, 0x80, 0x3d])
    } else {
        Space::with_width(0).into()
    };
    card(row![mark, text(t)].spacing(8).align_y(Alignment::Center))
}

/// Text that copies itself when clicked
fn copyable<'a>(value: &'a str, what: &str, mono: bool) -> Element<'a, OasisMsg> {
    let t = text(value).size(14);
    let t = if mono { t.font(style::MONO) } else { t };
    button(t)
        .padding(0)
        .style(button::text)
        .on_press(OasisMsg::Copy(value.to_string(), what.to_string()))
        .into()
}

/// What's showing: OASIS, or why it isn't (loading, signed out, a problem)
/// OASIS (scrolling on its own)
pub fn view<'a>(o: &'a OasisView, sample: bool, today: NaiveDate) -> Element<'a, OasisMsg> {
    scrollable(container(page(o, sample, today)).padding(16).width(Length::Fill))
        .height(Length::Fill)
        .into()
}

fn page<'a>(o: &'a OasisView, sample: bool, today: NaiveDate) -> Element<'a, OasisMsg> {
    let state = |title: &'a str, why: String, button_: Option<(&'a str, OasisMsg)>| -> Element<'a, OasisMsg> {
        let mut c = column![text(title).size(18).font(style::BOLD), text(why).style(style::muted)].spacing(8);
        if let Some((label, msg)) = button_ {
            c = c.push(button(text(label)).padding([6, 14]).style(button::primary).on_press(msg));
        }
        card(c)
    };
    let Some(data) = &o.data else {
        return match (&o.error, o.loading) {
            _ if sample => state(
                "OASIS",
                "OASIS shows for your real ROs, once you're signed in to Xtime.".into(),
                None,
            ),
            _ if o.setting_up => state(
                "Setting up PTS…",
                "Opening this vehicle in PTS once, so its pages are ready. This takes a few seconds.".into(),
                None,
            ),
            (_, true) => state("Loading OASIS…", format!("Asking Ford about {}", o.vin), None),
            (Some(FordError::SignedOut), _) => state(
                "Sign in to PTS",
                "OASIS needs your PTS sign-in.".into(),
                Some(("Sign in to PTS", OasisMsg::SignIn)),
            ),
            (Some(e), _) => state("Couldn't load OASIS", e.text(), Some(("Try again", OasisMsg::Refresh))),
            (None, false) => state(
                "OASIS",
                format!("OASIS for {} hasn't been read yet.", o.vin),
                Some(("Load OASIS", OasisMsg::Refresh)),
            ),
        };
    };

    let mut page = Column::new().spacing(18).push(top(o, data));
    if let Some(e) = &o.error {
        page = page.push(text(format!("Couldn't refresh OASIS ({})", e.text())).size(13).style(text::danger));
    }
    // the vehicle's information
    page = page.push(h2("Vehicle information"));
    if data.groups.is_empty() {
        page = page.push(note("OASIS didn't include the vehicle information for this VIN.", false));
    } else {
        let cards = data.groups.iter().map(|g| {
            let rows = Column::with_children(g.fields.iter().map(field_row)).spacing(6);
            container(column![text(g.title.as_str()).font(style::BOLD), rows].spacing(10))
                .padding(14)
                .width(330)
                .style(style::card)
                .into()
        });
        page = page.push(Row::with_children(cards).spacing(12).wrap());
    }
    page = page.push(fsa(data)).push(warnings(data)).push(warranty(o, data, today));
    page.into()
}

fn field_row(f: &Field) -> Element<'_, OasisMsg> {
    let value: Element<'_, OasisMsg> = if f.value.is_empty() {
        text("—").style(style::muted).into()
    } else {
        copyable(&f.value, &f.label, f.code)
    };
    row![text(f.label.as_str()).size(13).width(140).style(style::muted), value]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
}

fn top<'a>(o: &'a OasisView, d: &'a Oasis) -> Element<'a, OasisMsg> {
    let title = if d.title.is_empty() { "OASIS" } else { d.title.as_str() };
    let mut sub = row![copyable(&d.vin, "VIN", true)].spacing(10).align_y(Alignment::Center);
    if !d.stamp.is_empty() {
        sub = sub.push(text(format!("OASIS as of {}", d.stamp)).size(13).style(style::muted));
    }
    // what to look at further down
    let mut chips = row![].spacing(8);
    let open = d.fsa.items.len();
    if open > 0 {
        chips = chips.push(pill(
            format!("{open} open field service action{}", if open == 1 { "" } else { "s" }),
            [0xdc, 0x26, 0x26],
        ));
    } else if d.fsa.found {
        chips = chips.push(pill("No open field service actions", [0x15, 0x80, 0x3d]));
    }
    let warns = d.warnings.lines.len();
    if warns > 0 {
        chips = chips.push(pill(
            format!("{warns} warning message{}", if warns == 1 { "" } else { "s" }),
            [0xd9, 0x77, 0x06],
        ));
    }
    let refresh = button(text(if o.loading { "Refreshing…" } else { "Refresh" }).size(13))
        .padding([5, 12])
        .style(style::ghost)
        .on_press_maybe((!o.loading).then_some(OasisMsg::Refresh));
    card(
        column![
            row![text(title).size(22).font(style::BOLD), horizontal_space(), refresh].align_y(Alignment::Center),
            sub,
            chips,
        ]
        .spacing(8),
    )
}

fn fsa(d: &Oasis) -> Element<'_, OasisMsg> {
    let mut c = column![h2("Field service actions (recalls)")].spacing(10);
    let f = &d.fsa;
    if !f.found {
        return c
            .push(note("OASIS didn't include field service actions for this VIN.", false))
            .into();
    }
    if f.items.is_empty() {
        c = c.push(note("No open field service actions for this VIN.", true));
    } else {
        let rows = Column::with_children(f.items.iter().enumerate().map(|(n, i)| {
            // the number opens its document (when OASIS links one)
            let number = container(text(i.number.as_str()).font(style::MONO).size(14))
                .padding([3, 8])
                .style(style::pill(Color::from_rgb8(0xdc, 0x26, 0x26)));
            let number: Element<'_, OasisMsg> = if i.url.trim().is_empty() {
                number.into()
            } else {
                button(number).padding(0).style(button::text).on_press(OasisMsg::OpenFsa(n)).into()
            };
            row![
                number,
                text(i.description.as_str()).width(Length::Fill),
                button(text("Copy FSA").size(12))
                    .padding([3, 10])
                    .style(style::ghost)
                    .on_press(OasisMsg::Copy(i.number.clone(), "FSA".into())),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
        }))
        .spacing(10);
        c = c.push(card(rows));
    }
    for (n, (t, _)) in f.notices.iter().enumerate() {
        c = c.push(
            button(text(t.as_str()).size(13).style(text::primary))
                .padding(0)
                .style(button::text)
                .on_press(OasisMsg::OpenNotice(n)),
        );
    }
    c.into()
}

fn warnings(d: &Oasis) -> Element<'_, OasisMsg> {
    let c = column![h2("Warning messages")].spacing(10);
    let w = &d.warnings;
    if w.lines.is_empty() {
        let t = if w.found {
            "No warning messages for this VIN."
        } else {
            "OASIS didn't include warning messages for this VIN."
        };
        return c.push(note(t, w.found)).into();
    }
    let rows = Column::with_children(w.lines.iter().map(|l| {
        row![pill("!", [0xd9, 0x77, 0x06]), text(l.as_str())]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
    }))
    .spacing(8);
    c.push(card(rows)).into()
}

fn warranty<'a>(o: &'a OasisView, d: &'a Oasis, today: NaiveDate) -> Element<'a, OasisMsg> {
    let w = &d.warranty;
    let mut c = column![h2("Warranty coverage")].spacing(10);
    let date = |d: Option<NaiveDate>, raw: &str| {
        d.map(oasis::nice_date)
            .unwrap_or_else(|| if raw.is_empty() { "—".into() } else { raw.to_string() })
    };
    let fact = |label: &'a str, value: String| -> Element<'a, OasisMsg> {
        row![text(label).size(13).style(style::muted), text(value).font(style::BOLD)]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
    };
    let mut facts = Row::new()
        .spacing(22)
        .align_y(Alignment::Center)
        .push(fact("Warranty start", date(w.start, &w.start_text)))
        .push(fact("Build", date(w.build, &w.build_text)));
    if let Some(m) = w.months_in_service {
        facts = facts.push(fact("In service", format!("{m} months")));
    }
    facts = facts.push(
        row![
            text("Odometer").size(13).style(style::muted),
            text_input("miles", &o.odometer).on_input(OasisMsg::Odometer).width(110).padding(5),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    );
    c = c.push(facts.wrap());
    if w.start.is_none() && w.build.is_some() {
        c = c.push(note("No warranty start date in OASIS: time is counted from the build date.", false));
    }
    if w.coverages.is_empty() {
        return c.push(note("OASIS listed no warranty coverage for this VIN.", false)).into();
    }
    let start = w.start.or(w.build);
    let odo = o.odometer_miles();
    let rows = Column::with_children(w.coverages.iter().map(|cov| coverage(cov, start, odo, today))).spacing(14);
    c.push(card(rows)).into()
}

fn coverage(c: &Coverage, start: Option<NaiveDate>, odo: Option<f64>, today: NaiveDate) -> Element<'_, OasisMsg> {
    let (standing, why) = oasis::check(c, start, odo, today);
    let color = match standing {
        Standing::Covered => [0x15, 0x80, 0x3d],
        Standing::Expired => [0xdc, 0x26, 0x26],
        Standing::CheckMiles => [0xd9, 0x77, 0x06],
        Standing::SeeTerms => [0x5d, 0x66, 0x70],
    };
    let kind = if c.kind.is_empty() { "Coverage" } else { c.kind.as_str() };
    let mut col = column![
        row![
            pill(standing.label(), color),
            text(kind).font(style::BOLD),
            text(c.description.as_str()).size(13).style(style::muted),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        text(why).size(13),
    ]
    .spacing(4);
    if !c.info.is_empty() {
        col = col.push(text(c.info.as_str()).size(12).style(style::muted));
    }
    col.into()
}
