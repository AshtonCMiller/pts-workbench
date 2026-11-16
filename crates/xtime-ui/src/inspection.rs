//! The Xtime View's inspection form (the Electron app's xtime-view/inspection.js), at the bottom of the RO: each
//! category's items with Green / Yellow / Red, their sub-items and value box, saved to Xtime as Xtime's page saves them;
//! the store's other forms to switch to. Pressing a colour it already has clears it; sub-items add to the value.

use iced::widget::{Column, Row, Space, button, column, container, horizontal_space, pick_list, row, text, text_input};
use iced::{Alignment, Border, Color, Element, Length, Theme};
use xtime::inspect::{InspectionForm, Item, Press};

use ui_kit::{pill, style};

use crate::{Ask, Msg, XtimeView};

/// 0x15803d → the colour
const fn rgb(hex: u32) -> Color {
    Color {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

const GREEN: Color = rgb(0x15803d);
const YELLOW: Color = rgb(0xd99a06);
const RED: Color = rgb(0xb42318);

fn color_of(name: &str) -> Option<Color> {
    match name {
        "Green" => Some(GREEN),
        "Yellow" => Some(YELLOW),
        "Red" => Some(RED),
        _ => None,
    }
}

/// A round colour button: filled when it's the item's colour, a ring otherwise
fn dot_style(c: Color, on: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hover = matches!(status, button::Status::Hovered);
        button::Style {
            background: Some(
                if on {
                    c
                } else if hover {
                    Color { a: 0.25, ..c }
                } else {
                    Color::TRANSPARENT
                }
                .into(),
            ),
            border: Border {
                color: c,
                width: 2.0,
                radius: 10.0.into(),
            },
            ..Default::default()
        }
    }
}

/// A sub-item chip: tinted with its colour, solid when picked
fn chip_style(c: Option<Color>, on: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let base = c.unwrap_or(style::colors(theme).accent);
        let hover = matches!(status, button::Status::Hovered);
        button::Style {
            background: Some(
                Color {
                    a: if on {
                        0.9
                    } else if hover {
                        0.18
                    } else {
                        0.08
                    },
                    ..base
                }
                .into(),
            ),
            text_color: if on { Color::WHITE } else { theme.palette().text },
            border: Border {
                color: Color { a: 0.5, ..base },
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn section(xv: &XtimeView) -> Element<'_, Msg> {
    let mut head = Row::new()
        .spacing(10)
        .align_y(Alignment::Center)
        .push(text("Inspection Form").size(16).font(style::BOLD));
    let Some(form) = &xv.form else {
        let body: Element<'_, Msg> = match (&xv.form_error, xv.form_loading) {
            (_, true) | (None, false) => text("Reading the inspection form from Xtime…").style(style::muted).into(),
            (Some(e), false) => text(e.text()).style(style::muted).into(),
        };
        return column![head, body].spacing(8).into();
    };

    // the store's forms: switch the RO to another
    if form.forms.len() > 1 {
        let current = form.forms.iter().find(|f| f.id == form.id).cloned();
        let picked = xv.switch_to.clone().or(current.clone());
        head = head.push(
            pick_list(form.forms.as_slice(), picked, Msg::PickForm)
                .padding([4, 8])
                .text_size(13),
        );
        if xv.switch_to.is_some() && xv.switch_to != current {
            head = head.push(
                button(text(if xv.switching { "Switching…" } else { "Switch form" }).size(13))
                    .padding([4, 10])
                    .style(button::primary)
                    .on_press_maybe((!xv.switching).then_some(Msg::SwitchForm)),
            );
        }
    } else if !form.name.is_empty() {
        head = head.push(text(form.name.as_str()).size(13).style(style::muted));
    }

    let mut list = column![head, summary(form)].spacing(10);
    if let Some(e) = &xv.form_error {
        list = list.push(
            text(format!("Couldn't refresh: {}", e.text().trim_start_matches("Xtime: ")))
                .size(12)
                .style(style::muted),
        );
    }
    for c in &form.categories {
        let mut cat = column![text(c.name.as_str()).size(13).font(style::BOLD).style(style::muted)].spacing(4);
        for item in &c.items {
            cat = cat.push(item_row(xv, item));
        }
        list = list.push(container(cat).padding(12).width(Length::Fill).style(style::card));
    }
    list.into()
}

/// How many Red and Yellow, and the required items still empty
fn summary(form: &InspectionForm) -> Element<'_, Msg> {
    let (red, yellow, missing) = form.counts();
    let mut r = Row::new().spacing(8).align_y(Alignment::Center);
    if red > 0 {
        r = r.push(pill(format!("{red} red"), [0xb4, 0x23, 0x18]));
    }
    if yellow > 0 {
        r = r.push(pill(format!("{yellow} yellow"), [0xd9, 0x9a, 0x06]));
    }
    r.push(
        text(match missing {
            0 => "All required items filled in".to_string(),
            1 => "1 required item still empty".to_string(),
            n => format!("{n} required items still empty"),
        })
        .size(13)
        .style(style::muted),
    )
    .into()
}

fn item_row<'a>(xv: &'a XtimeView, item: &'a Item) -> Element<'a, Msg> {
    let key = item.key();
    let saving = xv.saving.contains(&key);
    let press = |p: Press| (!saving).then(|| Msg::PressItem(key.clone(), p));

    // Green / Yellow / Red
    let dots = row(["Green", "Yellow", "Red"].map(|c| {
        button(Space::new(16, 16))
            .padding(0)
            .style(dot_style(color_of(c).unwrap_or(GREEN), item.color == c))
            .on_press_maybe(press(Press::Color(c.to_string())))
            .into()
    }))
    .spacing(6);

    let mut name = Row::new()
        .spacing(4)
        .align_y(Alignment::Center)
        .push(text(item.name.as_str()).size(14));
    if item.required {
        name = name.push(text("*").size(14).style(text::danger));
    }
    // Add Service for this item: when it's Yellow or Red and Xtime offers services for it
    if (item.color == "Yellow" || item.color == "Red") && !item.services.is_empty() {
        name = name.push(
            button(text("Add Service").size(12))
                .padding([2, 10])
                .style(style::ghost)
                .on_press(Msg::Ask(Ask::AddService(Some(key.clone())))),
        );
    }
    if saving {
        name = name.push(text("Saving…").size(12).style(style::muted));
    }

    // sub-items (any colour, then those that make it Red / Yellow / Green), and the value box
    let picked = item.picked();
    let mut vals = Row::new().spacing(6).align_y(Alignment::Center);
    if item.show_value {
        let typed = xv.drafts.get(&key).cloned().unwrap_or_else(|| item.value.clone());
        let changed = xv.drafts.get(&key).is_some_and(|d| *d != item.value);
        let mut input = text_input(if item.required { "Required" } else { "Value" }, &typed)
            .size(13)
            .padding([3, 8])
            .width(180);
        if !saving {
            input = input
                .on_input({
                    let key = key.clone();
                    move |t| Msg::ItemDraft(key.clone(), t)
                })
                .on_submit(Msg::PressItem(key.clone(), Press::Value(typed.clone())));
        }
        vals = vals.push(input);
        if changed {
            vals = vals.push(text("Enter to save").size(11).style(style::muted));
        }
    }
    let subs = item
        .sub_items
        .iter()
        .map(|s| (s, item.color.clone(), None))
        .chain(item.sub_red.iter().map(|s| (s, "Red".to_string(), Some(RED))))
        .chain(item.sub_yellow.iter().map(|s| (s, "Yellow".to_string(), Some(YELLOW))))
        .chain(item.sub_green.iter().map(|s| (s, "Green".to_string(), Some(GREEN))));
    for (label, sets, tint) in subs {
        let on = picked.iter().any(|p| p == label);
        vals = vals.push(
            button(text(label.as_str()).size(12))
                .padding([2, 10])
                .style(chip_style(tint, on))
                .on_press_maybe(press(Press::Sub(label.clone(), sets))),
        );
    }
    if item.show_value || !item.value.is_empty() {
        vals = vals.push(
            button(text("Clear").size(12))
                .padding([2, 10])
                .style(style::ghost)
                .on_press_maybe(press(Press::ClearValue)),
        );
    }

    let main: Column<'a, Msg> = column![name, vals.wrap()].spacing(4);
    let edge = color_of(&item.color).filter(|_| item.answered);
    container(row![dots, main, horizontal_space()].spacing(14).align_y(Alignment::Start))
        .padding([8, 10])
        .width(Length::Fill)
        .style(move |theme: &Theme| container::Style {
            background: edge.map(|c| Color { a: 0.06, ..c }.into()),
            border: Border {
                color: edge.unwrap_or(style::colors(theme).line),
                width: if edge.is_some() { 1.0 } else { 0.0 },
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}
