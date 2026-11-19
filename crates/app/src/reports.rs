//! The Feedback screen's state and what its buttons do: send a bug report or an idea, see your own reports (with
//! their status and replies), and for the admin, everyone's (set status, reply, delete). The requests themselves are
//! in the `feedback` crate. Its messages are wrapped in the app's `Message::Report`.

use std::collections::HashMap;

use feedback::{Change, Client, Error, Kind, NewReport, Report, Status};
use iced::Task;
use iced::widget::text_editor;

use crate::ui::anim::{self, Phase};

/// The Feedback screen's tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Send,
    Mine,
    All,
}

/// Which of everyone's reports to show (the admin)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Open,
    New,
    Bugs,
    Ideas,
    Everything,
}

/// Which of your own reports to show
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MineFilter {
    All,
    Open,
    Done,
}

impl MineFilter {
    pub const ALL: [MineFilter; 3] = [MineFilter::All, MineFilter::Open, MineFilter::Done];

    pub fn label(self) -> &'static str {
        match self {
            MineFilter::All => "All",
            MineFilter::Open => "Open",
            MineFilter::Done => "Done",
        }
    }

    pub fn keeps(self, r: &Report) -> bool {
        match self {
            MineFilter::All => true,
            MineFilter::Open => r.status.is_open(),
            MineFilter::Done => !r.status.is_open(),
        }
    }
}

impl Filter {
    pub const ALL: [Filter; 5] = [Filter::Open, Filter::New, Filter::Bugs, Filter::Ideas, Filter::Everything];

    pub fn label(self) -> &'static str {
        match self {
            Filter::Open => "Open",
            Filter::New => "New",
            Filter::Bugs => "Bugs",
            Filter::Ideas => "Ideas",
            Filter::Everything => "Everything",
        }
    }

    pub fn keeps(self, r: &Report) -> bool {
        match self {
            Filter::Open => r.status.is_open(),
            Filter::New => r.status == Status::New,
            Filter::Bugs => r.kind == Kind::Bug,
            Filter::Ideas => r.kind == Kind::Feature,
            Filter::Everything => true,
        }
    }
}

/// A list being loaded
#[derive(Debug, Clone, Default)]
pub enum Load<T> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    Failed(String),
    /// The admin's password is needed (with why, after a wrong one)
    NeedsKey(Option<String>),
}

#[derive(Debug, Clone)]
pub enum ReportMsg {
    Tab(Tab),
    Kind(Kind),
    Title(String),
    Body(text_editor::Action),
    Send,
    Sent(Result<i64, Error>),
    Mine(Result<Vec<Report>, Error>),
    All(Result<Vec<Report>, Error>),
    Filter(Filter),
    SetStatus(i64, Status),
    ReplyText(i64, String),
    SaveReply(i64),
    /// Delete asks once more ("Really delete?") before it deletes
    Delete(i64),
    /// A change saved (or not); the list is loaded again
    Changed(Result<(), Error>),
    KeyText(String),
    Unlock,
    /// The admin's count of new reports (the badge on Feedback)
    Count,
    /// The old tab's content has left: show the new one's
    TabArrive,
    MineFilter(MineFilter),
    /// Marking one done: the version it was fixed in
    FixedIn(String),
    ConfirmDone,
    CancelDone,
    /// Change the version a done one was fixed in
    EditFixedIn(i64),
    Counted(Result<Vec<Report>, Error>),
}

/// Keyring entry for the admin's password (Windows Credential Manager / the Secret Service on Linux)
fn key_entry() -> Option<keyring::Entry> {
    keyring::Entry::new("PTS Workbench", "feedback-admin").ok()
}

fn saved_key() -> Option<String> {
    key_entry()?.get_password().ok().filter(|k| !k.is_empty())
}

pub struct Reports {
    client: Client,
    reporter: String,
    pub tab: Tab,
    /// The tab whose content is shown (while one leaves, still the old one), and its transition
    pub shown_tab: Tab,
    pub tab_phase: Phase,
    pub mine_filter: MineFilter,
    /// Being marked done: which one, and the version it was fixed in (as typed)
    pub pending_done: Option<(i64, String)>,
    pub kind: Kind,
    pub title: String,
    pub body: text_editor::Content,
    pub sending: bool,
    /// The line under Send ("Sent. Thanks!", or what went wrong)
    pub sent: Option<String>,
    pub mine: Load<Vec<Report>>,
    pub all: Load<Vec<Report>>,
    pub filter: Filter,
    /// Replies being written (the admin), by report
    pub replies: HashMap<i64, String>,
    /// The report whose Delete was pressed once
    pub confirm_delete: Option<i64>,
    pub key_text: String,
    /// New reports (the admin's badge)
    pub new_count: usize,
    /// Where in the app Feedback was opened from (sent with a report)
    pub screen: String,
    /// The line at the top of the admin's list ("Saved", or what went wrong)
    pub note: Option<String>,
}

impl Reports {
    /// The feedback server's client (the app's admin mode checks its password with it)
    pub fn client(&self) -> Client {
        self.client.clone()
    }

    /// Keeps the admin's password (typed in admin mode: the Feedback list opens with it too)
    pub fn keep_key(key: &str) {
        if let Some(e) = key_entry() {
            let _ = e.set_password(key);
        }
    }

    pub fn new(client: Client, reporter: String) -> Self {
        Self {
            client,
            reporter,
            tab: Tab::Send,
            kind: Kind::Bug,
            title: String::new(),
            body: text_editor::Content::new(),
            sending: false,
            sent: None,
            mine: Load::Idle,
            all: Load::Idle,
            filter: Filter::Open,
            replies: HashMap::new(),
            confirm_delete: None,
            key_text: String::new(),
            new_count: 0,
            screen: String::new(),
            note: None,
            shown_tab: Tab::Send,
            tab_phase: Phase::Still,
            mine_filter: MineFilter::All,
            pending_done: None,
        }
    }

    /// Feedback opened from `screen`: the admin with new reports lands on them, everyone else on Send
    pub fn open(&mut self, screen: String, admin: bool) -> Task<ReportMsg> {
        self.screen = screen;
        self.sent = None;
        self.confirm_delete = None;
        let tab = if admin && self.new_count > 0 { Tab::All } else { Tab::Send };
        // opening starts on its tab (no transition)
        self.shown_tab = tab;
        self.tab_phase = Phase::Still;
        self.pending_done = None;
        self.show(tab, admin)
    }

    fn show(&mut self, tab: Tab, admin: bool) -> Task<ReportMsg> {
        // the content slides out, then the new tab's slides in (the highlight moves right away)
        let moving = if tab != self.shown_tab && !self.tab_phase.is_leaving() {
            self.tab_phase = Phase::Leaving(std::time::Instant::now());
            Task::perform(tokio::time::sleep(anim::LEAVE), |_| ReportMsg::TabArrive)
        } else {
            Task::none()
        };
        self.tab = tab;
        let load = match tab {
            Tab::Send => Task::none(),
            Tab::Mine => self.load_mine(),
            Tab::All if admin => self.load_all(),
            Tab::All => Task::none(),
        };
        Task::batch([moving, load])
    }

    /// The running version, as a default for "fixed in"
    fn this_version() -> String {
        std::env::var(launcher::ENV_VERSION).unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into())
    }

    fn fixed_in_of(&self, id: i64) -> Option<String> {
        match &self.all {
            Load::Ready(list) => list
                .iter()
                .find(|r| r.id == id)
                .and_then(|r| r.fixed_in.clone())
                .filter(|v| !v.trim().is_empty()),
            _ => None,
        }
    }

    fn load_mine(&mut self) -> Task<ReportMsg> {
        self.mine = Load::Loading;
        let (client, reporter) = (self.client.clone(), self.reporter.clone());
        Task::perform(async move { client.mine(&reporter).await }, ReportMsg::Mine)
    }

    fn load_all(&mut self) -> Task<ReportMsg> {
        let Some(key) = saved_key() else {
            self.all = Load::NeedsKey(None);
            return Task::none();
        };
        if !matches!(self.all, Load::Ready(_)) {
            self.all = Load::Loading;
        }
        let client = self.client.clone();
        Task::perform(async move { client.all(&key).await }, ReportMsg::All)
    }

    /// `name`, `version`, `platform`: what goes with a sent report
    pub fn update(&mut self, msg: ReportMsg, admin: bool, name: &str) -> Task<ReportMsg> {
        match msg {
            ReportMsg::Tab(t) => return self.show(t, admin),
            ReportMsg::Kind(k) => self.kind = k,
            ReportMsg::Title(t) => self.title = t,
            ReportMsg::Body(action) => self.body.perform(action),
            ReportMsg::Send => {
                if self.title.trim().is_empty() {
                    self.sent = Some("Give it a short title first.".into());
                    return Task::none();
                }
                if self.sending {
                    return Task::none();
                }
                self.sending = true;
                self.sent = Some("Sending…".into());
                let report = NewReport {
                    kind: self.kind,
                    title: self.title.clone(),
                    body: self.body.text(),
                    name: if name.trim().is_empty() {
                        "Someone".into()
                    } else {
                        name.trim().to_string()
                    },
                    reporter: self.reporter.clone(),
                    version: std::env::var(launcher::ENV_VERSION).unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into()),
                    platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
                    screen: self.screen.chars().take(120).collect(),
                };
                let client = self.client.clone();
                return Task::perform(async move { client.send(&report).await }, ReportMsg::Sent);
            }
            ReportMsg::Sent(r) => {
                self.sending = false;
                match r {
                    Ok(_) => {
                        self.title.clear();
                        self.body = text_editor::Content::new();
                        self.sent = Some("Sent. Thanks! You can check on it under My reports.".into());
                        if admin {
                            return Task::done(ReportMsg::Count);
                        }
                    }
                    Err(e) => self.sent = Some(format!("Couldn't send it: {e}")),
                }
            }
            ReportMsg::Mine(r) => {
                self.mine = match r {
                    Ok(list) => Load::Ready(list),
                    Err(e) => Load::Failed(format!("Couldn't load your reports: {e}")),
                }
            }
            ReportMsg::All(r) => match r {
                Ok(list) => {
                    self.new_count = list.iter().filter(|r| r.status == Status::New).count();
                    self.all = Load::Ready(list);
                }
                Err(Error::BadPassword) => self.all = Load::NeedsKey(Some("That password was wrong.".into())),
                Err(e) => self.all = Load::Failed(format!("Couldn't load feedback: {e}")),
            },
            ReportMsg::Filter(f) => self.filter = f,
            // Done asks which version fixed it first (filled in with this one, or what it said before)
            ReportMsg::SetStatus(id, Status::Done) => {
                self.pending_done = Some((id, self.fixed_in_of(id).unwrap_or_else(Self::this_version)));
            }
            ReportMsg::SetStatus(id, status) => {
                return self.change(
                    id,
                    Change {
                        status: Some(status),
                        ..Change::default()
                    },
                );
            }
            ReportMsg::EditFixedIn(id) => self.pending_done = Some((id, self.fixed_in_of(id).unwrap_or_else(Self::this_version))),
            ReportMsg::FixedIn(v) => {
                if let Some((_, text)) = &mut self.pending_done {
                    *text = v;
                }
            }
            ReportMsg::ConfirmDone => {
                if let Some((id, version)) = self.pending_done.take() {
                    return self.change(
                        id,
                        Change {
                            status: Some(Status::Done),
                            fixed_in: Some(version),
                            ..Change::default()
                        },
                    );
                }
            }
            ReportMsg::CancelDone => self.pending_done = None,
            ReportMsg::TabArrive => {
                self.shown_tab = self.tab;
                self.tab_phase = Phase::Entering(std::time::Instant::now());
            }
            ReportMsg::MineFilter(f) => self.mine_filter = f,
            ReportMsg::ReplyText(id, text) => {
                self.replies.insert(id, text);
            }
            ReportMsg::SaveReply(id) => {
                let reply = self.replies.get(&id).cloned().unwrap_or_default();
                return self.change(
                    id,
                    Change {
                        reply: Some(reply),
                        ..Change::default()
                    },
                );
            }
            ReportMsg::Delete(id) => {
                if self.confirm_delete != Some(id) {
                    self.confirm_delete = Some(id);
                    return Task::none();
                }
                self.confirm_delete = None;
                let Some(key) = saved_key() else { return Task::none() };
                let client = self.client.clone();
                return Task::perform(async move { client.delete(&key, id).await }, ReportMsg::Changed);
            }
            ReportMsg::Changed(r) => {
                self.note = Some(match r {
                    Ok(()) => "Saved".into(),
                    Err(e) => format!("Couldn't save: {e}"),
                });
                return self.load_all();
            }
            ReportMsg::KeyText(t) => self.key_text = t,
            ReportMsg::Unlock => {
                let key = self.key_text.trim().to_string();
                if key.is_empty() {
                    return Task::none();
                }
                match key_entry().map(|e| e.set_password(&key)) {
                    Some(Ok(())) => {
                        self.key_text.clear();
                        return self.load_all();
                    }
                    _ => self.all = Load::Failed("This computer can't keep the password safely (no keyring).".into()),
                }
            }
            ReportMsg::Count => {
                if !admin {
                    return Task::none();
                }
                let Some(key) = saved_key() else { return Task::none() };
                let client = self.client.clone();
                return Task::perform(async move { client.all(&key).await }, ReportMsg::Counted);
            }
            ReportMsg::Counted(r) => {
                if let Ok(list) = r {
                    self.new_count = list.iter().filter(|r| r.status == Status::New).count();
                }
            }
        }
        Task::none()
    }

    fn change(&mut self, id: i64, change: Change) -> Task<ReportMsg> {
        let Some(key) = saved_key() else {
            self.all = Load::NeedsKey(None);
            return Task::none();
        };
        let client = self.client.clone();
        Task::perform(async move { client.update(&key, id, change).await }, ReportMsg::Changed)
    }
}
