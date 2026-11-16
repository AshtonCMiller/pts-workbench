//! One of Ford's documents, opened (a TSB, GSB or SSM; an FSA's document): fetched from Ford and drawn in the app's
//! style ([`crate::document`]), its pictures fetched after it. Inside it, another bulletin's link opens here, a PDF
//! opens in the computer's PDF viewer, anything else in the browser. Ford's own PDFs (some bulletins are only that)
//! open the same way.

use std::collections::HashMap;

use ford::doc::{Block, query_value};
use ford::tsb::Fetched;
use iced::widget::{Column, button, column, container, horizontal_space, row, text};
use iced::{Alignment, Element, Length, Task};
use ui_kit::{pill, style};

use crate::document::{self, DocMsg, Pic};
use crate::{Env, FordError, Out};

/// The most pictures fetched for one document
const MAX_PICS: usize = 40;

#[derive(Debug, Clone, Default)]
pub struct ArticleView {
    /// "TSB", "GSB", "SSM", "FSA"…
    pub kind: String,
    pub id: String,
    pub summary: String,
    pub date: String,
    pub url: String,
    pub loading: bool,
    pub error: Option<FordError>,
    pub fetched: Option<Fetched>,
    pub pics: HashMap<String, Pic>,
    /// A PDF is being fetched to open
    pub opening: bool,
    /// What happened last (a PDF opened, couldn't be opened)
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ArticleMsg {
    Loaded(Result<Fetched, FordError>),
    Pic(String, Result<Vec<u8>, FordError>),
    Doc(DocMsg),
    Retry,
    /// Open the document's PDF (Ford publishes some only as one)
    OpenPdf,
    Opened(Result<(), String>),
    SignIn,
    /// Back to what it was opened from (the one that opened it closes it)
    Back,
}

/// A bulletin's address on Ford's site
pub fn is_bulletin(url: &str) -> bool {
    url.to_ascii_lowercase().contains("/vdirsnet/tsb_ssm/displayarticle.aspx")
}

fn is_pdf(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase();
    path.ends_with(".pdf")
}

impl ArticleView {
    pub fn new(kind: &str, id: &str, summary: &str, date: &str, url: &str) -> Self {
        Self {
            kind: kind.to_string(),
            id: id.to_string(),
            summary: summary.to_string(),
            date: date.to_string(),
            url: url.to_string(),
            ..Default::default()
        }
    }

    /// Another bulletin, from its address (its kind and number from the address)
    pub fn from_link(url: &str, text: &str) -> Self {
        let kind = query_value(url, "type").map(|t| t.to_uppercase()).unwrap_or_else(|| "TSB".into());
        let id = query_value(url, "id").unwrap_or_else(|| text.to_string());
        Self::new(&kind, &id, &query_value(url, "t").unwrap_or_default(), "", url)
    }

    /// The heading: "TSB 26-2414"
    pub fn title(&self) -> String {
        format!("{} {}", self.kind, self.id).trim().to_string()
    }

    pub fn load(&mut self, env: &Env) -> Task<ArticleMsg> {
        if self.loading || self.url.is_empty() {
            return Task::none();
        }
        self.loading = true;
        self.error = None;
        let (ford, url) = (env.ford.clone(), self.url.clone());
        Task::perform(
            async move { ford.document(&url).await.map_err(FordError::from) },
            ArticleMsg::Loaded,
        )
    }

    /// Fetches the document's pictures (each on its own)
    fn load_pics(&mut self, env: &Env) -> Task<ArticleMsg> {
        let Some(Fetched::Doc { blocks, .. }) = &self.fetched else {
            return Task::none();
        };
        let urls: Vec<String> = Block::images(blocks).into_iter().take(MAX_PICS).collect();
        let mut tasks = Vec::new();
        for u in urls {
            if self.pics.contains_key(&u) {
                continue;
            }
            self.pics.insert(u.clone(), Pic::Loading);
            let ford = env.ford.clone();
            tasks.push(Task::perform(
                async move {
                    let r = ford.bytes(&u).await.map(|(b, _)| b).map_err(FordError::from);
                    (u, r)
                },
                |(u, r)| ArticleMsg::Pic(u, r),
            ));
        }
        Task::batch(tasks)
    }

    pub fn update(&mut self, msg: ArticleMsg, env: &Env) -> (Task<ArticleMsg>, Vec<Out>) {
        let mut outs = Vec::new();
        let task = match msg {
            ArticleMsg::Loaded(r) => {
                self.loading = false;
                match r {
                    Ok(f) => {
                        self.fetched = Some(f);
                        self.load_pics(env)
                    }
                    Err(e) => {
                        self.error = Some(e);
                        Task::none()
                    }
                }
            }
            ArticleMsg::Pic(u, r) => {
                let pic = match r {
                    Ok(bytes) => Pic::Ready(iced::widget::image::Handle::from_bytes(bytes)),
                    Err(_) => Pic::Failed,
                };
                self.pics.insert(u, pic);
                Task::none()
            }
            ArticleMsg::Doc(DocMsg::Link(url)) => {
                if is_bulletin(&url) {
                    // another bulletin: here, in its place
                    *self = ArticleView::from_link(&url, "");
                    self.load(env)
                } else if is_pdf(&url) {
                    self.open_pdf(env, url)
                } else {
                    if let Err(e) = crate::open_external(&url) {
                        self.note = Some(format!("Couldn't open the link ({e})"));
                    }
                    Task::none()
                }
            }
            ArticleMsg::Doc(DocMsg::Picture(_)) => Task::none(),
            ArticleMsg::Retry => {
                self.fetched = None;
                self.pics.clear();
                self.load(env)
            }
            ArticleMsg::OpenPdf => match &self.fetched {
                Some(Fetched::Pdf { url }) => {
                    let url = url.clone();
                    self.open_pdf(env, url)
                }
                _ => Task::none(),
            },
            ArticleMsg::Opened(r) => {
                self.opening = false;
                self.note = Some(match r {
                    Ok(()) => "Opened in your PDF viewer".into(),
                    Err(e) => format!("Couldn't open the PDF ({e})"),
                });
                Task::none()
            }
            ArticleMsg::SignIn => {
                outs.push(Out::SignIn);
                Task::none()
            }
            ArticleMsg::Back => Task::none(),
        };
        (task, outs)
    }

    /// Fetches a PDF from Ford and opens it in the computer's PDF viewer
    fn open_pdf(&mut self, env: &Env, url: String) -> Task<ArticleMsg> {
        if self.opening {
            return Task::none();
        }
        self.opening = true;
        self.note = None;
        let ford = env.ford.clone();
        let name = self.title();
        Task::perform(
            async move {
                let (bytes, _) = ford.bytes(&url).await.map_err(|e| FordError::from(e).text())?;
                crate::open_file(&name, "pdf", &bytes)
            },
            ArticleMsg::Opened,
        )
    }
}

/// The document: its heading (Back, kind, number, what it's about), then the document itself (or why not)
pub fn view(a: &ArticleView) -> Element<'_, ArticleMsg> {
    let mut head = row![
        button(text("‹ Back").size(13))
            .padding([4, 10])
            .style(style::ghost)
            .on_press(ArticleMsg::Back),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if !a.kind.is_empty() {
        head = head.push(pill(a.kind.as_str(), kind_color(&a.kind)));
    }
    head = head.push(text(a.id.as_str()).size(17).font(style::BOLD));
    if !a.date.is_empty() {
        head = head.push(text(format!("Effective {}", a.date)).size(12).style(style::muted));
    }
    head = head.push(horizontal_space());
    let mut top = Column::new().spacing(6).push(head);
    if !a.summary.is_empty() {
        top = top.push(text(a.summary.as_str()).style(style::muted));
    }
    if let Some(n) = &a.note {
        top = top.push(text(n.as_str()).size(12).style(style::muted));
    }

    let body: Element<'_, ArticleMsg> = match (&a.fetched, &a.error, a.loading) {
        (_, _, true) => note("Loading the document…", None),
        (_, Some(FordError::SignedOut), _) => note("Sign in to PTS to read this.", Some(("Sign in to PTS", ArticleMsg::SignIn))),
        (_, Some(e), _) => note2(format!("Couldn't load it ({})", e.text()), ("Try again", ArticleMsg::Retry)),
        (Some(Fetched::Pdf { .. }), _, _) => {
            let label = if a.opening { "Opening…" } else { "Open the PDF" };
            card(
                column![
                    text("Ford publishes this one as a PDF."),
                    button(text(label))
                        .padding([6, 14])
                        .style(button::primary)
                        .on_press_maybe((!a.opening).then_some(ArticleMsg::OpenPdf)),
                ]
                .spacing(10),
            )
        }
        (Some(Fetched::Doc { blocks, .. }), _, _) if Block::text(blocks).is_empty() && Block::images(blocks).is_empty() => {
            note("Ford's document came back empty.", Some(("Try again", ArticleMsg::Retry)))
        }
        (Some(Fetched::Doc { blocks, .. }), _, _) => card(document::view(blocks, &a.pics).map(ArticleMsg::Doc)),
        (None, None, false) => note("", None),
    };
    column![top, body].spacing(14).width(Length::Fill).into()
}

pub fn kind_color(kind: &str) -> [u8; 3] {
    match kind {
        "TSB" => [0x1a, 0x5f, 0xb4],
        "GSB" => [0x15, 0x80, 0x3d],
        "SSM" => [0xd9, 0x77, 0x06],
        "FSA" => [0xdc, 0x26, 0x26],
        _ => [0x5d, 0x66, 0x70],
    }
}

fn card<'a>(content: impl Into<Element<'a, ArticleMsg>>) -> Element<'a, ArticleMsg> {
    container(content).padding(18).width(Length::Fill).style(style::card).into()
}

fn note<'a>(t: &'a str, action: Option<(&'a str, ArticleMsg)>) -> Element<'a, ArticleMsg> {
    let mut c = column![text(t).style(style::muted)].spacing(10);
    if let Some((label, msg)) = action {
        c = c.push(button(text(label)).padding([6, 14]).style(button::primary).on_press(msg));
    }
    card(c)
}

fn note2<'a>(t: String, action: (&'a str, ArticleMsg)) -> Element<'a, ArticleMsg> {
    card(
        column![
            text(t).style(style::muted),
            button(text(action.0)).padding([6, 14]).style(button::primary).on_press(action.1),
        ]
        .spacing(10),
    )
}
