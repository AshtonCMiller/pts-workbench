//! The Workbench: the vehicle picked in the sidebar, with its tabs (Xtime View, Service Manual) and the Service Manual's
//! own tabs (OASIS, Connected Vehicle, Diagnostics…). Until Xtime and Ford are connected, each tab says what it'll show.

use iced::widget::{Row, button, column, container, horizontal_space, row, scrollable, text};
use iced::{Alignment, Element, Length};

use super::{anim, pill, style};
use crate::app::{App, InnerKey, Message};
use crate::workspace::{ManualTab, Tab, Workspace, mode_label};

pub fn view(app: &App) -> Element<'_, Message> {
    let Some(ws) = app.shown_workspace() else {
        return container(
            column![
                text("No vehicle open").size(18).font(style::BOLD),
                text("Pick one of your ROs in the sidebar, or from your Xtime dashboard.").style(style::muted),
            ]
            .spacing(6)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into();
    };

    // the tab strips show the vehicle's tabs as they are now (their highlight slides right away)
    let main_tabs = anim::tabs(
        Tab::ALL.iter().position(|t| *t == ws.tab).unwrap_or(0),
        Tab::ALL
            .iter()
            .map(|&t| {
                button(text(t.label()))
                    .padding([6, 12])
                    .style(style::tab(ws.tab == t))
                    .on_press(Message::SetTab(t))
                    .into()
            })
            .collect(),
    );
    let tabs = Row::new()
        .spacing(4)
        .align_y(Alignment::Center)
        .push(main_tabs)
        .push(horizontal_space())
        .push(text(format!("RO {} · {}", ws.ro.ro, ws.vehicle())).style(style::muted))
        .push(
            button(text("Copy VIN"))
                .padding([5, 10])
                .style(style::ghost)
                .on_press(Message::CopyVin),
        );

    let mut page = column![container(tabs.padding([6, 10])).width(Length::Fill).style(style::panel)];
    if ws.tab == Tab::ServiceManual {
        let sub = anim::tabs(
            ManualTab::ALL.iter().position(|t| *t == ws.manual).unwrap_or(0),
            ManualTab::ALL
                .iter()
                .map(|&t| {
                    button(text(t.label()).size(13))
                        .padding([4, 10])
                        .style(style::tab(ws.manual == t))
                        .on_press(Message::SetManualTab(t))
                        .into()
                })
                .collect(),
        )
        .spacing(2.0);
        page = page.push(container(container(sub).padding([4, 10])).width(Length::Fill).style(style::panel));
    }
    // the content of the tab shown (while one leaves, still the old one)
    let (tab, manual_tab) = match app.inner.shown {
        InnerKey::Vehicle(t, m) => (t, m),
        _ => (ws.tab, ws.manual),
    };
    let content = match tab {
        Tab::XtimeView => xtime_view(ws),
        Tab::ServiceManual => manual(ws, manual_tab),
    };
    page.push(anim::page(
        scrollable(container(content).padding(16).width(Length::Fill)).height(Length::Fill),
        app.inner.phase,
    ))
    .into()
}

/// One label / value line of a card
fn fact<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    row![
        text(label).width(140).style(style::muted),
        text(if value.is_empty() { "—".to_string() } else { value })
    ]
    .spacing(10)
    .into()
}

fn vehicle_card(ws: &Workspace) -> Element<'_, Message> {
    let (mode, color) = mode_label(ws.ro.mode);
    container(
        column![
            row![
                text(ws.vehicle()).size(18).font(style::BOLD),
                if mode.is_empty() { text("").into() } else { pill(mode, color) }
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            fact("RO", ws.ro.ro.clone()),
            fact("Tag", ws.ro.tag.clone()),
            fact("VIN", ws.ro.vin.clone()),
        ]
        .spacing(6),
    )
    .padding(14)
    .width(Length::Fill)
    .style(style::card)
    .into()
}

fn coming<'a>(title: &'a str, about: &'a str, needs: &'a str) -> Element<'a, Message> {
    container(
        column![
            text(title).size(16).font(style::BOLD),
            text(about),
            text(needs).size(13).style(style::muted)
        ]
        .spacing(6),
    )
    .padding(14)
    .width(Length::Fill)
    .style(style::card)
    .into()
}

fn xtime_view(ws: &Workspace) -> Element<'_, Message> {
    let waiting = [
        (ws.ro.waiting_parts, "parts"),
        (ws.ro.waiting_customer, "the customer"),
        (ws.ro.waiting_warranty, "warranty"),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .map(|(_, w)| *w)
    .collect::<Vec<_>>()
    .join(", ");
    column![
        vehicle_card(ws),
        container(
            column![
                text("Status").size(16).font(style::BOLD),
                fact("Waiting on", waiting),
                fact("Advisor", ws.ro.dash.advisor_name.clone())
            ]
            .spacing(6)
        )
        .padding(14)
        .width(Length::Fill)
        .style(style::card),
        coming(
            "Lines",
            "The RO's lines: concerns, corrections, technician notes; adding and editing lines; marking them done.",
            "Comes when Xtime is connected."
        ),
    ]
    .spacing(12)
    .into()
}

fn manual(ws: &Workspace, tab: ManualTab) -> Element<'_, Message> {
    column![
        vehicle_card(ws),
        coming(tab.label(), tab.about(), "Comes when Ford PTS is connected.")
    ]
    .spacing(12)
    .into()
}
