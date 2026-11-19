//! The Workbench: the vehicle picked in the sidebar, with its tabs (Xtime View, Service Manual) and the Service Manual's
//! own tabs (OASIS, Connected Vehicle, Diagnostics…). The Xtime View is crates/xtime-ui's, the Service Manual's tabs
//! crates/ford-ui's. Diagnostics drops down a menu when hovered: Diagnostics Home, Vehicle History (PTS's own page,
//! inside the app) and Vehicle Software (still to come).

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
    // (a Quick Lane profile's start with Maintenance)
    let vehicle_tabs = app.vehicle_tabs();
    let main_tabs = anim::tabs(
        vehicle_tabs.iter().position(|t| *t == ws.tab).unwrap_or(0),
        vehicle_tabs
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
        // (a Quick Lane profile's: no wiring)
        let manual_tabs = app.manual_tabs();
        let mut items: Vec<Element<'_, Message>> = manual_tabs
            .iter()
            .map(|&t| {
                if t == ManualTab::Diagnostics {
                    return diag_tab(app, ws);
                }
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
        // (Vehicle History and Vehicle Software are under Diagnostics)
        let active = manual_tabs
            .iter()
            .position(|t| *t == ws.manual || (t.is_diag() && ws.manual.is_diag()))
            .or_else(|| {
                ws.docs
                    .iter()
                    .position(|d| ManualTab::Doc(d.key) == ws.manual)
                    .map(|i| manual_tabs.len() + i)
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
        Tab::ServiceManual => container(manual(app, ws, manual_tab))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        // a Quick Lane's: the specs a quick service needs
        Tab::Maintenance => maintenance(ws),
        // the websites themselves (crates/webpane: a web view over this room)
        Tab::XtimeSite | Tab::PtsSite => site(app, tab, &ws.ro.ro, ws.is_sample()),
    };
    page.push(anim::page(content, app.inner.phase)).into()
}

/// A Quick Lane's Maintenance tab: the specs read from the vehicle's Workshop Manual (ford::specs), each page's rows
/// in a card that opens the whole page
fn maintenance(ws: &Workspace) -> Element<'_, Message> {
    use crate::workspace::Maint;
    let ro = ws.ro.ro.clone();
    let head = column![
        text("Maintenance").size(20).font(style::BOLD),
        text(format!("{}  ·  {}", ws.vehicle(), ws.ro.vin)).style(style::muted),
    ]
    .spacing(2);
    let mut page = column![head].spacing(14);
    match &ws.maint {
        _ if ws.is_sample() => {
            page = page.push(text("The sample ROs aren't real vehicles: connect Xtime to see a vehicle's specs.").style(style::muted));
        }
        Maint::NotRead | Maint::Reading => {
            page = page.push(
                row![
                    anim::dot(iced::Color::from_rgb8(0x1a, 0x5f, 0xb4), true),
                    text("Reading the specs from the Workshop Manual…")
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        Maint::Failed(e) => {
            page = page.push(
                row![
                    text(format!("Couldn't read the specs ({e}).")).width(Length::Fill),
                    button(text("Try again"))
                        .padding([5, 12])
                        .style(style::ghost)
                        .on_press(Message::MaintRetry(ro.clone())),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        Maint::Read(groups) if groups.is_empty() => {
            page = page.push(
                text("The Workshop Manual's Specifications pages didn't have the usual rows for this vehicle. Its pages are in the Service Manual tab.")
                    .style(style::muted),
            );
        }
        Maint::Read(groups) => {
            for g in groups {
                let mut rows = column![].spacing(0);
                for (i, (label, value)) in g.lines.iter().enumerate() {
                    rows = rows.push(
                        container(
                            row![
                                text(label.as_str()).size(13).width(Length::FillPortion(3)),
                                text(value.as_str()).size(14).font(style::BOLD).width(Length::FillPortion(2)),
                            ]
                            .spacing(12)
                            .align_y(Alignment::Center),
                        )
                        .padding([7, 10])
                        .width(Length::Fill)
                        .style(move |t: &iced::Theme| container::Style {
                            background: (i % 2 == 1).then(|| style::colors(t).hover.into()),
                            ..Default::default()
                        }),
                    );
                }
                // where it's from: a Workshop Manual page (to open), or PTS's Specification Sheet
                let mut head = row![
                    column![
                        text(g.name.as_str()).size(16).font(style::BOLD),
                        text(match &g.page {
                            Some((section, ..)) => section.clone(),
                            None => "Specification Sheet".to_string(),
                        })
                        .size(11)
                        .style(style::muted)
                    ]
                    .spacing(1),
                    horizontal_space(),
                ]
                .align_y(Alignment::Center);
                if let Some((section, title, link)) = &g.page {
                    head = head.push(
                        button(text("Open the page").size(12))
                            .padding([4, 10])
                            .style(style::ghost)
                            .on_press(Message::OpenSpec(ro.clone(), link.clone(), format!("{section} · {title}"))),
                    );
                }
                page = page.push(
                    container(column![head, rows,].spacing(8))
                        .padding(14)
                        .width(Length::Fill)
                        .style(style::card),
                );
            }
            page = page.push(
                text(
                    "From PTS's Specification Sheet and the Workshop Manual's Specifications pages: check the page itself (Open the page) when a value matters.",
                )
                .size(12)
                .style(style::muted),
            );
        }
    }
    scrollable(container(container(page).max_width(900)).padding(20).center_x(Length::Fill))
        .height(Length::Fill)
        .into()
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
    // (a card over it: the page's picture, taken as it was covered, so it still looks to be there)
    let content: Element<'a, Message> = match &app.pane_picture {
        Some((pid, picture)) if *pid == id && !sample => iced::widget::image(picture.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(iced::ContentFit::Fill)
            .into(),
        _ => container(text(note).style(style::muted))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into(),
    };
    webpane::hole(id, content, Message::PanePlace)
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

/// The Service Manual's Diagnostics tab: hovering it drops down its menu (Diagnostics Home, which is the tab itself,
/// Vehicle History and Vehicle Software)
fn diag_tab<'a>(app: &App, ws: &'a Workspace) -> Element<'a, Message> {
    let on = ws.manual.is_diag();
    // (which one's showing, when it isn't the search)
    let label = match ws.manual {
        ManualTab::History | ManualTab::Software => format!("Diagnostics · {}", ws.manual.label()),
        _ => "Diagnostics".to_string(),
    };
    let anchor = button(row![text(label).size(13), text("▾").size(10)].spacing(5).align_y(Alignment::Center))
        .padding([4, 10])
        .style(style::tab(on))
        .on_press(Message::SetManualTab(ManualTab::Diagnostics));
    let item = |t: ManualTab, title: &'static str, about: &'static str| -> Element<'static, Message> {
        button(column![text(title).size(13).font(style::BOLD), text(about).size(11).style(style::muted)].spacing(1))
            .width(Length::Fill)
            .padding([6, 10])
            .style(style::list_item(ws.manual == t))
            .on_press(Message::SetManualTab(t))
            .into()
    };
    let menu = container(
        column![
            item(ManualTab::Diagnostics, "Diagnostics Home", "Search DTCs and symptoms"),
            item(ManualTab::History, "Vehicle History", "Every PTS and FDRS session on this VIN"),
            item(ManualTab::Software, "Vehicle Software", "The modules' software"),
        ]
        .spacing(2),
    )
    .padding(6)
    .width(260)
    .style(style::floating_card);
    // (a little room above it, so it doesn't sit right on the tab)
    let menu = container(menu).padding(iced::Padding {
        top: 4.0,
        ..iced::Padding::ZERO
    });
    // (over Vehicle History: it waits until PTS's page has been swapped for its picture, else the page would cover it)
    ui_kit::menu::hover_menu(anchor, menu)
        .on_open(Message::DiagMenu)
        .ready(app.page_clear())
        .into()
}

/// Vehicle History: PTS's own page (every PTS and FDRS session recorded for the VIN), inside the app
fn history<'a>(app: &'a App, ws: &'a Workspace) -> Element<'a, Message> {
    let bar = row![
        text("Vehicle History · every PTS and FDRS session recorded for this VIN, from PTS")
            .size(12)
            .style(style::muted),
        horizontal_space(),
        button(text("Reload").size(12))
            .padding([3, 10])
            .style(style::ghost)
            .on_press(Message::ReloadPane),
    ]
    .align_y(Alignment::Center)
    .padding([4, 10]);
    column![bar, site_room(app, crate::app::history_pane_id(&ws.ro.ro), ws.is_sample())].into()
}

fn manual<'a>(app: &'a App, ws: &'a Workspace, tab: ManualTab) -> Element<'a, Message> {
    let ro = ws.ro.ro.clone();
    match tab {
        // crates/ford-ui: its own messages, for this RO
        ManualTab::Oasis => {
            let today = chrono::Local::now().date_naive();
            ford_ui::oasis::view(&ws.oasis, &ws.cv, ws.is_sample(), today).map(move |m| Message::Oasis(ro.clone(), m))
        }
        ManualTab::ConnectedVehicle => ford_ui::connected::view(&ws.cv, ws.is_sample()).map(move |m| Message::Cv(ro.clone(), m)),
        ManualTab::History => history(app, ws),
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
