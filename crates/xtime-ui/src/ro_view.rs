//! The Xtime View's state and what changes it: reading the RO (and its inspection form), opening lines, Done, the
//! inspection form's items and forms, deleting a line, dragging lines to reorder them (and recommendations under a
//! concern, or out from under one). Its look: view.rs and inspection.rs.
//!
//! The app keeps one [`XtimeView`] per open vehicle and sends it [`Msg`]s for that RO. What only the app can do comes
//! back from [`XtimeView::update`] as [`Out`]s: a notice for the bottom bar, "signed out", the RO as it was just read
//! (for your hours and spiff totals), and the buttons meant for the app ([`Ask`]: Add Service, Flag, Print Spiff…).

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use iced::Task;
use xtime::Xtime;
use xtime::inspect::{FormChoice, InspectionForm, Press, RedService};
use xtime::ro::{Place, RoView, same_id};
use xtime::stage::{Check, Move, Moved, Rating, RatingAsk};

use crate::LoadError;

/// What the Xtime View asks Xtime with
#[derive(Clone)]
pub struct Env {
    pub xtime: Xtime,
    pub store: String,
    /// Your technician ID (whose ROs are yours, for Send to Approval / Review)
    pub tech_id: String,
    /// The app is showing its sample ROs (they aren't in Xtime: nothing is read)
    pub sample: bool,
}

#[derive(Debug, Clone, Default)]
pub struct XtimeView {
    /// The RO number
    pub ro: String,
    pub view: Option<RoView>,
    pub loading: bool,
    /// Why the last read didn't work (the last good RO stays shown)
    pub error: Option<LoadError>,
    /// When it was last read (seconds since 1970)
    pub at: Option<i64>,
    /// Lines shown opened (by line ID), and when each was opened (for its slide in)
    pub open: HashMap<String, Instant>,
    pub show_inspection: bool,
    /// Lines whose Done (or Delete) is being sent to Xtime
    pub busy: HashSet<String>,
    /// A line asked to be deleted (pressed once; Xtime's line ID)
    pub deleting: Option<String>,
    /// The RO's inspection form (read with the RO), and its state
    pub form: Option<InspectionForm>,
    pub form_loading: bool,
    pub form_error: Option<LoadError>,
    /// Items being saved to Xtime (by item ID)
    pub saving: HashSet<String>,
    /// What's typed in items' value boxes, not saved yet (by item ID)
    pub drafts: HashMap<String, String>,
    /// Another form picked in the drop-down (switched with Switch form)
    pub switch_to: Option<FormChoice>,
    pub switching: bool,
    /// The moves Xtime offers you on this RO (Send to Approval, Send to Review…), worked out as Xtime's page does
    pub stage: Option<Check>,
    pub stage_loading: bool,
    /// A move pressed (asked once more), the one being made, and the rating Xtime wants first (with what's picked)
    pub confirm: Option<Move>,
    pub moving: Option<Move>,
    pub rating: Option<RatingState>,
    /// A line being dragged (and where it would land if let go now)
    pub drag: Option<Drag>,
    /// A line whose move is being sent to Xtime (its line ID)
    pub line_moving: Option<String>,
    /// The RO being read when a move was made: what comes back is from before it (read again)
    stale: bool,
}

/// A line being dragged by its handle
#[derive(Debug, Clone)]
pub struct Drag {
    /// Its line ID
    pub key: String,
    /// A customer concern (they only go among the concerns; recommendations go under one, or on their own)
    pub primary: bool,
    pub target: Option<Target>,
    /// When it was picked up, and when the target last changed (for their animations)
    pub since: Instant,
    pub target_since: Instant,
}

/// Where a dragged line would land: in one of Xtime's lists, before a line (its ID), or at the end
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub place: Place,
    pub before: Option<String>,
}

impl Drag {
    /// Whether this line can be dropped there
    pub fn fits(&self, place: &Place) -> bool {
        match place {
            Place::Concerns => self.primary,
            Place::Under(c) => !self.primary && !same_id(c, &serde_json::Value::String(self.key.clone())),
            Place::Recommendations => !self.primary,
        }
    }
}

/// The store's "Repair Order Quality" rating of the advisor, asked before the RO goes on
#[derive(Debug, Clone, Default)]
pub struct RatingState {
    pub mv: Option<Move>,
    pub ask: RatingAsk,
    /// 1–5 (0: none yet)
    pub stars: u8,
    pub option: Option<String>,
    pub text: String,
}

impl RatingState {
    /// Whether it can be sent (as Xtime's: 3 stars or fewer need a reason, picked or typed)
    pub fn ready(&self) -> bool {
        if self.stars == 0 {
            return false;
        }
        if self.stars > 3 {
            return true;
        }
        if self.ask.drop_down {
            return match self.option.as_deref() {
                Some("Other") => !self.text.trim().is_empty(),
                Some(_) => true,
                None => false,
            };
        }
        !self.ask.text_field || !self.text.trim().is_empty()
    }

    fn reason(&self) -> String {
        if self.stars > 3 {
            return String::new();
        }
        match self.option.as_deref() {
            Some(o) if self.ask.drop_down && o != "Other" => o.to_string(),
            _ => self.text.trim().to_string(),
        }
    }
}

/// What the Xtime View does
#[derive(Debug, Clone)]
pub enum Msg {
    /// Read the RO from Xtime (`true`: even if it was read moments ago)
    Load(bool),
    Loaded(Result<RoView, LoadError>),
    /// Open or close a line (by line ID)
    ToggleLine(String),
    ToggleInspection,
    /// Mark a line done or not (by line ID), as Xtime's Done button does; Xtime's answer
    SetDone(String, bool),
    DoneSaved(String, bool, Result<(), LoadError>),
    /// Delete a line (asks first: press twice); Xtime's answer
    DeleteLine(String),
    LineDeleted(String, Result<(), LoadError>),
    /// The inspection form: read it; something pressed on an item (its ID); Xtime's answer (item ID, the colour and
    /// value saved, what a Red one did); typing in a value box; another form picked, switching to it
    LoadForm,
    FormLoaded(Result<InspectionForm, LoadError>),
    PressItem(String, Press),
    ItemSaved(String, String, String, Result<RedService, LoadError>),
    ItemDraft(String, String),
    PickForm(FormChoice),
    SwitchForm,
    FormSwitched(Result<(), LoadError>),
    /// The moves Xtime offers (read with the RO); a move pressed (asked once more: None takes the question away),
    /// made, and how it went
    StageLoaded(Result<Check, LoadError>),
    AskMove(Option<Move>),
    MakeMove(Move),
    Moved(Move, Result<Moved, LoadError>),
    /// The rating Xtime asks for first: stars, a reason picked, a reason typed, Send, Cancel
    RateStars(u8),
    RateOption(String),
    RateText(String),
    SendRating,
    CancelRating,
    /// Dragging a line: picked up by its handle (its line ID); over somewhere it can go; off the lists; let go
    /// (`true`: dropped where it's over; `false`: Escape, put back); Xtime's answer to the move
    DragStart(String),
    DragOver(Target),
    DragOff,
    DragEnd(bool),
    LineMoved(String, Result<(), LoadError>),
    /// A button meant for the app
    Ask(Ask),
}

/// Buttons on the Xtime View that the app answers
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// Add Service (for an inspection item: its ID)
    AddService(Option<String>),
    /// Edit Service for a line (Xtime's line ID)
    EditService(String),
    /// Flag a line for your hours, or take the flag off (Xtime's line ID)
    Flag(String, bool),
    PrintSpiff,
    /// Mark the RO spiffed, or not
    MarkSpiffed(bool),
    /// Copy something shown: the text, and what it is ("Part number")
    Copy(String, String),
    /// Message about this RO: the chat panel, on its conversation (started if there's none)
    Chat,
}

/// What the app does after an update
#[derive(Debug, Clone)]
pub enum Out {
    /// A message for the bottom bar
    Notice(String),
    /// Xtime has signed you out (the bottom bar offers to sign in)
    SignedOut,
    /// The RO was just read (the app keeps its lines in your hours, and its spiff total)
    Read(RoView),
    /// The RO was moved on to this stage here (the app doesn't notify you of your own move)
    Moved(i64),
    Ask(Ask),
}

type Step = (Task<Msg>, Vec<Out>);

fn just(out: Vec<Out>) -> Step {
    (Task::none(), out)
}

impl XtimeView {
    pub fn new(ro: String) -> Self {
        Self { ro, ..Default::default() }
    }

    /// Reads the RO, unless it's being read, or was read in the last 20 s (unless `force`)
    pub fn load(&mut self, env: &Env, force: bool) -> Task<Msg> {
        if env.sample {
            return Task::none(); // the samples aren't in Xtime
        }
        let fresh = self.at.is_some_and(|at| crate::now() - at < 20) && self.error.is_none();
        if self.loading || (fresh && !force) {
            return Task::none();
        }
        self.loading = true;
        let (x, store, ro) = (env.xtime.clone(), env.store.clone(), self.ro.clone());
        Task::perform(async move { x.ro_view(&store, &ro).await.map_err(LoadError::from) }, Msg::Loaded)
    }

    /// Reads the inspection form (not while it's being read, or an item's being saved or typed)
    pub fn load_form(&mut self, env: &Env) -> Task<Msg> {
        if env.sample || self.form_loading || !self.saving.is_empty() || !self.drafts.is_empty() {
            return Task::none();
        }
        self.form_loading = true;
        let (x, store, ro) = (env.xtime.clone(), env.store.clone(), self.ro.clone());
        Task::perform(
            async move { x.inspection(&store, &ro).await.map_err(LoadError::from) },
            Msg::FormLoaded,
        )
    }

    fn letter(&self, line_id: &str) -> String {
        self.view
            .as_ref()
            .and_then(|v| v.lines.iter().find(|l| l.key() == line_id))
            .map(|l| l.dms.clone())
            .unwrap_or_default()
    }

    /// Reads the moves Xtime offers you (only in Inspection and Repair, the stages a technician moves an RO on from)
    pub fn load_stage(&mut self, env: &Env) -> Task<Msg> {
        let mode = self.view.as_ref().map(|v| v.mode).unwrap_or(0);
        if env.sample || self.stage_loading || self.moving.is_some() || !matches!(mode, 3 | 6) {
            if !matches!(mode, 3 | 6) {
                self.stage = None;
            }
            return Task::none();
        }
        self.stage_loading = true;
        let (x, store, ro, tech) = (env.xtime.clone(), env.store.clone(), self.ro.clone(), env.tech_id.clone());
        Task::perform(
            async move { x.stage_check(&store, &ro, &tech).await.map_err(LoadError::from) },
            Msg::StageLoaded,
        )
    }

    pub fn update(&mut self, msg: Msg, env: &Env) -> Step {
        // a line done, or an inspection item saved: what Xtime offers may have changed
        let restage = matches!(&msg, Msg::DoneSaved(_, _, Ok(_)) | Msg::ItemSaved(_, _, _, Ok(_)));
        let (task, outs) = self.step(msg, env);
        if restage {
            return (Task::batch([task, self.load_stage(env)]), outs);
        }
        (task, outs)
    }

    /// A line dropped: moved here right away, then in Xtime (its own RO page's way: the list it's in, where in it)
    fn move_line(&mut self, env: &Env, key: &str, t: Target) -> Step {
        let Some(v) = self.view.as_mut() else { return just(vec![]) };
        let Some(line) = v.lines.iter().find(|l| l.key() == key) else {
            return just(vec![]);
        };
        let id = line.id.clone();
        let was_group = line.group_id.clone();
        // where it lands, as Xtime counts it: before a line, or after the last one shown in that list
        let shown: Vec<serde_json::Value> = shown_in(v, &t.place).into_iter().map(|l| l.id.clone()).collect();
        let before = t
            .before
            .as_ref()
            .and_then(|b| v.lines.iter().find(|l| &l.key() == b))
            .map(|l| l.id.clone());
        let order = v.drop_order(&id, &t.place, before.as_ref(), &shown.iter().collect::<Vec<_>>());
        let group = match &t.place {
            Place::Under(c) => Some(c.clone()),
            _ => None,
        };
        // nothing changes: nothing sent
        let same_list = match (&group, &was_group) {
            (Some(a), Some(b)) => same_id(a, b),
            (None, g) => g.as_ref().is_none_or(|g| g.is_null()),
            _ => false,
        };
        let now_at = v.xtime_list(&t.place).iter().position(|l| same_id(&l.id, &id)).map(|i| i + 1);
        if same_list && now_at == Some(order) {
            return just(vec![]);
        }
        // on screen now: under its new concern (or none), in its new place
        let from = v.lines.iter().position(|l| same_id(&l.id, &id)).expect("found above");
        let mut moved = v.lines.remove(from);
        moved.group_id = group.clone();
        let to = match &before {
            Some(b) => v.lines.iter().position(|l| same_id(&l.id, b)).unwrap_or(v.lines.len()),
            None => shown
                .iter()
                .filter(|s| !same_id(s, &id))
                .filter_map(|s| v.lines.iter().position(|l| same_id(&l.id, s)))
                .max()
                .map_or_else(|| first_slot(v, &t.place), |i| i + 1),
        };
        v.lines.insert(to.min(v.lines.len()), moved);
        // and in Xtime
        self.line_moving = Some(key.to_string());
        self.busy.insert(key.to_string());
        if self.loading {
            self.stale = true;
        }
        let (x, store, ro, key) = (env.xtime.clone(), env.store.clone(), self.ro.clone(), key.to_string());
        (
            Task::perform(
                async move { x.move_line(&store, &ro, &id, group.as_ref(), order).await.map_err(LoadError::from) },
                move |r| Msg::LineMoved(key.clone(), r),
            ),
            vec![],
        )
    }

    fn make_move(&mut self, env: &Env, mv: Move, rating: Option<Rating>) -> Task<Msg> {
        self.confirm = None;
        self.moving = Some(mv);
        let (x, store, ro, tech) = (env.xtime.clone(), env.store.clone(), self.ro.clone(), env.tech_id.clone());
        Task::perform(
            async move { x.move_ro(&store, &ro, &tech, mv, rating).await.map_err(LoadError::from) },
            move |r| Msg::Moved(mv, r),
        )
    }

    fn step(&mut self, msg: Msg, env: &Env) -> Step {
        match msg {
            Msg::Load(force) => return (self.load(env, force), vec![]),
            Msg::Loaded(result) => {
                self.loading = false;
                // read before (or while) a line was moved: not what Xtime has now
                if self.stale || self.line_moving.is_some() {
                    self.stale = false;
                    let again = if self.line_moving.is_none() {
                        self.load(env, true)
                    } else {
                        Task::none()
                    };
                    return (again, vec![]);
                }
                match result {
                    Ok(view) => {
                        self.error = None;
                        self.at = Some(crate::now());
                        self.view = Some(view.clone());
                        // read with the RO: its inspection form, and the moves Xtime offers you
                        let stage = self.load_stage(env);
                        return (Task::batch([self.load_form(env), stage]), vec![Out::Read(view)]);
                    }
                    Err(e) => {
                        let out = if e == LoadError::SignedOut { vec![Out::SignedOut] } else { vec![] };
                        self.error = Some(e);
                        return just(out);
                    }
                }
            }
            Msg::ToggleLine(key) => {
                if self.open.remove(&key).is_none() {
                    self.open.insert(key, Instant::now());
                }
            }
            Msg::ToggleInspection => self.show_inspection = !self.show_inspection,
            Msg::SetDone(key, done) => {
                if self.busy.contains(&key) {
                    return just(vec![]);
                }
                let Some(line) = self.view.as_mut().and_then(|v| v.lines.iter_mut().find(|l| l.key() == key)) else {
                    return just(vec![]);
                };
                if !line.can_be_done() {
                    return just(vec![]); // declined, or waiting on approval: the work can't be done
                }
                line.completed = done; // the box changes right away; it goes back if Xtime says no
                let id = line.id.clone();
                self.busy.insert(key.clone());
                let (x, store, ro) = (env.xtime.clone(), env.store.clone(), self.ro.clone());
                return (
                    Task::perform(
                        async move { x.set_done(&store, &ro, &id, done).await.map_err(LoadError::from) },
                        move |r| Msg::DoneSaved(key.clone(), done, r),
                    ),
                    vec![],
                );
            }
            Msg::DoneSaved(key, done, result) => {
                self.busy.remove(&key);
                let letter = self.letter(&key);
                let mut out = Vec::new();
                match result {
                    Ok(()) => out.push(Out::Notice(match (done, letter.is_empty()) {
                        (true, false) => format!("Line {letter} marked done"),
                        (false, false) => format!("Line {letter} not done"),
                        (true, true) => "Line marked done".into(),
                        (false, true) => "Line not done".into(),
                    })),
                    Err(e) => {
                        // put the box back as Xtime has it
                        if let Some(line) = self.view.as_mut().and_then(|v| v.lines.iter_mut().find(|l| l.key() == key)) {
                            line.completed = !done;
                        }
                        if e == LoadError::SignedOut {
                            out.push(Out::SignedOut);
                        }
                        out.push(Out::Notice(format!("Couldn't change the line in Xtime ({})", e.plain())));
                    }
                }
                // read it again: Xtime may have changed more (the RO's stage, who did it)
                return (self.load(env, true), out);
            }
            Msg::DeleteLine(key) => {
                // the first press asks
                if self.deleting.as_deref() != Some(key.as_str()) {
                    self.deleting = Some(key);
                    return just(vec![]);
                }
                let Some(id) = self
                    .view
                    .as_ref()
                    .and_then(|v| v.lines.iter().find(|l| l.key() == key))
                    .map(|l| l.id.clone())
                else {
                    return just(vec![]);
                };
                self.busy.insert(key.clone());
                let (x, store, ro) = (env.xtime.clone(), env.store.clone(), self.ro.clone());
                return (
                    Task::perform(
                        async move { x.delete_line(&store, &ro, &id).await.map_err(LoadError::from) },
                        move |r| Msg::LineDeleted(key.clone(), r),
                    ),
                    vec![],
                );
            }
            Msg::LineDeleted(key, result) => {
                let letter = self.letter(&key);
                self.busy.remove(&key);
                self.deleting = None;
                return match result {
                    Ok(()) => {
                        let notice = if letter.is_empty() {
                            "Deleted the service".to_string()
                        } else {
                            format!("Deleted line {letter}")
                        };
                        (self.load(env, true), vec![Out::Notice(notice)])
                    }
                    Err(e) => just(vec![Out::Notice(format!("Couldn't delete the service ({})", e.plain()))]),
                };
            }
            Msg::LoadForm => return (self.load_form(env), vec![]),
            Msg::FormLoaded(result) => {
                self.form_loading = false;
                match result {
                    // not while something's being saved or typed: that would undo it on screen
                    Ok(f) if self.saving.is_empty() && self.drafts.is_empty() => {
                        self.form = Some(f);
                        self.form_error = None;
                    }
                    Ok(_) => {}
                    Err(e) => self.form_error = Some(e),
                }
            }
            Msg::PressItem(key, press) => {
                if self.saving.contains(&key) {
                    return just(vec![]);
                }
                let Some(item) = self.form.as_ref().and_then(|f| f.item(&key)) else {
                    return just(vec![]);
                };
                let Some((color, value)) = xtime::inspect::next_answer(item, &press) else {
                    self.drafts.remove(&key);
                    return just(vec![]);
                };
                let id = item.id.clone();
                self.saving.insert(key.clone());
                let (x, store, ro) = (env.xtime.clone(), env.store.clone(), self.ro.clone());
                return (
                    Task::perform(
                        async move {
                            let r = x.save_item(&store, &ro, &id, &color, &value).await.map_err(LoadError::from);
                            (key, color, value, r)
                        },
                        |(key, color, value, r)| Msg::ItemSaved(key, color, value, r),
                    ),
                    vec![],
                );
            }
            Msg::ItemSaved(key, color, value, result) => {
                self.saving.remove(&key);
                match result {
                    Ok(red) => {
                        self.drafts.remove(&key);
                        if let Some(item) = self.form.as_mut().and_then(|f| f.item_mut(&key)) {
                            item.color = color;
                            item.value = value;
                            item.answered = true;
                        }
                        match red {
                            RedService::Added => {
                                // Xtime added the item's service: read the RO again to show it
                                return (self.load(env, true), vec![Out::Notice("Saved · Xtime added its service".into())]);
                            }
                            RedService::NotAdded(why) => {
                                return just(vec![Out::Notice(format!("Saved, but Xtime didn't add its service ({why})"))]);
                            }
                            RedService::None => {}
                        }
                    }
                    Err(e) => {
                        let mut out = vec![Out::Notice(format!("Couldn't save to Xtime ({})", e.plain()))];
                        if e == LoadError::SignedOut {
                            out.push(Out::SignedOut);
                        }
                        return just(out);
                    }
                }
            }
            Msg::ItemDraft(key, text) => {
                self.drafts.insert(key, text);
            }
            Msg::PickForm(choice) => self.switch_to = Some(choice),
            Msg::SwitchForm => {
                let Some(to) = self.switch_to.clone().filter(|_| !self.switching) else {
                    return just(vec![]);
                };
                self.switching = true;
                let (x, store, ro) = (env.xtime.clone(), env.store.clone(), self.ro.clone());
                return (
                    Task::perform(
                        async move { x.switch_form(&store, &ro, &to.id).await.map_err(LoadError::from) },
                        Msg::FormSwitched,
                    ),
                    vec![],
                );
            }
            Msg::FormSwitched(result) => {
                self.switching = false;
                return match result {
                    Ok(()) => {
                        let name = self.switch_to.take().map(|f| f.name).unwrap_or_default();
                        self.form = None;
                        self.drafts.clear();
                        (self.load_form(env), vec![Out::Notice(format!("Switched to {name}"))])
                    }
                    Err(e) => just(vec![Out::Notice(format!("Couldn't switch the form ({})", e.plain()))]),
                };
            }
            Msg::StageLoaded(r) => {
                self.stage_loading = false;
                match r {
                    Ok(c) => self.stage = Some(c),
                    Err(LoadError::SignedOut) => return just(vec![Out::SignedOut]),
                    // (the buttons stay as they were; the next read tries again)
                    Err(_) => {}
                }
            }
            Msg::AskMove(mv) => self.confirm = mv,
            Msg::MakeMove(mv) => {
                if self.moving.is_some() || env.sample {
                    return just(vec![]);
                }
                return (self.make_move(env, mv, None), vec![]);
            }
            Msg::Moved(mv, r) => {
                self.moving = None;
                let ro = self.ro.clone();
                return match r {
                    Ok(Moved::Done) => {
                        self.rating = None;
                        self.stage = None;
                        let said = match mv.target() {
                            Some(4) => format!("RO {ro} sent to Approval"),
                            Some(6) => format!("RO {ro} is in Repair"),
                            Some(7) => format!("RO {ro} sent to Review"),
                            _ => format!("RO {ro}: started working"),
                        };
                        let mut outs = vec![Out::Notice(said)];
                        if let Some(m) = mv.target() {
                            outs.push(Out::Moved(m));
                        }
                        (self.load(env, true), outs)
                    }
                    Ok(Moved::NeedsRating(ask)) => {
                        self.rating = Some(RatingState {
                            mv: Some(mv),
                            ask,
                            ..Default::default()
                        });
                        just(vec![])
                    }
                    Err(LoadError::SignedOut) => just(vec![Out::SignedOut]),
                    Err(e) => {
                        let task = self.load_stage(env);
                        (task, vec![Out::Notice(format!("Couldn't move RO {ro} on ({})", e.plain()))])
                    }
                };
            }
            Msg::RateStars(n) => {
                if let Some(r) = &mut self.rating {
                    r.stars = n.clamp(1, 5);
                }
            }
            Msg::RateOption(o) => {
                if let Some(r) = &mut self.rating {
                    r.option = Some(o);
                }
            }
            Msg::RateText(t) => {
                if let Some(r) = &mut self.rating {
                    r.text = t.chars().take(500).collect();
                }
            }
            Msg::SendRating => {
                let Some(r) = self.rating.clone().filter(RatingState::ready) else {
                    return just(vec![]);
                };
                let Some(mv) = r.mv else { return just(vec![]) };
                if self.moving.is_some() {
                    return just(vec![]);
                }
                let rating = Rating {
                    target_user: r.ask.target_user.clone(),
                    stars: r.stars,
                    reason: r.reason(),
                };
                return (self.make_move(env, mv, Some(rating)), vec![]);
            }
            Msg::CancelRating => self.rating = None,
            Msg::DragStart(key) => {
                let Some(line) = self.view.as_ref().and_then(|v| v.lines.iter().find(|l| l.key() == key)) else {
                    return just(vec![]);
                };
                if env.sample || self.line_moving.is_some() || self.busy.contains(&key) || line.is_inspection() {
                    return just(vec![]);
                }
                let now = Instant::now();
                self.drag = Some(Drag {
                    primary: line.primary,
                    key,
                    target: None,
                    since: now,
                    target_since: now,
                });
            }
            Msg::DragOver(t) => {
                if let Some(d) = self.drag.as_mut().filter(|d| d.fits(&t.place) && d.target.as_ref() != Some(&t)) {
                    // (over itself: it stays where it is)
                    d.target = (t.before.as_deref() != Some(d.key.as_str())).then_some(t);
                    d.target_since = Instant::now();
                }
            }
            Msg::DragOff => {
                if let Some(d) = self.drag.as_mut().filter(|d| d.target.is_some()) {
                    d.target = None;
                    d.target_since = Instant::now();
                }
            }
            Msg::DragEnd(drop) => {
                let Some(d) = self.drag.take() else { return just(vec![]) };
                let Some(t) = d.target.filter(|_| drop) else { return just(vec![]) };
                return self.move_line(env, &d.key, t);
            }
            Msg::LineMoved(key, result) => {
                self.line_moving = None;
                self.busy.remove(&key);
                if self.loading {
                    self.stale = true; // (read again when that comes back)
                }
                let letter = self.letter(&key);
                let out = match result {
                    Ok(()) => vec![],
                    Err(e) => {
                        let mut out = vec![Out::Notice(format!(
                            "Couldn't move {} in Xtime ({})",
                            if letter.is_empty() {
                                "the line".to_string()
                            } else {
                                format!("line {letter}")
                            },
                            e.plain()
                        ))];
                        if e == LoadError::SignedOut {
                            out.push(Out::SignedOut);
                        }
                        out
                    }
                };
                // as Xtime has it now (a move that didn't work goes back)
                return (self.load(env, true), out);
            }
            Msg::Ask(a) => return just(vec![Out::Ask(a)]),
        }
        just(vec![])
    }
}

/// The lines of one of Xtime's lists that the Xtime View shows there (the inspection lines are shown on their own)
pub fn shown_in<'a>(v: &'a RoView, place: &Place) -> Vec<&'a xtime::ro::Line> {
    v.xtime_list(place).into_iter().filter(|l| !l.is_inspection()).collect()
}

/// Where a line goes in the RO's lines when it's the first in an empty list: after its concern (under one), after the
/// last concern (among them), or at the end (a recommendation on its own)
fn first_slot(v: &RoView, place: &Place) -> usize {
    match place {
        Place::Under(c) => v.lines.iter().position(|l| same_id(&l.id, c)).map_or(v.lines.len(), |i| i + 1),
        Place::Concerns => v.lines.iter().rposition(|l| l.primary).map_or(0, |i| i + 1),
        Place::Recommendations => v.lines.len(),
    }
}
