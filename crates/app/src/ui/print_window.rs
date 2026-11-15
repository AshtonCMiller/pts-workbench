//! The app's own Print window (a card over everything, like Settings): a preview of every page as it'll print, the
//! printer (the default picked to start with), copies, and Print. No system dialog: the pages go straight to the
//! printer picked here (print/).

use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Program};
use iced::widget::{Column, Space, button, column, container, horizontal_space, pick_list, row, scrollable, text};
use iced::{Alignment, Color, Element, Font, Length, Pixels, Point, Rectangle, Renderer, Shadow, Size, Theme, Vector, font};

use super::style;
use crate::app::{App, Message};
use crate::print::{Doc, Op, PAGE_H, PAGE_W};

/// How wide a page shows in the preview
const PREVIEW_W: f32 = 430.0;

/// The font the pages print in: Segoe UI on Windows; Courier elsewhere (what the PDF uses, print/pdf.rs)
fn page_font(bold: bool) -> Font {
    let base = if cfg!(windows) {
        Font::with_name("Segoe UI")
    } else {
        Font::MONOSPACE
    };
    Font {
        weight: if bold { font::Weight::Bold } else { font::Weight::Normal },
        ..base
    }
}

/// One page, drawn as it prints (white paper, scaled down)
struct Page<'a> {
    ops: &'a [Op],
}

impl Program<Message> for Page<'_> {
    type State = ();

    fn draw(&self, _state: &(), renderer: &Renderer, _theme: &Theme, bounds: Rectangle, _cursor: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let k = bounds.width / PAGE_W;
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::WHITE);
        let ink = |grey: bool| {
            if grey {
                Color::from_rgb8(0x55, 0x55, 0x55)
            } else {
                Color::from_rgb8(0x11, 0x11, 0x11)
            }
        };
        for op in self.ops {
            match op {
                Op::Text {
                    x,
                    y,
                    size,
                    bold,
                    grey,
                    text,
                } => frame.fill_text(canvas::Text {
                    content: text.clone(),
                    position: Point::new(x * k, y * k),
                    color: ink(*grey),
                    size: Pixels(size * k),
                    font: page_font(*bold),
                    ..canvas::Text::default()
                }),
                Op::Rule { x, y, w, h, grey } => frame.fill_rectangle(
                    Point::new(x * k, y * k),
                    Size::new(w * k, (h * k).max(0.6)),
                    if *grey { Color::from_rgb8(0xbb, 0xbb, 0xbb) } else { ink(false) },
                ),
            }
        }
        vec![frame.into_geometry()]
    }
}

/// Every page, one under the other, each like a sheet of paper
fn preview(doc: &Doc) -> Element<'_, Message> {
    let count = doc.pages.len();
    let mut pages = Column::new().spacing(14).align_x(Alignment::Center).padding(18);
    for (i, ops) in doc.pages.iter().enumerate() {
        let sheet =
            container(Canvas::new(Page { ops }).width(PREVIEW_W).height(PREVIEW_W * PAGE_H / PAGE_W)).style(|_: &Theme| container::Style {
                shadow: Shadow {
                    color: Color { a: 0.25, ..Color::BLACK },
                    offset: Vector::new(0.0, 4.0),
                    blur_radius: 14.0,
                },
                ..Default::default()
            });
        pages = pages.push(sheet);
        if count > 1 {
            pages = pages.push(text(format!("Page {} of {count}", i + 1)).size(12).style(style::muted));
        }
    }
    container(scrollable(pages).height(600))
        .width(PREVIEW_W + 52.0)
        .style(|theme: &Theme| container::Style {
            background: Some(style::colors(theme).line.into()),
            border: iced::Border {
                radius: 10.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

pub fn card(app: &App) -> Element<'_, Message> {
    let Some(job) = &app.print_job else {
        return Space::with_width(0).into();
    };

    // the printer: found ones to pick from (the default to start with)
    let printer: Element<'_, Message> = match &job.printers {
        None => text("Finding printers…").style(style::muted).into(),
        Some(p) if p.names.is_empty() => text("No printers found").style(style::muted).into(),
        Some(p) => pick_list(p.names.as_slice(), job.printer.clone(), Message::PickPrinter)
            .width(Length::Fill)
            .padding(8)
            .into(),
    };
    let is_default = match (&job.printers, &job.printer) {
        (Some(p), Some(name)) => p.default.as_ref() == Some(name),
        _ => false,
    };

    let step = |label: &'static str, to: u32| {
        button(text(label).size(16).align_x(Alignment::Center))
            .width(34)
            .padding([2, 0])
            .style(style::ghost)
            .on_press_maybe((to != job.copies && (1..=99).contains(&to)).then_some(Message::Copies(to)))
    };
    let copies = row![
        step("-", job.copies.saturating_sub(1)),
        text(job.copies.to_string()).size(16).width(36).align_x(Alignment::Center),
        step("+", job.copies + 1),
    ]
    .align_y(Alignment::Center);

    let pages = job.preview.pages.len();
    let mut side = column![
        column![
            text("Print").size(22).font(style::BOLD),
            text(job.what.as_str()).style(style::muted),
        ]
        .spacing(2),
        column![text("Printer").font(style::BOLD), printer].spacing(6),
    ]
    .spacing(20)
    .width(280);
    // the system's printer settings, to add a printer that isn't here (it shows up in the list once added)
    let manage = button(text("Manage printers").size(13).style(text::primary))
        .padding(0)
        .style(button::text)
        .on_press(Message::ManagePrinters);
    side = side.push(
        row![
            text(if is_default { "Your default printer" } else { "" })
                .size(12)
                .style(style::muted),
            horizontal_space(),
            manage,
        ]
        .align_y(Alignment::Center),
    );
    side = side.push(column![text("Copies").font(style::BOLD), copies].spacing(6)).push(
        text(format!("{pages} page{} · US Letter", if pages == 1 { "" } else { "s" }))
            .size(13)
            .style(style::muted),
    );
    if let Some(e) = &job.error {
        side = side.push(text(e.as_str()).size(13).style(text::danger));
    }
    let can_print = !job.busy && job.printer.is_some();
    side = side.push(Space::with_height(Length::Fill)).push(
        row![
            horizontal_space(),
            button(text("Cancel"))
                .padding([8, 16])
                .style(style::ghost)
                .on_press(Message::ClosePrint),
            button(text(if job.busy { "Printing…" } else { "Print" }).font(style::BOLD))
                .padding([8, 22])
                .style(button::primary)
                .on_press_maybe(can_print.then_some(Message::PrintNow)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

    container(row![preview(&job.preview), side.height(600)].spacing(24))
        .padding(24)
        .style(style::floating_card)
        .into()
}
