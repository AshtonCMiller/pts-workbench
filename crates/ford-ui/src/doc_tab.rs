//! A Service Manual tab of its own for one of Ford's documents (an FSA opened from OASIS, or one of Ford's notices):
//! its documents listed down the left (Dealer bulletin, Technical instructions, Owner letter…, as on Ford's site; the
//! list slides away with Contents), the one picked drawn as pages filling the rest ([`crate::pdf`]). The dealer bulletin shows first. Without PDFium beside the app, a document opens in
//! the computer's PDF viewer instead. A page with no documents is shown as Ford wrote it.

use std::collections::HashMap;
use std::sync::Arc;

use ford::fsa::FsaPage;
use iced::widget::image::Handle;
use iced::widget::{Column, Space, button, column, container, image, row, text};
use iced::{Alignment, Element, Length, Task};
use ui_kit::scroll::scrollable;
use ui_kit::{anim, pill, style};

use crate::article::kind_color;
use crate::document::{self, Pic};
use crate::pdf::{self, PdfFail};
use crate::{Env, FordError, Out};

/// The widest pages are shown, and how wide the list of documents is
const PAGE_W: f32 = 900.0;
const SIDE_W: f32 = 250.0;

/// One of the tab's documents
#[derive(Debug, Clone)]
pub enum PdfState {
    Loading,
    /// Drawn: the PDF itself (to open elsewhere too), and its pages
    Pages(Arc<Vec<u8>>, Vec<Handle>),
    /// PDFium isn't beside the app: the PDF, to open in the computer's viewer
    NoViewer(Arc<Vec<u8>>),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct DocTab {
    /// Which tab it is (tabs come and go)
    pub key: u32,
    /// "FSA", "Notice"
    pub kind: String,
    pub id: String,
    pub title: String,
    pub url: String,
    pub loading: bool,
    pub error: Option<FordError>,
    pub page: Option<FsaPage>,
    /// The document showing (its place in the page's list)
    pub selected: usize,
    pub pdfs: HashMap<String, PdfState>,
    pub note: Option<String>,
    /// (pictures of a page shown as Ford wrote it: none are fetched)
    pub pics: HashMap<String, Pic>,
    /// The list of documents on the left is showing
    pub contents_open: bool,
}

#[derive(Debug, Clone)]
pub enum DocMsg {
    Loaded(Result<FsaPage, FordError>),
    Select(usize),
    Pdf(String, Result<(Arc<Vec<u8>>, Vec<Handle>), PdfFail>),
    Retry,
    /// Open the document showing in the computer's PDF viewer
    OpenOutside,
    Opened(Result<(), String>),
    SignIn,
    /// A link in a page shown as Ford wrote it
    Link(String),
    /// Show or hide the list of documents
    ToggleContents,
}

impl DocTab {
    pub fn new(key: u32, kind: &str, id: &str, title: &str, url: &str) -> Self {
        Self {
            key,
            kind: kind.to_string(),
            id: id.to_string(),
            title: title.to_string(),
            url: url.to_string(),
            loading: false,
            error: None,
            page: None,
            selected: 0,
            pdfs: HashMap::new(),
            note: None,
            pics: HashMap::new(),
            contents_open: true,
        }
    }

    /// What the tab says: "FSA 24S10", "Notice"
    pub fn label(&self) -> String {
        format!("{} {}", self.kind, self.id).trim().to_string()
    }

    pub fn load(&mut self, env: &Env) -> Task<DocMsg> {
        if self.loading {
            return Task::none();
        }
        self.loading = true;
        self.error = None;
        let (ford, url) = (env.ford.clone(), self.url.clone());
        Task::perform(async move { ford.fsa_page(&url).await.map_err(FordError::from) }, DocMsg::Loaded)
    }

    /// Fetches and draws the document showing, unless it has been already
    fn load_selected(&mut self, env: &Env) -> Task<DocMsg> {
        let Some(doc) = self.page.as_ref().and_then(|p| p.docs.get(self.selected)) else {
            return Task::none();
        };
        if matches!(
            self.pdfs.get(&doc.url),
            Some(PdfState::Loading | PdfState::Pages(..) | PdfState::NoViewer(_))
        ) {
            return Task::none();
        }
        let url = doc.url.clone();
        self.pdfs.insert(url.clone(), PdfState::Loading);
        let ford = env.ford.clone();
        Task::perform(
            async move {
                let r = match ford.bytes(&url).await {
                    Ok((bytes, _)) => {
                        let keep = Arc::new(bytes.clone());
                        pdf::render(bytes).await.map(|pages| (keep, pages))
                    }
                    Err(e) => Err(PdfFail::Failed(FordError::from(e).text())),
                };
                (url, r)
            },
            |(u, r)| DocMsg::Pdf(u, r),
        )
    }

    pub fn update(&mut self, msg: DocMsg, env: &Env) -> (Task<DocMsg>, Vec<Out>) {
        let mut outs = Vec::new();
        let task = match msg {
            DocMsg::Loaded(r) => {
                self.loading = false;
                match r {
                    Ok(p) => {
                        // the dealer bulletin first, when there is one
                        self.selected = p.docs.iter().position(|d| d.name == "Dealer bulletin").unwrap_or(0);
                        self.page = Some(p);
                        self.load_selected(env)
                    }
                    Err(e) => {
                        self.error = Some(e);
                        Task::none()
                    }
                }
            }
            DocMsg::Select(i) => {
                self.selected = i;
                self.note = None;
                self.load_selected(env)
            }
            DocMsg::Pdf(url, r) => {
                let state = match r {
                    Ok((bytes, pages)) => PdfState::Pages(bytes, pages),
                    Err(PdfFail::NoPdfium(bytes)) => PdfState::NoViewer(bytes),
                    Err(PdfFail::Failed(e)) => PdfState::Failed(e),
                };
                self.pdfs.insert(url, state);
                Task::none()
            }
            DocMsg::Retry => {
                if self.page.is_none() {
                    self.load(env)
                } else {
                    if let Some(d) = self.page.as_ref().and_then(|p| p.docs.get(self.selected)) {
                        self.pdfs.remove(&d.url);
                    }
                    self.load_selected(env)
                }
            }
            DocMsg::OpenOutside => {
                let doc = self.page.as_ref().and_then(|p| p.docs.get(self.selected));
                let bytes = doc.and_then(|d| match self.pdfs.get(&d.url) {
                    Some(PdfState::Pages(b, _) | PdfState::NoViewer(b)) => Some(b.clone()),
                    _ => None,
                });
                if let (Some(d), Some(b)) = (doc, bytes) {
                    let name = format!("{} {}", self.label(), d.name);
                    let r = crate::open_file(&name, "pdf", &b);
                    self.note = Some(match r {
                        Ok(()) => "Opened in your PDF viewer".into(),
                        Err(e) => format!("Couldn't open it ({e})"),
                    });
                }
                Task::none()
            }
            DocMsg::Opened(r) => {
                if let Err(e) = r {
                    self.note = Some(format!("Couldn't open it ({e})"));
                }
                Task::none()
            }
            DocMsg::SignIn => {
                outs.push(Out::SignIn);
                Task::none()
            }
            DocMsg::ToggleContents => {
                self.contents_open = !self.contents_open;
                Task::none()
            }
            DocMsg::Link(url) => {
                if let Err(e) = crate::open_external(&url) {
                    self.note = Some(format!("Couldn't open the link ({e})"));
                }
                Task::none()
            }
        };
        (task, outs)
    }
}

fn card<'a>(content: impl Into<Element<'a, DocMsg>>) -> Element<'a, DocMsg> {
    container(content).padding(18).width(Length::Fill).style(style::card).into()
}

fn state<'a>(t: String, action: Option<(&'a str, DocMsg)>) -> Element<'a, DocMsg> {
    let mut c = column![text(t).style(style::muted)].spacing(10);
    if let Some((label, msg)) = action {
        c = c.push(button(text(label)).padding([6, 14]).style(button::primary).on_press(msg));
    }
    container(card(c)).padding(16).into()
}

pub fn view(d: &DocTab) -> Element<'_, DocMsg> {
    // the heading: what it is, and what can be done with the document showing
    let mut head = row![].spacing(10).align_y(Alignment::Center);
    if d.page.as_ref().is_some_and(|p| !p.docs.is_empty()) {
        head = head.push(
            button(text("Contents").size(13))
                .padding([5, 10])
                .style(style::toggle(d.contents_open))
                .on_press(DocMsg::ToggleContents),
        );
    }
    head = head.push(pill(d.kind.as_str(), kind_color(&d.kind)));
    if !d.id.is_empty() {
        head = head.push(text(d.id.as_str()).size(18).font(style::BOLD));
    }
    head = head.push(text(d.title.as_str()).style(style::muted).width(Length::Fill));
    let current = d.page.as_ref().and_then(|p| p.docs.get(d.selected));
    let can_open = current.is_some_and(|c| matches!(d.pdfs.get(&c.url), Some(PdfState::Pages(..) | PdfState::NoViewer(_))));
    if can_open {
        head = head.push(
            button(text("Open in PDF viewer").size(13))
                .padding([5, 12])
                .style(style::ghost)
                .on_press(DocMsg::OpenOutside),
        );
    }
    let mut top = Column::new().spacing(10).push(head);
    if let Some(n) = &d.note {
        top = top.push(text(n.as_str()).size(12).style(style::muted));
    }

    let Some(page) = &d.page else {
        let s = match (&d.error, d.loading) {
            (_, true) => state("Loading…".into(), None),
            (Some(FordError::SignedOut), _) => state("Sign in to PTS to see this.".into(), Some(("Sign in to PTS", DocMsg::SignIn))),
            (Some(e), _) => state(format!("Couldn't load it ({})", e.text()), Some(("Try again", DocMsg::Retry))),
            (None, false) => state(String::new(), Some(("Load", DocMsg::Retry))),
        };
        return column![container(top).padding(16), s].into();
    };

    // no documents: the page as Ford wrote it
    if page.docs.is_empty() {
        let body: Element<'_, DocMsg> = if page.blocks.is_empty() {
            card(text("Ford's page has no documents for this one.").style(style::muted))
        } else {
            card(document::view(&page.blocks, &d.pics).map(|m| match m {
                document::DocMsg::Link(u) => DocMsg::Link(u),
                document::DocMsg::Picture(u) => DocMsg::Link(u),
            }))
        };
        return column![
            container(top).padding([16, 16]),
            scrollable(container(body).padding([0, 16])).height(Length::Fill)
        ]
        .into();
    }

    let body: Element<'_, DocMsg> = match current.and_then(|c| d.pdfs.get(&c.url)) {
        None | Some(PdfState::Loading) => state("Loading the document…".into(), None),
        Some(PdfState::Failed(e)) => state(format!("Couldn't load it ({e})"), Some(("Try again", DocMsg::Retry))),
        Some(PdfState::NoViewer(_)) => state(
            "PDFs show here once pdfium.dll is beside the app. It can still be opened in your PDF viewer.".into(),
            Some(("Open in PDF viewer", DocMsg::OpenOutside)),
        ),
        Some(PdfState::Pages(_, pages)) => {
            let sheets = Column::with_children(pages.iter().map(|h| {
                container(image(h.clone()).width(Length::Fill))
                    .max_width(PAGE_W)
                    .style(|_: &iced::Theme| container::Style {
                        shadow: iced::Shadow {
                            color: iced::Color {
                                a: 0.25,
                                ..iced::Color::BLACK
                            },
                            offset: iced::Vector::new(0.0, 3.0),
                            blur_radius: 12.0,
                        },
                        ..Default::default()
                    })
                    .into()
            }))
            .spacing(16)
            .padding([4, 16])
            .align_x(Alignment::Center)
            .width(Length::Fill);
            scrollable(sheets).height(Length::Fill).into()
        }
    };
    // its documents down the left (sliding away with Contents), the one picked filling the rest
    let list = Column::with_children(page.docs.iter().enumerate().map(|(i, doc)| {
        button(text(doc.name.as_str()).size(13))
            .width(Length::Fill)
            .padding([8, 10])
            .style(style::list_item(i == d.selected))
            .on_press(DocMsg::Select(i))
            .into()
    }))
    .spacing(2);
    let side = container(
        column![
            text("Documents").size(12).font(style::BOLD).style(style::muted),
            scrollable(list).height(Length::Fill),
        ]
        .spacing(8),
    )
    .padding(10)
    .width(SIDE_W)
    .height(Length::Fill)
    .style(style::panel);
    row![
        anim::side(side, d.contents_open, anim::Edge::Left, SIDE_W),
        column![container(top).padding([16, 16]), body, Space::with_height(0)]
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
}
