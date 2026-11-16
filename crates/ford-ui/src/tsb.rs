//! The TSBs tab (the Electron app's `manual/tsb/`): Ford's TSBs, GSBs, SSMs (and ISBs) for the vehicle's VIN, newest
//! first, with a filter box, Ford's categories, and All / TSBs / GSBs / SSMs with how many of each. Clicking one opens
//! it, sliding in over the list (which stays where it was), drawn in the app's style ([`crate::article`]).

use ford::tsb::{Article, Category, TsbList};
use iced::widget::{Column, Space, button, column, container, horizontal_space, pick_list, row, text, text_input};
use iced::{Alignment, Element, Length, Task};
use ui_kit::scroll::scrollable;
use ui_kit::{anim, pill, style};

use crate::article::{self, ArticleMsg, ArticleView, kind_color};
use crate::{Env, FordError, Out};

/// A choice in the categories list
#[derive(Debug, Clone, PartialEq)]
pub struct CatChoice {
    pub id: Option<String>,
    pub label: String,
}

impl std::fmt::Display for CatChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TsbView {
    pub vin: String,
    pub list: Option<TsbList>,
    pub loading: bool,
    pub error: Option<FordError>,
    pub query: String,
    pub category: Option<String>,
    /// "TSB", "GSB"… (none: all)
    pub kind: Option<String>,
    /// The one open (also while it slides back out), and whether it's open
    pub article: Option<ArticleView>,
    pub open: bool,
}

#[derive(Debug, Clone)]
pub enum TsbMsg {
    Refresh,
    Loaded(Result<TsbList, FordError>),
    Query(String),
    Category(CatChoice),
    Kind(Option<String>),
    /// Open one (its place on Ford's list)
    Open(usize),
    Article(ArticleMsg),
    SignIn,
}

const KINDS: [&str; 4] = ["TSB", "GSB", "SSM", "ISB"];

impl TsbView {
    pub fn new(vin: impl Into<String>) -> Self {
        Self {
            vin: vin.into(),
            ..Default::default()
        }
    }

    /// Reads the list, unless it's being read or was read already (unless `force`)
    pub fn load(&mut self, env: &Env, force: bool) -> Task<TsbMsg> {
        if self.loading || env.sample || self.vin.trim().is_empty() || (self.list.is_some() && !force) {
            return Task::none();
        }
        self.loading = true;
        let (ford, vin) = (env.ford.clone(), self.vin.clone());
        Task::perform(async move { ford.tsb_list(&vin).await.map_err(FordError::from) }, TsbMsg::Loaded)
    }

    /// Opens a bulletin from somewhere else (Diagnostics, Connected Vehicle later): by its number when it's on the
    /// list, else by its address
    pub fn open_article(&mut self, env: &Env, a: ArticleView) -> Task<TsbMsg> {
        let mut a = a;
        let t = a.load(env).map(TsbMsg::Article);
        self.article = Some(a);
        self.open = true;
        t
    }

    pub fn update(&mut self, msg: TsbMsg, env: &Env) -> (Task<TsbMsg>, Vec<Out>) {
        let mut outs = Vec::new();
        let task = match msg {
            TsbMsg::Refresh => self.load(env, true),
            TsbMsg::Loaded(r) => {
                self.loading = false;
                match r {
                    Ok(l) => {
                        self.list = Some(l);
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                }
                Task::none()
            }
            TsbMsg::Query(q) => {
                self.query = q;
                Task::none()
            }
            TsbMsg::Category(c) => {
                self.category = c.id;
                Task::none()
            }
            TsbMsg::Kind(k) => {
                self.kind = k;
                Task::none()
            }
            TsbMsg::Open(i) => match self.list.as_ref().and_then(|l| l.articles.get(i)) {
                Some(a) => {
                    let view = ArticleView::new(&a.kind, &a.id, &a.summary, &a.date, &a.url);
                    self.open_article(env, view)
                }
                None => Task::none(),
            },
            TsbMsg::Article(ArticleMsg::Back) => {
                self.open = false;
                Task::none()
            }
            TsbMsg::Article(m) => match &mut self.article {
                Some(a) => {
                    let (t, o) = a.update(m, env);
                    outs.extend(o);
                    t.map(TsbMsg::Article)
                }
                None => Task::none(),
            },
            TsbMsg::SignIn => {
                outs.push(Out::SignIn);
                Task::none()
            }
        };
        (task, outs)
    }

    /// What the filter box and category leave (with their places on Ford's list)
    fn filtered(&self) -> Vec<(usize, &Article)> {
        let Some(l) = &self.list else { return Vec::new() };
        let words: Vec<String> = self.query.to_lowercase().split_whitespace().map(str::to_string).collect();
        l.articles
            .iter()
            .enumerate()
            .filter(|(_, a)| self.category.as_ref().is_none_or(|c| a.categories.contains(c)))
            .filter(|(_, a)| {
                let hay = format!("{} {} {} {}", a.title, a.id, a.summary, a.categories.join(" ")).to_lowercase();
                words.iter().all(|w| hay.contains(w))
            })
            .collect()
    }
}

fn state<'a>(title: &'a str, why: String, action: Option<(&'a str, TsbMsg)>) -> Element<'a, TsbMsg> {
    let mut c = column![text(title).size(18).font(style::BOLD), text(why).style(style::muted)].spacing(8);
    if let Some((label, msg)) = action {
        c = c.push(button(text(label)).padding([6, 14]).style(button::primary).on_press(msg));
    }
    container(c).padding(14).width(Length::Fill).style(style::card).into()
}

pub fn view(t: &TsbView, sample: bool) -> Element<'_, TsbMsg> {
    let Some(list) = &t.list else {
        let s = match (&t.error, t.loading) {
            _ if sample => state("TSBs", "TSBs show for your real ROs, once you're signed in to Xtime.".into(), None),
            (_, true) => state("Loading TSBs…", format!("Asking Ford about {}", t.vin), None),
            (Some(FordError::SignedOut), _) => state(
                "Sign in to PTS",
                "TSBs need your PTS sign-in.".into(),
                Some(("Sign in to PTS", TsbMsg::SignIn)),
            ),
            (Some(e), _) => state("Couldn't load TSBs", e.text(), Some(("Try again", TsbMsg::Refresh))),
            (None, false) => state(
                "TSBs",
                format!("The TSBs for {} haven't been read yet.", t.vin),
                Some(("Load TSBs", TsbMsg::Refresh)),
            ),
        };
        return container(s).padding(16).width(Length::Fill).into();
    };

    // the list: filter, category, kinds, then the bulletins
    let shown_all = t.filtered();
    let count = |k: Option<&str>| shown_all.iter().filter(|(_, a)| k.is_none_or(|k| a.kind == k)).count();
    let kinds: Vec<Option<&str>> = std::iter::once(None)
        .chain(
            KINDS
                .iter()
                .filter(|k| list.articles.iter().any(|a| a.kind == **k))
                .map(|k| Some(*k)),
        )
        .collect();
    let active = kinds.iter().position(|k| *k == t.kind.as_deref()).unwrap_or(0);
    let tabs = anim::tabs(
        active,
        kinds
            .iter()
            .map(|k| {
                let label = match k {
                    None => format!("All  {}", count(None)),
                    Some(k) => format!("{k}s  {}", count(Some(k))),
                };
                button(text(label).size(13))
                    .padding([4, 12])
                    .style(style::tab(t.kind.as_deref() == *k))
                    .on_press(TsbMsg::Kind(k.map(str::to_string)))
                    .into()
            })
            .collect(),
    );
    let shown: Vec<(usize, &Article)> = shown_all
        .into_iter()
        .filter(|(_, a)| t.kind.as_ref().is_none_or(|k| a.kind == *k))
        .collect();

    let mut choices = vec![CatChoice {
        id: None,
        label: "All categories".into(),
    }];
    choices.extend(list.categories.iter().map(|c: &Category| CatChoice {
        id: Some(c.id.clone()),
        label: format!(
            "{}{}{} ({})",
            "    ".repeat(c.depth),
            if c.depth > 0 { "› " } else { "" },
            c.label,
            c.count
        ),
    }));
    let picked = choices.iter().find(|c| c.id == t.category).cloned();
    let mut filters = row![
        text_input("Filter: number, words…", &t.query)
            .on_input(TsbMsg::Query)
            .padding(7)
            .width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if !list.categories.is_empty() {
        filters = filters.push(pick_list(choices, picked, TsbMsg::Category).padding(7).width(280));
    }
    let refresh = button(text(if t.loading { "Refreshing…" } else { "Refresh" }).size(13))
        .padding([5, 12])
        .style(style::ghost)
        .on_press_maybe((!t.loading).then_some(TsbMsg::Refresh));
    let head = row![
        text("TSBs").size(20).font(style::BOLD),
        text(format!("{} of {}", shown.len(), list.articles.len())).style(style::muted),
        horizontal_space(),
        refresh
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let items: Element<'_, TsbMsg> = if shown.is_empty() {
        let why = if list.articles.is_empty() {
            "Ford lists no TSBs, GSBs or SSMs for this VIN."
        } else {
            "Nothing matches that."
        };
        container(text(why).style(style::muted)).padding(20).into()
    } else {
        Column::with_children(shown.into_iter().map(|(i, a)| item(i, a))).spacing(4).into()
    };
    let mut top = column![head, filters, tabs].spacing(10);
    if let Some(e) = &t.error {
        top = top.push(text(format!("Couldn't refresh ({})", e.text())).size(13).style(text::danger));
    }
    let base = column![
        container(top).padding([16, 16]).width(Length::Fill),
        scrollable(container(items).padding([0, 16]).width(Length::Fill)).height(Length::Fill),
    ]
    .height(Length::Fill);

    // the bulletin open, sliding in over the list
    let over: Element<'_, TsbMsg> = match &t.article {
        Some(a) => {
            container(scrollable(container(article::view(a).map(TsbMsg::Article)).padding(16).width(Length::Fill)).height(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(style::page)
                .into()
        }
        None => Space::new(Length::Fill, Length::Fill).into(),
    };
    anim::push(base, over, t.open).into()
}

fn item(i: usize, a: &Article) -> Element<'_, TsbMsg> {
    let mut first = row![pill(a.kind.as_str(), kind_color(&a.kind)), text(a.id.as_str()).font(style::BOLD)]
        .spacing(8)
        .align_y(Alignment::Center);
    if !a.date.is_empty() {
        first = first
            .push(horizontal_space())
            .push(text(a.date.as_str()).size(12).style(style::muted));
    }
    let mut c = column![first].spacing(4);
    if !a.summary.is_empty() {
        c = c.push(text(a.summary.as_str()).size(13));
    }
    button(c)
        .width(Length::Fill)
        .padding([8, 10])
        .style(style::list_item(false))
        .on_press(TsbMsg::Open(i))
        .into()
}
