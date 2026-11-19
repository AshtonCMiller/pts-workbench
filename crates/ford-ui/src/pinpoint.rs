//! Pinpoint tests in the Workshop Manual (the Electron app's `manual/viewer/pinpoints.js`): Ford's single-step view.
//! A test slides in over its procedure, one step at a time: what to check, the question, and its answers as big
//! buttons; an answer goes on to its step (or, when it depends on the vehicle, offers the steps to pick from), or
//! ends the test with what to do. Back goes to the step before, Start over to the first; "Show all steps" lists
//! them all. Opening a test and each answer are recorded to Vehicle History, as Ford's own manual does, so warranty
//! can see the test was run and its results (older tests: Ford records only their start).
//!
//! The answering itself (what's recorded, where an answer goes) is in [`crate::manual`]; this is the test's state
//! and how it's drawn.

use std::collections::HashMap;
use std::time::Instant;

use ford::doc::{Answer, Block, Pinpoint, Step};
use iced::widget::{Column, Space, button, column, container, horizontal_space, row, text};
use iced::{Alignment, Border, Color, Element, Length, Theme};
use ui_kit::scroll::scrollable;
use ui_kit::{anim, pill, style};

use crate::FordError;
use crate::document::{self, DocMsg, Pic};

/// Where a test is: its step, the steps before (and their answers), what's been answered, what Vehicle History said
#[derive(Debug, Clone, Default)]
pub struct TestState {
    /// Its start was recorded (opening it again isn't)
    pub started: bool,
    /// The step showing (Ford's id for it)
    pub step: String,
    /// The steps answered on the way here, and their answers (for Back)
    pub history: Vec<(String, String)>,
    /// Each step's answer (Ford's id: "Yes")
    pub answers: HashMap<String, String>,
    /// A step whose answer depends on the vehicle: which step to go to is being picked
    pub choosing: Option<String>,
    pub rec: Rec,
    /// Newer tests: its steps being fetched, or why they couldn't be
    pub loading: bool,
    pub load_error: Option<FordError>,
    /// The step's slide in (when it changed)
    pub phase: anim::Phase,
}

impl TestState {
    /// Shows a step: the new one slides up into place
    pub fn show(&mut self, step: String) {
        self.step = step;
        self.choosing = None;
        self.phase = anim::Phase::Entering(Instant::now());
    }
}

/// What Vehicle History said last
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Rec {
    #[default]
    Idle,
    Busy,
    Done(String),
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum TestMsg {
    /// Open a test (its letter), at a step (or where it was left)
    Open(String, Option<String>),
    /// Back to the procedure
    Close,
    /// An answer: the step, which answer, and the step it goes to when that was clicked in it
    Answer {
        step: String,
        answer: usize,
        jump: Option<String>,
    },
    /// Go to a step (or another test) by Ford's id
    Go(String),
    Back,
    Restart,
    AllSteps(bool),
    /// Try fetching its steps again
    Retry,
    Reported(String, Result<(), FordError>, String),
    Steps(String, Result<(Vec<Block>, Vec<Step>), FordError>),
    /// Something else in it: a link, a picture
    Doc(DocMsg),
    /// Where it's scrolled to, as it scrolls (to keep its place)
    Scrolled(f32),
}

pub fn scroll_id() -> iced::widget::scrollable::Id {
    iced::widget::scrollable::Id::new("manual-test")
}

/// Where each step's number is: "Step A3 · 3 of 10"
fn position(p: &Pinpoint, step: &Step) -> String {
    let n = p.steps.iter().position(|s| s.id == step.id).map(|i| i + 1).unwrap_or(1);
    format!("Step {} · {} of {}", step.label, n, p.steps.len())
}

/// A step's label from its id (for "Go to A3"), or the id
fn label_of(p: &Pinpoint, id: &str) -> String {
    p.step(id).map(|s| s.label.clone()).unwrap_or_else(|| id.to_string())
}

fn tinted(color: Color, width: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Color { a: 0.10, ..color }.into()),
        border: Border {
            color: Color { a: 0.55, ..color },
            width,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

const GREEN: Color = Color::from_rgb(0.08, 0.50, 0.24);
const RED: Color = Color::from_rgb(0.86, 0.15, 0.15);
const AMBER: Color = Color::from_rgb(0.85, 0.47, 0.02);

/// An answer: its big button (Yes / No), what it says (its "GO to" clickable), and when it's been picked, marked
fn answer<'a>(
    p: &'a Pinpoint,
    s: &'a Step,
    i: usize,
    a: &'a Answer,
    st: &'a TestState,
    pics: &'a HashMap<String, Pic>,
) -> Element<'a, TestMsg> {
    let chosen = st.answers.get(&s.id) == Some(&a.id);
    let step = s.id.clone();
    let b = button(
        container(text(a.label.as_str()).size(15).font(style::BOLD))
            .center_x(Length::Fill)
            .padding([8, 0]),
    )
    .width(86)
    .style(move |theme: &Theme, status| {
        if chosen {
            button::primary(theme, status)
        } else {
            style::toggle(false)(theme, status)
        }
    })
    .on_press(TestMsg::Answer {
        step: step.clone(),
        answer: i,
        jump: None,
    });
    let said = document::blocks(&a.blocks, pics).map(move |m| match m {
        // its "GO to A2": this answer, that step
        DocMsg::Link(u) if u.starts_with("jump:") => TestMsg::Answer {
            step: step.clone(),
            answer: i,
            jump: Some(u[5..].to_string()),
        },
        other => TestMsg::Doc(other),
    });
    let mut c = Column::new()
        .spacing(10)
        .push(row![b, container(said).width(Length::Fill)].spacing(14).align_y(Alignment::Center));
    // it depends on the vehicle: the steps to pick from
    if chosen && st.choosing.as_deref() == Some(s.id.as_str()) && a.jumps.len() > 1 {
        let mut picks = row![text("Which applies to this vehicle?").size(13).style(style::muted)]
            .spacing(8)
            .align_y(Alignment::Center);
        for j in &a.jumps {
            picks = picks.push(
                button(text(format!("Go to {}", label_of(p, j))).size(13))
                    .padding([5, 12])
                    .style(button::primary)
                    .on_press(TestMsg::Answer {
                        step: s.id.clone(),
                        answer: i,
                        jump: Some(j.clone()),
                    }),
            );
        }
        c = c.push(container(picks).padding(iced::Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 100.0,
        }));
    }
    let mut card = container(c).padding(10).width(Length::Fill);
    card = if chosen {
        card.style(|theme: &Theme| {
            let c = style::colors(theme);
            container::Style {
                background: Some(c.sel.into()),
                border: Border {
                    color: c.accent,
                    width: 1.5,
                    radius: 8.0.into(),
                },
                ..Default::default()
            }
        })
    } else {
        card.style(|theme: &Theme| container::Style {
            border: Border {
                color: style::colors(theme).line,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
    };
    card.into()
}

/// One step: its label and title, what to do, the question, its answers; when an answer ended the test, that said
fn step_card<'a>(p: &'a Pinpoint, s: &'a Step, st: &'a TestState, current: bool, pics: &'a HashMap<String, Pic>) -> Element<'a, TestMsg> {
    let mut c = Column::new().spacing(14).push(
        row![
            pill(s.label.as_str(), [0x1a, 0x5f, 0xb4]),
            text(s.title.as_str()).size(17).font(style::BOLD).width(Length::Fill)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    );
    if !s.blocks.is_empty() {
        c = c.push(document::blocks(&s.blocks, pics).map(|m| match m {
            DocMsg::Link(u) if u.starts_with("jump:") => TestMsg::Go(u[5..].to_string()),
            other => TestMsg::Doc(other),
        }));
    }
    if !s.question.is_empty() {
        c = c.push(
            container(text(s.question.as_str()).size(15).font(style::BOLD))
                .padding([10, 14])
                .width(Length::Fill)
                .style(|theme: &Theme| tinted(style::colors(theme).accent, 1.0)(theme)),
        );
    }
    if s.answers.is_empty() {
        // (an older test without marked answers: on to the next step by hand)
        if let Some(next) = p.steps.iter().skip_while(|x| x.id != s.id).nth(1) {
            c = c.push(
                button(text(format!("Next: {} ›", next.label)).size(14))
                    .padding([8, 16])
                    .style(button::primary)
                    .on_press(TestMsg::Go(next.id.clone())),
            );
        }
    }
    for (i, a) in s.answers.iter().enumerate() {
        c = c.push(answer(p, s, i, a, st, pics));
    }
    // the answer picked ends the test here
    let ended = st
        .answers
        .get(&s.id)
        .and_then(|id| s.answers.iter().find(|a| &a.id == id))
        .is_some_and(Answer::ends);
    if ended {
        c = c.push(
            container(
                row![
                    text("✓ The test ends here: do what the answer says.").size(14).color(GREEN),
                    horizontal_space(),
                    button(text("Start over").size(13))
                        .padding([5, 12])
                        .style(style::ghost)
                        .on_press(TestMsg::Restart),
                ]
                .align_y(Alignment::Center),
            )
            .padding([10, 14])
            .width(Length::Fill)
            .style(tinted(GREEN, 1.0)),
        );
    }
    let card = container(c).padding(18).width(Length::Fill);
    if current {
        card.style(|theme: &Theme| {
            let mut s = style::card(theme);
            s.border.color = style::colors(theme).accent;
            s.border.width = 1.5;
            s
        })
        .into()
    } else {
        card.style(style::card).into()
    }
}

/// The bar above the step: Back, where it is (and how far through), what Vehicle History said, Start over
fn bar<'a>(p: &'a Pinpoint, s: &'a Step, st: &'a TestState) -> Element<'a, TestMsg> {
    let n = p.steps.iter().position(|x| x.id == s.id).map(|i| i + 1).unwrap_or(1) as u16;
    let total = p.steps.len().max(1) as u16;
    let bar_part = |portion: u16, done: bool| {
        container(Space::new(Length::Fill, 4))
            .width(Length::FillPortion(portion))
            .style(move |theme: &Theme| container::Style {
                background: Some(
                    if done {
                        style::colors(theme).accent
                    } else {
                        style::colors(theme).line
                    }
                    .into(),
                ),
                border: Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
    };
    let mut progress = row![bar_part(n.max(1), true)].spacing(2);
    if n < total {
        progress = progress.push(bar_part(total - n, false));
    }
    let (said, color) = match &st.rec {
        Rec::Idle if p.legacy => ("Ford records only the start of this kind of test, not each step".to_string(), None),
        Rec::Idle => ("Answers are recorded to Vehicle History".to_string(), None),
        Rec::Busy => ("Recording…".to_string(), None),
        Rec::Done(t) => (format!("✓ {t}"), Some(GREEN)),
        Rec::Failed(t) => (format!("⚠ {t}"), Some(RED)),
    };
    let said = text(said).size(12);
    let said = match color {
        Some(c) => said.color(c),
        None => said.style(style::muted),
    };
    column![
        row![
            button(text("‹ Back").size(13))
                .padding([5, 12])
                .style(style::ghost)
                .on_press_maybe((!st.history.is_empty()).then_some(TestMsg::Back)),
            text(position(p, s)).size(14).font(style::BOLD),
            horizontal_space(),
            said,
            button(text("Start over").size(13))
                .padding([5, 12])
                .style(style::ghost)
                .on_press(TestMsg::Restart),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        progress,
    ]
    .spacing(8)
    .into()
}

fn intro<'a>(p: &'a Pinpoint, pics: &'a HashMap<String, Pic>) -> Element<'a, TestMsg> {
    container(
        column![
            text("Introduction").size(12).font(style::BOLD).style(style::muted),
            document::blocks(&p.intro, pics).map(|m| match m {
                DocMsg::Link(u) if u.starts_with("jump:") => TestMsg::Go(u[5..].to_string()),
                other => TestMsg::Doc(other),
            })
        ]
        .spacing(8),
    )
    .padding(16)
    .width(Length::Fill)
    .style(style::card)
    .into()
}

/// A test, over its procedure: its title and the way it's shown (one step, all of them), then the test
pub fn view<'a>(p: &'a Pinpoint, st: &'a TestState, all_steps: bool, pics: &'a HashMap<String, Pic>) -> Element<'a, TestMsg> {
    let top = row![
        button(text("‹ Procedure").size(13))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(TestMsg::Close),
        text(p.title.as_str()).size(17).font(style::BOLD).width(Length::Fill),
        button(text(if all_steps { "One step at a time" } else { "Show all steps" }).size(13))
            .padding([5, 12])
            .style(style::toggle(all_steps))
            .on_press(TestMsg::AllSteps(!all_steps)),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let mut body = Column::new().spacing(14);
    let current = p.step(&st.step).or(p.steps.first());
    match current {
        None => {
            let note: Element<'a, TestMsg> = match (&st.load_error, st.loading) {
                (_, true) => text("Loading the test's steps…").style(style::muted).into(),
                (Some(e), false) => column![
                    text(format!("Couldn't load the test's steps ({})", e.text())).style(style::muted),
                    button(text("Try again"))
                        .padding([6, 14])
                        .style(button::primary)
                        .on_press(TestMsg::Retry),
                ]
                .spacing(10)
                .into(),
                (None, false) => text("Ford didn't send steps for this test.").style(style::muted).into(),
            };
            if !p.intro.is_empty() {
                body = body.push(intro(p, pics));
            }
            body = body.push(container(note).padding(18).width(Length::Fill).style(style::card));
        }
        Some(s) if all_steps => {
            if !p.intro.is_empty() {
                body = body.push(intro(p, pics));
            }
            for x in &p.steps {
                body = body.push(step_card(p, x, st, x.id == s.id, pics));
            }
        }
        Some(s) => {
            body = body.push(container(bar(p, s, st)).padding([12, 16]).width(Length::Fill).style(style::card));
            if st.history.is_empty() && p.steps.first().is_some_and(|f| f.id == s.id) && !p.intro.is_empty() {
                body = body.push(intro(p, pics));
            }
            body = body.push(anim::page(step_card(p, s, st, false, pics), st.phase));
        }
    }
    // the test's own color line along its top, so it reads as a test
    let head = container(top).padding([12, 20]).width(Length::Fill).style(style::panel);
    column![
        container(Space::new(Length::Fill, 3)).style(|_: &Theme| container::Style {
            background: Some(AMBER.into()),
            ..Default::default()
        }),
        head,
        scrollable(container(body.max_width(1000)).padding(20).width(Length::Fill))
            .id(scroll_id())
            .on_scroll(|v| TestMsg::Scrolled(v.absolute_offset().y))
            .height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
}
