//! The Feedback screen: Send (a bug or an idea), My reports (status and replies), and for the admin, All reports
//! (set status, reply, delete). Its tabs slide like the rest of the app, and their content moves between tabs.

use feedback::{Kind, Report, Status};
use iced::widget::{Column, button, column, container, horizontal_space, pick_list, row, text, text_editor, text_input};
// scrolling that glides (crates/ui-kit)
use iced::{Alignment, Element, Length};
use ui_kit::scroll::scrollable;

use super::{anim, pill, style};
use crate::app::{App, Message};
use crate::reports::{Filter, Load, MineFilter, ReportMsg, Reports, Tab};

fn msg(m: ReportMsg) -> Message {
    Message::Report(m)
}

/// "Nov 15, 2026" from milliseconds since 1970 (UTC; no date library needed)
pub fn date(ms: i64) -> String {
    // days since 1970 → year / month / day (Howard Hinnant's civil-from-days)
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{} {day}, {year}", MONTHS[(month - 1) as usize])
}

fn kind_tag<'a>(k: Kind) -> Element<'a, Message> {
    match k {
        Kind::Bug => pill("Bug", [0xb4, 0x23, 0x18]),
        Kind::Feature => pill("Idea", [0x1a, 0x5f, 0xb4]),
    }
}

fn status_tag<'a>(s: Status) -> Element<'a, Message> {
    let color = match s {
        Status::New => [0xd9, 0x77, 0x06],
        Status::Planned => [0x7a, 0x5a, 0xf5],
        Status::InProgress => [0x0e, 0x74, 0x90],
        Status::Done => [0x15, 0x80, 0x3d],
        Status::WontDo => [0x5d, 0x66, 0x70],
    };
    pill(s.label(), color)
}

/// Feedback, as a card over everything (from the bottom bar)
pub fn card(app: &App) -> Element<'_, Message> {
    let r = &app.reports;
    let admin = app.is_admin();

    let mut tab_list = vec![(Tab::Send, "Send".to_string()), (Tab::Mine, "My reports".to_string())];
    if admin {
        tab_list.push((
            Tab::All,
            if r.new_count > 0 {
                format!("All reports  {}", r.new_count)
            } else {
                "All reports".into()
            },
        ));
    }
    let tabs = anim::tabs(
        tab_list.iter().position(|(t, _)| *t == r.tab).unwrap_or(0),
        tab_list
            .into_iter()
            .map(|(t, label)| {
                button(text(label))
                    .padding([6, 14])
                    .style(style::tab(r.tab == t))
                    .on_press(msg(ReportMsg::Tab(t)))
                    .into()
            })
            .collect(),
    );

    // the tab whose content is shown (while one leaves, still the old one)
    // the tab whose content is shown (while one leaves, still the old one)
    let content = match r.shown_tab {
        Tab::Send => send(r),
        Tab::Mine => mine(r),
        Tab::All => all(r),
    };

    let head = row![
        text("Feedback").size(20).font(style::BOLD),
        horizontal_space(),
        button(text("Close"))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(Message::CloseFeedback),
    ]
    .align_y(Alignment::Center);
    container(scrollable(
        container(column![head, tabs, anim::page(content, r.tab_phase).on_card()].spacing(14)).padding(24),
    ))
    .width(760)
    .style(style::floating_card)
    .into()
}

fn send(r: &Reports) -> Element<'_, Message> {
    let kinds = anim::tabs(
        if r.kind == Kind::Bug { 0 } else { 1 },
        [(Kind::Bug, "Something's broken"), (Kind::Feature, "An idea")]
            .into_iter()
            .map(|(k, label)| {
                button(text(label))
                    .padding([5, 12])
                    .style(style::tab(r.kind == k))
                    .on_press(msg(ReportMsg::Kind(k)))
                    .into()
            })
            .collect(),
    );
    column![
        kinds,
        column![
            text("Title").font(style::BOLD),
            text_input(
                if r.kind == Kind::Bug {
                    "What went wrong, in a few words"
                } else {
                    "Your idea, in a few words"
                },
                &r.title
            )
            .on_input(|t| msg(ReportMsg::Title(t)))
            .padding(8),
        ]
        .spacing(4),
        column![
            text("Details").font(style::BOLD),
            text_editor(&r.body)
                .placeholder("What were you doing, what did you expect, what happened instead?")
                .on_action(|a| msg(ReportMsg::Body(a)))
                .height(160)
                .padding(8),
            text(format!(
                "Sent with your name, the app's version and where you were ({}). Never VINs, ROs or customer details.",
                r.screen
            ))
            .size(12)
            .style(style::muted),
        ]
        .spacing(4),
        row![
            text(r.sent.clone().unwrap_or_default()).size(13),
            horizontal_space(),
            button(text("Send").font(style::BOLD))
                .padding([8, 20])
                .style(button::primary)
                .on_press_maybe((!r.sending).then_some(msg(ReportMsg::Send))),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(14)
    .into()
}

fn loading<'a, T>(load: &'a Load<T>, ready: impl FnOnce(&'a T) -> Element<'a, Message>) -> Element<'a, Message> {
    match load {
        Load::Idle | Load::Loading => text("Loading…").style(style::muted).into(),
        Load::Failed(e) => text(e.as_str()).into(),
        Load::NeedsKey(why) => key_form(why.as_deref()),
        Load::Ready(v) => ready(v),
    }
}

/// "Fixed in v0.5.0", for a done one that says which version
fn fixed_tag<'a>(x: &Report) -> Option<Element<'a, Message>> {
    x.fixed_in_label().map(|v| pill_owned(format!("Fixed in {v}"), [0x15, 0x80, 0x3d]))
}

/// A pill whose text is made here (pill() takes text that lives on)
fn pill_owned<'a>(label: String, rgb: [u8; 3]) -> Element<'a, Message> {
    container(text(label).size(11))
        .padding([1, 8])
        .style(style::pill(iced::Color::from_rgb8(rgb[0], rgb[1], rgb[2])))
        .into()
}

fn mine(r: &Reports) -> Element<'_, Message> {
    loading(&r.mine, |list| {
        if list.is_empty() {
            return text("You haven't sent anything yet.").style(style::muted).into();
        }
        // everything you've sent from this computer, done ones too
        let shown: Vec<&Report> = list.iter().filter(|x| r.mine_filter.keeps(x)).collect();
        let filters = anim::tabs(
            MineFilter::ALL.iter().position(|f| *f == r.mine_filter).unwrap_or(0),
            MineFilter::ALL
                .iter()
                .map(|&f| {
                    button(text(f.label()).size(13))
                        .padding([4, 10])
                        .style(style::tab(r.mine_filter == f))
                        .on_press(msg(ReportMsg::MineFilter(f)))
                        .into()
                })
                .collect(),
        )
        .spacing(2.0);
        let bar = row![
            text(format!("{} of {}", shown.len(), list.len())).size(13).style(style::muted),
            horizontal_space(),
            filters
        ]
        .align_y(Alignment::Center);
        let items: Vec<Element<'_, Message>> = if shown.is_empty() {
            vec![text("Nothing here.").style(style::muted).into()]
        } else {
            shown.into_iter().map(mine_item).collect()
        };
        column![bar, Column::with_children(items).spacing(8)].spacing(10).into()
    })
}

fn mine_item(x: &Report) -> Element<'_, Message> {
    let mut top = row![kind_tag(x.kind), text(x.title.as_str()).font(style::BOLD), horizontal_space()]
        .spacing(8)
        .align_y(Alignment::Center);
    if let Some(f) = fixed_tag(x) {
        top = top.push(f);
    }
    let top = top
        .push(status_tag(x.status))
        .push(text(date(x.created_at)).size(12).style(style::muted));
    let mut item = column![top].spacing(6);
    if let Some(reply) = x.reply.as_deref().filter(|s| !s.trim().is_empty()) {
        item = item.push(row![text("Reply:").font(style::BOLD).size(13), text(reply).size(13)].spacing(6));
    }
    container(item).padding(10).width(Length::Fill).style(style::card).into()
}

fn key_form<'a>(why: Option<&'a str>) -> Element<'a, Message> {
    // the text typed is in the app's state; this only shows the form
    column![
        text("Enter the Feedback password (the REPORTS_ADMIN_KEY set on the server). It's kept by this computer's keyring.").size(13),
        text(why.unwrap_or("")).size(13),
    ]
    .spacing(6)
    .into()
}

fn all(r: &Reports) -> Element<'_, Message> {
    if let Load::NeedsKey(why) = &r.all {
        return column![
            key_form(why.as_deref()),
            row![
                text_input("Feedback password", &r.key_text)
                    .secure(true)
                    .on_input(|t| msg(ReportMsg::KeyText(t)))
                    .on_submit(msg(ReportMsg::Unlock))
                    .padding(8),
                button(text("Unlock").font(style::BOLD))
                    .padding([8, 16])
                    .style(button::primary)
                    .on_press(msg(ReportMsg::Unlock)),
            ]
            .spacing(8),
        ]
        .spacing(10)
        .into();
    }
    loading(&r.all, |list| {
        let shown: Vec<&Report> = list.iter().filter(|x| r.filter.keeps(x)).collect();
        let filters = anim::tabs(
            Filter::ALL.iter().position(|f| *f == r.filter).unwrap_or(0),
            Filter::ALL
                .iter()
                .map(|&f| {
                    button(text(f.label()).size(13))
                        .padding([4, 10])
                        .style(style::tab(r.filter == f))
                        .on_press(msg(ReportMsg::Filter(f)))
                        .into()
                })
                .collect(),
        )
        .spacing(2.0);
        let bar = row![
            text(format!("{} of {}", shown.len(), list.len())).size(13).style(style::muted),
            text(r.note.clone().unwrap_or_default()).size(13),
            horizontal_space(),
            filters,
            button(text("Refresh").size(13))
                .padding([4, 10])
                .style(style::ghost)
                .on_press(msg(ReportMsg::Tab(Tab::All))),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let items: Vec<Element<'_, Message>> = if shown.is_empty() {
            vec![text("Nothing here.").style(style::muted).into()]
        } else {
            shown.into_iter().map(|x| report_item(r, x)).collect()
        };
        column![bar, Column::with_children(items).spacing(8)].spacing(10).into()
    })
}

fn report_item<'a>(r: &'a Reports, x: &'a Report) -> Element<'a, Message> {
    let id = x.id;
    let top = row![
        kind_tag(x.kind),
        text(x.title.as_str()).font(style::BOLD),
        horizontal_space(),
        pick_list(&Status::ALL[..], Some(x.status), move |s| msg(ReportMsg::SetStatus(id, s)))
            .padding([4, 8])
            .text_size(13),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let meta = [
        x.name.clone().unwrap_or_else(|| "Someone".into()),
        date(x.created_at),
        x.version.clone().map(|v| format!("v{v}")).unwrap_or_default(),
        x.screen.clone().unwrap_or_default(),
        x.platform.clone().unwrap_or_default(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    let reply = r.replies.get(&id).map(String::as_str).or(x.reply.as_deref()).unwrap_or("");
    let deleting = r.confirm_delete == Some(id);
    let mut item = column![top, text(meta).size(12).style(style::muted)].spacing(6);
    match &r.pending_done {
        // being marked done: which version fixed it?
        Some((pid, version)) if *pid == id => {
            item = item.push(
                row![
                    text("Fixed in v").size(13),
                    text_input("0.5.0", version)
                        .on_input(|v| msg(ReportMsg::FixedIn(v)))
                        .on_submit(msg(ReportMsg::ConfirmDone))
                        .padding(6)
                        .width(120),
                    button(text("Mark done").size(13).font(style::BOLD))
                        .padding([5, 12])
                        .style(button::primary)
                        .on_press(msg(ReportMsg::ConfirmDone)),
                    button(text("Cancel").size(13))
                        .padding([5, 10])
                        .style(style::ghost)
                        .on_press(msg(ReportMsg::CancelDone)),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        _ => {
            if let Some(f) = fixed_tag(x) {
                item = item.push(
                    row![
                        f,
                        button(text("Change").size(12))
                            .padding([2, 8])
                            .style(button::text)
                            .on_press(msg(ReportMsg::EditFixedIn(id)))
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                );
            }
        }
    }
    if let Some(body) = x.body.as_deref().filter(|b| !b.trim().is_empty()) {
        item = item.push(text(body).size(13));
    }
    item = item.push(
        row![
            text_input("Reply (they see it in My reports)", reply)
                .on_input(move |t| msg(ReportMsg::ReplyText(id, t)))
                .on_submit(msg(ReportMsg::SaveReply(id)))
                .padding(6),
            button(text("Save reply").size(13))
                .padding([5, 10])
                .style(style::ghost)
                .on_press(msg(ReportMsg::SaveReply(id))),
            button(text(if deleting { "Really delete?" } else { "Delete" }).size(13))
                .padding([5, 10])
                .style(if deleting { button::danger } else { style::ghost })
                .on_press(msg(ReportMsg::Delete(id))),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    );
    // new ones stand out
    let look = if x.status == Status::New {
        style::card_highlight
    } else {
        style::card
    };
    container(item).padding(10).width(Length::Fill).style(look).into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates_read_like_the_electron_app() {
        assert_eq!(super::date(0), "Jan 1, 1970");
        assert_eq!(super::date(1_763_190_000_000), "Nov 15, 2025");
        assert_eq!(super::date(951_782_400_000), "Feb 29, 2000"); // leap day
    }
}
