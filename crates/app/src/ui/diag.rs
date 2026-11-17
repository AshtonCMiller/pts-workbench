//! Diagnostics: the VCI and the vehicle plugged into it (crates/app/src/diag.rs has the steps). Three cards, top to
//! bottom in the order you'd use them: the VCI (connect), the vehicle (its VIN, and your RO for it), the self test (its
//! trouble codes), the network test.
//! The communication log folds out at the bottom.

use iced::widget::{Column, Space, button, column, container, horizontal_space, pick_list, progress_bar, row, text};
use iced::{Alignment, Color, Element, Length};
use ui_kit::scroll::scrollable;
use vci::modules::Bus;
use vci::scan::{Module, Status};

use super::{anim, pill, style};
use crate::app::{App, Message};
use crate::diag::{Diag, DiagMsg, LOW_MV, Link};

const GREEN: [u8; 3] = [0x2e, 0xa0, 0x43];
const AMBER: [u8; 3] = [0xd2, 0x99, 0x22];
const RED: [u8; 3] = [0xcf, 0x22, 0x2e];
const GREY: [u8; 3] = [0x5d, 0x66, 0x70];

fn msg(m: DiagMsg) -> Message {
    Message::Vci(m)
}

fn card<'a>(title: &'a str, right: Element<'a, Message>, body: Element<'a, Message>) -> Element<'a, Message> {
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

fn primary<'a>(label: &'a str, on: Option<DiagMsg>) -> Element<'a, Message> {
    button(text(label).font(style::BOLD))
        .padding([7, 16])
        .style(button::primary)
        .on_press_maybe(on.map(msg))
        .into()
}

fn ghost<'a>(label: &'a str, on: Option<DiagMsg>) -> Element<'a, Message> {
    button(text(label))
        .padding([6, 12])
        .style(style::ghost)
        .on_press_maybe(on.map(msg))
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let d = &app.diag;
    let mut page = Column::new().spacing(16).max_width(880);
    if let Some(e) = &d.error {
        page = page.push(
            container(
                row![
                    text(e.as_str()).size(13).width(Length::Fill),
                    ghost("Dismiss", Some(DiagMsg::DismissError))
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([10, 14])
            .width(Length::Fill)
            .style(|_: &iced::Theme| {
                let red = Color::from_rgb8(RED[0], RED[1], RED[2]);
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
            }),
        );
    }
    page = page.push(vci_card(d));
    if d.connected() || d.vin.is_some() {
        page = page.push(vehicle_card(app));
    }
    if d.connected() || d.scan.is_some() {
        page = page.push(selftest_card(d));
        page = page.push(scan_card(d));
    }
    page = page.push(log_card(d));
    container(scrollable(container(page).padding(24).center_x(Length::Fill)))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
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

fn vehicle_card(app: &App) -> Element<'_, Message> {
    let d = &app.diag;
    let again = ghost("Identify again", (d.connected() && !d.busy()).then_some(DiagMsg::Identify));
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
        let ro = app
            .my_ros
            .iter()
            .find(|r| r.vin.eq_ignore_ascii_case(vin))
            .or_else(|| app.open.iter().map(|w| &w.ro).find(|r| r.vin.eq_ignore_ascii_case(vin)));
        let mine: Element<'_, Message> = match ro {
            Some(r) => {
                let what = [r.year.as_str(), r.make.as_str(), r.model.as_str()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ");
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
        text("The PCM didn't give its VIN. Check the ignition's on, then Identify again.")
            .size(13)
            .into()
    } else {
        text("Not identified yet.").style(style::muted).into()
    };
    card("Vehicle", again, body)
}

// ---------- the self test ----------

fn code_row(c: &vci::dtc::Dtc) -> Element<'_, Message> {
    let mut tags = row![].spacing(6).align_y(Alignment::Center);
    for w in c.status_words() {
        let color = match w {
            "Current" | "Warning light" => RED,
            "Pending" | "Stored" | "Failed this drive" => AMBER,
            _ => GREY,
        };
        tags = tags.push(pill(w, color));
    }
    row![
        text(c.full()).font(style::MONO).width(110),
        text(vci::dtc::failure_type(c.ft).map(|f| f.to_string()).unwrap_or_default())
            .size(13)
            .style(style::muted)
            .width(Length::Fill),
        tags,
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn selftest_card(d: &Diag) -> Element<'_, Message> {
    let idle = d.connected() && !d.busy();
    let t = d.selftest.as_ref();
    let has_results = t.is_some_and(|t| t.done()) && !d.testing;
    let mut right = row![].spacing(6).align_y(Alignment::Center);
    if d.testing {
        right = right.push(ghost(
            if d.stopping { "Stopping…" } else { "Stop" },
            (!d.stopping && !t.is_some_and(|t| t.clearing())).then_some(DiagMsg::StopSelfTest),
        ));
    } else {
        if has_results {
            right = right
                .push(ghost("Copy codes", Some(DiagMsg::CopyCodes)))
                .push(ghost("Clear codes", (idle && !d.confirm_clear).then_some(DiagMsg::AskClear)));
        }
        right = right.push(primary(
            if t.is_some() { "Read again" } else { "Read codes" },
            idle.then_some(DiagMsg::RunSelfTest),
        ));
    }
    let Some(t) = t else {
        return card(
            "Self test",
            right.into(),
            text(
                "Reads the trouble codes from every module (only the ones the network test found, if you've run it). \
                 Clearing them asks first.",
            )
            .size(13)
            .style(style::muted)
            .into(),
        );
    };

    let mut body = column![].spacing(12);
    // "clear them all?"
    if d.confirm_clear {
        let n = t.modules.iter().filter(|m| m.present()).count();
        body = body.push(
            container(
                column![
                    text(format!("Clear the codes in all {n} module{}?", if n == 1 { "" } else { "s" })).font(style::BOLD),
                    text("Ignition on, engine off. Copy the codes first if you need them for the RO: they can't be brought back.").size(13),
                    row![
                        horizontal_space(),
                        ghost("Cancel", Some(DiagMsg::CancelClear)),
                        button(text("Clear codes").font(style::BOLD))
                            .padding([7, 16])
                            .style(button::danger)
                            .on_press(msg(DiagMsg::ConfirmClear)),
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
    if d.testing {
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
        body = body.push(
            text(format!(
                "{} in {present} module{}{}",
                match n {
                    0 => "No codes".to_string(),
                    1 => "1 code".to_string(),
                    n => format!("{n} codes"),
                },
                if present == 1 { "" } else { "s" },
                if asked < total {
                    format!(" (stopped after {asked} of {total})")
                } else {
                    String::new()
                }
            ))
            .font(style::BOLD),
        );
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
            list = list.push(code_row(c));
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
    card("Self test", right.into(), body.into())
}

// ---------- the network test ----------

fn status_pill<'a>(m: &Module) -> Element<'a, Message> {
    match m.status {
        Status::Answered => pill("Answered", GREEN),
        Status::Refused(_) => pill("There, won't say", AMBER),
        Status::NoAnswer => pill("No answer", GREY),
        Status::Waiting => pill("Waiting", GREY),
    }
}

/// A module's short name ("ABS"), or its address for one found by the full search ("7A5")
fn tag(short: &str, address: u32) -> String {
    if short == vci::modules::UNLISTED.0 {
        format!("{address:03X}")
    } else {
        short.to_string()
    }
}

fn module_row(m: &Module) -> Element<'_, Message> {
    let mut detail = column![].spacing(1);
    if !m.part.is_empty() {
        detail = detail.push(text(m.part.as_str()).size(13).font(style::MONO));
    }
    if !m.software.is_empty() {
        detail = detail.push(text(format!("Software {}", m.software)).size(11).style(style::muted));
    }
    if let Status::Refused(code) = m.status {
        detail = detail.push(text(format!("Refused: {}", vci::uds::refusal(code))).size(11).style(style::muted));
    }
    container(
        row![
            column![
                text(tag(m.short, m.address)).font(style::BOLD),
                text(match m.bus {
                    Bus::Hs => format!("{:03X}", m.address),
                    other => format!("{:03X} · {}", m.address, other.label()),
                })
                .size(11)
                .font(style::MONO)
                .style(style::muted)
            ]
            .width(96),
            text(m.name).size(13).width(Length::FillPortion(3)),
            container(detail).width(Length::FillPortion(3)),
            container(status_pill(m)).width(120).align_x(iced::alignment::Horizontal::Right),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([8, 10])
    .into()
}

fn scan_card(d: &Diag) -> Element<'_, Message> {
    let action = if d.scanning {
        ghost(
            if d.stopping { "Stopping…" } else { "Stop" },
            (!d.stopping).then_some(DiagMsg::StopScan),
        )
    } else {
        primary(
            if d.scan.is_some() { "Run again" } else { "Run network test" },
            (d.connected() && !d.busy()).then_some(DiagMsg::RunScan),
        )
    };
    let Some(scan) = &d.scan else {
        return card(
            "Network test",
            action,
            text(
                "Asks each module who it is: its part number and software. On newer Fords the gateway (GWM) passes the \
                 requests on to every network; on older ones, pins 3 / 11 (MS-CAN) are asked too. Nothing is changed.",
            )
            .size(13)
            .style(style::muted)
            .into(),
        );
    };
    let (asked, total, on) = scan.progress();
    let mut body = column![].spacing(10);
    if d.scanning {
        let now = on
            .map(|m| match (m.listed, m.bus) {
                (false, b) => format!("Full search: {:03X} on {}…", m.address, b.label()),
                (true, Bus::Hs) => format!("Asking the {} ({})…", m.short, m.name),
                (true, b) => format!("Asking the {} on {} (pins {})…", m.short, b.label(), b.pins_label()),
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
            text(format!(
                "{} module{} answered{}",
                scan.found(),
                if scan.found() == 1 { "" } else { "s" },
                if asked < total {
                    format!(" (stopped after {asked} of {total})")
                } else {
                    String::new()
                }
            ))
            .font(style::BOLD),
        );
    }
    // the ones there, in the order asked; then the rest in one line
    let present: Vec<&Module> = scan.modules.iter().filter(|m| m.present()).collect();
    if !present.is_empty() {
        let mut list = Column::new();
        for (i, m) in present.iter().enumerate() {
            if i > 0 {
                list = list.push(container(Space::new(Length::Fill, 1)).style(|t: &iced::Theme| container::Style {
                    background: Some(style::colors(t).line.into()),
                    ..Default::default()
                }));
            }
            list = list.push(module_row(m));
        }
        body = body.push(container(list).style(style::panel));
    }
    // the list's modules that didn't answer on pins 6 / 14; the other networks, what answered there
    let silent: Vec<&str> = scan
        .modules
        .iter()
        .filter(|m| m.listed && m.bus == Bus::Hs && m.status == Status::NoAnswer)
        .map(|m| m.short)
        .collect();
    if !silent.is_empty() {
        body = body.push(text(format!("No answer: {}", silent.join(", "))).size(12).style(style::muted));
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
    card("Network test", action, body.into())
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