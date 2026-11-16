//! The Xtime View's state and what changes it: reading the RO (and its inspection form), opening lines, Done, the
//! inspection form's items and forms, deleting a line. Its look: view.rs and inspection.rs.
//!
//! The app keeps one [`XtimeView`] per open vehicle and sends it [`Msg`]s for that RO. What only the app can do comes
//! back from [`XtimeView::update`] as [`Out`]s: a notice for the bottom bar, "signed out", the RO as it was just read
//! (for your hours and spiff totals), and the buttons meant for the app ([`Ask`]: Add Service, Flag, Print Spiff…).

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use iced::Task;
use xtime::Xtime;
use xtime::inspect::{FormChoice, InspectionForm, Press, RedService};
use xtime::ro::RoView;

use crate::LoadError;

/// What the Xtime View asks Xtime with
#[derive(Clone)]
pub struct Env {
    pub xtime: Xtime,
    pub store: String,
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

    pub fn update(&mut self, msg: Msg, env: &Env) -> Step {
        match msg {
            Msg::Load(force) => return (self.load(env, force), vec![]),
            Msg::Loaded(result) => {
                self.loading = false;
                match result {
                    Ok(view) => {
                        self.error = None;
                        self.at = Some(crate::now());
                        self.view = Some(view.clone());
                        // read with the RO: its inspection form
                        return (self.load_form(env), vec![Out::Read(view)]);
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
            Msg::Ask(a) => return just(vec![Out::Ask(a)]),
        }
        just(vec![])
    }
}
