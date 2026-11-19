//! What Diagnostics looks like (lib.rs has the steps), in tabs:
//!
//!   Vehicle ID   the VCI (connect), the vehicle (its VIN, and your RO for it), the communication log
//!   Dashboard    once it's identified: its modules by network (a list, or the network's layout) on the left, the
//!                applications on the right
//!   Self Test    an application, in its own tab (every module, or one): its trouble codes, and clearing them

use iced::widget::{Column, Row, Space, button, column, container, horizontal_space, pick_list, progress_bar, row, text, text_input};
use iced::{Alignment, Color, Element, Length};
use ui_kit::scroll::scrollable;
use vci::modules::Bus;
use vci::scan::{Module, Status};

use ui_kit::{anim, pill, style};

use crate::{Diag, DiagMsg, LOW_MV, Link, Lookup, SelfTestApp, Tab, dtc_key, tag};

const GREEN: [u8; 3] = [0x2e, 0xa0, 0x43];
const AMBER: [u8; 3] = [0xd2, 0x99, 0x22];
const RED: [u8; 3] = [0xcf, 0x22, 0x2e];
const GREY: [u8; 3] = [0x5d, 0x66, 0x70];

/// The networks, in the order they're shown
const NETWORKS: [Bus; 5] = [Bus::Hs, Bus::Hs2, Bus::Ms, Bus::Hs2b, Bus::Hs3];

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgb8(c[0], c[1], c[2])
}

/// (the screen's messages are Diagnostics' own: the app passes them on)
type Message = DiagMsg;

fn msg(m: DiagMsg) -> Message {
    m
}

/// Your RO for the vehicle plugged in (the app finds it: one of yours, or one open in the Workbench)
#[derive(Debug, Clone)]
pub struct RoInfo {
    pub ro: String,
    pub tag: String,
    /// "2017 Ford F-150"
    pub vehicle: String,
}

pub(crate) fn card<'a>(
    title: impl iced::widget::text::IntoFragment<'a>,
    right: Element<'a, Message>,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    container(
        column![
            row![text(title).size(16).font(style::BOLD), horizontal_space(), right]
                .spacing(8)
                .align_y(Alignment::Center),
            body
        ]
        .spacing(12),
    )
    .padding(18)
    .width(Length::Fill)
    .style(style::card)
    .into()
}

pub(crate) fn primary<'a>(label: &'a str, on: Option<DiagMsg>) -> Element<'a, Message> {
    button(text(label).font(style::BOLD))
        .padding([7, 16])
        .style(button::primary)
        .on_press_maybe(on.map(msg))
        .into()
}

pub(crate) fn ghost<'a>(label: &'a str, on: Option<DiagMsg>) -> Element<'a, Message> {
    button(text(label))
        .padding([6, 12])
        .style(style::ghost)
        .on_press_maybe(on.map(msg))
        .into()
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// A thin line (the network layout's wires, a list's dividers)
fn wire<'a>(width: impl Into<Length>, height: impl Into<Length>) -> Element<'a, Message> {
    container(Space::new(width, height))
        .style(|t: &iced::Theme| container::Style {
            background: Some(style::colors(t).line.into()),
            ..Default::default()
        })
        .into()
}

/// Diagnostics, and your RO for the vehicle (when it's one of yours)
pub fn view(d: &Diag, ro: Option<RoInfo>) -> Element<'_, Message> {
    let mut page = Column::new().height(Length::Fill);
    page = page.push(container(tab_strip(d).padding([6, 10])).width(Length::Fill).style(style::panel));
    if let Some(e) = &d.error {
        page = page.push(container(error_banner(e)).padding([12, 24]).center_x(Length::Fill));
    }
    let content: Element<'_, Message> = match d.tab {
        Tab::Id => id_page(d, ro),
        Tab::Dashboard => dashboard(d),
        Tab::App(id) => match (d.app(id), d.live(id)) {
            (Some(a), _) => selftest_page(d, a),
            (_, Some(l)) => crate::live::page(d, l),
            _ => id_page(d, ro),
        },
    };
    page.push(anim::page(content, anim::Phase::Entering(d.tab_since))).into()
}

/// A page that scrolls, its cards centred
pub(crate) fn scrolling<'a>(body: impl Into<Element<'a, Message>>, max_width: f32) -> Element<'a, Message> {
    container(scrollable(
        container(container(body).max_width(max_width)).padding(24).center_x(Length::Fill),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn error_banner(e: &str) -> Element<'_, Message> {
    container(
        row![text(e).size(13).width(Length::Fill), ghost("Dismiss", Some(DiagMsg::DismissError))]
            .spacing(10)
            .align_y(Alignment::Center),
    )
    .padding([10, 14])
    .max_width(1100.0)
    .width(Length::Fill)
    .style(|_: &iced::Theme| {
        let red = rgb(RED);
        container::Style {
            text_color: Some(red),
            background: Some(Color { a: 0.10, ..red }.into()),
            border: iced::Border {
                color: Color { a: 0.45, ..red },
                width: 1.0,
                radius: 10.0.into(),
            },
            ..Default::default()
        }
    })
    .into()
}

// ---------- the tabs ----------

fn tab_button<'a>(label: &'a str, t: Tab, on: Tab) -> Element<'a, Message> {
    button(text(label))
        .padding([6, 12])
        .style(style::tab(on == t))
        .on_press(msg(DiagMsg::SetTab(t)))
        .into()
}

fn tab_strip(d: &Diag) -> Row<'_, Message> {
    let mut items: Vec<Element<'_, Message>> = Vec::new();
    let mut tabs: Vec<Tab> = Vec::new();
    items.push(tab_button("Vehicle ID", Tab::Id, d.tab));
    tabs.push(Tab::Id);
    if d.dashboard_shown() {
        items.push(tab_button("Dashboard", Tab::Dashboard, d.tab));
        tabs.push(Tab::Dashboard);
    }
    for (id, title, working) in d.app_tabs() {
        let t = Tab::App(id);
        let mut label = row![].spacing(6).align_y(Alignment::Center);
        if working {
            label = label.push(anim::dot(rgb(AMBER), true));
        }
        label = label.push(text(title));
        items.push(
            row![
                button(label)
                    .padding([6, 4])
                    .style(style::tab(d.tab == t))
                    .on_press(msg(DiagMsg::SetTab(t))),
                button(text("×"))
                    .padding([6, 8])
                    .style(style::tab(d.tab == t))
                    .on_press(msg(DiagMsg::CloseApp(id))),
            ]
            .align_y(Alignment::Center)
            .into(),
        );
        tabs.push(t);
    }
    let active = tabs.iter().position(|t| *t == d.tab).unwrap_or(0);

    // on the right: the connection, at a glance
    let (color, label) = match &d.link {
        Link::Connected { .. } => (GREEN, "Connected"),
        Link::Connecting => (AMBER, "Connecting…"),
        Link::Closed => (GREY, "Not connected"),
    };
    let mut status = row![anim::dot(rgb(color), d.busy()), text(label).size(13)]
        .spacing(8)
        .align_y(Alignment::Center);
    if let Some(mv) = d.millivolts.filter(|_| d.connected()) {
        status = status.push(pill(
            format!("{:.1} V", f64::from(mv) / 1000.0),
            if mv < LOW_MV { AMBER } else { GREEN },
        ));
    }
    if let Some(vin) = &d.vin {
        status = status.push(text(vin.as_str()).size(12).font(style::MONO).style(style::muted));
    }
    Row::new()
        .spacing(8)
        .align_y(Alignment::Center)
        .push(anim::tabs(active, items))
        .push(horizontal_space())
        .push(status)
}

// ---------- Vehicle ID ----------

fn id_page(d: &Diag, ro: Option<RoInfo>) -> Element<'_, Message> {
    let mut page = Column::new().spacing(16);
    page = page.push(vci_card(d));
    if d.connected() || d.vin.is_some() {
        page = page.push(vehicle_card(d, ro));
    }
    page = page.push(log_card(d));
    scrolling(page, 880.0)
}

// ---------- the VCI ----------

fn vci_card(d: &Diag) -> Element<'_, Message> {
    let (color, label) = match &d.link {
        Link::Connected { .. } => (GREEN, "Connected"),
        Link::Connecting => (AMBER, "Connecting…"),
        Link::Closed if d.looking => (AMBER, "Looking for the VCI…"),
        Link::Closed => (GREY, "Not connected"),
    };
    let status = row![
        anim::dot(
            Color::from_rgb8(color[0], color[1], color[2]),
            (d.busy() && !d.connected()) || d.scanning
        ),
        text(label).size(13)
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let body: Element<'_, Message> = match &d.devices {
        None => text("Looking for Ford's VCI…").style(style::muted).into(),
        Some(list) if list.is_empty() => column![
            text("No VCI found on this computer.").font(style::BOLD),
            text(
                "Diagnostics uses the VCM 3 (or VCMM) through Ford's VCI Software, the same as FDRS. If FDRS works on \
                 this laptop, plug the VCI in and look again."
            )
            .size(13)
            .style(style::muted),
            row![ghost("Look again", (!d.busy()).then_some(DiagMsg::Look))],
        ]
        .spacing(8)
        .into(),
        Some(list) => {
            let names: Vec<String> = list.iter().map(|x| x.name.clone()).collect();
            let chosen = d.device().map(|x| x.name.clone());
            let free = !d.busy() && !d.connected();
            let pick = pick_list(names, chosen, |n| msg(DiagMsg::Pick(n))).padding([6, 10]).width(170);
            let action = if d.connected() {
                ghost("Disconnect", Some(DiagMsg::Disconnect))
            } else {
                primary(
                    if d.link == Link::Connecting { "Connecting…" } else { "Connect" },
                    free.then_some(DiagMsg::Connect),
                )
            };
            let mut c = column![
                row![
                    text("VCI").size(13).style(style::muted),
                    pick,
                    action,
                    horizontal_space(),
                    ghost("Look again", free.then_some(DiagMsg::Look)),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
            ]
            .spacing(10);
            match &d.link {
                Link::Connected { firmware, api } => {
                    let mut facts = row![].spacing(8).align_y(Alignment::Center);
                    if let Some(mv) = d.millivolts {
                        let v = format!("Battery {:.1} V", f64::from(mv) / 1000.0);
                        facts = facts.push(pill(v, if mv < LOW_MV { AMBER } else { GREEN }));
                    }
                    if !firmware.is_empty() {
                        facts = facts.push(text(format!("Firmware {firmware}")).size(12).style(style::muted));
                    }
                    if !api.is_empty() {
                        facts = facts.push(text(format!("J2534 {api}")).size(12).style(style::muted));
                    }
                    c = c.push(facts);
                    if d.millivolts.is_some_and(|mv| mv < LOW_MV) {
                        c = c.push(text("The battery's low: put a charger on it before testing (modules can drop out).").size(13));
                    }
                }
                Link::Connecting => {
                    c = c.push(
                        text("Finding the VCI (USB or Wi-Fi) and joining the vehicle's network…")
                            .size(13)
                            .style(style::muted),
                    )
                }
                Link::Closed => {
                    c = c.push(
                        text("Plug the VCI into the vehicle's OBD port and this laptop, ignition on, then Connect.")
                            .size(13)
                            .style(style::muted),
                    )
                }
            }
            c.into()
        }
    };
    card("Vehicle connection", status.into(), body)
}

// ---------- the vehicle ----------

fn vehicle_card(d: &Diag, ro: Option<RoInfo>) -> Element<'_, Message> {
    let idle = d.connected() && !d.busy();
    let mut right = row![ghost("Identify again", idle.then_some(DiagMsg::Identify))]
        .spacing(6)
        .align_y(Alignment::Center);
    if d.vin.is_some() || d.no_vin {
        right = right.push(primary(
            "Dashboard →",
            (d.dashboard_shown() || idle).then_some(DiagMsg::ToDashboard),
        ));
    }
    let body: Element<'_, Message> = if d.identifying {
        text("Asking the PCM for the VIN…").style(style::muted).into()
    } else if let Some(vin) = &d.vin {
        let mut c = column![
            row![text(vin.as_str()).size(22).font(style::MONO), ghost("Copy", Some(DiagMsg::CopyVin)),]
                .spacing(10)
                .align_y(Alignment::Center)
        ]
        .spacing(10);
        // your RO for it (one of yours, or a vehicle open in the Workbench)
        let mine: Element<'_, Message> = match ro {
            Some(r) => {
                let what = r.vehicle;
                let tag = if r.tag.is_empty() {
                    String::new()
                } else {
                    format!("  ·  Tag {}", r.tag)
                };
                container(
                    row![
                        column![
                            text(format!("RO {}{tag}", r.ro)).font(style::BOLD),
                            text(what).size(13).style(style::muted)
                        ]
                        .spacing(2),
                        horizontal_space(),
                        primary("Open RO", Some(DiagMsg::OpenRo(r.ro.clone()))),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([10, 14])
                .style(style::card_highlight)
                .into()
            }
            None => text("Not one of your ROs.").size(13).style(style::muted).into(),
        };
        c = c.push(mine);
        c.into()
    } else if d.no_vin {
        text(
            "The PCM didn't give its VIN. Check the ignition's on, then Identify again; or go on to the Dashboard \
             without it.",
        )
        .size(13)
        .into()
    } else {
        text("Not identified yet.").style(style::muted).into()
    };
    card("Vehicle", right.into(), body)
}

// ---------- the Dashboard ----------
//
// Laid out like FDRS's Toolbox: the network test on the left (each network a box of module tiles, colored by what
// was found), the applications on the right (a list to search; a module picked: just its own).

fn dashboard(d: &Diag) -> Element<'_, Message> {
    scrolling(
        row![
            container(network_card(d)).width(Length::FillPortion(5)),
            container(applications_card(d)).width(Length::FillPortion(6)),
        ]
        .spacing(16),
        1500.0,
    )
}

/// The network as FDRS names it: "HS1", "HS2", "MS1", "HS3"
fn net_name(bus: Bus) -> &'static str {
    match bus {
        Bus::Hs => "HS1",
        Bus::Hs2 | Bus::Hs2b => "HS2",
        Bus::Ms => "MS1",
        Bus::Hs3 => "HS3",
    }
}

/// What a module's tile shows: it answered (and its codes, once a self test's read them)
#[derive(Clone, Copy, PartialEq)]
enum TileState {
    Answered,
    NoCodes,
    Stored,
    Current,
}

impl TileState {
    fn color(self) -> [u8; 3] {
        match self {
            TileState::Answered | TileState::NoCodes => GREEN,
            TileState::Stored => AMBER,
            TileState::Current => RED,
        }
    }
}

fn tile_state(d: &Diag, address: u32, bus: Bus) -> TileState {
    let latest = d
        .apps
        .iter()
        .rev()
        .filter_map(|a| a.test.as_ref())
        .find_map(|t| t.modules.iter().find(|m| m.address == address && m.bus == bus && m.present()));
    match latest.map(|m| m.codes()) {
        None => TileState::Answered,
        Some([]) => TileState::NoCodes,
        Some(c) if c.iter().any(|c| c.current()) => TileState::Current,
        Some(_) => TileState::Stored,
    }
}

/// A module's tile: its border colored by what was found, filled when it's picked
fn tile_style(state: TileState, selected: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |theme, status| {
        let c = style::colors(theme);
        let edge = rgb(state.color());
        let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(
                if selected {
                    c.sel
                } else if hover {
                    c.hover
                } else {
                    c.panel
                }
                .into(),
            ),
            text_color: theme.palette().text,
            border: iced::Border {
                color: if selected { c.accent } else { edge },
                width: if selected { 2.0 } else { 1.5 },
                radius: 6.0.into(),
            },
            shadow: iced::Shadow {
                color: Color {
                    a: if hover { 0.18 } else { 0.08 },
                    ..Color::BLACK
                },
                offset: iced::Vector::new(0.0, if hover { 2.0 } else { 1.0 }),
                blur_radius: if hover { 6.0 } else { 2.0 },
            },
        }
    }
}

/// The legend: a swatch and what it means
fn swatch<'a>(color: [u8; 3], label: &'a str) -> Element<'a, Message> {
    row![
        container(Space::new(10, 10)).style(move |_: &iced::Theme| container::Style {
            background: Some(rgb(color).into()),
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }),
        text(label).size(11).style(style::muted),
    ]
    .spacing(5)
    .align_y(Alignment::Center)
    .into()
}

fn network_card(d: &Diag) -> Element<'_, Message> {
    let view_pick = row![
        button(text("Grid").size(12))
            .padding([4, 10])
            .style(style::toggle(!d.topology))
            .on_press(msg(DiagMsg::Topology(false))),
        button(text("Layout").size(12))
            .padding([4, 10])
            .style(style::toggle(d.topology))
            .on_press(msg(DiagMsg::Topology(true))),
    ]
    .spacing(4);
    let action = if d.scanning {
        ghost(
            if d.stopping { "Stopping…" } else { "Stop" },
            (!d.stopping).then_some(DiagMsg::StopScan),
        )
    } else {
        ghost(
            if d.scan.is_some() {
                "Rerun Network Test"
            } else {
                "Run Network Test"
            },
            (d.connected() && !d.busy()).then_some(DiagMsg::RunScan),
        )
    };
    let right = row![view_pick, action].spacing(10).align_y(Alignment::Center);

    let Some(scan) = &d.scan else {
        return card(
            "Network Test",
            right.into(),
            text(if d.connected() {
                "Finding the vehicle's modules…"
            } else {
                "Connect to the vehicle (Vehicle ID) to find its modules."
            })
            .size(13)
            .style(style::muted)
            .into(),
        );
    };
    let (asked, total, on) = scan.progress();
    let mut body = column![].spacing(12);
    // the legend (FDRS's "Network Test Results"), or how far it's got
    if d.scanning {
        let now = on
            .map(|m| match (m.listed, m.bus) {
                (false, b) => format!("Full search: {:03X} on {}…", m.address, net_name(b)),
                (true, b) => format!("Asking the {} on {}…", m.short, net_name(b)),
            })
            .unwrap_or_default();
        body = body.push(
            column![
                row![
                    text(now).size(13),
                    horizontal_space(),
                    text(format!("{asked} / {total}")).size(12).style(style::muted)
                ],
                progress_bar(0.0..=total as f32, asked as f32).height(6),
            ]
            .spacing(6),
        );
    } else {
        body = body.push(
            row![
                text(format!(
                    "{} module{} responded{}",
                    scan.found(),
                    plural(scan.found()),
                    if asked < total {
                        format!(" (stopped after {asked} of {total})")
                    } else {
                        String::new()
                    }
                ))
                .size(13)
                .font(style::BOLD),
                horizontal_space(),
                swatch(GREEN, "Responded"),
                swatch(RED, "Current DTCs"),
                swatch(AMBER, "Stored DTCs"),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        );
    }

    let networks: Vec<(Bus, Vec<&Module>)> = NETWORKS
        .iter()
        .map(|&b| (b, scan.modules.iter().filter(|m| m.bus == b && m.present()).collect::<Vec<_>>()))
        .filter(|(_, l)| !l.is_empty())
        .collect();
    if !networks.is_empty() {
        body = body.push(if d.topology {
            topology(d, &networks)
        } else {
            module_grid(d, &networks)
        });
    }

    // the module picked: what it is
    if let Some(m) = the_selected(d) {
        let mut info = column![
            row![
                text(tag(m.short, m.address)).font(style::BOLD),
                text(m.name).size(13),
                horizontal_space(),
                text(format!("{:03X} on {}", m.address, net_name(m.bus)))
                    .size(11)
                    .font(style::MONO)
                    .style(style::muted),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        ]
        .spacing(4);
        for (label, value) in [("Part", &m.part), ("Software", &m.software)] {
            if !value.is_empty() {
                info = info.push(
                    row![
                        text(label).size(12).style(style::muted).width(64),
                        text(value.as_str()).size(12).font(style::MONO)
                    ]
                    .align_y(Alignment::Center),
                );
            }
        }
        if let Status::Refused(_) = m.status {
            info = info.push(status_pill(m));
        }
        body = body.push(container(info).padding([10, 12]).width(Length::Fill).style(style::card_highlight));
    }

    // what didn't answer, and why a network's missing
    let silent: Vec<&str> = scan
        .modules
        .iter()
        .filter(|m| m.listed && m.bus == Bus::Hs && m.status == Status::NoAnswer)
        .map(|m| m.short)
        .collect();
    if !silent.is_empty() && !d.scanning {
        body = body.push(text(format!("Not responding: {}", silent.join(", "))).size(12).style(style::muted));
    }
    if !d.scanning {
        for bus in Bus::OTHERS {
            let asked: Vec<&Module> = scan
                .modules
                .iter()
                .filter(|m| m.bus == bus && m.status != Status::Waiting)
                .collect();
            let unreachable = d.scan_notes.iter().any(|n| n.starts_with(&bus.full_label()));
            if !asked.is_empty() && !unreachable && !asked.iter().any(|m| m.present()) {
                body = body.push(
                    text(format!("Nothing answered on {}.", bus.full_label()))
                        .size(12)
                        .style(style::muted),
                );
            }
        }
    }
    for n in &d.scan_notes {
        body = body.push(text(n.as_str()).size(12).style(style::muted));
    }
    // the full search: every other diagnostic address (once the list's done)
    let searched = scan.modules.iter().any(|m| !m.listed);
    if !d.scanning && scan.done() && !searched {
        body = body.push(
            row![
                text("A module missing? The full search asks every other diagnostic address (about half a minute).")
                    .size(12)
                    .style(style::muted)
                    .width(Length::Fill),
                ghost("Full search", (d.connected() && !d.busy()).then_some(DiagMsg::FullSearch)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    card("Network Test", right.into(), body.into())
}

/// Each network a box of module tiles (FDRS's layout): click one to pick it
fn module_grid<'a>(d: &'a Diag, networks: &[(Bus, Vec<&'a Module>)]) -> Element<'a, Message> {
    let mut c = column![].spacing(12);
    for (bus, list) in networks {
        let mut tiles = Row::new().spacing(8);
        for m in list {
            let selected = d.selected == Some((m.address, m.bus));
            tiles = tiles.push(
                button(
                    container(text(tag(m.short, m.address)).size(13).font(style::BOLD))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                )
                .width(84)
                .height(38)
                .padding(0)
                .style(tile_style(tile_state(d, m.address, m.bus), selected))
                .on_press(msg(DiagMsg::Select(m.address, m.bus))),
            );
        }
        c = c.push(
            column![
                row![
                    text(net_name(*bus)).size(13).font(style::BOLD),
                    text(format!("pins {}  ·  {} kbit/s", bus.pins_label(), bus.baud() / 1000))
                        .size(11)
                        .style(style::muted),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                container(tiles.wrap())
                    .padding(10)
                    .width(Length::Fill)
                    .style(|t: &iced::Theme| container::Style {
                        border: iced::Border {
                            color: style::colors(t).line,
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }),
            ]
            .spacing(6),
        );
    }
    c.into()
}

/// One of the applications: what it does, and the module it's for (none: every module)
struct Application {
    name: String,
    target: Option<(vci::modules::Entry, Bus)>,
    open: DiagMsg,
}

fn applications(d: &Diag) -> Vec<Application> {
    let mut list = vec![
        Application {
            name: "All Modules - Self Test (read and clear DTCs)".into(),
            target: None,
            open: DiagMsg::OpenSelfTest(None),
        },
        Application {
            name: "DataLogger (live PIDs from up to 5 modules)".into(),
            target: None,
            open: DiagMsg::OpenLogger,
        },
    ];
    if let Some(scan) = &d.scan {
        for m in scan.modules.iter().filter(|m| m.present()) {
            let who = format!("{} - {}", tag(m.short, m.address), m.name);
            list.push(Application {
                name: format!("{who} - Self Test"),
                target: Some(m.entry()),
                open: DiagMsg::OpenSelfTest(Some(m.entry())),
            });
        }
    }
    list
}

/// The applications (FDRS's list on the right): searched, and a module picked shows just its own. Each opens in its
/// own tab.
fn applications_card(d: &Diag) -> Element<'_, Message> {
    let search = text_input("Search", &d.app_search)
        .on_input(|s| msg(DiagMsg::AppSearch(s)))
        .size(13)
        .padding([5, 10])
        .width(200);

    // All | the module picked
    let picked = the_selected(d);
    let mut filters = row![
        button(text("All").size(12))
            .padding([4, 12])
            .style(style::toggle(picked.is_none()))
            .on_press_maybe(picked.map(|m| msg(DiagMsg::Select(m.address, m.bus)))),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    if let Some(m) = picked {
        filters = filters.push(
            button(
                row![text(tag(m.short, m.address)).size(12).font(style::BOLD), text("×").size(12)]
                    .spacing(6)
                    .align_y(Alignment::Center),
            )
            .padding([4, 12])
            .style(style::toggle(true))
            .on_press(msg(DiagMsg::Select(m.address, m.bus))),
        );
    }

    let words = d.app_search.trim().to_lowercase();
    let shown: Vec<Application> = applications(d)
        .into_iter()
        .filter(|a| match (picked, a.target) {
            (Some(m), Some(((addr, _, _), bus))) => addr == m.address && bus == m.bus,
            (Some(_), None) => false,
            (None, _) => true,
        })
        .filter(|a| words.is_empty() || a.name.to_lowercase().contains(&words))
        .collect();

    let mut rows = Column::new();
    if shown.is_empty() {
        rows = rows.push(container(text("No applications match.").size(13).style(style::muted)).padding([12, 14]));
    }
    for (i, a) in shown.into_iter().enumerate() {
        let open = d.connected().then(|| msg(a.open));
        rows = rows.push(
            button(
                row![
                    text(a.name).size(13).width(Length::Fill),
                    text("Open")
                        .size(12)
                        .font(style::BOLD)
                        .style(|t: &iced::Theme| iced::widget::text::Style {
                            color: Some(style::colors(t).accent),
                        }),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([9, 14])
            .width(Length::Fill)
            .style(move |t: &iced::Theme, s: button::Status| {
                let c = style::colors(t);
                let hover = matches!(s, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: Some(
                        if hover {
                            c.sel
                        } else if i % 2 == 1 {
                            c.hover
                        } else {
                            c.panel
                        }
                        .into(),
                    ),
                    text_color: if matches!(s, button::Status::Disabled) {
                        c.muted
                    } else {
                        t.palette().text
                    },
                    ..Default::default()
                }
            })
            .on_press_maybe(open),
        );
    }
    let table = container(rows)
        .width(Length::Fill)
        .clip(true)
        .style(|t: &iced::Theme| container::Style {
            border: iced::Border {
                color: style::colors(t).line,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

    let mut body = column![filters, table].spacing(10);
    if !d.connected() {
        body = body.push(text("Connect to the vehicle to open one.").size(12).style(style::muted));
    }
    card("Applications", search.into(), body.into())
}

/// The codes the latest self test found in a module (none: it hasn't been read)
fn codes_in(d: &Diag, address: u32, bus: Bus) -> Option<usize> {
    d.apps.iter().rev().filter_map(|a| a.test.as_ref()).find_map(|t| {
        t.modules
            .iter()
            .find(|m| m.address == address && m.bus == bus && m.present())
            .map(|m| m.codes().len())
    })
}

fn the_selected(d: &Diag) -> Option<&Module> {
    let (a, bus) = d.selected?;
    d.scan.as_ref()?.modules.iter().find(|m| m.address == a && m.bus == bus)
}

/// The network's layout: from the DLC's pins, a wire to each network, its modules along it
fn topology<'a>(d: &'a Diag, networks: &[(Bus, Vec<&'a Module>)]) -> Element<'a, Message> {
    let gateway = d.scan.as_ref().is_some_and(|s| s.gateway());
    let mut lines = Column::new().spacing(14);
    for (bus, list) in networks {
        let mut chips = Row::new().spacing(6);
        for m in list {
            let selected = d.selected == Some((m.address, m.bus));
            let mut face = column![
                text(tag(m.short, m.address)).size(13).font(style::BOLD),
                text(format!("{:03X}", m.address)).size(10).font(style::MONO).style(style::muted),
            ]
            .spacing(1)
            .align_x(Alignment::Center);
            match codes_in(d, m.address, m.bus) {
                Some(0) => face = face.push(text("OK").size(10).color(rgb(GREEN))),
                Some(n) => face = face.push(text(format!("{n} DTC")).size(10).color(rgb(RED))),
                None => {}
            }
            chips = chips.push(
                button(face)
                    .padding([6, 10])
                    .style(style::toggle(selected))
                    .on_press(msg(DiagMsg::Select(m.address, m.bus))),
            );
        }
        let is_gateway_net = gateway && list.iter().any(|m| m.address == vci::modules::GWM);
        let dlc = column![
            text("DLC").size(11).style(style::muted),
            text(bus.pins_label()).size(12).font(style::BOLD),
        ]
        .align_x(Alignment::Center);
        let mut label = column![
            text(bus.label()).size(12).font(style::BOLD),
            text(format!("{} kbit/s", bus.baud() / 1000)).size(10).style(style::muted),
        ]
        .spacing(1);
        if is_gateway_net {
            label = label.push(text("Gateway").size(10).color(rgb(AMBER)));
        }
        let net = row![
            container(dlc).padding([4, 8]).width(64).style(style::card_highlight),
            wire(14.0, 2.0),
            container(label).padding([4, 8]).style(style::card),
            wire(14.0, 2.0),
            chips.wrap(),
        ]
        .align_y(Alignment::Center);
        lines = lines.push(net);
    }
    let mut c = column![lines].spacing(10);
    if gateway {
        c = c.push(
            text(
                "The gateway (GWM) passes requests on to the networks behind it, so some modules can be reached on \
                  fewer pins than they're wired to.",
            )
            .size(12)
            .style(style::muted),
        );
    }
    container(c).padding(12).width(Length::Fill).style(style::panel).into()
}

// ---------- Self Test ----------

fn selftest_page<'a>(d: &'a Diag, app: &'a SelfTestApp) -> Element<'a, Message> {
    let id = app.id;
    let running = d.testing == Some(id);
    let idle = d.connected() && !d.busy();
    let t = app.test.as_ref();
    let has_results = t.is_some_and(|t| t.done()) && !running;
    let mut right = row![].spacing(6).align_y(Alignment::Center);
    if running {
        right = right.push(ghost(
            if d.stopping { "Stopping…" } else { "Stop" },
            (!d.stopping && !t.is_some_and(|t| t.clearing())).then_some(DiagMsg::StopSelfTest),
        ));
    } else {
        if has_results {
            right = right
                .push(ghost("Copy codes", Some(DiagMsg::CopyCodes(id))))
                .push(ghost("Clear codes", (idle && !app.confirm_clear).then_some(DiagMsg::AskClear(id))));
        }
        right = right.push(primary(
            if t.is_some() { "Read again" } else { "Read codes" },
            idle.then_some(DiagMsg::RunSelfTest(id)),
        ));
    }
    let about = match app.target {
        Some(((a, _, name), bus)) => format!("{name}  ·  {a:03X} on {}", bus.full_label()),
        None => "Every module the vehicle has (the ones on the Dashboard)".into(),
    };
    let mut body = column![text(about).size(13).style(style::muted)].spacing(12);
    let title = app.title();

    let Some(t) = t else {
        body = body.push(
            text(if d.connected() {
                "Reads the trouble codes. Clearing them asks first."
            } else {
                "Connect to the vehicle (Vehicle ID) first."
            })
            .size(13)
            .style(style::muted),
        );
        return scrolling(card(title, right.into(), body.into()), 880.0);
    };

    // "clear them?"
    if app.confirm_clear {
        let n = t.modules.iter().filter(|m| m.present()).count();
        let what = match app.target {
            Some(((a, short, _), _)) => format!("Clear the {}'s codes?", tag(short, a)),
            None => format!("Clear the codes in all {n} module{}?", plural(n)),
        };
        body = body.push(
            container(
                column![
                    text(what).font(style::BOLD),
                    text("Ignition on, engine off. Copy the codes first if you need them for the RO: they can't be brought back.").size(13),
                    row![
                        horizontal_space(),
                        ghost("Cancel", Some(DiagMsg::CancelClear(id))),
                        button(text("Clear codes").font(style::BOLD))
                            .padding([7, 16])
                            .style(button::danger)
                            .on_press(msg(DiagMsg::ConfirmClear(id))),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(8),
            )
            .padding(14)
            .style(style::card_highlight),
        );
    }
    let (asked, total, on) = t.progress();
    if running {
        let doing = if t.clearing() { "Clearing" } else { "Reading" };
        let now = on.map(|m| format!("{doing} the {}…", tag(m.short, m.address))).unwrap_or_default();
        body = body.push(
            column![
                row![
                    text(now).size(13),
                    horizontal_space(),
                    text(format!("{asked} / {total}")).size(12).style(style::muted)
                ],
                progress_bar(0.0..=total as f32, asked as f32).height(6),
            ]
            .spacing(6),
        );
    } else {
        let n = t.total_codes();
        let present = t.modules.iter().filter(|m| m.present()).count();
        let summary = if app.target.is_some() && present == 0 && asked >= total {
            "The module didn't answer".to_string()
        } else {
            format!(
                "{} in {present} module{}{}",
                match n {
                    0 => "No codes".to_string(),
                    1 => "1 code".to_string(),
                    n => format!("{n} codes"),
                },
                plural(present),
                if asked < total {
                    format!(" (stopped after {asked} of {total})")
                } else {
                    String::new()
                }
            )
        };
        body = body.push(text(summary).font(style::BOLD));
        // what the codes mean comes from Ford's search: why it couldn't say
        let failed = t
            .modules
            .iter()
            .flat_map(|m| m.codes().iter().map(|c| dtc_key(c, m.short)))
            .find_map(|k| match d.dtcs.get(&k) {
                Some(Lookup::Failed(e)) => Some(e.clone()),
                _ => None,
            });
        if let Some(e) = failed {
            body = body.push(
                text(format!(
                    "Couldn't look up what the codes mean ({e}). Find Pinpoint Test still searches the Service Manual."
                ))
                .size(12)
                .style(style::muted),
            );
        } else if d.vin.is_none() && t.total_codes() > 0 {
            body = body.push(
                text("Without the VIN, the codes can't be looked up in the Service Manual.")
                    .size(12)
                    .style(style::muted),
            );
        }
    }
    // each module with codes, its codes under it
    for m in t.modules.iter().filter(|m| !m.codes().is_empty()) {
        let mut list = column![
            row![
                text(tag(m.short, m.address)).font(style::BOLD),
                text(m.name).size(13).style(style::muted),
                horizontal_space(),
                pill(
                    format!("{}", m.codes().len()),
                    if m.codes().iter().any(|c| c.current()) { RED } else { AMBER }
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        ]
        .spacing(6);
        for c in m.codes() {
            list = list.push(code_row(d, c, m.short));
        }
        body = body.push(container(list).padding([10, 12]).width(Length::Fill).style(style::panel));
    }
    let names = |f: &dyn Fn(&vci::dtc::Module) -> bool| {
        t.modules
            .iter()
            .filter(|m| f(m))
            .map(|m| tag(m.short, m.address))
            .collect::<Vec<_>>()
    };
    let clean = names(&|m| matches!(&m.result, vci::dtc::Found::Codes(c) if c.is_empty()));
    let refused = names(&|m| matches!(m.result, vci::dtc::Found::Refused(_)));
    let not_cleared: Vec<String> = t
        .modules
        .iter()
        .filter_map(|m| match m.clear {
            Some(Err(0)) => Some(format!("{} (no answer)", tag(m.short, m.address))),
            Some(Err(n)) => Some(format!("{} ({})", tag(m.short, m.address), vci::uds::refusal(n))),
            _ => None,
        })
        .collect();
    for (label, list) in [
        ("No codes", clean.join(", ")),
        ("Wouldn't give codes", refused.join(", ")),
        ("Didn't clear", not_cleared.join(", ")),
    ] {
        if !list.is_empty() {
            body = body.push(text(format!("{label}: {list}")).size(12).style(style::muted));
        }
    }
    scrolling(card(title, right.into(), body.into()), 880.0)
}

// ---------- shared ----------

/// A code: what it means (Ford's description, or its failure type until that's in), its status, and its pinpoint test
fn code_row<'a>(d: &'a Diag, c: &'a vci::dtc::Dtc, short: &str) -> Element<'a, Message> {
    let key = dtc_key(c, short);
    let mut tags = row![].spacing(6).align_y(Alignment::Center);
    for w in c.status_words() {
        let color = match w {
            "Current" | "Warning light" => RED,
            "Pending" | "Stored" | "Failed this drive" => AMBER,
            _ => GREY,
        };
        tags = tags.push(pill(w, color));
    }
    let ft = vci::dtc::failure_type(c.ft).map(|f| f.to_string()).unwrap_or_default();
    let lookup = d.dtcs.get(&key);
    let mut about = column![].spacing(2).width(Length::Fill);
    match lookup {
        Some(Lookup::Found(l)) if !l.description.is_empty() => {
            about = about.push(text(l.description.clone()).size(13));
            if let Some((_, title)) = &l.pinpoint {
                about = about.push(text(title.clone()).size(11).style(style::muted));
            } else if !ft.is_empty() && !l.description.to_lowercase().contains(&ft) {
                about = about.push(text(ft).size(11).style(style::muted));
            }
        }
        Some(Lookup::Asking) => {
            about = about.push(
                row![anim::dot(rgb(GREY), true), text("Looking it up…").size(13).style(style::muted)]
                    .spacing(6)
                    .align_y(Alignment::Center),
            );
        }
        _ => {
            about = about.push(text(ft).size(13).style(style::muted));
        }
    }
    let can_find = d.vin.is_some() && !matches!(lookup, Some(Lookup::Asking));
    row![
        text(c.full()).font(style::MONO).width(110),
        about,
        tags,
        button(text("Find Pinpoint Test").size(12))
            .padding([5, 10])
            .style(style::ghost)
            .on_press_maybe(can_find.then(|| msg(DiagMsg::FindPinpoint(key)))),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn status_pill<'a>(m: &Module) -> Element<'a, Message> {
    match m.status {
        Status::Answered => pill("Answered", GREEN),
        Status::Refused(_) => pill("There, won't say", AMBER),
        Status::NoAnswer => pill("No answer", GREY),
        Status::Waiting => pill("Waiting", GREY),
    }
}

// ---------- the communication log ----------

fn log_card(d: &Diag) -> Element<'_, Message> {
    let right = row![ghost(if d.show_log { "Hide" } else { "Show" }, Some(DiagMsg::ToggleLog)),].spacing(6);
    if !d.show_log {
        return card(
            "Communication log",
            right.into(),
            text(format!("{} line{}", d.log.len(), if d.log.len() == 1 { "" } else { "s" }))
                .size(12)
                .style(style::muted)
                .into(),
        );
    }
    let right = right
        .push(ghost("Copy", (!d.log.is_empty()).then_some(DiagMsg::CopyLog)))
        .push(ghost("Clear", (!d.log.is_empty() && !d.busy()).then_some(DiagMsg::ClearLog)));
    // the newest at the bottom; the last few hundred shown (Copy has them all)
    let from = d.log.len().saturating_sub(400);
    let lines = d.log[from..]
        .iter()
        .fold(Column::new().spacing(1), |c, l| c.push(text(l.as_str()).size(12).font(style::MONO)));
    card(
        "Communication log",
        right.into(),
        column![
            text("Each request to the vehicle and its answer. It includes the VIN, so it stays in the app unless you copy it.")
                .size(12)
                .style(style::muted),
            container(scrollable(container(lines).padding(10).width(Length::Fill)).height(260))
                .style(style::panel)
                .width(Length::Fill),
        ]
        .spacing(8)
        .into(),
    )
}
