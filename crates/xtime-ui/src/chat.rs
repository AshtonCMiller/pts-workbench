//! The Dealership chat panel (on the right, from the top bar's Chat): your Xtime conversations, newest first, and one
//! conversation's messages. With a vehicle showing, "RO …" narrows the list to that RO's conversations. The data comes
//! from the `xtime` crate's chat; messages are only kept in memory.
//!
//! A conversation has a message box at the bottom: Enter sends (Shift+Enter for a new line), and typing @ offers the
//! people you can mention (↑ ↓ to choose, Enter or Tab to pick, Esc to close). Opening a conversation marks its
//! messages read in Xtime too, as Xtime's page does. Each conversation keeps its unsent text while the app is open.
//!
//! An RO with no conversation yet can have one started (the RO tab's "Start a conversation", or the Xtime View's
//! "Message about this RO"): the RO's chat channel is read from Xtime, and the first message sent there starts it, as
//! Xtime's own RO page does.

use std::collections::HashMap;
use std::sync::Arc;

use iced::keyboard::{Key, key::Named};
use iced::widget::text_editor::{Action, Binding, Content, Edit, KeyPress};
use iced::widget::{
    Column, Space, button, column, container, horizontal_space, rich_text, row, scrollable::RelativeOffset, span, text, text_editor,
};
use iced::{Alignment, Border, Color, Element, Length, Task, Theme};
use ui_kit::scroll::scrollable;
use ui_kit::{anim, pill, style};
use xtime::chat::{ChatConfig, ChatMessage, ChatPerson, conversations, to_markup};

use crate::{Env, LoadError};

/// How far back the list goes: everything from the last 14 days, and older messages (back 120 days) on your ROs
const DAYS: i64 = 14;
const RO_DAYS: i64 = 120;

/// Which conversations the list shows
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    /// The RO of the vehicle showing
    ThisRo,
}

#[derive(Debug, Default)]
pub struct ChatPanel {
    /// Every message the list read (the list shows one per conversation)
    pub messages: Vec<ChatMessage>,
    pub loading: bool,
    /// When the list last loaded (seconds since 1970)
    pub loaded_at: Option<i64>,
    pub error: Option<LoadError>,
    /// The conversation open (its channel id), and the ones read so far
    pub open: Option<String>,
    /// The conversation drawn over the list: the open one, or the last one while it slides back out
    pub showing: Option<String>,
    pub history: HashMap<String, Vec<ChatMessage>>,
    /// The conversation being read (its channel id)
    pub history_loading: Option<String>,
    pub history_error: Option<LoadError>,
    pub filter: Filter,
    /// The RO whose conversation the RO tab shows (the vehicle showing when it was picked)
    pub this_ro: Option<String>,
    /// Conversations opened here: when (milliseconds), so only messages after it count as new
    pub seen: HashMap<String, i64>,
    /// What's typed in each conversation's message box (kept until it's sent)
    pub drafts: Drafts,
    /// Messages you've sent, shown in their conversation straight away while they go to Xtime (one at a time, in
    /// order), until Xtime's copy shows up there
    pub outbox: Vec<Outgoing>,
    /// The people you can @mention (read once), and the ones picked in messages (name, Xtime's id for them)
    pub people: Vec<ChatPerson>,
    pub people_loading: bool,
    pub picked: Vec<(String, String)>,
    /// The highlighted choice in the @mention list, and an @ whose list was closed with Esc (line, column)
    pub mention_active: usize,
    pub mention_closed: Option<(usize, usize)>,
    /// Each conversation's newest message marked read in Xtime (so it isn't marked again)
    pub marked: HashMap<String, i64>,
    /// The last outgoing message's number
    pub next_key: u64,
    /// Conversations started here, not in the list yet (channel id → its RO)
    pub starting: HashMap<String, String>,
    /// An RO whose chat channel is being read from Xtime (to start its conversation), and why that didn't work
    pub finding: Option<String>,
    pub start_error: Option<String>,
}

/// A message you sent, on its way to Xtime
#[derive(Debug, Clone)]
pub struct Outgoing {
    pub key: u64,
    pub channel: String,
    pub ro: String,
    /// As typed ("@Brenton Skeeters …"), and as sent (mentions in Xtime's markup)
    pub text: String,
    pub markup: String,
    /// When it was sent (milliseconds)
    pub time: i64,
    pub state: OutState,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutState {
    /// Waiting for the one before it
    Queued,
    Sending,
    /// Xtime has it (shown until its copy turns up in the conversation)
    Sent,
    /// Didn't go: why
    Failed(String),
}

/// The message boxes' text, by conversation
#[derive(Default)]
pub struct Drafts(HashMap<String, Content>);

impl std::fmt::Debug for Drafts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.keys()).finish()
    }
}

#[derive(Debug, Clone)]
pub enum ChatMsg {
    /// Read the list again (and the open conversation)
    Refresh,
    Listed(Result<Vec<ChatMessage>, LoadError>),
    /// Open a conversation (its channel id); back to the list
    Open(String),
    Back,
    History(String, Result<Vec<ChatMessage>, LoadError>),
    Filter(Filter),
    /// The RO tab: that RO's conversation, straight away
    ThisRo(String),
    /// Start an RO's conversation (or open it, when there's one): its chat channel read from Xtime
    Start(String),
    Found(String, Result<String, LoadError>),
    /// Open the conversation's RO in the Workbench
    OpenRo(String),
    /// Typing (and moving about) in the open conversation's message box
    Edit(Action),
    /// Send what's in the message box; how it went (the outgoing message's number)
    Send,
    Sent(u64, Result<(), LoadError>),
    /// A message that didn't go: try again, or put it back in the box to change it
    Retry(u64),
    EditFailed(u64),
    /// Marked read in Xtime (the conversation, the newest message's time): how many
    MarkedRead(String, i64, Result<usize, LoadError>),
    People(Result<Vec<ChatPerson>, LoadError>),
    /// The @mention list: move the highlight, pick one (or the highlighted one), close it
    MentionMove(i32),
    MentionPick(usize),
    MentionPickActive,
    MentionClose,
}

/// What only the app can do
#[derive(Debug, Clone)]
pub enum ChatOut {
    SignedOut,
    OpenRo(String),
    /// Chat's settings (your dealer code and Xtime username), learned from Xtime: kept for next time
    Learned(ChatConfig),
}

fn scroll_id() -> iced::widget::scrollable::Id {
    iced::widget::scrollable::Id::new("chat-conversation")
}

fn now_ms() -> i64 {
    crate::now() * 1000
}

impl ChatPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Conversations with something new (for the Chat button)
    /// An RO's conversation (the newest, when there's more than one; or one started here)
    pub fn channel_for_ro(&self, ro: &str) -> Option<String> {
        self.conversations()
            .into_iter()
            .find(|c| c.ro == ro)
            .map(|c| c.channel_id)
            .or_else(|| self.starting.iter().find(|(_, r)| r.as_str() == ro).map(|(ch, _)| ch.clone()))
    }

    /// The RO a conversation's about (from the list, or one started here)
    pub fn ro_of(&self, ch: &str) -> String {
        self.conversations()
            .into_iter()
            .find(|c| c.channel_id == ch)
            .map(|c| c.ro)
            .filter(|r| !r.is_empty())
            .or_else(|| self.starting.get(ch).cloned())
            .unwrap_or_default()
    }

    pub fn unread(&self) -> usize {
        self.conversations().iter().filter(|c| c.unread).count()
    }

    /// One per conversation, newest first; unread only when something came after it was opened here
    pub fn conversations(&self) -> Vec<ChatMessage> {
        let fresh: Vec<ChatMessage> = self
            .messages
            .iter()
            .map(|m| {
                let seen = self.seen.get(&m.channel_id).copied().unwrap_or(0);
                ChatMessage {
                    unread: m.unread && m.time.unwrap_or(i64::MAX) > seen,
                    ..m.clone()
                }
            })
            .collect();
        conversations(&fresh)
    }

    /// Reads the list (your ROs: their older messages count too), unless it's loading already
    pub fn load(&mut self, env: &Env, ros: Vec<String>) -> Task<ChatMsg> {
        if self.loading {
            return Task::none();
        }
        self.loading = true;
        let (x, store) = (env.xtime.clone(), env.store.clone());
        let list = Task::perform(
            async move { x.chat_list(&store, DAYS, RO_DAYS, &ros).await.map_err(LoadError::from) },
            ChatMsg::Listed,
        );
        match self.open.clone() {
            Some(ch) => Task::batch([list, self.load_history(env, ch)]),
            None => list,
        }
    }

    fn load_history(&mut self, env: &Env, channel: String) -> Task<ChatMsg> {
        if self.history_loading.as_ref() == Some(&channel) {
            return Task::none();
        }
        self.history_loading = Some(channel.clone());
        let (x, store) = (env.xtime.clone(), env.store.clone());
        Task::perform(
            async move {
                let r = x.chat_history(&store, &channel).await.map_err(LoadError::from);
                (channel, r)
            },
            |(ch, r)| ChatMsg::History(ch, r),
        )
    }

    /// Marks the conversation read in Xtime when someone else's message in it is newer than the last one marked
    fn mark_read(&mut self, env: &Env, ch: &str) -> Task<ChatMsg> {
        let newest = self
            .history
            .get(ch)
            .into_iter()
            .flatten()
            .chain(self.messages.iter().filter(|m| m.channel_id == ch))
            .filter(|m| !m.mine && m.unread)
            .filter_map(|m| m.time)
            .max();
        let Some(newest) = newest else { return Task::none() };
        if self.marked.get(ch).is_some_and(|at| *at >= newest) {
            return Task::none();
        }
        self.marked.insert(ch.to_string(), newest);
        let (x, store, ch) = (env.xtime.clone(), env.store.clone(), ch.to_string());
        Task::perform(
            async move {
                let r = x.chat_mark_read(&store, &ch).await.map_err(LoadError::from);
                (ch, r)
            },
            move |(ch, r)| ChatMsg::MarkedRead(ch, newest, r),
        )
    }

    /// Sends the next outgoing message, when none is on its way (one at a time, so they arrive in order)
    fn pump(&mut self, env: &Env) -> Task<ChatMsg> {
        if self.outbox.iter().any(|o| o.state == OutState::Sending) {
            return Task::none();
        }
        let Some(o) = self.outbox.iter_mut().find(|o| o.state == OutState::Queued) else {
            return Task::none();
        };
        o.state = OutState::Sending;
        let (x, store) = (env.xtime.clone(), env.store.clone());
        let (key, ch, ro, markup) = (o.key, o.channel.clone(), o.ro.clone(), o.markup.clone());
        Task::perform(
            async move { x.chat_send(&store, &ch, &ro, &markup).await.map_err(LoadError::from) },
            move |r| ChatMsg::Sent(key, r),
        )
    }

    /// Sent messages whose copy has turned up in their conversation (yours, the same text, from about then) are no
    /// longer shown separately; nor ones sent over 5 minutes ago
    fn settle_outbox(&mut self, ch: &str) {
        let history = self.history.get(ch);
        let now = now_ms();
        self.outbox.retain(|o| {
            if o.channel != ch || o.state != OutState::Sent {
                return true;
            }
            let arrived = history.is_some_and(|h| {
                h.iter()
                    .any(|m| m.mine && m.text.trim() == o.text.trim() && m.time.is_none_or(|t| t >= o.time - 120_000))
            });
            !arrived && now - o.time < 5 * 60_000
        });
    }

    /// The people you can mention, read once
    fn load_people(&mut self, env: &Env) -> Task<ChatMsg> {
        if !self.people.is_empty() || self.people_loading {
            return Task::none();
        }
        self.people_loading = true;
        let (x, store) = (env.xtime.clone(), env.store.clone());
        Task::perform(async move { x.chat_roster(&store).await.map_err(LoadError::from) }, ChatMsg::People)
    }

    /// The open conversation's message box
    fn draft(&mut self) -> Option<&mut Content> {
        let ch = self.open.clone()?;
        Some(self.drafts.0.entry(ch).or_default())
    }

    /// The people matching the @ being typed in the open conversation's message box (up to 6)
    pub fn mention_choices(&self) -> Vec<&ChatPerson> {
        let Some(content) = self.open.as_ref().and_then(|ch| self.drafts.0.get(ch)) else {
            return Vec::new();
        };
        let Some((line, col, q)) = mention_at(content) else {
            return Vec::new();
        };
        if self.mention_closed == Some((line, col)) {
            return Vec::new();
        }
        self.people.iter().filter(|p| name_matches(&p.name, &q)).take(6).collect()
    }

    /// Puts the person picked in place of the @ being typed ("@bre" → "@Brenton Skeeters ")
    fn pick(&mut self, person: ChatPerson) {
        let Some(content) = self.draft() else { return };
        let Some((_, _, q)) = mention_at(content) else { return };
        for _ in 0..q.chars().count() + 1 {
            content.perform(Action::Edit(Edit::Backspace));
        }
        content.perform(Action::Edit(Edit::Paste(Arc::new(format!("@{} ", person.name)))));
        if !self.picked.iter().any(|(n, _)| *n == person.name) {
            self.picked.push((person.name, person.id));
        }
        self.mention_active = 0;
    }

    /// `ros`: your ROs' numbers (for Refresh)
    pub fn update(&mut self, msg: ChatMsg, env: &Env, ros: Vec<String>) -> (Task<ChatMsg>, Vec<ChatOut>) {
        let mut outs = Vec::new();
        let task = match msg {
            ChatMsg::Refresh => self.load(env, ros),
            ChatMsg::Listed(r) => {
                self.loading = false;
                match r {
                    Ok(list) => {
                        self.messages = list;
                        self.error = None;
                        let listed: Vec<String> = self.messages.iter().map(|m| m.channel_id.clone()).collect();
                        self.starting.retain(|ch, _| !listed.contains(ch));
                        self.loaded_at = Some(crate::now());
                        if let Some(c) = env.xtime.chat_config() {
                            outs.push(ChatOut::Learned(c));
                        }
                    }
                    Err(e) => {
                        if e == LoadError::SignedOut {
                            outs.push(ChatOut::SignedOut);
                        }
                        self.error = Some(e);
                    }
                }
                // the RO tab waiting for its RO's conversation: there now
                let waiting = self
                    .this_ro
                    .clone()
                    .filter(|_| self.filter == Filter::ThisRo && self.open.is_none());
                match waiting.and_then(|ro| self.channel_for_ro(&ro)) {
                    Some(ch) => {
                        let (task, mut more) = self.update(ChatMsg::Open(ch), env, ros);
                        outs.append(&mut more);
                        task
                    }
                    None => Task::none(),
                }
            }
            ChatMsg::Open(ch) => {
                self.open = Some(ch.clone());
                self.showing = Some(ch.clone());
                self.history_error = None;
                self.seen.insert(ch.clone(), now_ms());
                self.mention_active = 0;
                self.drafts.0.entry(ch.clone()).or_default();
                let read = self.load_history(env, ch.clone());
                let receipts = self.mark_read(env, &ch);
                let people = self.load_people(env);
                // what's been read already shows at once, at its newest
                let snap = if self.history.contains_key(&ch) {
                    iced::widget::scrollable::snap_to(scroll_id(), RelativeOffset::END)
                } else {
                    Task::none()
                };
                Task::batch([snap, read, receipts, people])
            }
            ChatMsg::Back => {
                self.open = None;
                Task::none()
            }
            ChatMsg::History(ch, r) => {
                if self.history_loading.as_ref() == Some(&ch) {
                    self.history_loading = None;
                }
                // a conversation started here has nothing to read yet (Xtime may not know it until its first message)
                let r = match r {
                    Err(e) if e != LoadError::SignedOut && self.starting.contains_key(&ch) => Ok(Vec::new()),
                    r => r,
                };
                match r {
                    Ok(list) => {
                        let grew = self.history.get(&ch).map(Vec::len) != Some(list.len());
                        self.history.insert(ch.clone(), list);
                        self.settle_outbox(&ch);
                        if self.open.as_deref() == Some(ch.as_str()) {
                            self.history_error = None;
                            self.seen.insert(ch.clone(), now_ms());
                            // new messages from others while it's open: read
                            let receipts = self.mark_read(env, &ch);
                            if grew {
                                let snap = iced::widget::scrollable::snap_to(scroll_id(), RelativeOffset::END);
                                return (Task::batch([snap, receipts]), outs);
                            }
                            return (receipts, outs);
                        }
                    }
                    Err(e) => {
                        if e == LoadError::SignedOut {
                            outs.push(ChatOut::SignedOut);
                        }
                        if self.open.as_deref() == Some(ch.as_str()) {
                            self.history_error = Some(e);
                        }
                    }
                }
                Task::none()
            }
            ChatMsg::Filter(f) => {
                self.filter = f;
                if f == Filter::All {
                    self.open = None; // (the RO's conversation slides away: the list)
                }
                Task::none()
            }
            ChatMsg::ThisRo(ro) => {
                self.filter = Filter::ThisRo;
                self.this_ro = Some(ro.clone());
                match self.channel_for_ro(&ro) {
                    Some(ch) => return self.update(ChatMsg::Open(ch), env, ros),
                    None => {
                        self.open = None; // none yet: the tab says so
                        Task::none()
                    }
                }
            }
            ChatMsg::Start(ro) => {
                self.filter = Filter::ThisRo;
                self.this_ro = Some(ro.clone());
                self.start_error = None;
                if let Some(ch) = self.channel_for_ro(&ro) {
                    return self.update(ChatMsg::Open(ch), env, ros);
                }
                if self.finding.is_some() {
                    return (Task::none(), outs);
                }
                self.open = None;
                self.finding = Some(ro.clone());
                let (x, store) = (env.xtime.clone(), env.store.clone());
                Task::perform(
                    async move {
                        let r = x.ro_view(&store, &ro).await.map(|v| v.chat_channel).map_err(LoadError::from);
                        (ro, r)
                    },
                    |(ro, r)| ChatMsg::Found(ro, r),
                )
            }
            ChatMsg::Found(ro, r) => {
                if self.finding.as_deref() == Some(ro.as_str()) {
                    self.finding = None;
                }
                match r {
                    Ok(ch) if !ch.is_empty() => {
                        self.starting.insert(ch.clone(), ro.clone());
                        // (still on this RO's tab: its new conversation, ready for the first message)
                        if self.filter == Filter::ThisRo && self.this_ro.as_deref() == Some(ro.as_str()) {
                            return self.update(ChatMsg::Open(ch), env, ros);
                        }
                    }
                    Ok(_) => self.start_error = Some(format!("Xtime doesn't have chat for RO {ro}.")),
                    Err(e) => {
                        if e == LoadError::SignedOut {
                            outs.push(ChatOut::SignedOut);
                        }
                        self.start_error = Some(format!("Couldn't start the conversation ({})", e.plain()));
                    }
                }
                Task::none()
            }
            ChatMsg::OpenRo(ro) => {
                outs.push(ChatOut::OpenRo(ro));
                Task::none()
            }
            ChatMsg::Edit(action) => {
                let editing = action.is_edit();
                if let Some(content) = self.draft() {
                    content.perform(action);
                }
                if editing {
                    self.mention_active = 0;
                }
                Task::none()
            }
            ChatMsg::Send => {
                let Some(ch) = self.open.clone() else {
                    return (Task::none(), outs);
                };
                let typed = self.drafts.0.get(&ch).map(|c| c.text()).unwrap_or_default();
                let typed = typed.trim().to_string();
                if typed.is_empty() {
                    return (Task::none(), outs);
                }
                let ro = self.ro_of(&ch);
                // in the conversation at once (marked as going), the box emptied for the next one
                self.next_key += 1;
                self.outbox.push(Outgoing {
                    key: self.next_key,
                    channel: ch.clone(),
                    ro,
                    markup: to_markup(&typed, &self.picked),
                    text: typed,
                    time: now_ms(),
                    state: OutState::Queued,
                });
                self.drafts.0.insert(ch, Content::new());
                self.mention_closed = None;
                let snap = iced::widget::scrollable::snap_to(scroll_id(), RelativeOffset::END);
                Task::batch([snap, self.pump(env)])
            }
            ChatMsg::Sent(key, r) => {
                let Some(i) = self.outbox.iter().position(|o| o.key == key) else {
                    return (self.pump(env), outs);
                };
                let ch = self.outbox[i].channel.clone();
                let mut tasks = Vec::new();
                match r {
                    Ok(()) => {
                        self.outbox[i].state = OutState::Sent;
                        // the conversation again, with Xtime's copy of it
                        if self.history_loading.as_ref() == Some(&ch) {
                            self.history_loading = None;
                        }
                        tasks.push(self.load_history(env, ch));
                    }
                    Err(e) => {
                        if e == LoadError::SignedOut {
                            outs.push(ChatOut::SignedOut);
                        }
                        self.outbox[i].state = OutState::Failed(e.plain());
                    }
                }
                tasks.push(self.pump(env));
                Task::batch(tasks)
            }
            ChatMsg::Retry(key) => {
                if let Some(o) = self.outbox.iter_mut().find(|o| o.key == key) {
                    o.state = OutState::Queued;
                    o.time = now_ms();
                }
                self.pump(env)
            }
            ChatMsg::EditFailed(key) => {
                if let Some(i) = self.outbox.iter().position(|o| o.key == key) {
                    let o = self.outbox.remove(i);
                    let draft = self.drafts.0.entry(o.channel).or_default();
                    let now = draft.text();
                    let text = if now.trim().is_empty() {
                        o.text
                    } else {
                        format!("{}\n{}", o.text, now.trim_end())
                    };
                    *draft = Content::with_text(&text);
                }
                Task::none()
            }
            ChatMsg::MarkedRead(ch, newest, r) => {
                // didn't work: tried again the next time the conversation is read
                if r.is_err() && self.marked.get(&ch) == Some(&newest) {
                    self.marked.remove(&ch);
                }
                Task::none()
            }
            ChatMsg::People(r) => {
                self.people_loading = false;
                if let Ok(p) = r {
                    self.people = p;
                }
                Task::none()
            }
            ChatMsg::MentionMove(d) => {
                let n = self.mention_choices().len() as i32;
                if n > 0 {
                    self.mention_active = (self.mention_active as i32 + d).rem_euclid(n) as usize;
                }
                Task::none()
            }
            ChatMsg::MentionPick(i) => {
                if let Some(p) = self.mention_choices().get(i).map(|p| (*p).clone()) {
                    self.pick(p);
                }
                Task::none()
            }
            ChatMsg::MentionPickActive => {
                let i = self.mention_active;
                return self.update(ChatMsg::MentionPick(i), env, ros_none());
            }
            ChatMsg::MentionClose => {
                let at = self.open.as_ref().and_then(|ch| self.drafts.0.get(ch)).and_then(mention_at);
                self.mention_closed = at.map(|(l, c, _)| (l, c));
                Task::none()
            }
        };
        (task, outs)
    }
}

fn ros_none() -> Vec<String> {
    Vec::new()
}

/// The @ being typed just before the cursor: its line and column, and what's typed after it (lowercase). An @ counts
/// at the start or after a space, followed by a name so far ("@", "@bre", "@brenton sk").
fn mention_at(content: &Content) -> Option<(usize, usize, String)> {
    let (line, col) = content.cursor_position();
    let text = content.line(line)?.to_string();
    let before: String = text.chars().take(col).collect();
    let at = before.rfind('@')?;
    if before[..at].chars().last().is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    let q = &before[at + 1..];
    let ok = q.is_empty()
        || (q.chars().next().is_some_and(char::is_alphabetic)
            && q.chars().all(|c| c.is_alphabetic() || ".'- ".contains(c))
            && q.matches(' ').count() <= 1);
    ok.then(|| (line, before[..at].chars().count(), q.to_lowercase()))
}

/// "Brenton Skeeters" matches "", "bre", "brenton s" and "ske"
fn name_matches(name: &str, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let words: Vec<String> = name.to_lowercase().split_whitespace().map(str::to_string).collect();
    (0..words.len()).any(|i| words[i..].join(" ").starts_with(q))
}

/// What the panel shows from the rest of the app
pub struct ChatShown<'a> {
    /// The RO of the vehicle showing (for "RO …")
    pub ro: Option<&'a str>,
    /// Your ROs (an RO's conversation offers to open it)
    pub my_ros: Vec<String>,
    /// You're not signed in to Xtime (there's nothing to show yet)
    pub signed_out: bool,
    pub now: i64,
}

/// "2:05 PM" today, "Tue 2:05 PM" this week, "Nov 3" before that
fn when(ms: Option<i64>, now: i64) -> String {
    use chrono::{Local, TimeZone};
    let Some(t) = ms.and_then(|ms| Local.timestamp_millis_opt(ms).earliest()) else {
        return String::new();
    };
    let Some(n) = Local.timestamp_opt(now, 0).earliest() else {
        return String::new();
    };
    let days = (n.date_naive() - t.date_naive()).num_days();
    match days {
        ..=0 => t.format("%-I:%M %p").to_string(),
        1..=6 => t.format("%a %-I:%M %p").to_string(),
        _ => t.format("%b %-d").to_string(),
    }
}

/// The first so many characters, with "…" when cut
fn short(s: &str, n: usize) -> String {
    let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    match one_line.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", one_line[..i].trim_end()),
        None => one_line,
    }
}

/// A conversation with something new in it
const UNREAD: Color = Color::from_rgb(0.23, 0.51, 0.96);
/// A mention in a message
const MENTION: Color = Color::from_rgb(0.48, 0.35, 0.96);

/// A message's words with its @mentions picked out: (words, a mention). Each mention found is in `found`.
fn mention_spans(text: &str, names: &[String], found: &mut Vec<String>) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut rest = text;
    loop {
        // the next mention: the earliest "@Name" (the longest name, when two start in the same place)
        let next = names
            .iter()
            .filter(|n| !n.is_empty())
            .filter_map(|n| rest.find(&format!("@{n}")).map(|i| (i, n)))
            .min_by_key(|(i, n)| (*i, std::cmp::Reverse(n.len())));
        let Some((i, name)) = next else { break };
        if i > 0 {
            out.push((rest[..i].to_string(), false));
        }
        let len = name.len() + 1;
        out.push((rest[i..i + len].to_string(), true));
        if !found.contains(name) {
            found.push(name.clone());
        }
        rest = &rest[i + len..];
    }
    if !rest.is_empty() {
        out.push((rest.to_string(), false));
    }
    out
}

fn note<'a>(s: impl text::IntoFragment<'a>) -> Element<'a, ChatMsg> {
    container(text(s).size(13).style(style::muted).align_x(Alignment::Center))
        .width(Length::Fill)
        .padding([24, 12])
        .center_x(Length::Fill)
        .into()
}

/// The chat: its name and Refresh, All / this RO when a vehicle's showing, then the list, with a conversation sliding
/// in over it from the right when one's opened (and back out with Back). The list stays underneath the whole time,
/// so it's still where you left it. The RO tab is that RO's conversation itself (sliding in the same way).
pub fn panel<'a>(chat: &'a ChatPanel, shown: ChatShown<'a>) -> Element<'a, ChatMsg> {
    // the RO tab: for the vehicle showing (another vehicle since: the list, until its tab's picked)
    let ro_view = chat.filter == Filter::ThisRo && shown.ro.is_some() && chat.this_ro.as_deref() == shown.ro;
    let mut head = row![text("Dealership chat").size(16).font(style::BOLD), horizontal_space()].align_y(Alignment::Center);
    if !shown.signed_out {
        head = head.push(
            button(text(if chat.loading { "Checking…" } else { "Refresh" }).size(12))
                .padding([3, 8])
                .style(style::ghost)
                .on_press_maybe((!chat.loading).then_some(ChatMsg::Refresh)),
        );
    }
    let mut top_bar = column![head].spacing(10);
    if let Some(ro) = shown.ro {
        let tab = |label: String, on: bool, msg: ChatMsg| {
            button(text(label).size(13).font(style::BOLD))
                .padding([4, 12])
                .style(style::tab(on))
                .on_press(msg)
        };
        top_bar = top_bar.push(ui_kit::anim::tabs(
            usize::from(ro_view),
            vec![
                tab("All".into(), !ro_view, ChatMsg::Filter(Filter::All)).into(),
                tab(format!("RO {ro}"), ro_view, ChatMsg::ThisRo(ro.to_string())).into(),
            ],
        ));
    }

    // (no space above: the name and tabs are just there)
    let edge = iced::Padding {
        top: 0.0,
        right: 12.0,
        bottom: 12.0,
        left: 12.0,
    };
    let base = container(list(chat, &shown)).padding(edge).width(Length::Fill).height(Length::Fill);
    let over = |content: Element<'a, ChatMsg>| -> Element<'a, ChatMsg> {
        container(content)
            .padding(edge)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|theme: &Theme| container::Style {
                // a soft shadow along its left edge as it comes in
                shadow: iced::Shadow {
                    color: Color { a: 0.22, ..Color::BLACK },
                    offset: iced::Vector::new(-2.0, 0.0),
                    blur_radius: 14.0,
                },
                ..style::panel(theme)
            })
            .into()
    };
    let top: Element<'a, ChatMsg> = match (&chat.showing, ro_view && chat.open.is_none()) {
        // the RO tab, and no conversation on it yet
        (_, true) => over(if shown.signed_out {
            note("Your Xtime conversations show here once you're signed in to Xtime.")
        } else if chat.loaded_at.is_none() {
            note("Reading your chat…")
        } else {
            start_card(chat, shown.ro.unwrap_or(""))
        }),
        (Some(ch), false) => over(conversation(chat, ch, &shown, ro_view)),
        (None, false) => Space::new(Length::Fill, Length::Fill).into(),
    };
    let slid_in = if ro_view { true } else { chat.open.is_some() };
    container(column![container(top_bar).padding([12, 12]), ui_kit::anim::push(base, top, slid_in)].height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(style::panel)
        .into()
}

/// An RO with no conversation yet: start one
fn start_card<'a>(chat: &'a ChatPanel, ro: &'a str) -> Element<'a, ChatMsg> {
    let finding = chat.finding.as_deref() == Some(ro);
    let mut col = column![
        text(format!("No messages about RO {ro} yet.")).size(14).font(style::BOLD),
        text("Start a conversation about it in Xtime's Dealership chat. @mention someone so they're notified.")
            .size(13)
            .style(style::muted)
            .align_x(Alignment::Center),
        button(
            text(if finding { "Getting it ready…" } else { "Start a conversation" })
                .size(13)
                .font(style::BOLD)
        )
        .padding([6, 16])
        .style(button::primary)
        .on_press_maybe((!finding).then(|| ChatMsg::Start(ro.to_string()))),
    ]
    .spacing(10)
    .align_x(Alignment::Center);
    if let Some(e) = &chat.start_error {
        col = col.push(text(e.as_str()).size(12).style(text::danger));
    }
    container(col).width(Length::Fill).padding([24, 12]).center_x(Length::Fill).into()
}

fn list<'a>(chat: &'a ChatPanel, shown: &ChatShown<'a>) -> Element<'a, ChatMsg> {
    if shown.signed_out {
        return note("Your Xtime conversations show here once you're signed in to Xtime.");
    }
    let convs: Vec<ChatMessage> = chat.conversations();

    let content: Element<'a, ChatMsg> = if convs.is_empty() {
        match (&chat.error, chat.loaded_at) {
            (Some(LoadError::SignedOut), _) => note("Sign in to Xtime to see your chat (the bottom bar has Sign in)."),
            (Some(e), _) => note(format!("Couldn't read your chat ({})", e.plain())),
            (None, None) => note("Reading your chat…"),
            (None, Some(_)) => note(format!("No conversations in the last {DAYS} days.")),
        }
    } else {
        let items = Column::with_children(convs.into_iter().map(|c| conversation_item(c, shown.now))).spacing(2);
        scrollable(items).height(Length::Fill).remember("chat-list").into()
    };
    let mut col = Column::new().spacing(10);
    if let (Some(e), false) = (&chat.error, chat.messages.is_empty()) {
        col = col.push(
            text(format!("Couldn't check for new messages ({})", e.plain()))
                .size(12)
                .style(text::danger),
        );
    }
    col.push(content).height(Length::Fill).into()
}

fn conversation_item<'a>(c: ChatMessage, now: i64) -> Element<'a, ChatMsg> {
    let title = if c.ro.is_empty() {
        c.sender.clone()
    } else {
        format!("RO {}", c.ro)
    };
    let first = row![
        text(title).font(style::BOLD),
        horizontal_space(),
        text(when(c.time, now)).size(12).style(style::muted)
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let who = if c.mine {
        "You: ".to_string()
    } else if c.ro.is_empty() || c.sender.is_empty() {
        String::new()
    } else {
        format!("{}: ", c.sender)
    };
    let preview = text(format!("{who}{}", short(&c.text, 90))).size(13);
    let preview = if c.unread { preview } else { preview.style(style::muted) };
    // what needs you: a light on the left, centred (it pops in when something new comes)
    // (always there, clear when there's nothing new: so it pops when something comes)
    let light: Element<'a, ChatMsg> = anim::dot(if c.unread { UNREAD } else { Color::TRANSPARENT }, false).into();
    button(
        row![light, column![first, preview].spacing(3).width(Length::Fill)]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(8)
    .style(style::list_item(false))
    .on_press(ChatMsg::Open(c.channel_id))
    .into()
}

/// A conversation: Back to the list (not as the RO tab, which is left with All), its messages, the message box
fn conversation<'a>(chat: &'a ChatPanel, ch: &'a str, shown: &ChatShown<'a>, in_tab: bool) -> Element<'a, ChatMsg> {
    // its newest from the list says what it's about until its messages are read
    let about = chat.conversations().into_iter().find(|c| c.channel_id == ch);
    let ro = chat.ro_of(ch);
    let title = if !ro.is_empty() {
        format!("RO {ro}")
    } else {
        about.as_ref().map(|c| c.sender.clone()).unwrap_or_else(|| "Conversation".into())
    };

    let mut head = row![].spacing(8).align_y(Alignment::Center);
    if !in_tab {
        head = head.push(
            button(text("‹ Back").size(13))
                .padding([3, 8])
                .style(style::ghost)
                .on_press(ChatMsg::Back),
        );
    }
    head = head.push(text(title).size(15).font(style::BOLD)).push(horizontal_space());
    // the RO, when it's one of yours (and not the one showing)
    if !ro.is_empty() && shown.my_ros.contains(&ro) && shown.ro != Some(ro.as_str()) {
        head = head.push(
            button(text("Open RO").size(12))
                .padding([3, 8])
                .style(button::primary)
                .on_press(ChatMsg::OpenRo(ro.clone())),
        );
    }

    let msgs = chat.history.get(ch);
    let outgoing: Vec<&Outgoing> = chat.outbox.iter().filter(|o| o.channel == ch).collect();
    let content: Element<'a, ChatMsg> = match msgs {
        _ if msgs.is_some_and(|l| !l.is_empty()) || !outgoing.is_empty() => {
            let mut col = Column::new().spacing(8).padding([4, 2]);
            let mut last_sender: Option<&str> = None;
            for m in msgs.into_iter().flatten() {
                // the sender's name once, over the first of their messages in a row
                let named = !m.mine && last_sender != Some(m.sender.as_str());
                last_sender = Some(if m.mine { "" } else { m.sender.as_str() });
                col = col.push(bubble(m, named, shown.now));
            }
            // yours on their way (or just arrived)
            for o in outgoing {
                col = col.push(outgoing_bubble(o, shown.now));
            }
            scrollable(col).id(scroll_id()).height(Length::Fill).into()
        }
        Some(_) if chat.starting.contains_key(ch) => note(format!(
            "New conversation about RO {ro}. Your first message starts it in Xtime; @mention someone so they're notified."
        )),
        Some(_) => note("No messages in this conversation."),
        None => match &chat.history_error {
            Some(LoadError::SignedOut) => note("Sign in to Xtime to see this conversation."),
            Some(e) => note(format!("Couldn't read this conversation ({})", e.plain())),
            None => note("Reading the conversation…"),
        },
    };

    let content = container(content).width(Length::Fill).height(Length::Fill);
    column![head, content, composer(chat, ch)].spacing(10).height(Length::Fill).into()
}

/// The message box: the @mention list over it when there's one, the box, and what's going on (or Send)
fn composer<'a>(chat: &'a ChatPanel, ch: &'a str) -> Element<'a, ChatMsg> {
    let choices = chat.mention_choices();
    let picking = !choices.is_empty();

    // The @mention list's place is always there (empty when there's no list): the box keeps its place among the
    // composer's parts, so it keeps the keyboard when the list comes and goes
    let list: Element<'a, ChatMsg> = if picking {
        let active = chat.mention_active.min(choices.len() - 1);
        let names = Column::with_children(choices.iter().enumerate().map(|(i, p)| {
            button(text(p.name.clone()).size(13))
                .width(Length::Fill)
                .padding([5, 8])
                .style(style::list_item(i == active))
                .on_press(ChatMsg::MentionPick(i))
                .into()
        }))
        .spacing(2);
        container(names).padding(4).width(Length::Fill).style(style::card).into()
    } else {
        Space::new(Length::Fill, 0).into()
    };

    // (every conversation opened has its box: see Open)
    let draft = chat.drafts.0.get(ch);
    let editor: Element<'a, ChatMsg> = match draft {
        Some(content) => container(
            text_editor(content)
                .placeholder("Message… (@ to mention someone)")
                .padding(8)
                .size(14)
                .key_binding(move |kp: KeyPress| keys(kp, picking))
                .on_action(ChatMsg::Edit),
        )
        .max_height(140)
        .into(),
        None => Space::new(Length::Fill, 0).into(),
    };

    let has_text = draft.is_some_and(|c| !c.text().trim().is_empty());
    let hint = text("Enter to send · Shift+Enter for a new line").size(11).style(style::muted);
    let send = button(text("Send").size(13).font(style::BOLD))
        .padding([5, 14])
        .style(button::primary)
        .on_press_maybe(has_text.then_some(ChatMsg::Send));
    column![
        list,
        editor,
        row![hint, horizontal_space(), send].spacing(8).align_y(Alignment::Center)
    ]
    .spacing(6)
    .into()
}

/// The keys in the message box: with the @mention list up, ↑ ↓ move, Enter / Tab pick and Esc closes it; otherwise
/// Enter sends and Shift+Enter starts a new line
fn keys(kp: KeyPress, picking: bool) -> Option<Binding<ChatMsg>> {
    match kp.key.as_ref() {
        Key::Named(Named::ArrowDown) if picking => Some(Binding::Custom(ChatMsg::MentionMove(1))),
        Key::Named(Named::ArrowUp) if picking => Some(Binding::Custom(ChatMsg::MentionMove(-1))),
        Key::Named(Named::Enter | Named::Tab) if picking => Some(Binding::Custom(ChatMsg::MentionPickActive)),
        Key::Named(Named::Escape) if picking => Some(Binding::Custom(ChatMsg::MentionClose)),
        Key::Named(Named::Enter) if !kp.modifiers.shift() => Some(Binding::Custom(ChatMsg::Send)),
        _ => Binding::from_key_press(kp),
    }
}

/// One message: yours on the right in the accent colour, others' on the left; Xtime's own notices plain and centred
fn bubble<'a>(m: &'a ChatMessage, named: bool, now: i64) -> Element<'a, ChatMsg> {
    let time = text(when(m.time, now)).size(11).style(style::muted);
    if m.notification {
        return container(
            column![text(m.text.as_str()).size(12).style(style::muted).align_x(Alignment::Center), time].align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .center_x(Length::Fill)
        .padding([2, 16])
        .into();
    }
    let mine = m.mine;
    // @mentions stand out: bold, on a tint of their own
    let mut found = Vec::new();
    let words: Vec<text::Span<'a, ChatMsg>> = mention_spans(&m.text, &m.mentions, &mut found)
        .into_iter()
        .map(|(w, mention)| {
            if !mention {
                return span(w);
            }
            let tint = if mine { Color::WHITE } else { MENTION };
            span(w)
                .font(style::BOLD)
                .color(if mine { Color::WHITE } else { MENTION })
                .background(Color {
                    a: if mine { 0.22 } else { 0.14 },
                    ..tint
                })
                .border(Border {
                    radius: 4.0.into(),
                    ..Default::default()
                })
                .padding([0, 2])
        })
        .collect();
    let body = container(rich_text(words).size(14))
        .padding([7, 11])
        .max_width(250)
        .style(move |theme: &Theme| {
            let c = style::colors(theme);
            container::Style {
                background: Some(if mine { theme.palette().primary } else { c.hover }.into()),
                text_color: Some(if mine { Color::WHITE } else { theme.palette().text }),
                border: Border {
                    radius: 12.0.into(),
                    width: if mine { 0.0 } else { 1.0 },
                    color: c.line,
                },
                ..Default::default()
            }
        });
    let mut col = Column::new().spacing(3);
    if named && !m.sender.is_empty() {
        col = col.push(text(m.sender.as_str()).size(12).font(style::BOLD).style(style::muted));
    }
    let mut tags = row![time].spacing(6).align_y(Alignment::Center);
    // (a mention not found in the words, as Xtime wrote them: under it)
    let unseen: Vec<&String> = m.mentions.iter().filter(|n| !found.contains(n)).collect();
    if !unseen.is_empty() && !mine {
        tags = tags.push(pill(
            format!("@ {}", unseen.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(", ")),
            [0x7a, 0x5a, 0xf5],
        ));
    }
    col = col.push(body).push(tags);
    let col = if mine { col.align_x(Alignment::End) } else { col };
    let r = if mine {
        row![horizontal_space(), col]
    } else {
        row![col, horizontal_space()]
    };
    r.width(Length::Fill).into()
}

/// A message you sent, as it goes: your bubble (faded until Xtime has it) with how it's going under it; one that
/// didn't go says why, with Retry and Edit
fn outgoing_bubble<'a>(o: &'a Outgoing, now: i64) -> Element<'a, ChatMsg> {
    let pending = !matches!(o.state, OutState::Sent);
    let failed = matches!(o.state, OutState::Failed(_));
    let body = container(text(o.text.as_str()).size(14))
        .padding([7, 11])
        .max_width(250)
        .style(move |theme: &Theme| {
            let p = theme.palette();
            let bg = if failed {
                Color { a: 0.15, ..p.danger }
            } else if pending {
                Color { a: 0.55, ..p.primary }
            } else {
                p.primary
            };
            container::Style {
                background: Some(bg.into()),
                text_color: Some(if failed { p.text } else { Color::WHITE }),
                border: Border {
                    radius: 12.0.into(),
                    width: if failed { 1.0 } else { 0.0 },
                    color: p.danger,
                },
                ..Default::default()
            }
        });
    let status: Element<'a, ChatMsg> = match &o.state {
        OutState::Queued | OutState::Sending => row![text("Sending…").size(11).style(style::muted),].into(),
        OutState::Sent => text(when(Some(o.time), now)).size(11).style(style::muted).into(),
        OutState::Failed(why) => {
            let small = |label: &'static str, msg: ChatMsg| button(text(label).size(11)).padding([1, 8]).style(style::ghost).on_press(msg);
            row![
                text(format!("Not sent ({why})")).size(11).style(text::danger),
                small("Retry", ChatMsg::Retry(o.key)),
                small("Edit", ChatMsg::EditFailed(o.key)),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .wrap()
            .into()
        }
    };
    let col = column![body, status].spacing(3).align_x(Alignment::End);
    row![horizontal_space(), col].width(Length::Fill).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_names_as_you_type() {
        assert!(name_matches("Brenton Skeeters", ""));
        assert!(name_matches("Brenton Skeeters", "bre"));
        assert!(name_matches("Brenton Skeeters", "brenton sk"));
        assert!(name_matches("Brenton Skeeters", "ske"));
        assert!(!name_matches("Brenton Skeeters", "eet"));
    }

    #[test]
    fn finds_the_mention_being_typed() {
        let at = |t: &str| {
            let mut c = Content::with_text(t);
            c.perform(Action::Move(text_editor::Motion::DocumentEnd));
            mention_at(&c)
        };
        assert_eq!(at("hi @bre"), Some((0, 3, "bre".into())));
        assert_eq!(at("@"), Some((0, 0, String::new())));
        assert_eq!(at("hi @Brenton Sk"), Some((0, 3, "brenton sk".into())));
        assert_eq!(at("mail a@b"), None);
        assert_eq!(at("hi @bre and more words"), None);
    }

    #[test]
    fn shortens_previews() {
        assert_eq!(short("one  two\nthree", 50), "one two three");
        assert_eq!(short("abcdef", 3), "abc…");
    }

    #[test]
    fn opened_conversations_stop_counting() {
        let m = |ch: &str, t: i64| ChatMessage {
            channel_id: ch.into(),
            time: Some(t),
            unread: true,
            ..Default::default()
        };
        let mut p = ChatPanel::new();
        p.messages = vec![m("a", 10), m("b", 20)];
        assert_eq!(p.unread(), 2);
        p.seen.insert("a".into(), 15);
        assert_eq!(p.unread(), 1);
        p.messages.push(m("a", 30));
        assert_eq!(p.unread(), 2);
    }

    #[test]
    fn picks_out_mentions() {
        let names = vec!["Brenton".to_string(), "Brenton Skeeters".to_string(), "Ann".to_string()];
        let mut found = Vec::new();
        let s = mention_spans("hey @Brenton Skeeters and @Ann, see @Bob", &names, &mut found);
        assert_eq!(
            s,
            vec![
                ("hey ".to_string(), false),
                ("@Brenton Skeeters".to_string(), true),
                (" and ".to_string(), false),
                ("@Ann".to_string(), true),
                (", see @Bob".to_string(), false),
            ]
        );
        assert_eq!(found, vec!["Brenton Skeeters".to_string(), "Ann".to_string()]);
    }
}
