//! DataLogger (as FDRS has it): one application for live data, in its own tab, in three steps:
//!
//!   Modules    pick up to five of the vehicle's modules
//!   PIDs       each module's readings (it's asked which it has), under its name: tick the ones to watch
//!   Live View  every reading picked, in one list: its value on the left, a graph of its last values beside it
//!
//! The readings are OBD-II's (crates/vci/obd.rs): the powertrain modules have them (the PCM on any vehicle). Other
//! modules are listed, but can't be picked until the app knows their readings. The picked readings are read one
//! after another, over and over (every module's in turn).
//!
//! Only reads. Its requests aren't written to the communication log (there'd be thousands): what it found is.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::widget::{Column, button, canvas as draw, checkbox, column, container, horizontal_space, row, text, text_input};
use iced::{Alignment, Color, Element, Length, Point, Rectangle, mouse};
use ui_kit::{anim, style};
use vci::modules::{self, Bus, Entry};
use vci::obd::{self, Pid, Supported};
use vci::scan::Ask;

use crate::view::{card, ghost, primary, scrolling};
use crate::{Diag, DiagMsg, tag};

/// Values kept for each reading's graph
const KEEP: usize = 150;
/// At most this many modules (as FDRS), and readings in all (each one makes the rest update slower)
pub const MOST_MODULES: usize = 5;
pub const MOST_PIDS: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Modules,
    Pids,
    Live,
}

#[derive(Debug, Clone)]
pub enum LiveMsg {
    Step(Step),
    /// A module picked, or not
    ToggleModule(u32, Bus),
    /// A module's reading picked, or not
    TogglePid(u32, Bus, u8),
    /// The PIDs' search
    Filter(String),
    Start,
    Stop,
    /// What a module said (the request it answers)
    Asked(Ask, Result<Option<Vec<u8>>, String>),
}

/// A reading's values so far
#[derive(Debug, Default)]
pub struct Series {
    pub last: Option<f64>,
    pub history: VecDeque<f64>,
    pub low: Option<f64>,
    pub high: Option<f64>,
}

impl Series {
    fn push(&mut self, v: f64) {
        self.last = Some(v);
        if self.history.len() == KEEP {
            self.history.pop_front();
        }
        self.history.push_back(v);
        self.low = Some(self.low.map_or(v, |l| l.min(v)));
        self.high = Some(self.high.map_or(v, |h| h.max(v)));
    }
}

/// A module picked: which readings it has (asked once), the ones picked, their values
#[derive(Debug)]
pub struct Logged {
    pub target: (Entry, Bus),
    finding: Option<Supported>,
    /// The readings it has that the app can show (none: not asked yet)
    pub supported: Option<Vec<u8>>,
    pub problem: Option<String>,
    pub chosen: Vec<u8>,
    pub series: HashMap<u8, Series>,
}

impl Logged {
    fn new(target: (Entry, Bus)) -> Self {
        Logged {
            target,
            finding: None,
            supported: None,
            problem: None,
            chosen: Vec::new(),
            series: HashMap::new(),
        }
    }

    fn is(&self, address: u32, bus: Bus) -> bool {
        self.target.0.0 == address && self.target.1 == bus
    }

    pub fn tag(&self) -> String {
        let ((a, short, _), _) = self.target;
        tag(short, a)
    }
}

/// Whether the app knows a module's readings (OBD-II's: the powertrain modules, on pins 6 / 14)
pub fn has_readings(address: u32, bus: Bus) -> bool {
    obd::answers_obd(address) && bus == Bus::Hs
}

/// The DataLogger tab
#[derive(Debug)]
pub struct DataLogger {
    pub id: u64,
    pub step: Step,
    pub step_since: Instant,
    pub modules: Vec<Logged>,
    pub filter: String,
    /// Where it's got to in the readings picked
    at: usize,
}

impl DataLogger {
    pub fn new(id: u64) -> Self {
        DataLogger {
            id,
            step: Step::Modules,
            step_since: Instant::now(),
            modules: Vec::new(),
            filter: String::new(),
            at: 0,
        }
    }

    pub fn title(&self) -> String {
        "DataLogger".into()
    }

    pub fn go(&mut self, step: Step) {
        if self.step != step {
            self.step = step;
            self.step_since = Instant::now();
        }
    }

    /// Every reading picked: (which module, which reading)
    pub fn picked(&self) -> Vec<(usize, u8)> {
        self.modules
            .iter()
            .enumerate()
            .flat_map(|(i, m)| m.chosen.iter().map(move |p| (i, *p)))
            .collect()
    }

    /// A module not asked yet which readings it has
    pub fn needs_finding(&self) -> bool {
        self.modules
            .iter()
            .any(|m| m.supported.is_none() && has_readings(m.target.0.0, m.target.1))
    }

    pub fn finding(&self) -> bool {
        self.modules.iter().any(|m| m.finding.is_some())
    }

    /// Starting: the modules not asked yet are asked which readings they have
    pub fn start(&mut self) {
        for m in &mut self.modules {
            if m.supported.is_none() && has_readings(m.target.0.0, m.target.1) {
                m.problem = None;
                m.finding = Some(Supported::default());
            }
        }
    }

    fn ask(target: (Entry, Bus), data: Vec<u8>, timeout_ms: u32) -> Ask {
        let ((address, _, _), bus) = target;
        Ask {
            tx: address,
            rx: modules::answers_at(address),
            data,
            timeout_ms,
            bus,
        }
    }

    /// The next request (none: nothing to ask or read)
    pub fn next(&self) -> Option<Ask> {
        if let Some(m) = self.modules.iter().find(|m| m.finding.is_some()) {
            let base = m.finding.as_ref()?.next()?;
            return Some(Self::ask(m.target, obd::request(base), 400));
        }
        let picked = self.picked();
        let (i, pid) = *picked.get(self.at % picked.len().max(1))?;
        Some(Self::ask(self.modules[i].target, obd::request(pid), 250))
    }

    /// What came back. When a module's said which readings it has: what to write in the log about it.
    pub fn answer(&mut self, asked: &Ask, reply: Option<&[u8]>) -> Option<String> {
        let Some(m) = self.modules.iter_mut().find(|m| m.is(asked.tx, asked.bus)) else {
            // (a module let go of meanwhile)
            self.at += 1;
            return None;
        };
        if let Some(f) = &mut m.finding {
            f.answer(reply);
            if f.next().is_some() {
                return None;
            }
            let known: Vec<u8> = f.known().iter().map(|p| p.id).collect();
            if known.is_empty() {
                m.problem = Some(if f.ids.is_empty() {
                    "Didn't answer (is the ignition on?)".into()
                } else {
                    "Has no readings the app knows how to show".into()
                });
            }
            m.finding = None;
            let said = format!("{}: {} readings the app can show", m.tag(), known.len());
            m.supported = Some(known);
            self.at = 0;
            return Some(said);
        }
        let Some(&pid) = asked.data.get(1) else { return None };
        if let Some(v) = reply.and_then(|r| obd::pid(pid)?.value(r)) {
            m.series.entry(pid).or_default().push(v);
        }
        self.at += 1;
        if self.at >= self.picked().len() {
            self.at = 0;
        }
        None
    }

    /// Picking (modules, readings) and the search
    pub fn update(&mut self, m: LiveMsg) {
        match m {
            LiveMsg::ToggleModule(address, bus) => {
                if let Some(i) = self.modules.iter().position(|m| m.is(address, bus)) {
                    self.modules.remove(i);
                } else if self.modules.len() < MOST_MODULES && has_readings(address, bus) {
                    // (its name and short name: from the list, or found by the full search)
                    let entry = modules::MODULES.iter().find(|e| e.0 == address).copied().unwrap_or((
                        address,
                        modules::UNLISTED.0,
                        modules::UNLISTED.1,
                    ));
                    self.modules.push(Logged::new((entry, bus)));
                }
                self.at = 0;
            }
            LiveMsg::TogglePid(address, bus, pid) => {
                let total = self.picked().len();
                if let Some(m) = self.modules.iter_mut().find(|m| m.is(address, bus)) {
                    if let Some(i) = m.chosen.iter().position(|c| *c == pid) {
                        m.chosen.remove(i);
                        m.series.remove(&pid);
                    } else if total < MOST_PIDS {
                        m.chosen.push(pid);
                    }
                }
                self.at = 0;
            }
            LiveMsg::Filter(t) => self.filter = t,
            _ => {}
        }
    }
}

// ---------- what it looks like ----------

fn live(id: u64, m: LiveMsg) -> DiagMsg {
    DiagMsg::Live(id, m)
}

pub fn page<'a>(d: &'a Diag, l: &'a DataLogger) -> Element<'a, DiagMsg> {
    let id = l.id;
    let running = d.live_running == Some(id);
    let picked = l.picked().len();

    // its steps, as tabs (each open once what it needs is picked)
    let step = |label: &'static str, s: Step, open: bool| -> Element<'a, DiagMsg> {
        button(text(label).size(13))
            .padding([6, 14])
            .style(style::tab(l.step == s))
            .on_press_maybe(open.then(|| live(id, LiveMsg::Step(s))))
            .into()
    };
    let steps = anim::tabs(
        match l.step {
            Step::Modules => 0,
            Step::Pids => 1,
            Step::Live => 2,
        },
        vec![
            step("1  Modules", Step::Modules, true),
            step("2  PIDs", Step::Pids, !l.modules.is_empty()),
            step("3  Live View", Step::Live, picked > 0),
        ],
    );

    // what's happening, and Start / Stop
    let mut right = row![].spacing(8).align_y(Alignment::Center);
    if running {
        right = right.push(
            row![
                anim::dot(Color::from_rgb8(0x2e, 0xa0, 0x43), true),
                text(if l.finding() { "Asking for PIDs…" } else { "Recording" }).size(12)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
        right = right.push(ghost(
            if d.stopping { "Stopping…" } else { "Stop" },
            (!d.stopping).then(|| live(id, LiveMsg::Stop)),
        ));
    } else if l.step == Step::Live {
        right = right.push(primary(
            "Start",
            (d.connected() && !d.busy() && picked > 0).then(|| live(id, LiveMsg::Start)),
        ));
    }

    let body: Element<'a, DiagMsg> = match l.step {
        Step::Modules => modules_step(d, l),
        Step::Pids => pids_step(d, l),
        Step::Live => live_view(l),
    };
    let header = row![steps, horizontal_space(), right].align_y(Alignment::Center);
    scrolling(
        card(
            l.title(),
            iced::widget::Space::with_width(0).into(),
            column![header, anim::page(body, anim::Phase::Entering(l.step_since))]
                .spacing(14)
                .into(),
        ),
        1200.0,
    )
}

/// Step 1: the vehicle's modules, to pick up to five
fn modules_step<'a>(d: &'a Diag, l: &'a DataLogger) -> Element<'a, DiagMsg> {
    let id = l.id;
    let mut list = Column::new().spacing(2);
    let found: Vec<_> = d.scan.iter().flat_map(|s| s.modules.iter().filter(|m| m.present())).collect();
    if found.is_empty() {
        return text("Run the network test (Dashboard) first: the modules it finds are listed here.")
            .size(13)
            .style(style::muted)
            .into();
    }
    for m in found {
        let on = l.modules.iter().any(|x| x.is(m.address, m.bus));
        let can = has_readings(m.address, m.bus);
        let full = !on && l.modules.len() >= MOST_MODULES;
        let mut c = checkbox(format!("{}  ·  {}", tag(m.short, m.address), m.name), on).text_size(13);
        if can && !full || on {
            let (a, bus) = (m.address, m.bus);
            c = c.on_toggle(move |_| live(id, LiveMsg::ToggleModule(a, bus)));
        }
        list = list.push(
            container(
                row![
                    c,
                    horizontal_space(),
                    text(if can { "OBD-II PIDs" } else { "No PIDs yet" }).size(11).style(style::muted)
                ]
                .align_y(Alignment::Center),
            )
            .padding([6, 10]),
        );
    }
    column![
        row![
            text(format!("Pick up to {MOST_MODULES} modules")).size(13),
            horizontal_space(),
            text(format!("{} of {MOST_MODULES} picked", l.modules.len()))
                .size(12)
                .style(style::muted),
            primary("Next: PIDs", (!l.modules.is_empty()).then(|| live(id, LiveMsg::Step(Step::Pids)))),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        container(list).width(Length::Fill).style(style::panel),
    ]
    .spacing(10)
    .into()
}

/// Step 2: each module's readings, under its name, to tick
fn pids_step<'a>(d: &'a Diag, l: &'a DataLogger) -> Element<'a, DiagMsg> {
    let id = l.id;
    let words = l.filter.trim().to_lowercase();
    let picked = l.picked().len();
    let mut c = column![
        row![
            text_input("Search PIDs", &l.filter)
                .on_input(move |t| live(id, LiveMsg::Filter(t)))
                .size(13)
                .padding([5, 10])
                .width(260),
            horizontal_space(),
            text(format!("{picked} of {MOST_PIDS} picked")).size(12).style(style::muted),
            primary("Next: Live View", (picked > 0).then(|| live(id, LiveMsg::Step(Step::Live)))),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
    ]
    .spacing(14);
    for m in &l.modules {
        let ((address, _, name), bus) = m.target;
        let mut section = Column::new().spacing(2);
        match (&m.supported, &m.problem) {
            (_, Some(p)) => section = section.push(text(p.as_str()).size(13).style(style::muted)),
            (None, _) => {
                section = section.push(
                    text(if d.live_running == Some(id) {
                        "Asking which PIDs it has…"
                    } else if d.connected() {
                        "Not asked yet (it's asked when this step opens)."
                    } else {
                        "Connect to the vehicle first."
                    })
                    .size(13)
                    .style(style::muted),
                )
            }
            (Some(list), None) => {
                for p in list
                    .iter()
                    .filter_map(|&pid| obd::pid(pid))
                    .filter(|p| words.is_empty() || p.name.to_lowercase().contains(&words))
                {
                    let on = m.chosen.contains(&p.id);
                    let mut cb = checkbox(p.name, on).text_size(13);
                    if on || picked < MOST_PIDS {
                        let pid = p.id;
                        cb = cb.on_toggle(move |_| live(id, LiveMsg::TogglePid(address, bus, pid)));
                    }
                    section = section.push(
                        row![cb, horizontal_space(), text(p.unit).size(11).style(style::muted)]
                            .spacing(8)
                            .align_y(Alignment::Center),
                    );
                }
            }
        }
        c = c.push(
            column![
                row![
                    text(m.tag()).font(style::BOLD),
                    text(name).size(13).style(style::muted),
                    horizontal_space(),
                    text(format!("{} picked", m.chosen.len())).size(12).style(style::muted),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                container(section).padding([8, 12]).width(Length::Fill).style(style::panel),
            ]
            .spacing(6),
        );
    }
    c.into()
}

/// Step 3: every reading picked, in one list: its value on the left, its graph beside it
fn live_view(l: &DataLogger) -> Element<'_, DiagMsg> {
    let mut list = Column::new();
    let mut i = 0usize;
    for m in &l.modules {
        for &pid in &m.chosen {
            let Some(p) = obd::pid(pid) else { continue };
            list = list.push(reading_row(i, m, p, m.series.get(&pid)));
            i += 1;
        }
    }
    if i == 0 {
        return text("No PIDs picked yet.").size(13).style(style::muted).into();
    }
    container(list)
        .width(Length::Fill)
        .clip(true)
        .style(|t: &iced::Theme| container::Style {
            border: iced::Border {
                color: style::colors(t).line,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// One reading in the Live View: value, then its name over its graph
fn reading_row<'a>(i: usize, m: &'a Logged, p: &'static Pid, s: Option<&'a Series>) -> Element<'a, DiagMsg> {
    let value = s.and_then(|s| s.last).map(|v| p.show(v)).unwrap_or_else(|| "–".into());
    let points: Vec<f64> = s.map(|s| s.history.iter().copied().collect()).unwrap_or_default();
    let span = match (s.and_then(|s| s.low), s.and_then(|s| s.high)) {
        (Some(lo), Some(hi)) => format!("{} – {}", p.show(lo), p.show(hi)),
        _ => String::new(),
    };
    container(
        row![
            row![text(value).size(22).font(style::BOLD), text(p.unit).size(12).style(style::muted)]
                .spacing(5)
                .align_y(Alignment::End)
                .width(150),
            column![
                row![
                    text(format!("{}  ·  {}", m.tag(), p.name)).size(12),
                    horizontal_space(),
                    text(span).size(11).style(style::muted),
                ]
                .align_y(Alignment::Center),
                draw(Spark { points, range: p.range }).width(Length::Fill).height(34),
            ]
            .spacing(3)
            .width(Length::Fill),
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .padding([8, 14])
    .width(Length::Fill)
    .style(move |t: &iced::Theme| container::Style {
        background: Some(
            if i % 2 == 1 {
                style::colors(t).hover
            } else {
                style::colors(t).panel
            }
            .into(),
        ),
        ..Default::default()
    })
    .into()
}

/// A reading's last values, as a line
struct Spark {
    points: Vec<f64>,
    /// Its usual range (the graph's fitted to what's been seen, but never narrower than a twentieth of this)
    range: (f64, f64),
}

impl canvas::Program<DiagMsg> for Spark {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let c = style::colors(theme);
        let (w, h) = (bounds.width, bounds.height);
        frame.stroke(
            &Path::line(Point::new(0.0, h - 0.5), Point::new(w, h - 0.5)),
            Stroke::default().with_width(1.0).with_color(c.line),
        );
        if self.points.len() >= 2 {
            let (lo, hi) = self.points.iter().fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
            let least = (self.range.1 - self.range.0).abs() / 20.0;
            let mid = (lo + hi) / 2.0;
            let half = ((hi - lo) / 2.0).max(least / 2.0).max(1e-9) * 1.15;
            let (lo, hi) = (mid - half, mid + half);
            let step = w / (KEEP - 1) as f32;
            let start = KEEP - self.points.len();
            let y = |v: f64| (h - 3.0) - ((v - lo) / (hi - lo)) as f32 * (h - 6.0);
            let line = Path::new(|b| {
                for (i, v) in self.points.iter().enumerate() {
                    let p = Point::new((start + i) as f32 * step, y(*v));
                    if i == 0 { b.move_to(p) } else { b.line_to(p) }
                }
            });
            frame.stroke(&line, Stroke::default().with_width(1.8).with_color(c.accent));
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_then_pids_then_round_and_round() {
        let mut l = DataLogger::new(1);
        // the ABS has no readings the app knows: not picked
        l.update(LiveMsg::ToggleModule(0x760, Bus::Hs));
        assert!(l.modules.is_empty());
        l.update(LiveMsg::ToggleModule(0x7E0, Bus::Hs));
        l.update(LiveMsg::ToggleModule(0x7E1, Bus::Hs));
        assert_eq!(l.modules.len(), 2);
        assert!(l.needs_finding());
        l.start();
        // the PCM has three readings; the TCM doesn't answer
        let has = [0x05, 0x0C, 0x0D];
        while let Some(a) = l.next() {
            if !l.finding() {
                break;
            }
            let r = (a.tx == 0x7E0).then(|| obd::supported_answer(a.data[1], &has));
            l.answer(&a, r.as_deref());
        }
        assert_eq!(l.modules[0].supported.as_deref(), Some(&has[..]));
        assert!(l.modules[1].problem.is_some());
        // nothing picked yet: nothing to read
        assert!(l.next().is_none());
        l.update(LiveMsg::TogglePid(0x7E0, Bus::Hs, 0x0C));
        l.update(LiveMsg::TogglePid(0x7E0, Bus::Hs, 0x05));
        let a = l.next().unwrap();
        assert_eq!((a.tx, a.data.clone()), (0x7E0, vec![0x01, 0x0C]));
        l.answer(&a, Some(&[0x41, 0x0C, 0x0B, 0x20]));
        assert_eq!(l.modules[0].series[&0x0C].last, Some(712.0));
        assert_eq!(l.next().unwrap().data, vec![0x01, 0x05]);
        l.answer(&l.next().unwrap(), None);
        assert_eq!(l.next().unwrap().data, vec![0x01, 0x0C]);
        // one let go of: the rest go on
        l.update(LiveMsg::TogglePid(0x7E0, Bus::Hs, 0x0C));
        assert_eq!(l.picked(), vec![(0, 0x05)]);
        assert!(!l.modules[0].series.contains_key(&0x0C));
    }
}
