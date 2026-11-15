//! The app's state, the messages its screens send, and what each one changes (iced's state → view → update).

use iced::keyboard::{self, Key, key::Named};
use iced::{Element, Subscription, Task, Theme};
use workbench_net::Http;
use xtime::Xtime;
use xtime::dashboard::AssignedRo;

use crate::clock;
use crate::reports::{ReportMsg, Reports};
use crate::ros::{self, LoadError, Loaded};
use crate::settings::{Settings, ThemeChoice};
use crate::signin::{self, Outcome, Site};
use crate::timecard;
use crate::ui::anim::{self, Phase};
use crate::workspace::{ManualTab, Tab, Workspace};
use crate::{sample, ui};

/// Top bar: Xtime (your dashboard) or the Workbench (the vehicles you have open)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Xtime,
    Workbench,
}

/// What fills the window
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    /// First setup: store ID and dealer code before anything else
    Setup,
    Main,
}

/// A card over everything else (the page stays behind it, frosted)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    Settings,
    /// Bug reports and ideas (from the bottom bar)
    Feedback,
    /// The app's Print window: a preview, the printer and copies
    Print,
    /// Your hours: the pay-period report
    Hours,
}

/// The hours report's state
#[derive(Debug, Clone, Default)]
pub struct HoursView {
    /// Which pay period: 0 this one, -1 the one before…
    pub offset: i32,
    /// A flag whose hours are being changed, and what's typed
    pub editing: Option<(u64, String)>,
    /// A flag asked to be taken off (pressed once)
    pub removing: Option<u64>,
    /// When the pay period changed (it slides in)
    pub changed: Option<std::time::Instant>,
}

pub struct App {
    pub settings: Settings,
    /// The settings form while it's open (saved with Save)
    pub draft: Settings,
    pub screen: Screen,
    pub mode: Mode,
    /// Your ROs from Xtime's dashboard (samples until Xtime is connected)
    pub my_ros: Vec<AssignedRo>,
    pub sample_data: bool,
    /// Xtime's data, asked with the profile's sign-in
    pub xtime: Xtime,
    /// Your ROs are being loaded
    pub ros_loading: bool,
    /// When they last loaded (seconds since 1970)
    pub ros_loaded_at: Option<i64>,
    /// Why the last load didn't work (the last good list stays)
    pub ros_error: Option<LoadError>,
    pub search: String,
    /// Open vehicles, in the order they were opened
    pub open: Vec<Workspace>,
    /// The RO number of the vehicle showing
    pub selected: Option<String>,
    pub sidebar: bool,
    pub chat: bool,
    /// A short message in the top bar ("Settings saved", an error…)
    pub notice: Option<String>,
    /// The profile's web requests and sign-ins (its cookies, saved in the app's folder)
    pub http: Http,
    /// The site whose sign-in window is open
    pub signing_in: Option<Site>,
    /// Your hours (for now: the spiffs you've turned in)
    pub hours: hours::Hours,
    /// The hours report (a card over everything): which pay period, what's being changed
    pub hours_view: HoursView,
    /// What's in the app's Print window (its preview, printer, copies), while it's up
    pub print_job: Option<crate::print::Job>,
    /// Your ROs' approved recommendation totals (for "spiff available" in My ROs), and when each was last read
    pub spiff_totals: std::collections::HashMap<String, f64>,
    pub spiff_checked: std::collections::HashMap<String, i64>,
    pub checking_spiffs: bool,
    /// The time, for the clock (seconds since 1970; moves on every half minute)
    pub now: i64,
    /// The page shown (screen, mode, vehicle) and its transition: it slides out before the next one slides in
    pub outer: Transition<OuterKey>,
    /// The Feedback screen
    pub reports: Reports,
    /// The card over everything (Settings or Feedback), and where it is in coming or going
    pub overlay: Option<(Overlay, Phase)>,
    /// Whether the system (Windows) is set to dark, for the Auto appearance
    pub system_dark: bool,
    /// Within a vehicle: the tab whose content is shown (the tab strip itself stays; only its content moves)
    pub inner: Transition<InnerKey>,
}

/// Which page: the screen, Xtime or the Workbench, and the vehicle
pub type OuterKey = (Screen, Mode, Option<String>);
/// Within a page: which of the vehicle's tabs has its content shown
#[derive(Debug, Clone, PartialEq)]
pub enum InnerKey {
    None,
    Vehicle(Tab, ManualTab),
}

/// A page being shown, and where it is in its transition
#[derive(Debug, Clone)]
pub struct Transition<K> {
    pub shown: K,
    pub phase: Phase,
}

impl<K> Transition<K> {
    fn new(shown: K) -> Self {
        Self {
            shown,
            phase: Phase::Still,
        }
    }
}

/// Which transition a page change is for
#[derive(Debug, Clone, Copy)]
pub enum Level {
    Outer,
    Inner,
}

/// What the app tells Xtime and Ford it is (a desktop browser, as their pages expect)
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

/// The saved sign-ins (cookies). They're sign-ins: only ever in the app's own folder.
fn cookie_file() -> std::path::PathBuf {
    crate::settings::app_dir().join("cookies.json")
}

/// The sign-in windows' web profile ("remember me", saved passwords)
fn web_dir() -> std::path::PathBuf {
    crate::settings::app_dir().join("web")
}

#[derive(Debug, Clone)]
pub enum Message {
    SetMode(Mode),
    Search(String),
    /// Open (or show) a vehicle by RO number
    OpenRo(String),
    CloseRo(String),
    SetTab(Tab),
    SetManualTab(ManualTab),
    ToggleSidebar,
    ToggleChat,
    CopyVin,
    /// Copy something shown (a tag, a part number…): the text, and what it is ("Part number")
    Copy(String, String),
    /// Xtime View: read the RO from Xtime (its RO number); `true` even if it was read moments ago
    LoadView(String, bool),
    ViewLoaded(String, Result<xtime::ro::RoView, LoadError>),
    /// Every minute: the Xtime View showing reads its RO again
    RefreshView,
    /// Print the spiff sheet for the RO showing (Windows' Print window, then the printer)
    PrintSpiff,
    /// A print finished: what it was, and how it went
    Printed(String, Result<(), String>),
    /// The Print window: the printers found, the one picked, how many copies, print, close
    PrintersFound(crate::print::Printers),
    PickPrinter(String),
    Copies(u32),
    PrintNow,
    ClosePrint,
    /// Open the system's printer settings (to add one); while the Print window is up, the list is looked at again
    /// every few seconds, so a printer added there shows up
    ManagePrinters,
    RefreshPrinters,
    /// Your ROs, read in the background (for their spiff totals, and their lines for your hours)
    SpiffsChecked(Vec<(String, Option<xtime::ro::RoView>)>),
    /// Flag a line of the RO showing (by Xtime's line ID), or take the flag off
    FlagLine(String, bool),
    /// The hours report: open, close, which pay period (0 this one, -1 the one before…)
    OpenHours,
    CloseHours,
    HoursPeriod(i32),
    /// Tick flags confirmed (on your time sheet), or not
    ConfirmFlags(Vec<u64>, bool),
    /// Change a flag's hours: start, what's typed, keep it; or stop changing anything
    EditFlagHours(u64),
    FlagHoursInput(String),
    SaveFlagHours,
    CancelEdit,
    /// Take a flag off (asks first: press twice)
    RemoveFlag(u64),
    /// Mark the RO showing as spiffed (turned in), or not
    MarkSpiffed(bool),
    /// Open or close a line of the RO showing (by line ID)
    ToggleLine(String),
    ToggleInspection,
    /// Mark a line of the RO showing done or not (by line ID), as Xtime's Done button does
    SetDone(String, bool),
    /// Xtime's answer: RO number, line ID, done or not
    DoneSaved(String, String, bool, Result<(), LoadError>),
    OpenSettings,
    CloseSettings,
    Edit(Field, String),
    SetTheme(ThemeChoice),
    /// Every few seconds while the appearance is Auto: look at the system's setting again
    CheckSystemTheme,
    /// The system's setting: dark or not
    SystemTheme(bool),
    SaveSettings,
    DismissNotice,
    /// Open a site's sign-in window
    SignIn(Site),
    /// Its window closed: signed in (with the cookies), closed first, or failed
    SignInDone(Site, Outcome),
    /// Load your ROs from Xtime (on start, after signing in, after Settings change, every minute)
    LoadRos,
    RosLoaded(Result<Loaded, LoadError>),
    /// Clock in, or out
    ToggleClock,
    /// Every half minute (the clock's time)
    Tick,
    OpenFeedback,
    CloseFeedback,
    /// The Feedback screen's own messages
    Report(ReportMsg),
    /// A card over everything has finished going
    OverlayGone,
    /// The app's window is up: the launcher closes its loading window, and keeps using this version
    WindowOpened,
    /// The old page has left: show the new one
    Arrive(Level),
    /// Tab / Shift+Tab: the next / previous field
    FocusNext,
    FocusPrevious,
}

/// Settings form fields
#[derive(Debug, Clone, Copy)]
pub enum Field {
    EmployeeNumber,
    StoreId,
    DealerCode,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let settings = Settings::load();
        // your hours (brought over from the Electron app the first time; a forgotten clock-out ended)
        let opened = timecard::open();
        let notice = match (opened.imported, opened.auto_clocked_out) {
            (Some(n), _) => Some(format!("Brought over your hours from the old app ({n} flagged lines)")),
            (None, Some(at)) => Some(format!(
                "You were clocked out at {}, when the app was last open",
                chrono::TimeZone::timestamp_millis_opt(&chrono::Local, at)
                    .earliest()
                    .map(|t| t.format("%a %-I:%M %p").to_string())
                    .unwrap_or_default()
            )),
            _ => None,
        };
        let screen = if settings.needs_setup() { Screen::Setup } else { Screen::Main };
        let outer = Transition::new((screen.clone(), Mode::Workbench, None));
        let http = Http::with_saved_cookies(USER_AGENT, &cookie_file())
            .or_else(|_| Http::new(USER_AGENT)) // saved sign-ins unreadable: start signed out
            .expect("the app's web requests");
        let xtime = Xtime::new(http.clone(), xtime::paths::DATA_HOST).expect("Xtime's address");
        let reports = Reports::new(
            ::feedback::Client::new(http.clone()),
            ::feedback::reporter_id(&crate::settings::app_dir()),
        );
        let app = Self {
            draft: settings.clone(),
            settings,
            screen,
            mode: Mode::Workbench,
            my_ros: sample::my_ros(),
            sample_data: true,
            xtime,
            ros_loading: false,
            ros_loaded_at: None,
            ros_error: None,
            search: String::new(),
            open: Vec::new(),
            selected: None,
            sidebar: true,
            chat: false,
            notice,
            http,
            reports,
            signing_in: None,
            hours: opened.hours,
            hours_view: HoursView::default(),
            print_job: None,
            spiff_totals: Default::default(),
            spiff_checked: Default::default(),
            checking_spiffs: false,
            now: clock::now(),
            outer,
            overlay: None,
            system_dark: crate::system_theme::is_dark(),
            inner: Transition::new(InnerKey::None),
        };
        // first setup: straight into its first field
        let focus = if app.screen == Screen::Setup {
            ui::focus_first_field()
        } else {
            Task::none()
        };
        // the admin's count of new reports (and every 15 minutes, see subscription)
        let count = Task::done(Message::Report(ReportMsg::Count));
        // your ROs, when there's a sign-in to ask with (and every minute, see subscription)
        let ros = Task::done(Message::LoadRos);
        (app, Task::batch([focus, count, ros]))
    }

    pub fn title(&self) -> String {
        match self.workspace() {
            Some(ws) => format!("RO {} · {} · PTS Workbench", ws.ro.ro, ws.vehicle()),
            None => "PTS Workbench".into(),
        }
    }

    pub fn theme(&self) -> Theme {
        ui::style::theme(self.settings.theme.resolve(self.system_dark))
    }

    /// Whether there's a sign-in for a site (as far as the app can tell without asking it)
    /// The one person who reads everyone's feedback (by employee number)
    pub fn is_admin(&self) -> bool {
        self.settings.employee_number.trim() == ::feedback::ADMIN_EMPLOYEE
    }

    /// Where in the app you are, for a feedback report ("Workbench › Service Manual › OASIS"): never which vehicle
    fn screen_name(&self) -> String {
        match (&self.screen, self.mode, self.workspace()) {
            (_, Mode::Xtime, _) => "Xtime".into(),
            (_, Mode::Workbench, None) => "Workbench".into(),
            (_, Mode::Workbench, Some(ws)) => match ws.tab {
                Tab::ServiceManual => format!("Workbench › Service Manual › {}", ws.manual.label()),
                t => format!("Workbench › {}", t.label()),
            },
        }
    }

    pub fn signed_in(&self, site: Site) -> bool {
        self.http.has_cookies(&site.check_url())
    }

    /// Brings a card up over everything (or swaps the one that's up)
    fn show_overlay(&mut self, which: Overlay) {
        let still_up = matches!(self.overlay, Some((_, Phase::Still | Phase::Entering(_))));
        // a card already up just changes; otherwise this one comes in
        let phase = if still_up {
            Phase::Still
        } else {
            Phase::Entering(std::time::Instant::now())
        };
        self.overlay = Some((which, phase));
    }

    /// Sends the card away (it's gone once its leaving is done)
    fn close_overlay(&mut self) -> Task<Message> {
        match self.overlay {
            Some((which, Phase::Still | Phase::Entering(_))) => {
                self.overlay = Some((which, Phase::Leaving(std::time::Instant::now())));
                Task::perform(tokio::time::sleep(anim::MODAL_OUT), |_| Message::OverlayGone)
            }
            _ => Task::none(),
        }
    }

    /// The vehicle showing
    pub fn workspace(&self) -> Option<&Workspace> {
        let ro = self.selected.as_ref()?;
        self.open.iter().find(|w| &w.ro.ro == ro)
    }

    fn workspace_mut(&mut self) -> Option<&mut Workspace> {
        let ro = self.selected.clone()?;
        self.open.iter_mut().find(|w| w.ro.ro == ro)
    }

    /// Which page the app is on now
    fn outer_key(&self) -> OuterKey {
        (self.screen.clone(), self.mode, self.selected.clone())
    }

    fn inner_key(&self) -> InnerKey {
        match (&self.screen, self.workspace()) {
            (Screen::Main, Some(w)) if self.mode == Mode::Workbench => InnerKey::Vehicle(w.tab, w.manual),
            _ => InnerKey::None,
        }
    }

    /// The vehicle on the page shown (during a transition: the one leaving)
    pub fn shown_workspace(&self) -> Option<&Workspace> {
        let ro = self.outer.shown.2.as_ref()?;
        self.open.iter().find(|w| &w.ro.ro == ro)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = match message {
            Message::Arrive(level) => {
                self.arrive(level);
                Task::none()
            }
            other => self.apply(other),
        };
        Task::batch([task, self.start_transition()])
    }

    /// After any change: when the page (or the vehicle's tab) changed, the shown one starts leaving; the new one is
    /// shown when it's gone (`Arrive`). Changes while it's leaving just change where it'll arrive.
    fn start_transition(&mut self) -> Task<Message> {
        let after = |level| Task::perform(tokio::time::sleep(anim::LEAVE), move |_| Message::Arrive(level));
        if self.outer.shown != self.outer_key() {
            if self.outer.phase.is_leaving() {
                return Task::none();
            }
            self.outer.phase = Phase::Leaving(std::time::Instant::now());
            return after(Level::Outer);
        }
        if self.inner.shown != self.inner_key() && !self.outer.phase.is_leaving() {
            if self.inner.phase.is_leaving() {
                return Task::none();
            }
            self.inner.phase = Phase::Leaving(std::time::Instant::now());
            return after(Level::Inner);
        }
        Task::none()
    }

    fn arrive(&mut self, level: Level) {
        let now = std::time::Instant::now();
        match level {
            Level::Outer => {
                self.outer = Transition {
                    shown: self.outer_key(),
                    phase: Phase::Entering(now),
                };
                self.inner = Transition::new(self.inner_key()); // a new page comes in whole
            }
            Level::Inner => {
                self.inner = Transition {
                    shown: self.inner_key(),
                    phase: Phase::Entering(now),
                }
            }
        }
    }

    fn apply(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SetMode(m) => self.mode = m,
            Message::Search(s) => self.search = s,
            Message::OpenRo(ro) => {
                if !self.open.iter().any(|w| w.ro.ro == ro) {
                    if let Some(r) = self.my_ros.iter().find(|r| r.ro == ro) {
                        self.open.push(Workspace::new(r.clone()));
                    }
                }
                if self.open.iter().any(|w| w.ro.ro == ro) {
                    self.selected = Some(ro.clone());
                    self.mode = Mode::Workbench;
                    if self.workspace().is_some_and(|w| w.tab == Tab::XtimeView) {
                        return self.load_view(&ro, false);
                    }
                }
            }
            Message::CloseRo(ro) => {
                let at = self.open.iter().position(|w| w.ro.ro == ro);
                self.open.retain(|w| w.ro.ro != ro);
                if self.selected.as_deref() == Some(ro.as_str()) {
                    // show the neighbour, as closing a tab does
                    self.selected = at
                        .and_then(|i| self.open.get(i.min(self.open.len().saturating_sub(1))))
                        .map(|w| w.ro.ro.clone());
                }
            }
            Message::SetTab(t) => {
                if let Some(ws) = self.workspace_mut() {
                    ws.tab = t;
                }
                if t == Tab::XtimeView {
                    if let Some(ro) = self.selected.clone() {
                        return self.load_view(&ro, false);
                    }
                }
            }
            Message::SetManualTab(t) => {
                if let Some(ws) = self.workspace_mut() {
                    ws.tab = Tab::ServiceManual;
                    ws.manual = t;
                }
            }
            Message::ToggleSidebar => self.sidebar = !self.sidebar,
            Message::ToggleChat => self.chat = !self.chat,
            Message::Copy(value, what) => {
                self.notice = Some(format!("Copied {}: {value}", what.to_lowercase()));
                return iced::clipboard::write(value);
            }
            Message::LoadView(ro, force) => return self.load_view(&ro, force),
            Message::RefreshView => {
                let showing = self.workspace().filter(|w| self.mode == Mode::Workbench && w.tab == Tab::XtimeView);
                if let Some(ro) = showing.map(|w| w.ro.ro.clone()) {
                    return self.load_view(&ro, true);
                }
            }
            Message::ViewLoaded(ro, result) => {
                if let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) {
                    ws.xv.loading = false;
                    match result {
                        Ok(view) => {
                            let lines = timecard::lines_of(&view).into_iter().map(|(_, l)| l).collect();
                            self.hours.update_lines(&ro, lines, &timecard::ymm(&view), timecard::now_ms());
                            let _ = timecard::save(&self.hours);
                            self.spiff_totals.insert(ro.clone(), view.spiff_total());
                            self.spiff_checked.insert(ro.clone(), clock::now());
                            ws.xv.view = Some(view);
                            ws.xv.error = None;
                            ws.xv.at = Some(clock::now());
                        }
                        Err(e) => {
                            if e == LoadError::SignedOut {
                                self.ros_error = Some(LoadError::SignedOut); // the bottom bar offers to sign in
                            }
                            ws.xv.error = Some(e);
                        }
                    }
                }
                self.now = clock::now();
            }
            Message::ToggleLine(key) => {
                if let Some(ws) = self.workspace_mut() {
                    if ws.xv.open.remove(&key).is_none() {
                        ws.xv.open.insert(key, std::time::Instant::now());
                    }
                }
            }
            Message::PrintSpiff => {
                if self.print_job.is_some() {
                    return Task::none();
                }
                let Some(v) = self.workspace().and_then(|w| w.xv.view.clone()) else {
                    return Task::none();
                };
                if !v.spiff_earned() {
                    return Task::none();
                }
                // you, for a sheet whose RO doesn't name its technician
                let me = match (self.settings.name.trim(), self.settings.employee_number.trim()) {
                    ("", n) => n.to_string(),
                    (name, "") => name.to_string(),
                    (name, n) => format!("{name} (#{n})"),
                };
                let extra = crate::print::spiff::Extra {
                    technician: me,
                    printed: format!("Printed {}", chrono::Local::now().format("%b %-d, %Y, %-I:%M %p")),
                };
                let what = format!("Spiff for RO {}", v.ro);
                let build: crate::print::Build = std::sync::Arc::new(move |m| crate::print::spiff::sheet(m, &v, &extra));
                return self.open_print(what, build);
            }
            Message::PrintersFound(found) => {
                if let Some(job) = &mut self.print_job {
                    // the one picked stays picked while it's still there; otherwise the default (or the first)
                    let kept = job.printer.clone().filter(|p| found.names.contains(p));
                    job.printer = kept.or_else(|| found.default.clone().or_else(|| found.names.first().cloned()));
                    let none = "No printers are set up on this computer. Add one with Manage printers.";
                    if found.names.is_empty() {
                        job.error = Some(none.into());
                    } else if job.error.as_deref() == Some(none) {
                        job.error = None;
                    }
                    job.printers = Some(found);
                }
            }
            Message::ManagePrinters => {
                if let Err(e) = crate::print::manage_printers() {
                    if let Some(job) = &mut self.print_job {
                        job.error = Some(e);
                    }
                }
            }
            Message::RefreshPrinters => {
                if self.print_job.as_ref().is_some_and(|j| !j.busy) {
                    return Task::perform(
                        async { tokio::task::spawn_blocking(crate::print::printers).await.unwrap_or_default() },
                        Message::PrintersFound,
                    );
                }
            }
            Message::PickPrinter(name) => {
                if let Some(job) = &mut self.print_job {
                    job.printer = Some(name);
                    job.error = None;
                }
            }
            Message::Copies(n) => {
                if let Some(job) = &mut self.print_job {
                    job.copies = n.clamp(1, 99);
                }
            }
            Message::PrintNow => {
                let Some(job) = &mut self.print_job else {
                    return Task::none();
                };
                let Some(printer) = job.printer.clone().filter(|_| !job.busy) else {
                    return Task::none();
                };
                job.busy = true;
                job.error = None;
                let (build, copies, what) = (job.build.clone(), job.copies, job.what.clone());
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || crate::print::print_to(&printer, copies, &*build))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    move |r| Message::Printed(what.clone(), r),
                );
            }
            Message::ClosePrint => {
                if matches!(self.overlay, Some((Overlay::Print, _))) {
                    return self.close_overlay();
                }
            }
            Message::Printed(what, result) => match result {
                Ok(()) => {
                    self.notice = Some(format!("{what} sent to the printer"));
                    if matches!(self.overlay, Some((Overlay::Print, _))) {
                        return self.close_overlay();
                    }
                }
                Err(e) => match &mut self.print_job {
                    // still in the Print window: say so there, and let it be tried again
                    Some(job) if matches!(self.overlay, Some((Overlay::Print, Phase::Still | Phase::Entering(_)))) => {
                        job.busy = false;
                        job.error = Some(format!("Couldn't print: {e}"));
                    }
                    _ => self.notice = Some(format!("Couldn't print {}: {e}", what.to_lowercase())),
                },
            },
            Message::SpiffsChecked(found) => {
                self.checking_spiffs = false;
                let now = clock::now();
                let mut changed = false;
                for (ro, view) in found {
                    self.spiff_checked.insert(ro.clone(), now);
                    if let Some(v) = view {
                        self.spiff_totals.insert(ro.clone(), v.spiff_total());
                        // the hours keep each RO's lines (to flag them, and their hours)
                        let lines = timecard::lines_of(&v).into_iter().map(|(_, l)| l).collect();
                        self.hours.update_lines(&ro, lines, &timecard::ymm(&v), timecard::now_ms());
                        changed = true;
                    }
                }
                if changed {
                    let _ = timecard::save(&self.hours);
                }
            }
            Message::FlagLine(line_id, on) => return self.flag_line(&line_id, on),
            Message::OpenHours => {
                self.hours_view = HoursView::default();
                self.show_overlay(Overlay::Hours);
            }
            Message::CloseHours => {
                if matches!(self.overlay, Some((Overlay::Hours, _))) {
                    return self.close_overlay();
                }
            }
            Message::HoursPeriod(offset) => {
                self.hours_view = HoursView {
                    offset: offset.min(0),
                    changed: Some(std::time::Instant::now()),
                    ..HoursView::default()
                };
            }
            Message::ConfirmFlags(ids, on) => {
                for id in ids {
                    self.hours.set_confirmed(id, on);
                }
                self.save_hours();
            }
            Message::EditFlagHours(id) => {
                let now = self.hours.flags.iter().find(|f| f.id == id).map(|f| f.hours).unwrap_or(0.0);
                self.hours_view.editing = Some((id, format!("{now}")));
                self.hours_view.removing = None;
                return iced::widget::text_input::focus(crate::ui::hours_report::edit_id());
            }
            Message::FlagHoursInput(text) => {
                if let Some((_, t)) = &mut self.hours_view.editing {
                    *t = text;
                }
            }
            Message::SaveFlagHours => {
                if let Some((id, text)) = self.hours_view.editing.take() {
                    match text.trim().parse::<f64>() {
                        Ok(h) if h.is_finite() && h >= 0.0 && h < 100.0 => {
                            self.hours.set_flag_hours(id, h);
                            self.save_hours();
                        }
                        _ => self.notice = Some(format!("\"{}\" isn't a number of hours", text.trim())),
                    }
                }
            }
            Message::CancelEdit => {
                self.hours_view.editing = None;
                self.hours_view.removing = None;
            }
            Message::RemoveFlag(id) => {
                // twice: the first press asks
                if self.hours_view.removing == Some(id) {
                    self.hours.delete_flag(id);
                    self.hours_view.removing = None;
                    self.save_hours();
                } else {
                    self.hours_view.removing = Some(id);
                    self.hours_view.editing = None;
                }
            }
            Message::MarkSpiffed(on) => {
                let Some(ws) = self.workspace() else {
                    return Task::none();
                };
                let ro = ws.ro.ro.clone();
                let spiff = on.then(|| hours::Spiff {
                    at: timecard::now_ms(),
                    total: ws.xv.view.as_ref().map(|v| v.spiff_total()).unwrap_or(0.0),
                    tag: ws.ro.tag.clone(),
                    vehicle: ws.vehicle(),
                });
                self.hours.set_spiffed(&ro, spiff);
                self.notice = Some(match timecard::save(&self.hours) {
                    Ok(()) if on => format!("RO {ro} marked spiffed"),
                    Ok(()) => format!("RO {ro} no longer marked spiffed"),
                    Err(e) => format!("Couldn't save the spiff: {e}"),
                });
            }
            Message::ToggleInspection => {
                if let Some(ws) = self.workspace_mut() {
                    ws.xv.show_inspection = !ws.xv.show_inspection;
                }
            }
            Message::SetDone(key, done) => return self.set_done(key, done),
            Message::DoneSaved(ro, key, done, result) => {
                let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
                    return Task::none();
                };
                ws.xv.busy.remove(&key);
                let letter = ws
                    .xv
                    .view
                    .as_ref()
                    .and_then(|v| v.lines.iter().find(|l| l.key() == key))
                    .map(|l| l.dms.clone())
                    .unwrap_or_default();
                match result {
                    Ok(()) => {
                        self.notice = Some(match (done, letter.is_empty()) {
                            (true, false) => format!("Line {letter} marked done"),
                            (false, false) => format!("Line {letter} not done"),
                            (true, true) => "Line marked done".into(),
                            (false, true) => "Line not done".into(),
                        });
                    }
                    Err(e) => {
                        // put the box back as Xtime has it
                        if let Some(line) = ws.xv.view.as_mut().and_then(|v| v.lines.iter_mut().find(|l| l.key() == key)) {
                            line.completed = !done;
                        }
                        if e == LoadError::SignedOut {
                            self.ros_error = Some(LoadError::SignedOut);
                        }
                        self.notice = Some(format!(
                            "Couldn't change the line in Xtime ({})",
                            e.text().trim_start_matches("Xtime: ")
                        ));
                    }
                }
                // read it again: Xtime may have changed more (the RO's stage, who did it)
                return self.load_view(&ro, true);
            }
            Message::CopyVin => {
                if let Some(vin) = self.workspace().map(|w| w.ro.vin.clone()) {
                    self.notice = Some(format!("Copied {vin}"));
                    return iced::clipboard::write(vin);
                }
            }
            Message::OpenSettings => {
                self.draft = self.settings.clone();
                self.show_overlay(Overlay::Settings);
                return ui::focus_first_field();
            }
            Message::CloseSettings => {
                self.draft = self.settings.clone();
                return self.close_overlay();
            }
            Message::Edit(field, value) => {
                let f = match field {
                    Field::EmployeeNumber => &mut self.draft.employee_number,
                    Field::StoreId => &mut self.draft.store_id,
                    Field::DealerCode => &mut self.draft.dealer_code,
                };
                *f = value;
            }
            Message::SetTheme(t) => {
                self.draft.theme = t;
                self.settings.theme = t; // shown right away; kept with Save
                if t == ThemeChoice::Auto {
                    self.system_dark = crate::system_theme::is_dark();
                }
            }
            Message::CheckSystemTheme => {
                return Task::perform(
                    async { tokio::task::spawn_blocking(crate::system_theme::is_dark).await.unwrap_or(false) },
                    Message::SystemTheme,
                );
            }
            Message::SystemTheme(dark) => self.system_dark = dark,
            Message::SaveSettings => {
                let mut next = self.draft.clone().cleaned();
                if next.needs_setup() {
                    self.notice = Some("The store ID and dealer code are needed".into());
                    return Task::none();
                }
                // a new employee number: someone else, so their name and technician ID are looked up again
                if next.employee_number != self.settings.employee_number {
                    next.tech_id.clear();
                    next.name.clear();
                }
                let xtime_changed = (&next.store_id, &next.employee_number, &next.tech_id)
                    != (&self.settings.store_id, &self.settings.employee_number, &self.settings.tech_id);
                match next.save() {
                    Ok(()) => {
                        self.settings = next.clone();
                        self.draft = next;
                        self.notice = Some("Settings saved".into());
                        // someone else's ROs, or another store's: load them again
                        let reload = if xtime_changed {
                            self.xtime.reset();
                            self.ros_error = None;
                            Task::done(Message::LoadRos)
                        } else {
                            Task::none()
                        };
                        // first setup goes on to the app; Settings closes
                        if self.screen == Screen::Setup {
                            self.screen = Screen::Main;
                            return reload;
                        } else {
                            return Task::batch([reload, self.close_overlay()]);
                        }
                    }
                    Err(e) => self.notice = Some(format!("Couldn't save the settings: {e}")),
                }
            }
            Message::DismissNotice => self.notice = None,
            Message::SignIn(site) => {
                if self.signing_in.is_some() {
                    return Task::none(); // one sign-in window at a time
                }
                self.signing_in = Some(site);
                self.notice = Some(format!("Signing in to {}…", site.name()));
                return Task::perform(signin::run(site, web_dir(), self.settings.store_id.clone()), move |o| {
                    Message::SignInDone(site, o)
                });
            }
            Message::SignInDone(site, outcome) => {
                self.signing_in = None;
                let load = matches!((site, &outcome), (Site::Xtime, Outcome::SignedIn(_)));
                self.notice = Some(match outcome {
                    Outcome::SignedIn(cookies) => {
                        self.http.add_site_cookies(&cookies);
                        match self.http.save_cookies(&cookie_file()) {
                            Ok(()) => format!("Signed in to {}", site.name()),
                            Err(e) => format!("Signed in to {}, but couldn't keep the sign-in for next time: {e}", site.name()),
                        }
                    }
                    Outcome::Cancelled => format!("{} sign-in closed", site.name()),
                    Outcome::Failed(e) => format!("Couldn't sign in to {}: {e}", site.name()),
                });
                if load {
                    self.ros_error = None;
                    self.xtime.reset();
                    return Task::batch([Task::done(Message::LoadRos), Task::done(Message::RefreshView)]);
                }
            }
            Message::LoadRos => return self.load_ros(),
            Message::RosLoaded(result) => {
                let ok = result.is_ok();
                self.ros_loaded(result);
                if ok {
                    return self.check_spiffs();
                }
            }
            Message::ToggleClock => {
                self.now = clock::now();
                let now = timecard::now_ms();
                if self.hours.open_shift().is_some() {
                    self.hours.clock_out(now);
                } else {
                    self.hours.clock_in(now);
                }
                self.hours.heartbeat(now);
                self.save_hours();
            }
            Message::Tick => {
                self.now = clock::now();
                // the app is still running (for "forgot to clock out"): kept every few minutes
                let now = timecard::now_ms();
                if now - self.hours.last_seen.unwrap_or(0) >= 5 * 60_000 {
                    self.hours.heartbeat(now);
                    let _ = timecard::save(&self.hours);
                }
            }
            Message::Arrive(_) => {} // handled in update
            Message::OpenFeedback => {
                let task = self.reports.open(self.screen_name(), self.is_admin());
                self.show_overlay(Overlay::Feedback);
                return task.map(Message::Report);
            }
            Message::CloseFeedback => return self.close_overlay(),
            Message::WindowOpened => launcher::window_ready(),
            Message::OverlayGone => {
                if let Some((which, Phase::Leaving(_))) = self.overlay {
                    self.overlay = None;
                    if which == Overlay::Print {
                        self.print_job = None; // kept until now: the card shows it while it goes
                    }
                }
            }
            Message::Report(msg) => {
                let (admin, name) = (self.is_admin(), self.settings.name.clone());
                return self.reports.update(msg, admin, &name).map(Message::Report);
            }
            Message::FocusNext => return iced::widget::focus_next(),
            Message::FocusPrevious => return iced::widget::focus_previous(),
        }
        Task::none()
    }

    /// Reads an RO for its Xtime View, unless it's being read, or was read in the last 20 s (unless `force`)
    fn load_view(&mut self, ro: &str, force: bool) -> Task<Message> {
        let now = clock::now();
        let (store, x) = (self.settings.store_id.clone(), self.xtime.clone());
        if self.sample_data {
            return Task::none(); // the samples aren't in Xtime
        }
        let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
            return Task::none();
        };
        let fresh = ws.xv.at.is_some_and(|at| now - at < 20) && ws.xv.error.is_none();
        if ws.xv.loading || (fresh && !force) {
            return Task::none();
        }
        ws.xv.loading = true;
        let ro = ro.to_string();
        Task::perform(
            async move {
                let r = x.ro_view(&store, &ro).await.map_err(LoadError::from);
                (ro, r)
            },
            |(ro, r)| Message::ViewLoaded(ro, r),
        )
    }

    /// Marks a line of the RO showing done or not in Xtime (the box changes right away; it goes back if Xtime says no)
    fn set_done(&mut self, key: String, done: bool) -> Task<Message> {
        let (store, x) = (self.settings.store_id.clone(), self.xtime.clone());
        let Some(ws) = self.workspace_mut() else {
            return Task::none();
        };
        if ws.xv.busy.contains(&key) {
            return Task::none();
        }
        let ro = ws.ro.ro.clone();
        let Some(line) = ws.xv.view.as_mut().and_then(|v| v.lines.iter_mut().find(|l| l.key() == key)) else {
            return Task::none();
        };
        if !line.can_be_done() {
            return Task::none(); // declined, or waiting on approval: the work can't be done
        }
        line.completed = done;
        let id = line.id.clone();
        ws.xv.busy.insert(key.clone());
        Task::perform(
            async move {
                let r = x.set_done(&store, &ro, &id, done).await.map_err(LoadError::from);
                (ro, key, done, r)
            },
            |(ro, key, done, r)| Message::DoneSaved(ro, key, done, r),
        )
    }

    /// Loads your ROs, unless they're loading already, setup isn't done, or there's no Xtime sign-in to ask with
    fn load_ros(&mut self) -> Task<Message> {
        // signed out, or no (or an unknown) employee number: wait for a sign-in or Settings (they clear the error and load again)
        let waiting = matches!(
            self.ros_error,
            Some(LoadError::SignedOut | LoadError::NoEmployee | LoadError::NotFound(_))
        );
        if self.ros_loading
            || waiting
            || self.settings.needs_setup()
            || !self.signed_in(Site::Xtime)
            || self.signing_in == Some(Site::Xtime)
        {
            return Task::none();
        }
        self.ros_loading = true;
        let s = &self.settings;
        Task::perform(
            ros::load(self.xtime.clone(), s.tech_id.clone(), s.employee_number.clone(), s.store_id.clone()),
            Message::RosLoaded,
        )
    }

    fn save_hours(&mut self) {
        if let Err(e) = timecard::save(&self.hours) {
            self.notice = Some(format!("Couldn't save your hours: {e}"));
        }
    }

    /// Flags a line of the RO showing (by Xtime's line ID), or takes its flag off. The RO's lines are kept in the
    /// hours first, as Xtime has them now.
    fn flag_line(&mut self, line_id: &str, on: bool) -> Task<Message> {
        let Some(ws) = self.workspace() else {
            return Task::none();
        };
        let Some(v) = ws.xv.view.clone() else {
            return Task::none();
        };
        let ro = ws.ro.ro.clone();
        let lines = timecard::lines_of(&v);
        let Some(key) = lines.iter().find(|(id, _)| id == line_id).map(|(_, l)| l.key.clone()) else {
            return Task::none();
        };
        let now = timecard::now_ms();
        self.hours
            .update_lines(&ro, lines.into_iter().map(|(_, l)| l).collect(), &timecard::ymm(&v), now);
        if on {
            self.hours.flag_line(&ro, &key, now);
        } else {
            self.hours.unflag_line(&ro, &key);
        }
        self.save_hours();
        Task::none()
    }

    /// Brings up the app's Print window for a document (and looks for the printers meanwhile)
    fn open_print(&mut self, what: String, build: crate::print::Build) -> Task<Message> {
        self.print_job = Some(crate::print::Job::new(what, build));
        self.show_overlay(Overlay::Print);
        Task::perform(
            async { tokio::task::spawn_blocking(crate::print::printers).await.unwrap_or_default() },
            Message::PrintersFound,
        )
    }

    /// Reads the ROs on your list that haven't been read in the last 5 minutes, one after another, for their spiff
    /// totals (My ROs shows which have a spiff to turn in)
    fn check_spiffs(&mut self) -> Task<Message> {
        if self.checking_spiffs || self.sample_data {
            return Task::none();
        }
        let now = clock::now();
        let due: Vec<String> = self
            .my_ros
            .iter()
            .map(|r| r.ro.clone())
            .filter(|ro| self.spiff_checked.get(ro).is_none_or(|at| now - at >= 5 * 60))
            .collect();
        if due.is_empty() {
            return Task::none();
        }
        self.checking_spiffs = true;
        let (x, store) = (self.xtime.clone(), self.settings.store_id.clone());
        Task::perform(
            async move {
                let mut found = Vec::new();
                for ro in due {
                    let view = x.ro_view(&store, &ro).await.ok();
                    found.push((ro, view));
                }
                found
            },
            Message::SpiffsChecked,
        )
    }

    fn ros_loaded(&mut self, result: Result<Loaded, LoadError>) {
        self.ros_loading = false;
        let loaded = match result {
            Ok(l) => l,
            Err(e) => {
                self.ros_error = Some(e);
                return;
            }
        };
        self.now = clock::now();
        self.ros_error = None;
        self.ros_loaded_at = Some(self.now);
        if self.sample_data {
            // the samples' vehicles were never real: close them
            self.sample_data = false;
            self.open.clear();
            self.selected = None;
        }
        // open vehicles get their RO's latest (mode, waiting…); one that's gone from your list stays open as it was
        for ws in &mut self.open {
            if let Some(r) = loaded.ros.iter().find(|r| r.ro == ws.ro.ro) {
                ws.ro = r.clone();
            }
        }
        self.my_ros = loaded.ros;
        // who you are, from Xtime: the technician ID and name found from your employee number (or, for a name
        // missing from an earlier version, the name on your ROs)
        let before = (self.settings.tech_id.clone(), self.settings.name.clone());
        if let Some(me) = loaded.found {
            self.settings.tech_id = me.id;
            self.settings.name = if me.name.is_empty() { loaded.tech_name } else { me.name };
        } else if self.settings.name.trim().is_empty() && !loaded.tech_name.is_empty() {
            self.settings.name = loaded.tech_name;
        }
        if (&self.settings.tech_id, &self.settings.name) != (&before.0, &before.1) {
            self.draft.tech_id = self.settings.tech_id.clone();
            self.draft.name = self.settings.name.clone();
            if let Err(e) = self.settings.save() {
                self.notice = Some(format!("Couldn't save what Xtime said about you: {e}"));
            }
        }
        // Xtime may have renewed the sign-in along the way: keep it for next time
        let _ = self.http.save_cookies(&cookie_file());
    }

    /// Tab and Shift+Tab move between fields (iced doesn't on its own). Only keys no field used are seen here.
    /// Also: the clock's time moves on every half minute
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            keyboard::on_key_press(|key, modifiers| match key {
                Key::Named(Named::Tab) if modifiers.shift() => Some(Message::FocusPrevious),
                Key::Named(Named::Tab) => Some(Message::FocusNext),
                _ => None,
            }),
            iced::time::every(std::time::Duration::from_secs(30)).map(|_| Message::Tick),
            iced::window::open_events().map(|_| Message::WindowOpened),
            iced::time::every(std::time::Duration::from_secs(15 * 60)).map(|_| Message::Report(ReportMsg::Count)),
            // the Print window: printers added meanwhile (Manage printers) show up
            if matches!(self.overlay, Some((Overlay::Print, _))) {
                iced::time::every(std::time::Duration::from_secs(3)).map(|_| Message::RefreshPrinters)
            } else {
                Subscription::none()
            },
            // your ROs, every minute (load_ros skips it when there's no sign-in)
            iced::time::every(ros::EVERY).map(|_| Message::LoadRos),
            // the Xtime View showing, every minute
            iced::time::every(ros::EVERY).map(|_| Message::RefreshView),
            // Auto: follow the system when it changes (checked every 2 s; only while Auto is chosen)
            if self.settings.theme == ThemeChoice::Auto {
                iced::time::every(std::time::Duration::from_secs(2)).map(|_| Message::CheckSystemTheme)
            } else {
                Subscription::none()
            },
        ])
    }

    pub fn view(&self) -> Element<'_, Message> {
        ui::view(self)
    }
}
