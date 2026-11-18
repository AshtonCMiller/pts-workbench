//! The Workbench: the vehicle picked in the sidebar, with its tabs (Xtime View, Service Manual) and the Service Manual's
//! own tabs (OASIS, Connected Vehicle, Diagnostics…). The Xtime View is crates/xtime-ui's, the Service Manual's tabs
//! crates/ford-ui's (OASIS so far; the others say what they'll show until they come).

use iced::widget::{Row, button, column, container, horizontal_space, row, text};
// scrolling that glides (crates/ui-kit)
use iced::{Alignment, Element, Length};
use ui_kit::scroll::scrollable;

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

    // the tab whose content is showing: while one leaves, still the old one (its own row of tabs goes and comes with
    // it, so a page never shows under the other tab's row)
    let (tab, manual_tab) = match app.inner.shown {
        InnerKey::Vehicle(t, m) => (t, m),
        _ => (ws.tab, ws.manual),
    };
    let mut page = column![container(tabs.padding([6, 10])).width(Length::Fill).style(style::panel)];
    if tab == Tab::ServiceManual {
        // the Service Manual's own tabs, then a tab for each document opened (an FSA…), with its × to close it
        let mut items: Vec<Element<'_, Message>> = ManualTab::ALL
            .iter()
            .map(|&t| {
                button(text(t.label()).size(13))
                    .padding([4, 10])
                    .style(style::tab(ws.manual == t))
                    .on_press(Message::SetManualTab(t))
                    .into()
            })
            .collect();
        for d in &ws.docs {
            let t = ManualTab::Doc(d.key);
            items.push(
                row![
                    button(text(d.label()).size(13))
                        .padding([4, 4])
                        .style(style::tab(ws.manual == t))
                        .on_press(Message::SetManualTab(t)),
                    button(text("×").size(13))
                        .padding([4, 6])
                        .style(style::tab(ws.manual == t))
                        .on_press(Message::CloseDoc(d.key)),
                ]
                .align_y(Alignment::Center)
                .into(),
            );
        }
        let active = ManualTab::ALL
            .iter()
            .position(|t| *t == ws.manual)
            .or_else(|| {
                ws.docs
                    .iter()
                    .position(|d| ManualTab::Doc(d.key) == ws.manual)
                    .map(|i| ManualTab::ALL.len() + i)
            })
            .unwrap_or(0);
        let sub = anim::tabs(active, items).spacing(2.0);
        page = page.push(container(container(sub).padding([4, 10])).width(Length::Fill).style(style::panel));
    }
    // the Xtime View scrolls as one page; each Service Manual tab scrolls its own parts (a list under its filters, a
    // document sliding in over it…)
    let content: Element<'_, Message> = match tab {
        Tab::XtimeView => {
            // crates/xtime-ui: its own messages, for this RO
            let ro = ws.ro.ro.clone();
            let shown = app.xtime_shown(ws);
            let xv = xtime_ui::view::view(&ws.xv, &shown).map(move |m| Message::Xv(ro.clone(), m));
            scrollable(container(xv).padding(16).width(Length::Fill))
                .height(Length::Fill)
                .into()
        }
        Tab::ServiceManual => container(manual(ws, manual_tab)).width(Length::Fill).height(Length::Fill).into(),
        // the websites themselves (crates/webpane: a web view over this room)
        Tab::XtimeSite | Tab::PtsSite => site(app, tab, &ws.ro.ro, ws.is_sample()),
    };
    page.push(anim::page(content, app.inner.phase)).into()
}

/// Xtime's own RO page or PTS: a bar to reload it, then the room the page shows in
fn site<'a>(app: &'a App, tab: Tab, ro: &str, sample: bool) -> Element<'a, Message> {
    let what = if tab == Tab::PtsSite {
        "PTS, opened on this vehicle"
    } else {
        "Xtime's own RO page"
    };
    let bar = row![
        text(what).size(12).style(style::muted),
        horizontal_space(),
        button(text("Reload").size(12))
            .padding([3, 10])
            .style(style::ghost)
            .on_press(Message::ReloadPane),
    ]
    .align_y(Alignment::Center)
    .padding([4, 10]);
    column![bar, site_room(app, crate::app::pane_id(tab, ro), sample)].into()
}

/// The room a website shows in (what's drawn there: until it's over it, or why it can't be)
pub fn site_room<'a>(app: &'a App, id: String, sample: bool) -> Element<'a, Message> {
    let note = if sample {
        "The sample ROs aren't real: connect Xtime to see its pages.".to_string()
    } else {
        app.pane_error.clone().unwrap_or_else(|| "Opening…".into())
    };
    webpane::hole(
        id,
        container(text(note).style(style::muted))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        Message::PanePlace,
    )
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

fn manual<'a>(ws: &'a Workspace, tab: ManualTab) -> Element<'a, Message> {
    let ro = ws.ro.ro.clone();
    match tab {
        // crates/ford-ui: its own messages, for this RO
        ManualTab::Oasis => {
            let today = chrono::Local::now().date_naive();
            ford_ui::oasis::view(&ws.oasis, ws.is_sample(), today).map(move |m| Message::Oasis(ro.clone(), m))
        }
        ManualTab::Tsbs => ford_ui::tsb::view(&ws.tsb, ws.is_sample()).map(move |m| Message::Tsb(ro.clone(), m)),
        ManualTab::Workshop => ford_ui::manual::view(&ws.wsm, ws.is_sample()).map(move |m| Message::Wsm(ro.clone(), m)),
        ManualTab::Diagnostics => ford_ui::diagnostics::view(&ws.diag, ws.is_sample()).map(move |m| Message::Diag(ro.clone(), m)),
        ManualTab::Labor => ford_ui::labor::view(&ws.labor, ws.is_sample()).map(move |m| Message::Labor(ro.clone(), m)),
        ManualTab::Wiring => ford_ui::wiring::view(&ws.wiring, ws.is_sample()).map(move |m| Message::Wiring(ro.clone(), m)),
        ManualTab::Doc(key) => match ws.docs.iter().find(|d| d.key == key) {
            Some(d) => ford_ui::doc_tab::view(d).map(move |m| Message::Doc(ro.clone(), key, m)),
            None => text("This tab was closed.").into(),
        },
        _ => scrollable(
            container(column![vehicle_card(ws), coming(tab.label(), tab.about(), "Comes in a later stage.")].spacing(12))
                .padding(16)
                .width(Length::Fill),
        )
        .height(Length::Fill)
        .into(),
    }
}
