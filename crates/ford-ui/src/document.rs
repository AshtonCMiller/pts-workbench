//! Ford's documents drawn in the app's own style (light or dark, its fonts): the blocks the `ford` crate reads from
//! Ford's pages ([`ford::doc`]) as iced widgets. Headings, paragraphs (bold, italic, links you can click), warnings
//! and notes set off in colour, lists, tables (ruled when they're real tables), pictures, rules.

use std::collections::HashMap;

use ford::doc::{Block, Callout, Cell, Inline, Pinpoint};
use iced::widget::image::Handle;
use iced::widget::{
    Column, Row, Space, button, column, container, horizontal_rule, horizontal_space, image, mouse_area, rich_text, row, span, text,
};
use iced::{Alignment, Border, Color, Element, Font, Length, Theme, font};
use ui_kit::{pill, style};

/// A picture in a document: being fetched, ready, or couldn't be had
#[derive(Debug, Clone)]
pub enum Pic {
    Loading,
    Ready(Handle),
    Failed,
}

/// What a document does: a link was clicked (its address)
#[derive(Debug, Clone)]
pub enum DocMsg {
    Link(String),
    /// A picture was clicked (its address): to see it bigger
    Picture(String),
}

/// Links: a blue that reads on light and dark
const LINK: Color = Color::from_rgb(0.23, 0.51, 0.96);
/// The widest a picture is shown
const PIC_W: f32 = 720.0;

fn italic() -> Font {
    Font {
        style: font::Style::Italic,
        ..Font::DEFAULT
    }
}

fn bold_italic() -> Font {
    Font {
        style: font::Style::Italic,
        ..style::BOLD
    }
}

/// A paragraph's words, with their bold, italic and links
fn words<'a>(inl: &'a [Inline], size: f32) -> Element<'a, DocMsg> {
    let spans: Vec<text::Span<'a, DocMsg>> = inl
        .iter()
        .map(|i| match i {
            Inline::Text { text, bold, italic: it } => {
                let s = span(text.as_str());
                match (*bold, *it) {
                    (true, true) => s.font(bold_italic()),
                    (true, false) => s.font(style::BOLD),
                    (false, true) => s.font(italic()),
                    (false, false) => s,
                }
            }
            Inline::Link { text, url } => span(text.as_str()).color(LINK).link(DocMsg::Link(url.clone())),
            Inline::Break => span("\n"),
        })
        .collect();
    rich_text(spans).size(size).into()
}

fn callout_color(c: Callout) -> Color {
    match c {
        Callout::Warning => Color::from_rgb8(0xdc, 0x26, 0x26),
        Callout::Caution => Color::from_rgb8(0xd9, 0x77, 0x06),
        Callout::Notice => Color::from_rgb8(0x7a, 0x5a, 0xf5),
        Callout::Note => Color::from_rgb8(0x1a, 0x5f, 0xb4),
    }
}

fn block<'a>(b: &'a Block, pics: &'a HashMap<String, Pic>) -> Element<'a, DocMsg> {
    match b {
        Block::Heading(level, inl) => {
            let size = match level {
                1 => 22.0,
                2 => 19.0,
                3 => 17.0,
                _ => 15.0,
            };
            // headings are bold throughout
            let spans: Vec<text::Span<'a, DocMsg>> = inl
                .iter()
                .map(|i| match i {
                    Inline::Text { text, .. } => span(text.as_str()).font(style::BOLD),
                    Inline::Link { text, url } => span(text.as_str()).font(style::BOLD).color(LINK).link(DocMsg::Link(url.clone())),
                    Inline::Break => span("\n"),
                })
                .collect();
            container(rich_text(spans).size(size)).padding([4, 0]).into()
        }
        Block::Para(inl, None) => words(inl, 14.0),
        Block::Para(inl, Some(c)) => {
            let color = callout_color(*c);
            container(words(inl, 14.0))
                .padding([8, 12])
                .width(Length::Fill)
                .style(move |_: &Theme| container::Style {
                    background: Some(Color { a: 0.10, ..color }.into()),
                    border: Border {
                        color: Color { a: 0.55, ..color },
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                })
                .into()
        }
        Block::List { ordered, start, items } => Column::with_children(items.iter().enumerate().map(|(n, item)| {
            let mark = if *ordered { format!("{}.", start + n) } else { "•".to_string() };
            row![text(mark).size(14).width(22), blocks(item, pics)].spacing(4).into()
        }))
        .spacing(6)
        .into(),
        Block::Table { grid, rows } => table(*grid, rows, pics),
        Block::Image { url, alt } => picture(url, alt, pics),
        Block::Rule => horizontal_rule(1).into(),
        Block::Pinpoint(p) => pinpoint(p, pics),
        Block::Anchor(_) => Space::with_height(0).into(),
    }
}

/// The id a place in a procedure (Ford's anchor name) has on screen, to scroll to
pub fn anchor_id(name: &str) -> iced::widget::container::Id {
    iced::widget::container::Id::new(anchor_name(name))
}

/// (that id's name, to find it by)
pub fn anchor_name(name: &str) -> String {
    format!("anchor:{name}")
}

/// A pinpoint test in its procedure: a card that opens it ("ppt:A"); one without steps ("Diagnostic steps are not
/// provided…") is just its title and what it says
fn pinpoint<'a>(p: &'a Pinpoint, pics: &'a HashMap<String, Pic>) -> Element<'a, DocMsg> {
    if !p.has_steps() {
        return column![text(p.title.as_str()).size(17).font(style::BOLD), blocks(&p.intro, pics)]
            .spacing(8)
            .into();
    }
    let about = if p.steps.is_empty() {
        "Its steps load when you open it".to_string()
    } else {
        format!("{} steps, answered one at a time", p.steps.len())
    };
    button(
        row![
            pill("Pinpoint test", [0xd9, 0x77, 0x06]),
            column![
                text(p.title.as_str()).size(15).font(style::BOLD),
                text(about).size(12).style(style::muted)
            ]
            .spacing(2),
            horizontal_space(),
            text("Open ›").size(14).color(LINK),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([12, 14])
    .width(Length::Fill)
    .style(style::toggle(false))
    .on_press(DocMsg::Link(format!("ppt:{}", p.name)))
    .into()
}

fn picture<'a>(url: &'a str, alt: &'a str, pics: &'a HashMap<String, Pic>) -> Element<'a, DocMsg> {
    let note = |t: &'a str| -> Element<'a, DocMsg> {
        container(text(t).size(12).style(style::muted))
            .padding([10, 14])
            .style(style::card)
            .into()
    };
    match pics.get(url) {
        Some(Pic::Ready(h)) => mouse_area(container(image(h.clone())).max_width(PIC_W))
            .interaction(iced::mouse::Interaction::Pointer)
            .on_press(DocMsg::Picture(url.to_string()))
            .into(),
        Some(Pic::Failed) => note(if alt.is_empty() { "A picture that couldn't load" } else { alt }),
        _ => note("Loading the picture…"),
    }
}

fn table<'a>(grid: bool, rows: &'a [Vec<Cell>], pics: &'a HashMap<String, Pic>) -> Element<'a, DocMsg> {
    let line = move |theme: &Theme| style::colors(theme).line;
    let body = Column::with_children(rows.iter().map(|r| {
        let header = r.iter().all(|c| c.header);
        Row::with_children(r.iter().map(|c| {
            let cell = container(blocks(&c.blocks, pics))
                .padding(if grid { [6, 8] } else { [2, 6] })
                .width(Length::FillPortion(c.span));
            if grid {
                cell.style(move |theme: &Theme| container::Style {
                    background: header.then(|| style::colors(theme).hover.into()),
                    border: Border {
                        color: line(theme),
                        width: 0.5,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                })
                .into()
            } else {
                cell.into()
            }
        }))
        .into()
    }));
    if grid {
        container(body)
            .style(move |theme: &Theme| container::Style {
                border: Border {
                    color: line(theme),
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
            .into()
    } else {
        body.into()
    }
}

/// Blocks one under another
pub fn blocks<'a>(bs: &'a [Block], pics: &'a HashMap<String, Pic>) -> Element<'a, DocMsg> {
    if bs.is_empty() {
        return Space::with_height(0).into();
    }
    // a place links go to (an anchor) is the top of the block after it
    let mut col = Column::new().spacing(10).width(Length::Fill);
    let mut marks: Vec<&str> = Vec::new();
    for b in bs {
        if let Block::Anchor(names) = b {
            marks.extend(names.iter().map(String::as_str));
            continue;
        }
        let mut el = block(b, pics);
        for m in marks.drain(..) {
            el = container(el).width(Length::Fill).id(anchor_id(m)).into();
        }
        col = col.push(el);
    }
    for m in marks {
        col = col.push(container(Space::with_height(0)).id(anchor_id(m)));
    }
    col.into()
}

/// A whole document
pub fn view<'a>(bs: &'a [Block], pics: &'a HashMap<String, Pic>) -> Element<'a, DocMsg> {
    column![blocks(bs, pics)].align_x(Alignment::Start).width(Length::Fill).into()
}
