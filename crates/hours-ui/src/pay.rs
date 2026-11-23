//! Predictive Pay in the hours report (the math: crates/hours `pay`): what the pay period's check should come to,
//! from its flagged hours, and the form it's set up with (two pay stubs, the benefits and fees on them, the
//! retirement plan and the W-4s).

use hours::pay::{self, FedStatus, Line, MoStatus, MoW4, PaySetup, Stub, W4, money};
use iced::widget::{Column, Space, button, checkbox, column, container, horizontal_space, pick_list, row, text, text_input};
use iced::{Alignment, Element, Length};
use ui_kit::style;

use crate::{HoursMsg, fmt_h};

// ---------- the guess ----------

/// The pay period's check, as it's guessed (or the offer to set it up)
/// `start`: when the pay period shown starts (ms: its hourly rate); `rate_form`: a new rate being typed
pub(crate) fn predicted<'a>(
    setup: Option<&PaySetup>,
    flagged: f64,
    start: i64,
    open: bool,
    rate_form: Option<&'a RateForm>,
) -> Element<'a, HoursMsg> {
    let Some(s) = setup else {
        return container(
            row![
                column![
                    text("Predictive Pay").size(16).font(style::BOLD),
                    text("See what this check should come to from the hours you've flagged. Set it up once from two of your pay stubs.")
                        .size(13)
                        .style(style::muted),
                ]
                .spacing(4)
                .width(Length::Fill),
                button(text("Set it up").font(style::BOLD))
                    .padding([6, 14])
                    .style(button::success)
                    .on_press(HoursMsg::Pay(PayMsg::Edit)),
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        )
        .padding(14)
        .width(Length::Fill)
        .style(style::card)
        .into();
    };
    let learned = pay::learn(s);
    let Some(b) = pay::predict_for(s, &learned, flagged, start) else {
        return container(
            row![
                text("Predictive Pay needs your hourly rate, or the hours and what a stub came to.")
                    .size(13)
                    .width(Length::Fill),
                button(text("Finish setting it up"))
                    .padding([5, 12])
                    .style(style::ghost)
                    .on_press(HoursMsg::Pay(PayMsg::Edit)),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding(14)
        .width(Length::Fill)
        .style(style::card)
        .into();
    };

    let head = row![
        column![
            text("Predicted take-home").size(12).style(style::muted),
            text(if flagged > 0.0 { money(b.net) } else { "$0.00".into() })
                .size(28)
                .font(style::BOLD),
            text(format!(
                "{} flagged at {}/h · {} before anything comes out · spiffs not counted",
                fmt_h(b.hours),
                money(b.rate),
                money(b.gross)
            ))
            .size(12)
            .style(style::muted),
        ]
        .spacing(2)
        .width(Length::Fill),
        button(text(if open { "Hide details" } else { "Details" }).size(13))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(HoursMsg::Pay(PayMsg::Details)),
        button(text("New hourly rate").size(13))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(HoursMsg::Pay(PayMsg::NewRate)),
        button(text("Change").size(13))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(HoursMsg::Pay(PayMsg::Edit)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut c = column![head].spacing(12);
    if let Some(f) = rate_form {
        c = c.push(rate_editor(f, b.rate));
    }
    if open {
        let line = |name: String, amount: f64, minus: bool, bold: bool| -> Element<'a, HoursMsg> {
            let v = if minus && amount > 0.0 {
                format!("−{}", money(amount))
            } else {
                money(amount)
            };
            let (name, v) = (text(name).size(13), text(v).size(13));
            let (name, v) = if bold {
                (name.font(style::BOLD), v.font(style::BOLD))
            } else {
                (name, v)
            };
            row![
                name.width(Length::Fill),
                container(v).width(120).align_x(iced::alignment::Horizontal::Right)
            ]
            .spacing(8)
            .into()
        };
        let mut list = Column::new().spacing(5).push(line("Gross pay".into(), b.gross, false, true));
        if s.retirement > 0.0 {
            let kind = if s.roth { "Roth, after taxes" } else { "before income taxes" };
            list = list.push(line(
                format!("Retirement ({}%, {kind})", trimmed(s.retirement)),
                b.retirement,
                true,
                false,
            ));
        }
        for ((name, amount, pretax), fit) in b.lines.iter().zip(&learned.lines) {
            let when = if *pretax { "before taxes" } else { "after taxes" };
            list = list.push(line(format!("{name} ({}, {when})", fit.describe()), *amount, true, false));
        }
        let t = b.taxes;
        for (name, v) in [
            ("Federal income tax", t.federal),
            ("Social Security", t.social),
            ("Medicare", t.medicare),
            ("Missouri income tax", t.state),
        ] {
            list = list.push(line(name.into(), v, true, false));
        }
        list = list.push(Space::with_height(2)).push(line("Take-home".into(), b.net, false, true));
        c = c.push(container(list).padding([10, 12]).width(Length::Fill).style(style::panel));

        let mut notes = Column::new().spacing(3);
        if !s.rates.is_empty() {
            notes = notes.push(text(rate_history(s, &learned)).size(12).style(style::muted));
        }
        for ch in &learned.checks {
            notes = notes.push(text(check_note(ch)).size(12).style(style::muted));
        }
        if let Some(off) = learned.net_off.filter(|o| o.abs() >= 1.0) {
            notes = notes.push(
                text(format!(
                    "Your stubs' take-home was {} {} than all this comes to: is a benefit or fee missing?",
                    money(off.abs()),
                    if off > 0.0 { "more" } else { "less" }
                ))
                .size(12)
                .style(style::muted),
            );
        }
        if learned.checks.is_empty() {
            notes = notes.push(
                text("Taxes are figured from your W-4s. Type the taxes from your stubs into the setup to check them.")
                    .size(12)
                    .style(style::muted),
            );
        }
        c = c.push(notes);
    }
    container(c).padding(14).width(Length::Fill).style(style::card).into()
}

/// Typing a new hourly rate: from this pay period, or the next
fn rate_editor<'a>(f: &'a RateForm, now: f64) -> Element<'a, HoursMsg> {
    // (the one it's from: filled in)
    let picked: fn(&iced::Theme, button::Status) -> button::Style = button::primary;
    let when = |label: &'static str, next: bool| {
        button(text(label).size(13))
            .padding([5, 12])
            .style(if f.next == next { picked } else { style::ghost })
            .on_press(HoursMsg::Pay(PayMsg::RateNext(next)))
    };
    let mut c = column![
        row![
            text("New hourly rate").size(13).font(style::BOLD),
            text_input(&format!("now {}", money(now)), &f.text)
                .on_input(|v| HoursMsg::Pay(PayMsg::RateText(v)))
                .on_submit(HoursMsg::Pay(PayMsg::RateSave))
                .id(rate_field_id())
                .padding([5, 8])
                .size(13)
                .width(120),
            text("from").size(13).style(style::muted),
            when("This pay period", false),
            when("The next one", true),
            horizontal_space(),
            button(text("Cancel").size(13))
                .padding([5, 12])
                .style(style::ghost)
                .on_press(HoursMsg::Pay(PayMsg::RateCancel)),
            button(text("Save").size(13).font(style::BOLD))
                .padding([5, 14])
                .style(button::success)
                .on_press(HoursMsg::Pay(PayMsg::RateSave)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        text("Earlier pay periods keep the rate they had. Your stubs, and everything worked out from them, stay as they are.")
            .size(12)
            .style(style::muted),
    ]
    .spacing(6);
    if let Some(e) = &f.error {
        c = c.push(text(e.as_str()).size(12).style(text::danger));
    }
    container(c).padding([10, 12]).width(Length::Fill).style(style::panel).into()
}

/// The new hourly rate's box (focused when it opens)
pub fn rate_field_id() -> text_input::Id {
    text_input::Id::new("pay-new-rate")
}

/// "Hourly rate: $25.00 from your stubs, $27.50 from Nov 16, 2026"
fn rate_history(s: &PaySetup, l: &pay::Learned) -> String {
    use chrono::TimeZone;
    let mut parts: Vec<String> = Vec::new();
    if let Some(r) = l.rate {
        parts.push(format!("{} from your stubs", money(r)));
    }
    for r in &s.rates {
        let day = chrono::Local
            .timestamp_millis_opt(r.from)
            .single()
            .map(|d| d.format("%b %-d, %Y").to_string())
            .unwrap_or_default();
        parts.push(format!("{} from {day}", money(r.rate)));
    }
    format!("Hourly rate: {}", parts.join(", "))
}

fn check_note(ch: &pay::Check) -> String {
    if ch.matches() {
        format!("{}: your W-4 matches your stubs.", ch.name)
    } else {
        format!(
            "{}: your stubs show {} {} each check than your W-4 works out to, so it's {} each guess.",
            ch.name,
            money(ch.off.abs()),
            if ch.off > 0.0 { "more" } else { "less" },
            if ch.off > 0.0 { "added to" } else { "taken off" }
        )
    }
}

/// 6 → "6", 5.5 → "5.5", 25.75 → "25.75"; nothing → ""
fn trimmed(v: f64) -> String {
    if v == 0.0 {
        return String::new();
    }
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn typed(v: Option<f64>) -> String {
    v.map(trimmed).unwrap_or_default()
}

// ---------- the setup form ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StubField {
    Hours,
    Rate,
    Gross,
    Federal,
    Social,
    Medicare,
    State,
    Net,
}

/// A box in the form
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Stub(usize, StubField),
    LineName(usize),
    /// A line, on a stub
    LineAmount(usize, usize),
    Retirement,
    Dependents,
    OtherIncome,
    Deductions,
    Extra,
    MoExtra,
}

/// What Predictive Pay does
#[derive(Debug, Clone)]
pub enum PayMsg {
    /// Open the setup (with what's set up already)
    Edit,
    Cancel,
    Save,
    /// Show or hide what comes out of the check
    Details,
    Text(Field, String),
    Fed(FedStatus),
    TwoJobs(bool),
    Mo(MoStatus),
    Roth(bool),
    Pretax(usize, bool),
    /// A benefit (before taxes) or a fee (after)
    AddLine(bool),
    RemoveLine(usize),
    /// A new hourly rate (a raise): open it, type it, from this pay period or the next, keep it, or not
    NewRate,
    RateText(String),
    RateNext(bool),
    RateSave,
    RateCancel,
}

/// A new hourly rate, as it's being typed
#[derive(Debug, Clone, Default)]
pub struct RateForm {
    pub text: String,
    /// From the next pay period (else this one)
    pub next: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct StubForm {
    pub hours: String,
    pub rate: String,
    pub gross: String,
    pub federal: String,
    pub social: String,
    pub medicare: String,
    pub state: String,
    pub net: String,
}

impl StubForm {
    fn get(&mut self, f: StubField) -> &mut String {
        match f {
            StubField::Hours => &mut self.hours,
            StubField::Rate => &mut self.rate,
            StubField::Gross => &mut self.gross,
            StubField::Federal => &mut self.federal,
            StubField::Social => &mut self.social,
            StubField::Medicare => &mut self.medicare,
            StubField::State => &mut self.state,
            StubField::Net => &mut self.net,
        }
    }

    fn value(&self, f: StubField) -> &str {
        match f {
            StubField::Hours => &self.hours,
            StubField::Rate => &self.rate,
            StubField::Gross => &self.gross,
            StubField::Federal => &self.federal,
            StubField::Social => &self.social,
            StubField::Medicare => &self.medicare,
            StubField::State => &self.state,
            StubField::Net => &self.net,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LineForm {
    pub name: String,
    pub pretax: bool,
    pub amounts: [String; 2],
}

/// The setup, as it's being typed
#[derive(Debug, Clone, Default)]
pub struct PayForm {
    pub stubs: [StubForm; 2],
    pub lines: Vec<LineForm>,
    pub retirement: String,
    pub roth: bool,
    pub fed: FedStatus,
    pub two_jobs: bool,
    pub dependents: String,
    pub other_income: String,
    pub deductions: String,
    pub extra: String,
    pub mo: MoStatus,
    pub mo_extra: String,
    /// What's wrong with it (when Save was pressed)
    pub error: Option<String>,
}

const STUB_NAMES: [&str; 2] = ["the newest stub", "the stub before it"];

impl PayForm {
    /// The form for what's set up (none yet: a health insurance line to start with)
    pub fn from_setup(s: Option<&PaySetup>) -> Self {
        let Some(s) = s else {
            return PayForm {
                lines: vec![LineForm {
                    name: "Health insurance".into(),
                    pretax: true,
                    ..LineForm::default()
                }],
                ..PayForm::default()
            };
        };
        let stub = |i: usize| {
            let st = s.stubs.get(i).cloned().unwrap_or_default();
            StubForm {
                hours: trimmed(st.hours),
                rate: typed(st.rate),
                gross: typed(st.gross),
                federal: typed(st.federal),
                social: typed(st.social),
                medicare: typed(st.medicare),
                state: typed(st.state),
                net: typed(st.net),
            }
        };
        PayForm {
            stubs: [stub(0), stub(1)],
            lines: s
                .lines
                .iter()
                .map(|l| LineForm {
                    name: l.name.clone(),
                    pretax: l.pretax,
                    amounts: [
                        typed(l.amounts.first().copied().flatten()),
                        typed(l.amounts.get(1).copied().flatten()),
                    ],
                })
                .collect(),
            retirement: trimmed(s.retirement),
            roth: s.roth,
            fed: s.w4.status,
            two_jobs: s.w4.two_jobs,
            dependents: trimmed(s.w4.dependents),
            other_income: trimmed(s.w4.other_income),
            deductions: trimmed(s.w4.deductions),
            extra: trimmed(s.w4.extra),
            mo: s.mo.status,
            mo_extra: trimmed(s.mo.extra),
            error: None,
        }
    }

    /// The setup it makes (or what's wrong with it, in words). `at`: left for the app to set.
    pub fn to_setup(&self) -> Result<PaySetup, String> {
        let mut bad: Vec<String> = Vec::new();
        let mut num = |what: String, t: &str| match pay::parse_amount(t) {
            Ok(v) => v,
            Err(()) => {
                bad.push(what);
                None
            }
        };
        let mut stubs = Vec::new();
        for (i, f) in self.stubs.iter().enumerate() {
            let n = STUB_NAMES[i];
            stubs.push(Stub {
                hours: num(format!("{n}'s hours"), &f.hours).unwrap_or(0.0),
                rate: num(format!("{n}'s rate"), &f.rate),
                gross: num(format!("{n}'s gross pay"), &f.gross),
                federal: num(format!("{n}'s federal tax"), &f.federal),
                social: num(format!("{n}'s Social Security"), &f.social),
                medicare: num(format!("{n}'s Medicare"), &f.medicare),
                state: num(format!("{n}'s Missouri tax"), &f.state),
                net: num(format!("{n}'s take-home"), &f.net),
            });
        }
        let mut lines = Vec::new();
        for (i, l) in self.lines.iter().enumerate() {
            if l.name.trim().is_empty() && l.amounts.iter().all(|a| a.trim().is_empty()) {
                continue;
            }
            let name = if l.name.trim().is_empty() {
                format!("line {}", i + 1)
            } else {
                l.name.trim().to_string()
            };
            lines.push(Line {
                name: l.name.trim().to_string(),
                pretax: l.pretax,
                amounts: (0..2).map(|k| num(format!("{name} on {}", STUB_NAMES[k]), &l.amounts[k])).collect(),
            });
        }
        let retirement = num("the retirement plan's percentage".into(), &self.retirement).unwrap_or(0.0);
        let w4 = W4 {
            status: self.fed,
            two_jobs: self.two_jobs,
            dependents: num("Step 3 (dependents)".into(), &self.dependents).unwrap_or(0.0),
            other_income: num("Step 4(a) (other income)".into(), &self.other_income).unwrap_or(0.0),
            deductions: num("Step 4(b) (deductions)".into(), &self.deductions).unwrap_or(0.0),
            extra: num("Step 4(c) (extra each check)".into(), &self.extra).unwrap_or(0.0),
        };
        let mo = MoW4 {
            status: self.mo,
            extra: num("Missouri's extra each check".into(), &self.mo_extra).unwrap_or(0.0),
        };
        if !bad.is_empty() {
            return Err(format!("These aren't amounts: {}.", bad.join(", ")));
        }
        if stubs[0].rate().is_none() {
            return Err("Fill in the newest stub's hours, and its hourly rate or what it came to.".into());
        }
        if retirement > 100.0 {
            return Err("The retirement plan is a percentage of the check (6 for 6%).".into());
        }
        Ok(PaySetup {
            at: 0,
            // (the new rates since the stubs: kept from what's set up, when it's saved)
            rates: Vec::new(),
            w4,
            mo,
            retirement,
            roth: self.roth,
            stubs,
            lines,
        })
    }

    /// A change to the form (Edit, Save, Cancel and Details are the report's)
    pub(crate) fn update(&mut self, msg: PayMsg) {
        self.error = None;
        match msg {
            PayMsg::Text(f, v) => match f {
                Field::Stub(i, sf) => *self.stubs[i.min(1)].get(sf) = v,
                Field::LineName(i) => {
                    if let Some(l) = self.lines.get_mut(i) {
                        l.name = v;
                    }
                }
                Field::LineAmount(i, k) => {
                    if let Some(l) = self.lines.get_mut(i) {
                        l.amounts[k.min(1)] = v;
                    }
                }
                Field::Retirement => self.retirement = v,
                Field::Dependents => self.dependents = v,
                Field::OtherIncome => self.other_income = v,
                Field::Deductions => self.deductions = v,
                Field::Extra => self.extra = v,
                Field::MoExtra => self.mo_extra = v,
            },
            PayMsg::Fed(s) => self.fed = s,
            PayMsg::TwoJobs(on) => self.two_jobs = on,
            PayMsg::Mo(s) => self.mo = s,
            PayMsg::Roth(on) => self.roth = on,
            PayMsg::Pretax(i, on) => {
                if let Some(l) = self.lines.get_mut(i) {
                    l.pretax = on;
                }
            }
            PayMsg::AddLine(pretax) => self.lines.push(LineForm {
                pretax,
                ..LineForm::default()
            }),
            PayMsg::RemoveLine(i) => {
                if i < self.lines.len() {
                    self.lines.remove(i);
                }
            }
            PayMsg::Edit
            | PayMsg::Cancel
            | PayMsg::Save
            | PayMsg::Details
            | PayMsg::NewRate
            | PayMsg::RateText(_)
            | PayMsg::RateNext(_)
            | PayMsg::RateSave
            | PayMsg::RateCancel => {}
        }
    }
}

// the form's columns
const STUB_W: f32 = 130.0;
const PRETAX_W: f32 = 110.0;
const REMOVE_W: f32 = 76.0;

fn input<'a>(value: &'a str, placeholder: &'a str, f: Field, width: f32) -> Element<'a, HoursMsg> {
    text_input(placeholder, value)
        .on_input(move |v| HoursMsg::Pay(PayMsg::Text(f, v)))
        .on_submit(HoursMsg::Pay(PayMsg::Save))
        .padding([5, 8])
        .size(13)
        .width(width)
        .into()
}

/// A column's heading
fn th<'a>(t: &'a str, w: Length) -> Element<'a, HoursMsg> {
    container(text(t).size(12).font(style::BOLD).style(style::muted)).width(w).into()
}

fn heading<'a>(title: &'a str, about: &'a str) -> Element<'a, HoursMsg> {
    column![text(title).size(16).font(style::BOLD), text(about).size(12).style(style::muted)]
        .spacing(3)
        .into()
}

fn panel<'a>(body: impl Into<Element<'a, HoursMsg>>) -> Element<'a, HoursMsg> {
    container(body).padding(14).width(Length::Fill).style(style::panel).into()
}

/// The setup form (the hours report shows it in place of the report)
pub(crate) fn form<'a>(f: &'a PayForm) -> Element<'a, HoursMsg> {
    let head = row![
        text("Set up Predictive Pay").size(22).font(style::BOLD).width(Length::Fill),
        button(text("Cancel"))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(HoursMsg::Pay(PayMsg::Cancel)),
        button(text("Save").font(style::BOLD))
            .padding([5, 16])
            .style(button::success)
            .on_press(HoursMsg::Pay(PayMsg::Save)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let intro = text(
        "Fill this in once from your two newest pay stubs. What comes out for benefits and fees is worked out from \
         them (a fixed amount, a percentage, or some of each), and the taxes are figured from your W-4s and checked \
         against the stubs. It's encrypted, and syncs to your other computers and your phone.",
    )
    .size(13)
    .style(style::muted);

    // the stubs, side by side
    let stub_row = |label: &'static str, help: &'static str, sf: StubField| -> Element<'a, HoursMsg> {
        row![
            column![text(label).size(13), text(help).size(11).style(style::muted)].width(Length::Fill),
            input(f.stubs[0].value(sf), "", Field::Stub(0, sf), STUB_W),
            input(f.stubs[1].value(sf), "", Field::Stub(1, sf), STUB_W),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    };
    let stubs = column![
        row![
            Space::with_width(Length::Fill),
            th("Newest", STUB_W.into()),
            th("The one before", STUB_W.into())
        ]
        .spacing(10),
        stub_row("Hours paid", "Your flagged hours on it", StubField::Hours),
        stub_row("Hourly rate", "Or leave it blank and fill in gross pay", StubField::Rate),
        stub_row("Gross pay", "What it came to, before anything came out", StubField::Gross),
        Space::with_height(4),
        text("The taxes it shows (to check your W-4s; blank: not checked)")
            .size(12)
            .font(style::BOLD)
            .style(style::muted),
        stub_row("Federal income tax", "", StubField::Federal),
        stub_row("Social Security", "Sometimes \"OASDI\"", StubField::Social),
        stub_row("Medicare", "", StubField::Medicare),
        stub_row("Missouri income tax", "", StubField::State),
        Space::with_height(4),
        stub_row("Take-home", "Net pay (to check it all adds up)", StubField::Net),
    ]
    .spacing(8);

    // benefits and fees
    let mut lines = Column::new().spacing(8).push(
        row![
            th("Name", Length::Fill),
            th("Newest", STUB_W.into()),
            th("The one before", STUB_W.into()),
            th("Before taxes", PRETAX_W.into()),
            Space::with_width(REMOVE_W),
        ]
        .spacing(10),
    );
    for (i, l) in f.lines.iter().enumerate() {
        lines = lines.push(
            row![
                text_input("Dental insurance, union dues…", &l.name)
                    .on_input(move |v| HoursMsg::Pay(PayMsg::Text(Field::LineName(i), v)))
                    .padding([5, 8])
                    .size(13)
                    .width(Length::Fill),
                input(&l.amounts[0], "$", Field::LineAmount(i, 0), STUB_W),
                input(&l.amounts[1], "$", Field::LineAmount(i, 1), STUB_W),
                container(checkbox("", l.pretax).on_toggle(move |on| HoursMsg::Pay(PayMsg::Pretax(i, on)))).width(PRETAX_W),
                button(text("Remove").size(12))
                    .padding([3, 8])
                    .width(REMOVE_W)
                    .style(style::ghost)
                    .on_press(HoursMsg::Pay(PayMsg::RemoveLine(i))),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
    }
    lines = lines.push(
        row![
            button(text("+ Add a benefit").size(13))
                .padding([5, 12])
                .style(style::ghost)
                .on_press(HoursMsg::Pay(PayMsg::AddLine(true))),
            button(text("+ Add a fee").size(13))
                .padding([5, 12])
                .style(style::ghost)
                .on_press(HoursMsg::Pay(PayMsg::AddLine(false))),
        ]
        .spacing(6),
    );

    let retirement = row![
        input(&f.retirement, "0", Field::Retirement, 80.0),
        text("% of each check").size(13),
        horizontal_space(),
        checkbox("It's a Roth (taken out after taxes)", f.roth)
            .on_toggle(|on| HoursMsg::Pay(PayMsg::Roth(on)))
            .size(16)
            .text_size(13),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let small = |label: &'static str, value: &'a str, fld: Field| -> Element<'a, HoursMsg> {
        column![text(label).size(12).style(style::muted), input(value, "$0", fld, 150.0)]
            .spacing(3)
            .into()
    };
    let federal = column![
        row![
            text("Federal (Step 1c)").size(13).width(Length::Fill),
            pick_list(FedStatus::ALL, Some(f.fed), |s| HoursMsg::Pay(PayMsg::Fed(s)))
                .padding([5, 8])
                .text_size(13),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        checkbox("Step 2(c) is checked (two jobs, or your spouse works too)", f.two_jobs)
            .on_toggle(|on| HoursMsg::Pay(PayMsg::TwoJobs(on)))
            .size(16)
            .text_size(13),
        row![
            small("Step 3: dependents (a year)", &f.dependents, Field::Dependents),
            small("Step 4(a): other income", &f.other_income, Field::OtherIncome),
            small("Step 4(b): deductions", &f.deductions, Field::Deductions),
            small("Step 4(c): extra each check", &f.extra, Field::Extra),
        ]
        .spacing(10),
    ]
    .spacing(10);
    let missouri = row![
        text("Missouri (MO W-4)").size(13).width(Length::Fill),
        pick_list(MoStatus::ALL, Some(f.mo), |s| HoursMsg::Pay(PayMsg::Mo(s)))
            .padding([5, 8])
            .text_size(13),
        column![
            text("Line 2: extra each check").size(12).style(style::muted),
            input(&f.mo_extra, "$0", Field::MoExtra, 150.0)
        ]
        .spacing(3),
    ]
    .spacing(10)
    .align_y(Alignment::End);

    let mut c = column![
        head,
        intro,
        heading("Your two newest pay stubs", "Newest first. Two with different hours work best."),
        panel(stubs),
        heading(
            "Benefits and fees",
            "Everything else that comes out, as each stub shows it. Health, dental and vision insurance usually come \
             out before taxes. Leave out your retirement plan: it's next.",
        ),
        panel(lines),
        heading("Retirement plan", "What your 401(k) takes out of each check."),
        panel(retirement),
        heading(
            "Your W-4s",
            "As you filled them in for the dealership. Leave anything you didn't fill in blank."
        ),
        panel(column![federal, missouri].spacing(16)),
    ]
    .spacing(12);
    if let Some(e) = &f.error {
        c = c.push(text(e.as_str()).size(13).style(text::danger));
    }
    c = c.push(
        row![
            horizontal_space(),
            button(text("Cancel"))
                .padding([5, 12])
                .style(style::ghost)
                .on_press(HoursMsg::Pay(PayMsg::Cancel)),
            button(text("Save").font(style::BOLD))
                .padding([5, 16])
                .style(button::success)
                .on_press(HoursMsg::Pay(PayMsg::Save)),
        ]
        .spacing(8),
    );
    c.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_round_trips() {
        let mut f = PayForm::from_setup(None);
        assert_eq!(f.lines.len(), 1);
        // nothing typed: says what's needed
        assert!(f.to_setup().is_err());
        f.update(PayMsg::Text(Field::Stub(0, StubField::Hours), "80".into()));
        f.update(PayMsg::Text(Field::Stub(0, StubField::Rate), "$25.50".into()));
        f.update(PayMsg::Text(Field::LineAmount(0, 0), "95.5".into()));
        f.update(PayMsg::Text(Field::Retirement, "6%".into()));
        f.update(PayMsg::AddLine(false));
        let s = f.to_setup().unwrap();
        // (the blank fee isn't kept)
        assert_eq!(s.lines.len(), 1);
        assert_eq!((s.stubs[0].rate, s.retirement), (Some(25.5), 6.0));
        let back = PayForm::from_setup(Some(&s));
        assert_eq!(
            (
                back.stubs[0].rate.as_str(),
                back.retirement.as_str(),
                back.lines[0].amounts[0].as_str()
            ),
            ("25.5", "6", "95.5")
        );
        f.update(PayMsg::Text(Field::Stub(1, StubField::Gross), "lots".into()));
        assert!(f.to_setup().unwrap_err().contains("the stub before it's gross pay"));
    }
}
