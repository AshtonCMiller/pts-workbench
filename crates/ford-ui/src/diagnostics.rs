//! The Diagnostics tab (the Electron app's `manual/diagnostics/`): Ford's "Enhanced Search" for the vehicle. Type
//! DTCs (P0420, U3000:49) and/or words (no crank, liftgate); the results come back grouped like Ford's Diagnostics
//! tab, each group a tab with its count (the highlight slides between them). A pinpoint test opens right at the test
//! in the Workshop Manual tab; a procedure there too; a bulletin in the TSBs tab; wiring and videos on Ford's site.

use std::collections::{BTreeMap, HashMap};

use ford::diagnostics::{DiagPage, DiagRequest, DiagResult, DtcInfo, parse_query};
use iced::widget::{Column, button, checkbox, column, container, horizontal_space, row, text, text_input};
use iced::{Alignment, Element, Length, Task};
use ui_kit::scroll::scrollable;
use ui_kit::{anim, pill, style};

use crate::{Env, FordError, Out};

/// The groups, in Ford's order: their id, name, and the kinds in each
pub const GROUPS: [(&str, &str, &[&str]); 6] = [
    ("tsb", "TSB / GSB / SSM", &["tsb", "gsb", "ssm"]),
    ("wsmppt", "Pinpoint tests", &["wsmppt"]),
    ("wsmproc", "Procedures", &["wsmproc"]),
    ("pced", "PC/ED", &["pced"]),
    ("wiring", "Wiring", &["wiring"]),
    ("tsv", "Videos", &["tsv"]),
];
/// Results asked for at a time
const ROWS: u32 = 50;

fn input_id() -> text_input::Id {
    text_input::Id::new("diag-search")
}

fn group_of(id: &str) -> &'static [&'static str] {
    GROUPS.iter().find(|g| g.0 == id).map(|g| g.2).unwrap_or(&[])
}

/// One group's results so far
#[derive(Debug, Clone, Default)]
pub struct List {
    pub items: Vec<DiagResult>,
    pub loading: bool,
    pub error: Option<FordError>,
}

/// A search: what was asked, how many of each kind there are, Ford's words on its DTCs, each group's results
#[derive(Debug, Clone, Default)]
pub struct Search {
    pub query: String,
    pub text: String,
    pub dtcs: Vec<String>,
    pub exact: bool,
    pub counts: Option<BTreeMap<String, u64>>,
    pub dtc_info: Vec<DtcInfo>,
    pub lists: HashMap<String, List>,
    pub error: Option<FordError>,
}

impl Search {
    pub fn count(&self, group: &str) -> u64 {
        let c = self.counts.as_ref();
        group_of(group)
            .iter()
            .map(|k| c.and_then(|c| c.get(*k)).copied().unwrap_or(0))
            .sum()
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiagView {
    pub vin: String,
    /// What's typed
    pub query: String,
    pub exact: bool,
    /// The group showing
    pub group: String,
    pub search: Option<Search>,
    /// Answers to an older search are passed over
    token: u64,
}

#[derive(Debug, Clone)]
pub enum DiagMsg {
    Query(String),
    Exact(bool),
    Search,
    /// The first answer (how many of each kind), for search `token`
    Counted(u64, Result<DiagPage, FordError>),
    Group(String),
    More,
    Loaded(u64, String, Result<DiagPage, FordError>),
    /// Open the showing group's result (its place)
    Open(usize),
    SignIn,
}

impl DiagView {
    pub fn new(vin: impl Into<String>) -> Self {
        Self {
            vin: vin.into(),
            group: GROUPS[0].0.into(),
            ..Default::default()
        }
    }

    /// The search box, ready to type in
    pub fn focus(&self) -> Task<DiagMsg> {
        text_input::focus(input_id())
    }

    fn ask(&self, env: &Env, req: DiagRequest, f: impl Fn(Result<DiagPage, FordError>) -> DiagMsg + Send + 'static) -> Task<DiagMsg> {
        let (ford, vin) = (env.ford.clone(), self.vin.clone());
        Task::perform(async move { ford.diag_search(&vin, &req).await.map_err(FordError::from) }, f)
    }

    /// A group's (next) results
    fn load_group(&mut self, env: &Env, group: &str, more: bool) -> Task<DiagMsg> {
        let token = self.token;
        let Some(s) = self.search.as_mut() else { return Task::none() };
        let total = s.count(group);
        let list = s.lists.entry(group.to_string()).or_default();
        if list.loading || total == 0 || (!more && !list.items.is_empty()) || list.items.len() as u64 >= total {
            return Task::none();
        }
        list.loading = true;
        list.error = None;
        let req = DiagRequest {
            text: s.text.clone(),
            dtcs: s.dtcs.clone(),
            sources: group_of(group).iter().map(|k| k.to_string()).collect(),
            rows: ROWS,
            start: list.items.len() as u32,
            exact: s.exact,
        };
        let g = group.to_string();
        self.ask(env, req, move |r| DiagMsg::Loaded(token, g.clone(), r))
    }

    pub fn update(&mut self, msg: DiagMsg, env: &Env) -> (Task<DiagMsg>, Vec<Out>) {
        let mut outs = Vec::new();
        let task = match msg {
            DiagMsg::Query(q) => {
                self.query = q;
                Task::none()
            }
            DiagMsg::Exact(on) => {
                self.exact = on;
                if self.search.is_some() {
                    return self.update(DiagMsg::Search, env);
                }
                Task::none()
            }
            DiagMsg::Search => {
                let q = self.query.trim().to_string();
                self.token += 1;
                if q.is_empty() || env.sample {
                    self.search = None;
                    return (Task::none(), outs);
                }
                let (dtcs, words) = parse_query(&q);
                self.search = Some(Search {
                    query: q,
                    text: words.clone(),
                    dtcs: dtcs.clone(),
                    exact: self.exact,
                    ..Default::default()
                });
                // first, how many of each kind (Ford asks for one row of everything first)
                let token = self.token;
                let req = DiagRequest {
                    text: words,
                    dtcs,
                    sources: Vec::new(),
                    rows: 1,
                    start: 0,
                    exact: self.exact,
                };
                self.ask(env, req, move |r| DiagMsg::Counted(token, r))
            }
            DiagMsg::Counted(token, r) => {
                if token != self.token {
                    return (Task::none(), outs);
                }
                let Some(s) = self.search.as_mut() else {
                    return (Task::none(), outs);
                };
                match r {
                    Ok(page) => {
                        s.counts = Some(page.counts);
                        s.dtc_info = page.dtc_info;
                        // the group chosen if it has results; otherwise the first that does (as Ford's)
                        let with: Vec<&str> = GROUPS.iter().map(|g| g.0).filter(|g| s.count(g) > 0).collect();
                        if !with.contains(&self.group.as_str()) {
                            self.group = with.first().copied().unwrap_or(GROUPS[0].0).to_string();
                        }
                        let g = self.group.clone();
                        self.load_group(env, &g, false)
                    }
                    Err(e) => {
                        s.error = Some(e);
                        Task::none()
                    }
                }
            }
            DiagMsg::Group(g) => {
                self.group = g.clone();
                self.load_group(env, &g, false)
            }
            DiagMsg::More => {
                let g = self.group.clone();
                self.load_group(env, &g, true)
            }
            DiagMsg::Loaded(token, g, r) => {
                if token != self.token {
                    return (Task::none(), outs);
                }
                if let Some(list) = self.search.as_mut().and_then(|s| s.lists.get_mut(&g)) {
                    list.loading = false;
                    match r {
                        Ok(page) => list.items.extend(page.results),
                        Err(e) => list.error = Some(e),
                    }
                }
                Task::none()
            }
            DiagMsg::Open(i) => {
                let r = self
                    .search
                    .as_ref()
                    .and_then(|s| s.lists.get(&self.group))
                    .and_then(|l| l.items.get(i))
                    .cloned();
                if let Some(r) = r {
                    match (&r.proc, r.source.as_str()) {
                        // (PC/ED comes later: until then, in the Workshop Manual tab too)
                        (Some(link), _) => outs.push(Out::OpenProcedure {
                            link: link.clone(),
                            title: r.title.clone(),
                        }),
                        (None, "tsb" | "gsb" | "ssm") if !r.article.is_empty() => outs.push(Out::OpenBulletin {
                            kind: r.source.to_uppercase(),
                            id: r.article.clone(),
                            title: r.title.clone(),
                            url: r.url.clone(),
                        }),
                        _ if !r.url.is_empty() => {
                            let _ = crate::open_external(&r.url);
                        }
                        _ => {}
                    }
                }
                Task::none()
            }
            DiagMsg::SignIn => {
                outs.push(Out::SignIn);
                Task::none()
            }
        };
        (task, outs)
    }
}

// ---------- the view ----------

fn kind_label(source: &str) -> (&'static str, [u8; 3]) {
    match source {
        "tsb" => ("TSB", [0x1a, 0x5f, 0xb4]),
        "gsb" => ("GSB", [0x15, 0x80, 0x3d]),
        "ssm" => ("SSM", [0xd9, 0x77, 0x06]),
        "wsmppt" => ("PPT", [0x7a, 0x5a, 0xf5]),
        "wsmproc" => ("WSM", [0x0e, 0x74, 0x90]),
        "pced" => ("PC/ED", [0xb4, 0x53, 0x09]),
        "wiring" => ("WIRING", [0x5d, 0x66, 0x70]),
        "tsv" => ("VIDEO", [0xdc, 0x26, 0x26]),
        _ => ("", [0x5d, 0x66, 0x70]),
    }
}

/// "2023-02-01" → "Feb 1, 2023"
fn nice_date(d: &str) -> String {
    let day = d.get(..10).unwrap_or(d);
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map(|t| t.format("%b %-d, %Y").to_string())
        .unwrap_or_else(|_| d.to_string())
}

fn result_item<'a>(i: usize, r: &'a DiagResult) -> Element<'a, DiagMsg> {
    let (kind, rgb) = kind_label(&r.source);
    let go = match r.source.as_str() {
        "wsmppt" | "wsmproc" | "pced" => "Open in manual ›",
        "tsb" | "gsb" | "ssm" => "Open in TSBs ›",
        _ => "Open ›",
    };
    let title = if r.article.is_empty() {
        r.title.clone()
    } else {
        format!("{} — {}", r.article, r.title)
    };
    let mut c = column![
        row![
            pill(kind, rgb),
            text(title).size(14).font(style::BOLD).width(Length::Fill),
            text(go).size(13).style(text::primary)
        ]
        .spacing(10)
        .align_y(Alignment::Center)
    ]
    .spacing(4);
    if !r.description.is_empty() {
        c = c.push(text(r.description.as_str()).size(13).style(style::muted));
    }
    let mut meta = row![].spacing(12);
    let mut any = false;
    if !r.date.is_empty() && !r.article.is_empty() {
        meta = meta.push(text(format!("Effective {}", nice_date(&r.date))).size(12).style(style::muted));
        any = true;
    }
    if !r.applies {
        meta = meta.push(
            text("May not apply to this vehicle's options")
                .size(12)
                .color(iced::Color::from_rgb8(0xd9, 0x77, 0x06)),
        );
        any = true;
    }
    if any {
        c = c.push(meta);
    }
    button(c)
        .width(Length::Fill)
        .padding([10, 12])
        .style(style::toggle(false))
        .on_press(DiagMsg::Open(i))
        .into()
}

fn card<'a>(content: impl Into<Element<'a, DiagMsg>>) -> Element<'a, DiagMsg> {
    container(content).padding(18).width(Length::Fill).style(style::card).into()
}

fn problem<'a>(e: &FordError) -> Element<'a, DiagMsg> {
    match e {
        FordError::SignedOut => card(
            column![
                text("Ford needs you to sign in to PTS first.").style(style::muted),
                button(text("Sign in to PTS"))
                    .padding([6, 14])
                    .style(button::primary)
                    .on_press(DiagMsg::SignIn)
            ]
            .spacing(10),
        ),
        e => card(
            column![
                text(format!("Couldn't search ({})", e.text())).style(style::muted),
                button(text("Try again"))
                    .padding([6, 14])
                    .style(style::ghost)
                    .on_press(DiagMsg::Search)
            ]
            .spacing(10),
        ),
    }
}

pub fn view(d: &DiagView, sample: bool) -> Element<'_, DiagMsg> {
    if sample {
        return container(card(
            text("Diagnostics searches your real ROs' vehicles, once you're signed in to Xtime.").style(style::muted),
        ))
        .padding(16)
        .into();
    }
    let bar = row![
        text_input("DTCs and/or words: P0420 U3000:49 no crank…", &d.query)
            .id(input_id())
            .on_input(DiagMsg::Query)
            .on_submit(DiagMsg::Search)
            .padding(9)
            .size(15),
        button(text("Search").font(style::BOLD))
            .padding([9, 18])
            .style(button::primary)
            .on_press(DiagMsg::Search),
        checkbox("Exact words", d.exact).text_size(13).on_toggle(DiagMsg::Exact),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let mut body = Column::new().spacing(12);
    match &d.search {
        None => {
            body = body.push(card(
                column![
                    text("Search this vehicle the way Ford's Diagnostics tab does.")
                        .size(15)
                        .font(style::BOLD),
                    text("Type one or more DTCs (P0420, U3000:49) and/or words (no crank, liftgate).").style(style::muted),
                    text(
                        "You get the matching TSBs, GSBs and SSMs, pinpoint tests and workshop procedures. Pinpoint tests open right \
                         at the test in this vehicle's Workshop Manual tab."
                    )
                    .style(style::muted),
                ]
                .spacing(8),
            ))
        }
        Some(s) if s.counts.is_none() => {
            body = body.push(match &s.error {
                Some(e) => problem(e),
                None => card(text(format!("Searching Ford for {}…", s.query)).style(style::muted)),
            })
        }
        Some(s) => {
            // Ford's words on the DTCs searched
            let info: Vec<&DtcInfo> = s.dtc_info.iter().filter(|x| !x.description.is_empty()).collect();
            if !info.is_empty() {
                let mut c = Column::new().spacing(4);
                for x in info {
                    c = c.push(row![text(x.name.as_str()).font(style::BOLD).width(90), text(x.description.as_str())].spacing(8));
                }
                body = body.push(container(c).padding([10, 14]).width(Length::Fill).style(style::card));
            }
            // the groups, each with its count (PC/ED only when there's some)
            let shown: Vec<&(&str, &str, &[&str])> = GROUPS.iter().filter(|g| g.0 != "pced" || s.count(g.0) > 0).collect();
            let active = shown.iter().position(|g| g.0 == d.group).unwrap_or(0);
            let tabs: Vec<Element<'_, DiagMsg>> = shown
                .iter()
                .map(|g| {
                    let n = s.count(g.0);
                    let on = g.0 == d.group;
                    button(row![text(g.1).size(13).font(style::BOLD), text(n.to_string()).size(12)].spacing(6))
                        .padding([5, 12])
                        .style(style::tab(on))
                        .on_press(DiagMsg::Group(g.0.to_string()))
                        .into()
                })
                .collect();
            body = body.push(anim::tabs(active, tabs));
            if shown.iter().all(|g| s.count(g.0) == 0) {
                body = body.push(card(
                    text(format!("Nothing found for {} on this vehicle.", s.query)).style(style::muted),
                ));
            } else {
                let list = s.lists.get(&d.group);
                let total = s.count(&d.group);
                match list {
                    _ if total == 0 => body = body.push(card(text("Nothing here for this search.").style(style::muted))),
                    None => body = body.push(card(text("Loading…").style(style::muted))),
                    Some(l) if l.items.is_empty() && l.error.is_some() => body = body.push(problem(l.error.as_ref().expect("an error"))),
                    Some(l) if l.items.is_empty() => body = body.push(card(text("Loading…").style(style::muted))),
                    Some(l) => {
                        let mut items = Column::new().spacing(6);
                        for (i, r) in l.items.iter().enumerate() {
                            items = items.push(result_item(i, r));
                        }
                        body = body.push(items);
                        if (l.items.len() as u64) < total {
                            body = body.push(
                                container(
                                    button(text(if l.loading {
                                        "Loading more…".to_string()
                                    } else {
                                        format!("Show more ({} of {total})", l.items.len())
                                    }))
                                    .padding([6, 16])
                                    .style(style::ghost)
                                    .on_press_maybe((!l.loading).then_some(DiagMsg::More)),
                                )
                                .center_x(Length::Fill),
                            );
                        }
                    }
                }
            }
        }
    }
    column![
        container(bar).padding([14, 16]).width(Length::Fill).style(style::panel),
        scrollable(container(body.max_width(1000)).padding(16).width(Length::Fill)).height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(nice_date("2023-02-01T00:00:00"), "Feb 1, 2023");
        assert_eq!(nice_date("soon"), "soon");
    }

    #[test]
    fn counts_by_group() {
        let s = Search {
            counts: Some(
                [("tsb".to_string(), 2), ("ssm".to_string(), 1), ("wsmppt".to_string(), 4)]
                    .into_iter()
                    .collect(),
            ),
            ..Default::default()
        };
        assert_eq!((s.count("tsb"), s.count("wsmppt"), s.count("wiring")), (3, 4, 0));
    }
}
