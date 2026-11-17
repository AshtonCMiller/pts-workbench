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

/// A line's hours or flag date being changed in the sidebar (see ui/lines.rs)
#[derive(Debug, Clone, PartialEq)]
pub struct LineEdit {
    pub ro: String,
    pub key: String,
    pub field: LineField,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineField {
    Hours,
    Date,
}

/// "You were clocked out at Tue 5:02 PM, when the app was last open"
fn clocked_out_notice(at: i64) -> String {
    format!(
        "You were clocked out at {}, when the app was last open",
        chrono::TimeZone::timestamp_millis_opt(&chrono::Local, at)
            .earliest()
            .map(|t| t.format("%a %-I:%M %p").to_string())
            .unwrap_or_default()
    )
}

/// The ROs whose lines are folded away in the sidebar (this computer's `folded-lines.json`)
fn folded_file() -> std::path::PathBuf {
    crate::settings::app_dir().join("folded-lines.json")
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
    /// Add Service / Edit Service (Xtime's Add Service window)
    Service,
    /// What's new (after an update, when one's waiting, from Settings)
    WhatsNew,
}

/// "Add an RO" in the sidebar: the RO number typed, a VIN (asked for when Xtime can't say), and how it's going
#[derive(Debug, Clone, Default)]
pub struct AddRo {
    pub ro: String,
    pub vin: String,
    pub looking: bool,
    pub error: Option<String>,
    pub ask_vin: bool,
}

pub fn add_ro_id() -> iced::widget::text_input::Id {
    iced::widget::text_input::Id::new("add-ro")
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
    /// Ford PTS (the Service Manual's tabs), asked with the profile's PTS sign-in
    pub ford: ford::Ford,
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
    /// Notifications: the list (kept on this computer), what was seen last (to notice what's changed), the list's
    /// panel open, and the newest one popped up in the corner
    pub notes: crate::notes::Notes,
    pub watch: crate::notes::Watch,
    pub notes_open: bool,
    pub toast: Option<(u64, Phase)>,
    /// The Dealership chat panel (crates/xtime-ui): your conversations, read from Xtime
    pub chat_panel: xtime_ui::chat::ChatPanel,
    /// Chat's settings as last kept in xtime-chat.json (written again only when Xtime says otherwise)
    pub chat_saved: Option<xtime::chat::ChatConfig>,
    /// A short message in the top bar ("Settings saved", an error…)
    pub notice: Option<String>,
    /// The profile's web requests and sign-ins (its cookies, saved in the app's folder)
    pub http: Http,
    /// The site whose sign-in window is open
    pub signing_in: Option<Site>,
    /// Your hours (for now: the spiffs you've turned in)
    pub hours: hours::Hours,
    /// The hours report (a card over everything): which pay period, what's being changed
    pub hours_view: hours_ui::HoursView,
    /// What's in the app's Print window (its preview, printer, copies), while it's up
    pub print_job: Option<crate::print::Job>,
    /// The Add / Edit Service window, while it's up
    pub service: Option<xtime_ui::service::ServiceEditor>,
    /// The store's services etc. for the Add Service window, read once per RO
    pub catalogs: std::collections::HashMap<String, xtime::service::Catalog>,
    /// Your ROs' approved recommendation totals (for "spiff available" in My ROs), and when each was last read
    pub spiff_totals: std::collections::HashMap<String, f64>,
    pub spiff_checked: std::collections::HashMap<String, i64>,
    pub checking_spiffs: bool,
    /// "Add an RO" in the sidebar, while it's open
    pub add_ro: Option<AddRo>,
    /// ROs in Dispatch with no technician (the sidebar offers to accept them), the one Accept was pressed on (asking
    /// to be sure), and the one being accepted
    pub dispatch: Vec<AssignedRo>,
    pub accept_ask: Option<String>,
    /// Whether the Dispatch list under My ROs is unfolded (folded each time the app starts)
    pub show_dispatch: bool,
    pub accepting: Option<String>,
    /// ROs opened on another computer that aren't on your list: when each was last looked up in Xtime
    pub waiting_tried: std::collections::HashMap<String, i64>,
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
    /// The app updating itself: what's going on, Beta updates
    pub updates: crate::updates::Updates,
    /// What was last kept of the open vehicles (kept again when it changes)
    open_saved: String,
    /// The sidebar's lines: the ROs whose lines are folded away, a line's hours or date being changed, a confirmed
    /// flag about to be taken off (asked once more first)
    pub folded_lines: std::collections::BTreeSet<String>,
    pub line_edit: Option<LineEdit>,
    pub unflag_ask: Option<(String, String)>,
    /// What's new, while its card is up; and the downloaded version whose notes were shown (once)
    pub whats_new: Option<crate::whats_new::Shown>,
    whats_new_for: Option<String>,
    /// Sync between your computers
    pub sync: crate::syncing::Syncing,
}

/// The dashboard's and My ROs' messages (crates/xtime-ui) are the app's own
impl From<xtime_ui::my_ros::RosMsg> for Message {
    fn from(m: xtime_ui::my_ros::RosMsg) -> Self {
        match m {
            xtime_ui::my_ros::RosMsg::Search(s) => Message::Search(s),
            xtime_ui::my_ros::RosMsg::Open(ro) => Message::OpenRo(ro),
        }
    }
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

/// Chat's settings (your dealer code and Xtime username: no customer data), as the Electron app kept them
fn chat_config_file() -> std::path::PathBuf {
    crate::settings::app_dir().join("xtime-chat.json")
}

fn load_chat_config() -> Option<xtime::chat::ChatConfig> {
    let text = std::fs::read_to_string(chat_config_file()).ok()?;
    serde_json::from_str::<xtime::chat::ChatConfig>(&text)
        .ok()
        .filter(|c| !c.dealer_code.is_empty() && !c.user_name.is_empty())
}

fn save_chat_config(c: &xtime::chat::ChatConfig) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(c).map_err(std::io::Error::other)?;
    std::fs::write(chat_config_file(), text)
}

/// The sign-in windows' web profile ("remember me", saved passwords)
fn web_dir() -> std::path::PathBuf {
    crate::settings::app_dir().join("web")
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Updates: look now; what was found; the download done; Beta updates on or off; start the new version
    CheckUpdates,
    UpdateFound(Result<Option<crate::updates::Found>, String>),
    UpdateDownloaded(Result<crate::updates::Ready, String>),
    SetBetaUpdates(bool),
    RestartToUpdate,
    /// The sidebar's lines: fold an RO's away (or back); change a line's hours or flag date (start, type, save,
    /// cancel); flag a line or take its flag off
    ToggleLines(String),
    EditLine(String, String, LineField),
    LineInput(String),
    SaveLine,
    CancelLine,
    SidebarFlag(String, String, bool),
    /// What's new: from Settings; closed (its items are then shown here)
    ShowWhatsNew,
    CloseWhatsNew,
    /// Sync: now (Settings, or due after a change); how it went; on or off
    SyncNow,
    SyncDue,
    Synced(Box<crate::syncing::Outcome>),
    SetSync(bool),
    SetMode(Mode),
    Search(String),
    /// Open (or show) a vehicle by RO number
    OpenRo(String),
    CloseRo(String),
    SetTab(Tab),
    SetManualTab(ManualTab),
    ToggleSidebar,
    ToggleChat,
    /// The chat panel's own messages (crates/xtime-ui)
    Chat(xtime_ui::chat::ChatMsg),
    /// Read the chat again (with your ROs; every 30 s while the panel is open)
    RefreshChat,
    CopyVin,
    /// Copy something shown (a tag, a part number…): the text, and what it is ("Part number")
    Copy(String, String),
    /// Every minute: the Xtime View showing reads its RO again
    RefreshView,
    /// The Xtime View of an open vehicle (its RO number): its own messages (crates/xtime-ui)
    Xv(String, xtime_ui::Msg),
    /// An open vehicle's OASIS tab (its RO number): its own messages (crates/ford-ui)
    Oasis(String, ford_ui::OasisMsg),
    /// An open vehicle's TSBs tab (its RO number): its own messages (crates/ford-ui)
    Tsb(String, ford_ui::TsbMsg),
    /// Something in an RO's Workshop Manual tab
    Wsm(String, ford_ui::ManualMsg),
    /// Something in an RO's Diagnostics tab
    Diag(String, ford_ui::DiagMsg),
    /// Something in an RO's Labor Times tab
    Labor(String, ford_ui::LaborMsg),
    /// One of an open vehicle's document tabs (its RO number, the tab's number): its own messages
    Doc(String, u32, ford_ui::DocTabMsg),
    /// Close a document tab of the vehicle showing (the tab's number)
    CloseDoc(u32),
    /// The vehicle of an RO was opened in PTS (to set PTS up for it): how it went
    PtsSetUp(String, Outcome),
    /// Print the spiff sheet for the RO showing (the app's Print window)
    PrintSpiff,
    /// Print the spiff sheet for an RO by its number (from the hours report: read from Xtime first if need be)
    PrintSpiffFor(String),
    SpiffViewRead(String, Result<xtime::ro::RoView, LoadError>),
    /// The Print window's "mark the RO spiffed once it's sent"
    MarkAfterPrint(bool),
    /// A print finished: what it was, how it went, and the spiff to mark once it's sent (RO, spiff)
    Printed(String, Result<(), String>, Option<(String, hours::Spiff)>),
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
    /// ROs read from Xtime (one just added, …): their lines, stage and parts kept up to date
    RosRead(Vec<(String, Option<xtime::ro::RoView>)>),
    /// ROs opened on another computer (not on your list) looked up in Xtime: opened here once their VIN is known
    WaitingRead(Vec<(String, Option<xtime::ro::RoView>)>),
    /// Notifications: the list opened or closed, one clicked (goes to its RO), all marked read, the list emptied, the
    /// pop-up's time up (or closed)
    ToggleNotes,
    OpenNote(u64),
    NotesAllRead,
    ClearNotes,
    ToastGone(u64),
    /// An RO just opened from a notification: its Xtime page shown
    NoteShowXtime(String),
    /// Accept on an RO in Dispatch (asks to be sure first: None takes the question away), Yes, and how it went
    AskAccept(Option<String>),
    /// Unfolds the Dispatch list under My ROs, or folds it away
    ToggleDispatch,
    AcceptRo(String),
    Accepted(String, Result<(), LoadError>),
    /// "Add an RO": opened (true) or closed, what's typed (the RO number, the VIN), Add, and what Xtime said
    AddRo(bool),
    AddRoNumber(String),
    AddRoVin(String),
    SubmitAddRo,
    AddRoFound(String, Result<xtime::ro::RoView, LoadError>),
    /// Flag a line of the RO showing (by Xtime's line ID), or take the flag off
    FlagLine(String, bool),
    /// The hours report: open, close, which pay period (0 this one, -1 the one before…)
    OpenHours,
    /// The hours report's own messages (crates/hours-ui)
    Hr(hours_ui::HoursMsg),
    /// Mark the RO showing as spiffed (turned in), or not
    MarkSpiffed(String, bool),
    /// Add Service on the RO showing (for an inspection item: its ID), or edit one of its lines (Xtime's line ID)
    OpenAddService(Option<String>),
    OpenEditService(String),
    /// The Add / Edit Service window's own messages
    Svc(xtime_ui::service::SvcMsg),
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
        // sync (on unless turned off): "forgot to clock out" waits until your other computers are heard from
        let sync_on = !settings.sync_off && crate::syncing::derive_keys(&settings.store_id, &settings.employee_number).is_some();
        let opened = timecard::open(sync_on);
        let notice = match (opened.imported, opened.auto_clocked_out) {
            (Some(n), _) => Some(format!("Brought over your hours from the old app ({n} flagged lines)")),
            (None, Some(at)) => Some(clocked_out_notice(at)),
            _ => None,
        };
        let screen = if settings.needs_setup() { Screen::Setup } else { Screen::Main };
        let outer = Transition::new((screen.clone(), Mode::Workbench, None));
        let http = Http::with_saved_cookies(USER_AGENT, &cookie_file())
            .or_else(|_| Http::new(USER_AGENT)) // saved sign-ins unreadable: start signed out
            .expect("the app's web requests");
        let xtime = Xtime::new(http.clone(), xtime::paths::DATA_HOST).expect("Xtime's address");
        // (its requests are recorded in manual.log, as the Electron app's: addresses and cookie names, never values)
        let ford = ford::Ford::new(http.clone(), settings.dealer_code.clone())
            .expect("PTS's address")
            .with_log(crate::settings::app_dir().join("manual.log"))
            // the Workshop Manual's contents, kept a week (they're large, and the same for every RO of a vehicle)
            .with_cache(crate::settings::app_dir().join("manual-cache"));
        // chat's settings, kept from before (the Electron app's file has the same name and fields)
        let chat_saved = load_chat_config();
        if let Some(c) = &chat_saved {
            xtime.set_chat_config(c.clone());
        }
        let reports = Reports::new(
            ::feedback::Client::new(http.clone()),
            ::feedback::reporter_id(&crate::settings::app_dir()),
        );
        // the vehicles open last time (the Electron app's too), where they were left
        let saved = crate::vehicles::load();
        let open: Vec<Workspace> = saved
            .list
            .iter()
            .map(|v| {
                let mut ws = Workspace::new(v.assigned());
                ws.tab = v.tab;
                ws.manual = v.manual;
                ws.labor.saved = v.labor.clone();
                ws
            })
            .collect();
        let selected = saved.active.clone();
        let mut app = Self {
            draft: settings.clone(),
            settings,
            screen,
            mode: Mode::Workbench,
            my_ros: sample::my_ros(),
            sample_data: true,
            xtime,
            ford,
            ros_loading: false,
            ros_loaded_at: None,
            ros_error: None,
            search: String::new(),
            open,
            selected,
            sidebar: true,
            chat: false,
            notes: crate::notes::Notes::load(crate::timecard::now_ms()),
            watch: Default::default(),
            notes_open: false,
            toast: None,
            chat_panel: xtime_ui::chat::ChatPanel::new(),
            chat_saved,
            notice,
            http,
            reports,
            signing_in: None,
            hours: opened.hours,
            hours_view: hours_ui::HoursView::new(),
            print_job: None,
            service: None,
            catalogs: Default::default(),
            spiff_totals: Default::default(),
            spiff_checked: Default::default(),
            checking_spiffs: false,
            add_ro: None,
            dispatch: Vec::new(),
            accept_ask: None,
            show_dispatch: false,
            accepting: None,
            waiting_tried: Default::default(),
            now: clock::now(),
            outer,
            overlay: None,
            system_dark: crate::system_theme::is_dark(),
            inner: Transition::new(InnerKey::None),
            updates: crate::updates::Updates::new(),
            open_saved: String::new(),
            folded_lines: std::fs::read_to_string(folded_file())
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default(),
            line_edit: None,
            unflag_ask: None,
            whats_new: None,
            whats_new_for: None,
            sync: crate::syncing::Syncing::load(sync_on),
        };
        // (straight onto the vehicle showing, no transition in)
        app.outer = Transition::new(app.outer_key());
        app.inner = Transition::new(app.inner_key());
        app.open_saved = crate::vehicles::snapshot(&app.open, app.selected.as_deref());
        // what this version brings that hasn't been shown here (not over first setup)
        if app.screen == Screen::Main {
            if let Some(shown) = crate::whats_new::on_start() {
                app.whats_new = Some(shown);
                app.show_overlay(Overlay::WhatsNew);
            }
        }
        // the vehicles reopened: the tab showing read, the others' OASIS and TSBs ahead (with a PTS sign-in)
        let mut reopened = vec![app.load_manual()];
        for ro in app.open.iter().map(|w| w.ro.ro.clone()).collect::<Vec<_>>() {
            reopened.push(app.load_ahead(&ro));
        }
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
        let sync = app.sync_now();
        (app, Task::batch([focus, count, ros, Task::batch(reopened), sync]))
    }

    pub fn title(&self) -> String {
        match self.workspace() {
            Some(ws) => format!("RO {} · {} · PTS Workbench", ws.ro.ro, ws.vehicle()),
            None => "PTS Workbench".into(),
        }
    }

    pub fn theme(&self) -> Theme {
        ui::style::theme(self.settings.theme.resolve(self.system_dark) == ThemeChoice::Dark)
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
                self.restore_scroll()
            }
            other => self.apply(other),
        };
        self.remember_open();
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

    /// Looks for a newer version (unless updates are off here, or it's already looking or downloading)
    fn check_updates(&mut self) -> Task<Message> {
        if !self.updates.on() || self.updates.busy() {
            return Task::none();
        }
        self.updates.state = crate::updates::State::Checking;
        self.updates.checked_at = Some(self.now);
        Task::perform(
            crate::updates::check(self.http.clone(), self.updates.beta, self.updates.running.clone()),
            Message::UpdateFound,
        )
    }

    /// A page just shown is drawn anew (at its top): the Workshop Manual goes back where it was
    fn restore_scroll(&self) -> Task<Message> {
        match (&self.inner.shown, self.workspace()) {
            (InnerKey::Vehicle(Tab::ServiceManual, ManualTab::Workshop), Some(ws)) => {
                let ro = ws.ro.ro.clone();
                ws.wsm.restore().map(move |m| Message::Wsm(ro.clone(), m))
            }
            (InnerKey::Vehicle(Tab::ServiceManual, ManualTab::Labor), Some(ws)) => {
                let ro = ws.ro.ro.clone();
                ws.labor.restore().map(move |m| Message::Labor(ro.clone(), m))
            }
            _ => Task::none(),
        }
    }

    /// The open vehicles kept (workspaces.json) and, for sync, which ROs are open (the hours), when they've changed
    fn remember_open(&mut self) {
        let now = crate::vehicles::snapshot(&self.open, self.selected.as_deref());
        if now == self.open_saved {
            return;
        }
        self.open_saved = now;
        // (the samples aren't real vehicles: not kept)
        let real: Vec<&Workspace> = self.open.iter().filter(|w| !w.is_sample()).collect();
        let selected = self.selected.as_deref().filter(|s| real.iter().any(|w| w.ro.ro == *s));
        let saved = crate::vehicles::save(&real, selected);
        let mut open_ros = crate::vehicles::open_ros(&real);
        // (and the ones opened on another computer still waiting for their VIN here: they stay open there)
        for (ro, ymm) in &self.sync.waiting {
            open_ros.entry(ro.clone()).or_insert_with(|| hours::OpenRo { ymm: ymm.clone() });
        }
        if let Err(e) = saved {
            self.notice = Some(format!("Couldn't keep which vehicles are open: {e}"));
        }
        if self.hours.set_open_ros(open_ros) {
            self.save_hours();
        }
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
                if self.notes.read_ro(&ro) {
                    self.notes.save();
                }
                let mut ahead = Task::none();
                if !self.open.iter().any(|w| w.ro.ro == ro) {
                    if let Some(r) = self.my_ros.iter().find(|r| r.ro == ro) {
                        self.open.push(Workspace::new(r.clone()));
                        ahead = self.load_ahead(&ro);
                    }
                }
                if self.open.iter().any(|w| w.ro.ro == ro) {
                    self.selected = Some(ro.clone());
                    self.mode = Mode::Workbench;
                    if self.workspace().is_some_and(|w| w.tab == Tab::XtimeView) {
                        return Task::batch([self.load_view(&ro, false), ahead]);
                    }
                    return Task::batch([self.load_manual(), ahead]);
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
                return self.load_manual();
            }
            Message::SetManualTab(t) => {
                if let Some(ws) = self.workspace_mut() {
                    ws.tab = Tab::ServiceManual;
                    ws.manual = t;
                }
                return self.load_manual();
            }
            Message::Oasis(ro, msg) => {
                let env = self.ford_env_for(&ro);
                let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
                    return Task::none();
                };
                let (task, outs) = ws.oasis.update(msg, &env);
                let mut tasks = vec![{
                    let ro = ro.clone();
                    task.map(move |m| Message::Oasis(ro.clone(), m))
                }];
                for out in outs {
                    tasks.push(self.ford_out(&ro, out));
                }
                return Task::batch(tasks);
            }
            Message::Tsb(ro, msg) => {
                let env = self.ford_env_for(&ro);
                let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
                    return Task::none();
                };
                let (task, outs) = ws.tsb.update(msg, &env);
                let mut tasks = vec![{
                    let ro = ro.clone();
                    task.map(move |m| Message::Tsb(ro.clone(), m))
                }];
                for out in outs {
                    tasks.push(self.ford_out(&ro, out));
                }
                return Task::batch(tasks);
            }
            Message::Wsm(ro, msg) => {
                let env = self.ford_env_for(&ro);
                let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
                    return Task::none();
                };
                let (task, outs) = ws.wsm.update(msg, &env);
                let mut tasks = vec![{
                    let ro = ro.clone();
                    task.map(move |m| Message::Wsm(ro.clone(), m))
                }];
                for out in outs {
                    tasks.push(self.ford_out(&ro, out));
                }
                return Task::batch(tasks);
            }
            Message::Labor(ro, msg) => {
                let env = self.ford_env_for(&ro);
                let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
                    return Task::none();
                };
                let (task, outs) = ws.labor.update(msg, &env);
                let mut tasks = vec![{
                    let ro = ro.clone();
                    task.map(move |m| Message::Labor(ro.clone(), m))
                }];
                for out in outs {
                    tasks.push(self.ford_out(&ro, out));
                }
                return Task::batch(tasks);
            }
            Message::Diag(ro, msg) => {
                let env = self.ford_env_for(&ro);
                let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
                    return Task::none();
                };
                let (task, outs) = ws.diag.update(msg, &env);
                let mut tasks = vec![{
                    let ro = ro.clone();
                    task.map(move |m| Message::Diag(ro.clone(), m))
                }];
                for out in outs {
                    tasks.push(self.ford_out(&ro, out));
                }
                return Task::batch(tasks);
            }
            Message::Doc(ro, key, msg) => {
                let env = self.ford_env_for(&ro);
                let Some(d) = self
                    .open
                    .iter_mut()
                    .find(|w| w.ro.ro == ro)
                    .and_then(|w| w.docs.iter_mut().find(|d| d.key == key))
                else {
                    return Task::none();
                };
                let (task, outs) = d.update(msg, &env);
                let mut tasks = vec![{
                    let ro = ro.clone();
                    task.map(move |m| Message::Doc(ro.clone(), key, m))
                }];
                for out in outs {
                    tasks.push(self.ford_out(&ro, out));
                }
                return Task::batch(tasks);
            }
            Message::CloseDoc(key) => {
                if let Some(ws) = self.workspace_mut() {
                    ws.docs.retain(|d| d.key != key);
                    if ws.manual == ManualTab::Doc(key) {
                        ws.manual = ManualTab::Oasis;
                    }
                }
            }
            Message::PtsSetUp(ro, outcome) => {
                self.signing_in = None;
                let r = match outcome {
                    Outcome::SignedIn(cookies) => {
                        self.http.add_site_cookies(&cookies);
                        let _ = self.http.save_cookies(&cookie_file());
                        self.ford.reset();
                        Ok(())
                    }
                    Outcome::Cancelled => Err("the window was closed".to_string()),
                    Outcome::Failed(e) => Err(e),
                };
                // the TSBs tab, when it was waiting on PTS too
                let ok = r.is_ok();
                let env = self.ford_env_for(&ro);
                let tsb = match self.open.iter_mut().find(|w| w.ro.ro == ro) {
                    Some(ws) if ok && ws.tsb.error == Some(ford_ui::FordError::NotSetUp) => {
                        let ro = ro.clone();
                        ws.tsb.load(&env, true).map(move |m| Message::Tsb(ro.clone(), m))
                    }
                    _ => Task::none(),
                };
                // and the Workshop Manuals waiting on it (this RO's, and any other's that asked meanwhile)
                let wsm = Task::batch(
                    self.open
                        .iter()
                        .filter(|w| w.ro.ro == ro || w.wsm.setting_up)
                        .map(|w| Task::done(Message::Wsm(w.ro.ro.clone(), ford_ui::ManualMsg::SetUp(r.clone()))))
                        .collect::<Vec<_>>(),
                );
                return Task::batch([Task::done(Message::Oasis(ro, ford_ui::OasisMsg::SetUp(r))), tsb, wsm]);
            }
            Message::ToggleSidebar => self.sidebar = !self.sidebar,
            Message::ToggleChat => {
                self.chat = !self.chat;
                if self.chat {
                    self.notes_open = false;
                }
                // opened: read it again unless it was just read
                let stale = self.chat_panel.loaded_at.is_none_or(|at| clock::now() - at >= 30);
                if self.chat && stale {
                    return self.refresh_chat();
                }
            }
            Message::RefreshChat => return self.refresh_chat(),
            Message::ToggleNotes => {
                self.notes_open = !self.notes_open;
                if self.notes_open {
                    self.chat = false;
                }
            }
            Message::OpenNote(id) => return self.open_note(id),
            Message::NoteShowXtime(ro) => {
                if let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) {
                    ws.tab = Tab::XtimeView;
                }
                if self.selected.as_deref() == Some(ro.as_str()) {
                    return self.load_view(&ro, false);
                }
            }
            Message::NotesAllRead => {
                self.notes.mark_all_read();
                self.notes.save();
            }
            Message::ClearNotes => {
                self.notes.list.clear();
                self.notes.save();
                self.toast = None;
            }
            Message::ToastGone(id) => match self.toast {
                // shown its time: it slides away, then it's gone
                Some((t, Phase::Still | Phase::Entering(_))) if t == id => {
                    self.toast = Some((t, Phase::Leaving(std::time::Instant::now())));
                    return Task::perform(tokio::time::sleep(anim::MODAL_OUT), move |_| Message::ToastGone(id));
                }
                Some((t, Phase::Leaving(_))) if t == id => self.toast = None,
                _ => {}
            },
            Message::Chat(msg) => {
                let env = self.xtime_env();
                let ros = self.my_ros.iter().map(|r| r.ro.clone()).collect();
                let listed_before = self.chat_panel.loaded_at;
                let (task, outs) = self.chat_panel.update(msg, &env, ros);
                let mut tasks = vec![task.map(Message::Chat)];
                // the list read again: messages from others not seen before
                if self.chat_panel.loaded_at != listed_before && self.chat_panel.error.is_none() {
                    let list: Vec<crate::notes::ChatNow> = self
                        .chat_panel
                        .messages
                        .iter()
                        .map(|m| crate::notes::ChatNow {
                            id: m.id.clone(),
                            channel: m.channel_id.clone(),
                            ro: m.ro.clone(),
                            sender: m.sender.clone(),
                            text: m.text.clone(),
                            mine: m.mine,
                            mentions: m.mentions.clone(),
                        })
                        .collect();
                    let news = self.watch.chat(&list, &self.settings.name);
                    tasks.push(self.notify(news));
                }
                for out in outs {
                    match out {
                        xtime_ui::chat::ChatOut::SignedOut => self.ros_error = Some(LoadError::SignedOut),
                        xtime_ui::chat::ChatOut::OpenRo(ro) => tasks.push(Task::done(Message::OpenRo(ro))),
                        xtime_ui::chat::ChatOut::Learned(c) => {
                            if self.chat_saved.as_ref() != Some(&c) && save_chat_config(&c).is_ok() {
                                self.chat_saved = Some(c);
                            }
                        }
                    }
                }
                return Task::batch(tasks);
            }
            Message::Copy(value, what) => {
                self.notice = Some(format!("Copied {}: {value}", what.to_lowercase()));
                return iced::clipboard::write(value);
            }
            Message::Xv(ro, msg) => {
                let env = self.xtime_env();
                let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
                    return Task::none();
                };
                let (task, outs) = ws.xv.update(msg, &env);
                let task = {
                    let ro = ro.clone();
                    task.map(move |m| Message::Xv(ro.clone(), m))
                };
                return Task::batch([task, self.xtime_outs(&ro, outs)]);
            }
            Message::RefreshView => {
                let showing = self.workspace().filter(|w| self.mode == Mode::Workbench && w.tab == Tab::XtimeView);
                if let Some(ro) = showing.map(|w| w.ro.ro.clone()) {
                    return self.load_view(&ro, true);
                }
            }
            Message::PrintSpiff => {
                let Some(v) = self.workspace().and_then(|w| w.xv.view.clone()) else {
                    return Task::none();
                };
                return self.print_spiff(v);
            }
            Message::PrintSpiffFor(ro) => {
                // an RO open in the Workbench has been read already; any other is read from Xtime first
                if let Some(v) = self.open.iter().find(|w| w.ro.ro == ro).and_then(|w| w.xv.view.clone()) {
                    return self.print_spiff(v);
                }
                if self.sample_data {
                    return Task::none();
                }
                self.notice = Some(format!("Reading RO {ro} from Xtime to print its spiff…"));
                let (x, store) = (self.xtime.clone(), self.settings.store_id.clone());
                return Task::perform(
                    async move {
                        let r = x.ro_view(&store, &ro).await.map_err(LoadError::from);
                        (ro, r)
                    },
                    |(ro, r)| Message::SpiffViewRead(ro, r),
                );
            }
            Message::SpiffViewRead(ro, result) => match result {
                Ok(v) => {
                    self.notice = None;
                    self.hours.set_spiff_total(&ro, v.spiff_total());
                    self.spiff_totals.insert(ro, v.spiff_total());
                    return self.print_spiff(v);
                }
                Err(e) => {
                    self.notice = Some(format!(
                        "Couldn't read RO {ro} to print its spiff ({})",
                        e.text().trim_start_matches("Xtime: ")
                    ))
                }
            },
            Message::MarkAfterPrint(on) => {
                if let Some(job) = &mut self.print_job {
                    job.mark_spiffed = on;
                }
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
                let mark = job.spiff.clone().filter(|_| job.mark_spiffed);
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || crate::print::print_to(&printer, copies, &*build))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    move |r| Message::Printed(what.clone(), r, mark.clone()),
                );
            }
            Message::ClosePrint => {
                if matches!(self.overlay, Some((Overlay::Print, _))) {
                    return self.close_overlay();
                }
            }
            Message::Printed(what, result, mark) => match result {
                Ok(()) => {
                    self.notice = Some(format!("{what} sent to the printer"));
                    // marked spiffed once it's sent (the Print window's option)
                    if let Some((ro, spiff)) = mark {
                        let at = timecard::now_ms();
                        self.hours.set_spiffed(&ro, Some(hours::Spiff { at, ..spiff }));
                        self.save_hours();
                        self.notice = Some(format!("{what} sent to the printer, and RO {ro} marked spiffed"));
                    }
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
                return self.took_views(found);
            }
            Message::RosRead(found) => return self.took_views(found),
            Message::WaitingRead(found) => {
                let mut tasks = Vec::new();
                for (ro, v) in &found {
                    let Some(v) = v.as_ref().filter(|v| !v.vehicle.vin.is_empty()) else {
                        continue;
                    };
                    if self.sync.waiting.remove(ro).is_none() || self.sample_data || self.open.iter().any(|w| &w.ro.ro == ro) {
                        continue;
                    }
                    self.open.push(Workspace::new(xtime_ui::my_ros::from_view(v)));
                    tasks.push(self.load_ahead(ro));
                }
                tasks.push(self.took_views(found));
                return Task::batch(tasks);
            }
            Message::AddRo(open) => {
                self.add_ro = open.then(AddRo::default);
                if open {
                    return iced::widget::text_input::focus(add_ro_id());
                }
            }
            Message::AddRoNumber(t) => {
                if let Some(a) = &mut self.add_ro {
                    a.ro = t.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(20).collect();
                    a.error = None;
                }
            }
            Message::AddRoVin(t) => {
                if let Some(a) = &mut self.add_ro {
                    a.vin = t
                        .trim()
                        .to_uppercase()
                        .chars()
                        .filter(char::is_ascii_alphanumeric)
                        .take(17)
                        .collect();
                }
            }
            Message::SubmitAddRo => return self.submit_add_ro(),
            Message::AskAccept(ro) => self.accept_ask = ro,
            Message::ToggleDispatch => {
                self.show_dispatch = !self.show_dispatch;
                self.accept_ask = None;
            }
            Message::AcceptRo(ro) => {
                self.accept_ask = None;
                if self.accepting.is_some() || self.sample_data {
                    return Task::none();
                }
                let tech = self.settings.tech_id.trim().to_string();
                if tech.is_empty() {
                    self.notice = Some("Your technician ID isn't known yet: check your employee number in Settings.".into());
                    return Task::none();
                }
                self.accepting = Some(ro.clone());
                let (x, store) = (self.xtime.clone(), self.settings.store_id.clone());
                return Task::perform(
                    async move {
                        let r = x.accept_ro(&store, &ro, &tech).await.map_err(LoadError::from);
                        (ro, r)
                    },
                    |(ro, r)| Message::Accepted(ro, r),
                );
            }
            Message::Accepted(ro, r) => {
                self.accepting = None;
                match r {
                    Ok(()) => {
                        self.watch.moved_here(&ro, 3, timecard::now_ms());
                        self.notice = Some(format!("RO {ro} is yours: it's in Inspection now."));
                        // yours now: on your list (until the dashboard's next load says so too), and opened
                        if let Some(i) = self.dispatch.iter().position(|d| d.ro == ro) {
                            let mut r = self.dispatch.remove(i);
                            r.mode = Some(3.0);
                            if !self.my_ros.iter().any(|m| m.ro == ro) {
                                self.my_ros.push(r);
                            }
                        }
                        let open = self.update(Message::OpenRo(ro));
                        // (Xtime's list read again now, unless it's being read, so everything says where it really is)
                        return Task::batch([open, self.load_ros()]);
                    }
                    Err(LoadError::SignedOut) => {
                        self.ros_error = Some(LoadError::SignedOut);
                        self.notice = Some(format!("Couldn't accept RO {ro}: Xtime is signed out. Sign in and try again."));
                    }
                    Err(e) => self.notice = Some(format!("Couldn't accept RO {ro} ({}).", e.plain())),
                }
            }
            Message::AddRoFound(ro, r) => {
                let Some(a) = &mut self.add_ro else { return Task::none() };
                if a.ro != ro || !a.looking {
                    return Task::none(); // closed, or another RO asked for meanwhile
                }
                a.looking = false;
                match r {
                    Ok(v) if !v.vehicle.vin.is_empty() => {
                        let r = xtime_ui::my_ros::from_view(&v);
                        return self.open_added(r, Some(v));
                    }
                    Ok(_) => {
                        a.error = Some(format!("Xtime has no VIN on RO {ro} yet. Type it below to open the RO anyway."));
                        a.ask_vin = true;
                    }
                    Err(e) => {
                        a.error = Some(match e {
                            LoadError::SignedOut => {
                                "Xtime is signed out, so the RO can't be looked up. Sign in to Xtime, or type the VIN to open it anyway."
                                    .into()
                            }
                            LoadError::NotFound(_) => {
                                format!("Xtime doesn't have RO {ro}. Check the number, or type the VIN to open it anyway.")
                            }
                            e => format!("Couldn't look up RO {ro} ({}). Type the VIN to open it anyway.", e.plain()),
                        });
                        a.ask_vin = true;
                    }
                }
            }
            Message::FlagLine(line_id, on) => return self.flag_line(&line_id, on),
            Message::Hr(msg) => {
                let (task, outs) = self.hours_view.update(&mut self.hours, msg);
                let mut tasks = vec![task.map(Message::Hr)];
                for out in outs {
                    match out {
                        hours_ui::HoursOut::Changed => self.save_hours(),
                        hours_ui::HoursOut::Notice(n) => self.notice = Some(n),
                        hours_ui::HoursOut::PrintSpiff(ro) => tasks.push(Task::done(Message::PrintSpiffFor(ro))),
                        hours_ui::HoursOut::MarkSpiffed(ro) => tasks.push(Task::done(Message::MarkSpiffed(ro, true))),
                        hours_ui::HoursOut::Close => {
                            if matches!(self.overlay, Some((Overlay::Hours, _))) {
                                tasks.push(self.close_overlay());
                            }
                        }
                    }
                }
                return Task::batch(tasks);
            }
            Message::OpenHours => {
                self.hours_view = hours_ui::HoursView::new();
                self.show_overlay(Overlay::Hours);
            }
            Message::MarkSpiffed(ro, on) => {
                let spiff = on.then(|| self.spiff_record(&ro));
                self.hours.set_spiffed(&ro, spiff);
                self.notice = Some(match timecard::save(&self.hours) {
                    Ok(()) if on => format!("RO {ro} marked spiffed"),
                    Ok(()) => format!("RO {ro} no longer marked spiffed"),
                    Err(e) => format!("Couldn't save the spiff: {e}"),
                });
            }
            Message::OpenAddService(item) => {
                let Some(ws) = self.workspace() else {
                    return Task::none();
                };
                if self.sample_data || self.service.is_some() {
                    return Task::none();
                }
                let ro = ws.ro.ro.clone();
                // for an inspection item: its name and the services offered for it; a Yellow or Red item's colour
                let item = item.and_then(|k| ws.xv.form.as_ref().and_then(|f| f.item(&k)).cloned());
                let mut ed =
                    xtime_ui::service::ServiceEditor::new(ro.clone(), None, item.as_ref().map(|i| (i.name.clone(), i.services.clone())));
                ed.start_color = item.map(|i| i.color).filter(|c| c == "Yellow" || c == "Red");
                return self.open_service(ed);
            }
            Message::OpenEditService(line_id) => {
                let Some(ws) = self.workspace() else {
                    return Task::none();
                };
                if self.service.is_some() {
                    return Task::none();
                }
                let Some(line) = ws
                    .xv
                    .view
                    .as_ref()
                    .and_then(|v| v.lines.iter().find(|l| l.key() == line_id))
                    .cloned()
                else {
                    return Task::none();
                };
                let ed = xtime_ui::service::ServiceEditor::new(ws.ro.ro.clone(), Some(line), None);
                return self.open_service(ed);
            }
            Message::Svc(xtime_ui::service::SvcMsg::Close) => {
                if matches!(self.overlay, Some((Overlay::Service, _))) {
                    return self.close_overlay();
                }
            }
            Message::Svc(msg) => {
                if let xtime_ui::service::SvcMsg::CatalogLoaded(ro, Ok(c)) = &msg {
                    self.catalogs.insert(ro.clone(), c.clone());
                }
                let (store, x) = (self.settings.store_id.clone(), self.xtime.clone());
                let Some(ed) = &mut self.service else {
                    return Task::none();
                };
                let (task, done) = ed.update(msg, &x, &store);
                if done {
                    // saved: say so, close the window, read the RO again
                    let (ro, notice) = (ed.ro.clone(), ed.done_notice());
                    self.notice = Some(notice);
                    return Task::batch([self.close_overlay(), self.load_view(&ro, true)]);
                }
                return task.map(Message::Svc);
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
                        self.ford.set_dealer_code(next.dealer_code.clone());
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
                // signed in to PTS: the Service Manual tab showing tries again
                if site == Site::Ford && self.signed_in(Site::Ford) {
                    self.ford.reset();
                    for ws in &mut self.open {
                        if ws.oasis.error == Some(ford_ui::FordError::SignedOut) {
                            ws.oasis.error = None;
                        }
                        if ws.tsb.error == Some(ford_ui::FordError::SignedOut) {
                            ws.tsb.error = None;
                        }
                        if ws.wsm.error == Some(ford_ui::FordError::SignedOut) {
                            ws.wsm.error = None;
                        }
                        for d in &mut ws.wsm.docs {
                            if d.error == Some(ford_ui::FordError::SignedOut) {
                                d.error = None;
                            }
                        }
                    }
                    return self.load_manual();
                }
            }
            Message::LoadRos => return self.load_ros(),
            Message::RosLoaded(result) => {
                let ok = result.is_ok();
                self.ros_loaded(result);
                if ok {
                    // ROs opened on another computer whose VIN is now known; the chat's new messages (and the Chat
                    // button's count) with them, every minute
                    let waiting = self.open_waiting();
                    // what's changed on your list since the last look: notifications
                    let list: Vec<crate::notes::RoNow> = self
                        .my_ros
                        .iter()
                        .map(|r| crate::notes::RoNow {
                            ro: r.ro.clone(),
                            tag: r.tag.clone(),
                            vehicle: xtime_ui::my_ros::vehicle_name(r),
                            mode: r.mode.map(|m| m as i64),
                            waiting_parts: r.waiting_parts,
                            not_in_stock: r.dash.not_in_stock,
                            waiting_customer: r.waiting_customer,
                            waiting_warranty: r.waiting_warranty,
                            parts_status: parts_status_number(r.dash.parts_status.as_ref()),
                        })
                        .collect();
                    let news = self.watch.dashboard(&list, timecard::now_ms());
                    let notify = self.notify(news);
                    return Task::batch([self.check_spiffs(), self.refresh_chat(), waiting, notify]);
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
                // sync: every minute (and soon after a change, see SyncDue)
                let sync_due = self.sync.last_run.is_none_or(|t| self.now - t >= crate::syncing::EVERY);
                let sync = if sync_due { self.sync_now() } else { Task::none() };
                // updates: shortly after starting, then every half hour
                if self.updates.on() && self.updates.due(self.now) {
                    return Task::batch([sync, self.check_updates()]);
                }
                return sync;
            }
            Message::Arrive(_) => {} // handled in update
            Message::CheckUpdates => return self.check_updates(),
            Message::ToggleLines(ro) => {
                if !self.folded_lines.remove(&ro) {
                    self.folded_lines.insert(ro);
                }
                let _ = std::fs::write(folded_file(), serde_json::to_string(&self.folded_lines).unwrap_or_default());
            }
            Message::EditLine(ro, key, field) => {
                let Some(line) = self.hours.ro_summary(&ro).and_then(|s| s.lines.into_iter().find(|l| l.key == key)) else {
                    return Task::none();
                };
                let value = match field {
                    LineField::Hours => hours_ui::fmt_h(line.paid).trim_end_matches(" h").to_string(),
                    LineField::Date => match line.flag {
                        Some(f) => crate::ui::lines::edit_date(f.flagged_at),
                        None => return Task::none(),
                    },
                };
                self.line_edit = Some(LineEdit { ro, key, field, value });
                self.unflag_ask = None;
                let id = crate::ui::lines::edit_id();
                return Task::batch([
                    iced::widget::text_input::focus(id.clone()),
                    iced::widget::text_input::select_all(id),
                ]);
            }
            Message::LineInput(v) => {
                if let Some(e) = &mut self.line_edit {
                    e.value = v;
                }
            }
            Message::CancelLine => self.line_edit = None,
            Message::SaveLine => {
                let Some(e) = self.line_edit.clone() else { return Task::none() };
                let flag = self.hours.flag_for(&e.ro, &e.key).map(|f| f.id);
                let v = e.value.trim().replace(',', ".");
                match e.field {
                    // empty: back to Xtime's hours
                    LineField::Hours if v.is_empty() => self.hours.set_override(&e.ro, &e.key, None),
                    LineField::Hours => match v.parse::<f64>() {
                        Ok(n) if n.is_finite() && n >= 0.0 => match flag {
                            Some(id) => self.hours.set_flag_hours(id, n),
                            None => self.hours.set_override(&e.ro, &e.key, Some(n)),
                        },
                        _ => {
                            self.notice = Some("The hours should be a number, like 1.5".into());
                            return Task::none();
                        }
                    },
                    LineField::Date => {
                        let year = chrono::Datelike::year(&chrono::Local::now());
                        match (crate::ui::lines::parse_date(&v, year), flag) {
                            (Some(at), Some(id)) => self.hours.set_flagged_at(id, at),
                            _ => {
                                self.notice = Some("Type the date like 11/2/2026".into());
                                return Task::none();
                            }
                        }
                    }
                }
                self.line_edit = None;
                self.save_hours();
            }
            Message::SidebarFlag(ro, key, on) => {
                self.line_edit = None;
                if on {
                    self.unflag_ask = None;
                    if self.hours.flag_line(&ro, &key, timecard::now_ms()) {
                        self.save_hours();
                    }
                } else {
                    // a flag already confirmed on your time sheet: asked once more before it's taken off
                    let confirmed = self.hours.flag_for(&ro, &key).is_some_and(|f| f.confirmed);
                    let asked = self.unflag_ask.as_ref().is_some_and(|(r, k)| *r == ro && *k == key);
                    if confirmed && !asked {
                        self.unflag_ask = Some((ro, key));
                    } else {
                        self.unflag_ask = None;
                        self.hours.unflag_line(&ro, &key);
                        self.save_hours();
                    }
                }
            }
            Message::UpdateFound(found) => {
                use crate::updates::State;
                match found {
                    // (already downloaded and waiting for a restart)
                    Ok(Some(f)) if matches!(&self.updates.state, State::Ready(r) if r.version == f.manifest.version) => {}
                    Ok(Some(f)) => {
                        self.updates.state = State::Downloading(f.manifest.version.clone());
                        return Task::perform(crate::updates::download(self.http.clone(), f), Message::UpdateDownloaded);
                    }
                    Ok(None) => self.updates.state = State::Current(None),
                    Err(e) => self.updates.state = State::Failed(e),
                }
            }
            Message::UpdateDownloaded(r) => {
                use crate::updates::State;
                match r {
                    Ok(ready) => {
                        // what it brings, once (unless a card's up: then it's in Settings, and on the next start)
                        let new_notes = self.whats_new_for.as_deref() != Some(ready.version.as_str());
                        if new_notes && self.overlay.is_none() {
                            if let Some(shown) = crate::whats_new::for_update(&ready.version, &ready.notes) {
                                self.whats_new_for = Some(ready.version.clone());
                                self.whats_new = Some(shown);
                                self.show_overlay(Overlay::WhatsNew);
                            }
                        }
                        self.updates.state = State::Ready(ready);
                    }
                    Err(e) => self.updates.state = State::Failed(e),
                }
            }
            Message::SyncNow => return self.sync_now(),
            Message::SyncDue => {
                if self.sync.due.is_some_and(|d| d <= std::time::Instant::now()) {
                    self.sync.due = None;
                    return self.sync_now();
                }
            }
            Message::Synced(outcome) => return self.synced(*outcome),
            Message::SetSync(on) => {
                self.settings.sync_off = !on;
                self.draft.sync_off = !on;
                if let Err(e) = self.settings.save() {
                    self.notice = Some(format!("Couldn't save the setting: {e}"));
                }
                if on {
                    // joining afresh: what's here fills gaps, never overrules your other computers
                    self.sync.state.rejoin();
                    self.sync.save();
                    return self.sync_now();
                }
                self.sync.status = crate::syncing::Status::Off;
                self.sync.due = None;
            }
            Message::ShowWhatsNew => {
                self.whats_new = Some(match &self.updates.state {
                    crate::updates::State::Ready(r) => crate::whats_new::for_update(&r.version, &r.notes)
                        .map(|s| crate::whats_new::Shown { all: true, ..s })
                        .unwrap_or_else(crate::whats_new::recent),
                    _ => crate::whats_new::recent(),
                });
                self.show_overlay(Overlay::WhatsNew);
            }
            Message::CloseWhatsNew => {
                if let Some(s) = &self.whats_new {
                    crate::whats_new::mark_seen(&s.groups);
                }
                return self.close_overlay();
            }
            Message::SetBetaUpdates(on) => {
                self.updates.set_beta(on);
                return self.check_updates();
            }
            Message::RestartToUpdate => {
                if let crate::updates::State::Ready(r) = self.updates.state.clone() {
                    // (what's new in it was seen: not shown again once it's started)
                    if let Some(s) = &self.whats_new {
                        crate::whats_new::mark_seen(&s.groups);
                    }
                    match crate::updates::restart(&r) {
                        // (hours are kept as they change; the window closes and the launcher starts the new version)
                        Ok(()) => return iced::exit(),
                        Err(e) => self.updates.state = crate::updates::State::Failed(e),
                    }
                }
            }
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
                    // kept until now: the card shows it while it goes
                    match which {
                        Overlay::Print => self.print_job = None,
                        Overlay::Service => self.service = None,
                        Overlay::WhatsNew => self.whats_new = None,
                        _ => {}
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

    /// What the Service Manual's tabs ask Ford with
    /// What a vehicle's Service Manual asks Ford with (nothing, for a sample)
    fn ford_env_for(&self, ro: &str) -> ford_ui::Env {
        ford_ui::Env {
            ford: self.ford.clone(),
            sample: self.open.iter().find(|w| w.ro.ro == ro).is_some_and(Workspace::is_sample),
        }
    }

    /// Reads the Service Manual tab showing, when it hasn't been read yet
    fn load_manual(&mut self) -> Task<Message> {
        let env = self.ford_env_for(self.selected.as_deref().unwrap_or(""));
        let Some(ws) = self.workspace_mut().filter(|w| w.tab == Tab::ServiceManual) else {
            return Task::none();
        };
        let ro = ws.ro.ro.clone();
        match ws.manual {
            ManualTab::Oasis => ws.oasis.load(&env, false).map(move |m| Message::Oasis(ro.clone(), m)),
            ManualTab::Tsbs => ws.tsb.load(&env, false).map(move |m| Message::Tsb(ro.clone(), m)),
            ManualTab::Workshop => ws.wsm.load(&env, false).map(move |m| Message::Wsm(ro.clone(), m)),
            // ready to type in (nothing to read until something's searched)
            ManualTab::Diagnostics if ws.diag.search.is_none() => ws.diag.focus().map(move |m| Message::Diag(ro.clone(), m)),
            // SLTS opened (the first time)
            ManualTab::Labor => ws.labor.load(&env).map(move |m| Message::Labor(ro.clone(), m)),
            _ => Task::none(),
        }
    }

    /// A vehicle just opened: its OASIS is read in the background (as the Electron app's "loading ahead"), so it's
    /// there by the time its tab is opened. Only with a PTS sign-in (nothing asks to sign in from here).
    fn load_ahead(&mut self, ro: &str) -> Task<Message> {
        let env = self.ford_env_for(ro);
        if env.sample || !self.signed_in(Site::Ford) {
            return Task::none();
        }
        let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
            return Task::none();
        };
        let (r1, r2, r3, r4) = (ro.to_string(), ro.to_string(), ro.to_string(), ro.to_string());
        Task::batch([
            ws.oasis.load(&env, false).map(move |m| Message::Oasis(r1.clone(), m)),
            ws.tsb.load(&env, false).map(move |m| Message::Tsb(r2.clone(), m)),
            // the manual's contents (kept on disk a week, so usually quick)
            ws.wsm.load(&env, false).map(move |m| Message::Wsm(r3.clone(), m)),
            // SLTS opened, so Labor Times is ready (as the Electron app does)
            ws.labor.load(&env).map(move |m| Message::Labor(r4.clone(), m)),
        ])
    }

    /// What a Service Manual tab hands back: something to copy, or Ford's sign-in
    fn ford_out(&mut self, ro: &str, out: ford_ui::Out) -> Task<Message> {
        match out {
            ford_ui::Out::Copy(text, what) => Task::done(Message::Copy(text, what)),
            ford_ui::Out::SignIn => Task::done(Message::SignIn(Site::Ford)),
            // one of Ford's documents: a Service Manual tab of its own (the one already open for it, if any)
            ford_ui::Out::OpenDoc { kind, id, title, url } => {
                let env = self.ford_env_for(ro);
                let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
                    return Task::none();
                };
                ws.tab = Tab::ServiceManual;
                if let Some(d) = ws.docs.iter().find(|d| d.url == url) {
                    ws.manual = ManualTab::Doc(d.key);
                    return Task::none();
                }
                let key = ws.next_doc;
                ws.next_doc += 1;
                let mut d = ford_ui::DocTab::new(key, &kind, &id, &title, &url);
                let ro = ro.to_string();
                let task = d.load(&env).map(move |m| Message::Doc(ro.clone(), key, m));
                ws.docs.push(d);
                ws.manual = ManualTab::Doc(key);
                task
            }
            // from the Diagnostics tab: a procedure or pinpoint test in the Workshop Manual tab (in a tab of its own, at
            // the test)
            ford_ui::Out::OpenProcedure { link, title } => {
                let env = self.ford_env_for(ro);
                let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
                    return Task::none();
                };
                ws.tab = Tab::ServiceManual;
                ws.manual = ManualTab::Workshop;
                let r = ro.to_string();
                let r2 = ro.to_string();
                Task::batch([
                    ws.wsm.load(&env, false).map(move |m| Message::Wsm(r.clone(), m)),
                    ws.wsm.open_procedure(&env, link, title).map(move |m| Message::Wsm(r2.clone(), m)),
                ])
            }
            // …and a bulletin in the TSBs tab
            // Labor Times' printout, in the app's Print window
            ford_ui::Out::Print(job) => {
                let what = job.title.clone();
                let printed = format!("Printed {}", chrono::Local::now().format("%b %-d, %Y, %-I:%M %p"));
                let build: crate::print::Build = std::sync::Arc::new(move |m| crate::print::labor::sheet(m, &job, &printed));
                self.open_print(what, build)
            }
            ford_ui::Out::OpenBulletin { kind, id, title, url } => {
                let env = self.ford_env_for(ro);
                let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
                    return Task::none();
                };
                ws.tab = Tab::ServiceManual;
                ws.manual = ManualTab::Tsbs;
                let r = ro.to_string();
                let r2 = ro.to_string();
                let article = ford_ui::article::ArticleView::new(&kind, &id, &title, "", &url);
                Task::batch([
                    ws.tsb.load(&env, false).map(move |m| Message::Tsb(r.clone(), m)),
                    ws.tsb.open_article(&env, article).map(move |m| Message::Tsb(r2.clone(), m)),
                ])
            }
            // opening the vehicle in PTS (unseen) to set it up: one window at a time
            ford_ui::Out::SetUpPts(vin) => {
                let ro = ro.to_string();
                // already setting PTS up (OASIS asked too): its finishing reads this again
                if self.signing_in == Some(Site::Ford) {
                    return Task::none();
                }
                if self.signing_in.is_some() {
                    let busy = Err("another PTS or Xtime window is open".to_string());
                    return Task::done(Message::Oasis(ro, ford_ui::OasisMsg::SetUp(busy)));
                }
                self.signing_in = Some(Site::Ford);
                Task::perform(signin::set_up_pts(vin, web_dir()), move |o| Message::PtsSetUp(ro.clone(), o))
            }
        }
    }

    /// What the Xtime View asks Xtime with
    fn xtime_env(&self) -> xtime_ui::Env {
        xtime_ui::Env {
            xtime: self.xtime.clone(),
            store: self.settings.store_id.clone(),
            tech_id: self.settings.tech_id.clone(),
            sample: self.sample_data,
        }
    }

    /// Reads an RO for its Xtime View, unless it's being read, or was read in the last 20 s (unless `force`)
    fn load_view(&mut self, ro: &str, force: bool) -> Task<Message> {
        let env = self.xtime_env();
        let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro) else {
            return Task::none();
        };
        let ro = ro.to_string();
        ws.xv.load(&env, force).map(move |m| Message::Xv(ro.clone(), m))
    }

    /// What the Xtime View hands back: a notice, signed out, the RO just read (your hours keep its lines; its spiff
    /// total), and its buttons meant for the app
    fn xtime_outs(&mut self, ro: &str, outs: Vec<xtime_ui::Out>) -> Task<Message> {
        let mut tasks = Vec::new();
        for out in outs {
            match out {
                xtime_ui::Out::Notice(n) => self.notice = Some(n),
                xtime_ui::Out::SignedOut => self.ros_error = Some(LoadError::SignedOut), // the bottom bar offers to sign in
                xtime_ui::Out::Moved(mode) => self.watch.moved_here(ro, mode, timecard::now_ms()),
                xtime_ui::Out::Read(view) => {
                    let news = self.watch_lines(ro, &view);
                    tasks.push(self.notify(news));
                    let lines = timecard::lines_of(&view).into_iter().map(|(_, l)| l).collect();
                    self.hours.update_lines(ro, lines, &timecard::ymm(&view), timecard::now_ms());
                    self.spiff_totals.insert(ro.to_string(), view.spiff_total());
                    self.hours.set_spiff_total(ro, view.spiff_total());
                    let _ = timecard::save(&self.hours);
                    self.spiff_checked.insert(ro.to_string(), clock::now());
                    self.now = clock::now();
                }
                xtime_ui::Out::Ask(a) => tasks.push(Task::done(match a {
                    xtime_ui::Ask::AddService(item) => Message::OpenAddService(item),
                    xtime_ui::Ask::EditService(line) => Message::OpenEditService(line),
                    xtime_ui::Ask::Flag(line, on) => Message::FlagLine(line, on),
                    xtime_ui::Ask::PrintSpiff => Message::PrintSpiff,
                    xtime_ui::Ask::MarkSpiffed(on) => Message::MarkSpiffed(ro.to_string(), on),
                    xtime_ui::Ask::Copy(text, what) => Message::Copy(text, what),
                })),
            }
        }
        Task::batch(tasks)
    }

    /// Each RO's spiff for My ROs: turned in, or there to be turned in (approved recommendations over $150)
    pub fn spiff_badges(&self) -> std::collections::HashMap<String, xtime_ui::my_ros::SpiffBadge> {
        use xtime_ui::my_ros::SpiffBadge;
        self.my_ros
            .iter()
            .filter_map(|r| {
                if self.hours.spiffed(&r.ro).is_some() {
                    return Some((r.ro.clone(), SpiffBadge::Spiffed));
                }
                let t = self.spiff_totals.get(&r.ro).copied().filter(|t| xtime::ro::spiff_over(*t))?;
                Some((r.ro.clone(), SpiffBadge::Available(t)))
            })
            .collect()
    }

    /// What the Xtime View shows from the rest of the app: the time, your flags on the RO's lines, its spiff
    pub fn xtime_shown(&self, ws: &Workspace) -> xtime_ui::view::Shown {
        let flags = ws
            .xv
            .view
            .as_ref()
            .map(|v| {
                timecard::lines_of(v)
                    .into_iter()
                    .filter(|(_, l)| l.counts())
                    .map(|(id, l)| (id, self.hours.flag_for(&v.ro, &l.key).map(|f| f.hours)))
                    .collect()
            })
            .unwrap_or_default();
        xtime_ui::view::Shown {
            now: self.now,
            flags,
            spiffed: self.hours.spiffed(&ws.ro.ro).map(|s| s.total),
            can_print: self.print_job.is_none(),
            can_add_service: self.service.is_none(),
        }
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
        // (sent to your other computers soon)
        if !self.settings.sync_off {
            self.sync.changed();
        }
    }

    // ---------- sync ----------

    /// Syncs now (unless it's off, or already syncing: then once more after)
    fn sync_now(&mut self) -> Task<Message> {
        use crate::syncing::Status;
        if self.settings.sync_off {
            self.sync.status = Status::Off;
            return Task::none();
        }
        let Some(keys) = crate::syncing::derive_keys(&self.settings.store_id, &self.settings.employee_number) else {
            self.sync.status = Status::Failed("enter your employee number in Settings".into());
            return Task::none();
        };
        if self.sync.running {
            self.sync.again = true;
            return Task::none();
        }
        self.sync.running = true;
        self.sync.started_at = self.sync.generation;
        self.sync.last_run = Some(self.now);
        if !matches!(self.sync.status, Status::Synced(_)) {
            self.sync.status = Status::Syncing;
        }
        Task::perform(
            workbench_sync::run(
                self.http.clone(),
                crate::syncing::URL.to_string(),
                keys,
                self.hours.clone(),
                self.sync.state.clone(),
                timecard::now_ms(),
            ),
            |o| Message::Synced(Box::new(o)),
        )
    }

    /// A sync done: what came from your other computers put in (unless the hours changed here meanwhile: then it
    /// goes again), ROs opened or closed elsewhere opened or closed here, "forgot to clock out" checked the first time
    fn synced(&mut self, o: crate::syncing::Outcome) -> Task<Message> {
        use crate::syncing::Status;
        self.sync.running = false;
        let mut tasks = Vec::new();
        let unchanged_here = self.sync.generation == self.sync.started_at;
        match o.hours {
            Some(h) if unchanged_here => {
                // (what's only this computer's and changed meanwhile stays: when it was last seen running)
                let last_seen = self.hours.last_seen;
                self.hours = h;
                self.hours.last_seen = last_seen.max(self.hours.last_seen);
                if let Err(e) = timecard::save(&self.hours) {
                    self.notice = Some(format!("Couldn't save your hours: {e}"));
                }
                self.sync.state = o.state;
                tasks.push(self.reconcile_open_ros());
            }
            Some(_) => self.sync.again = true,
            None => self.sync.state = o.state,
        }
        self.sync.save();
        self.sync.status = match o.result {
            Ok(()) => Status::Synced(timecard::now_ms()),
            Err(e) => Status::Failed(e),
        };
        // the first time: a shift left open from an earlier day, now that your other computers are heard from
        if std::mem::take(&mut self.sync.check_after_first) {
            if let Some(at) = self.hours.startup_check(&chrono::Local, timecard::now_ms()) {
                let _ = timecard::save(&self.hours);
                self.notice = Some(clocked_out_notice(at));
            }
        }
        if std::mem::take(&mut self.sync.again) {
            tasks.push(self.sync_now());
        }
        Task::batch(tasks)
    }

    /// ROs opened on another computer open here (once their VIN is known: from your ROs in Xtime); ROs closed on
    /// another computer close here (only when an app that syncs open ROs closed them)
    fn reconcile_open_ros(&mut self) -> Task<Message> {
        let want = self.hours.open_ros.clone();
        let mut tasks = Vec::new();
        let mut keep = false;
        let open_here: Vec<String> = self.open.iter().filter(|w| !w.is_sample()).map(|w| w.ro.ro.clone()).collect();
        for ro in open_here {
            if want.contains_key(&ro) {
                continue;
            }
            if self.hours.open_closed_by.get(&ro).map(String::as_str) == Some("app") {
                tasks.push(Task::done(Message::CloseRo(ro)));
            } else {
                keep = true; // dropped by an older app (or never known there): it stays open, and says so again
            }
        }
        self.sync.waiting.retain(|ro, _| want.contains_key(ro));
        for (ro, w) in &want {
            if !self.open.iter().any(|x| &x.ro.ro == ro) {
                self.sync.waiting.insert(ro.clone(), w.ymm.clone());
            }
        }
        tasks.push(self.open_waiting());
        if keep {
            self.open_saved.clear(); // (kept again, with these)
        }
        Task::batch(tasks)
    }

    /// The ROs opened elsewhere that can be opened here now (their VIN is in your ROs), opened quietly
    fn open_waiting(&mut self) -> Task<Message> {
        let ready: Vec<xtime::dashboard::AssignedRo> = self
            .sync
            .waiting
            .keys()
            .filter_map(|ro| self.my_ros.iter().find(|r| &r.ro == ro && !r.vin.is_empty()).cloned())
            .collect();
        let mut tasks = Vec::new();
        for r in ready {
            self.sync.waiting.remove(&r.ro);
            if self.open.iter().any(|w| w.ro.ro == r.ro) || self.sample_data {
                continue;
            }
            let ro = r.ro.clone();
            self.open.push(Workspace::new(r));
            tasks.push(self.load_ahead(&ro));
        }
        // the rest aren't on your list (added by hand there): asked of Xtime, each at most every 10 minutes
        let now = clock::now();
        let ask: Vec<String> = self
            .sync
            .waiting
            .keys()
            .filter(|ro| !self.my_ros.iter().any(|r| &r.ro == *ro))
            .filter(|ro| self.waiting_tried.get(*ro).is_none_or(|at| now - at >= 10 * 60))
            .cloned()
            .collect();
        if !ask.is_empty() && !self.sample_data {
            for ro in &ask {
                self.waiting_tried.insert(ro.clone(), now);
            }
            let (x, store) = (self.xtime.clone(), self.settings.store_id.clone());
            tasks.push(Task::perform(
                async move {
                    let mut found = Vec::new();
                    for ro in ask {
                        let v = x.ro_view(&store, &ro).await.ok();
                        found.push((ro, v));
                    }
                    found
                },
                Message::WaitingRead,
            ));
        }
        Task::batch(tasks)
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

    /// What's kept when an RO is marked spiffed: when, what the recommendations came to, its tag and vehicle (from
    /// the RO open in the Workbench, or what the hours know of it)
    fn spiff_record(&self, ro: &str) -> hours::Spiff {
        let open = self.open.iter().find(|w| w.ro.ro == ro);
        let rec = self.hours.ros.get(ro);
        hours::Spiff {
            at: timecard::now_ms(),
            total: open
                .and_then(|w| w.xv.view.as_ref().map(|v| v.spiff_total()))
                .or_else(|| self.spiff_totals.get(ro).copied())
                .or_else(|| rec.and_then(|r| r.spiff_total))
                .unwrap_or(0.0),
            tag: open
                .map(|w| w.ro.tag.clone())
                .or_else(|| self.my_ros.iter().find(|r| r.ro == ro).map(|r| r.tag.clone()))
                .unwrap_or_default(),
            vehicle: open.map(|w| w.vehicle()).or_else(|| rec.map(|r| r.ymm.clone())).unwrap_or_default(),
        }
    }

    /// The app's Print window for an RO's spiff sheet (with "mark it spiffed once it's sent" when it isn't yet)
    fn print_spiff(&mut self, v: xtime::ro::RoView) -> Task<Message> {
        if self.print_job.is_some() || !v.spiff_earned() {
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
        let ro = v.ro.clone();
        let spiff = hours::Spiff {
            at: 0, // when it's sent
            total: v.spiff_total(),
            tag: v.tag.clone(),
            vehicle: timecard::ymm(&v),
        };
        let what = format!("Spiff for RO {ro}");
        let build: crate::print::Build = std::sync::Arc::new(move |m| crate::print::spiff::sheet(m, &v, &extra));
        let task = self.open_print(what, build);
        if let Some(job) = &mut self.print_job {
            if self.hours.spiffed(&ro).is_none() {
                job.spiff = Some((ro, spiff));
                job.mark_spiffed = true;
            }
        }
        task
    }

    /// Brings up the Add / Edit Service window (with the store's services, read once per RO)
    fn open_service(&mut self, mut ed: xtime_ui::service::ServiceEditor) -> Task<Message> {
        let task = match self.catalogs.get(&ed.ro).cloned() {
            Some(c) => {
                ed.start_with(c);
                Task::none()
            }
            None => xtime_ui::service::load_catalog(&self.xtime, &self.settings.store_id, &ed.ro).map(Message::Svc),
        };
        self.service = Some(ed);
        self.show_overlay(Overlay::Service);
        task
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

    /// Reads the chat again, when there's an Xtime sign-in to ask with (and Xtime is connected)
    fn refresh_chat(&mut self) -> Task<Message> {
        let signed_out = self.ros_error == Some(LoadError::SignedOut);
        if self.sample_data || signed_out || self.settings.needs_setup() || !self.signed_in(Site::Xtime) {
            return Task::none();
        }
        let env = self.xtime_env();
        let ros = self.my_ros.iter().map(|r| r.ro.clone()).collect();
        self.chat_panel.load(&env, ros).map(Message::Chat)
    }

    /// "Add an RO": one already open or on your list just opens; with a VIN typed it opens straight away; otherwise
    /// Xtime is asked for it (its VIN, vehicle, stage and lines)
    fn submit_add_ro(&mut self) -> Task<Message> {
        let Some(a) = &mut self.add_ro else { return Task::none() };
        let ro = a.ro.trim().to_string();
        if ro.is_empty() || a.looking {
            return Task::none();
        }
        if self.open.iter().any(|w| w.ro.ro == ro) || self.my_ros.iter().any(|r| r.ro == ro) {
            self.add_ro = None;
            return self.update(Message::OpenRo(ro));
        }
        if !a.vin.is_empty() {
            let r = AssignedRo {
                ro: ro.clone(),
                vin: a.vin.clone(),
                ..Default::default()
            };
            return self.open_added(r, None);
        }
        if self.sample_data {
            a.error = Some("Sign in to Xtime first so the RO can be looked up, or type its VIN to open it anyway.".into());
            a.ask_vin = true;
            return Task::none();
        }
        a.looking = true;
        a.error = None;
        let (x, store) = (self.xtime.clone(), self.settings.store_id.clone());
        Task::perform(
            async move {
                let r = x.ro_view(&store, &ro).await.map_err(LoadError::from);
                (ro, r)
            },
            |(ro, r)| Message::AddRoFound(ro, r),
        )
    }

    /// Opens an RO added by hand. From then on it's read from Xtime every minute while it's open (like your own ROs):
    /// its lines and approvals go in your hours, its stage and parts show in the sidebar.
    fn open_added(&mut self, r: AssignedRo, view: Option<xtime::ro::RoView>) -> Task<Message> {
        self.add_ro = None;
        let ro = r.ro.clone();
        if !self.open.iter().any(|w| w.ro.ro == ro) {
            self.open.push(Workspace::new(r));
        }
        let read = match view {
            Some(v) => self.took_views(vec![(ro.clone(), Some(v))]),
            // (the VIN typed: what Xtime has on it, if it can)
            None if !self.sample_data => {
                let (x, store, r) = (self.xtime.clone(), self.settings.store_id.clone(), ro.clone());
                Task::perform(async move { vec![(r.clone(), x.ro_view(&store, &r).await.ok())] }, Message::RosRead)
            }
            None => Task::none(),
        };
        let ahead = self.load_ahead(&ro);
        Task::batch([self.update(Message::OpenRo(ro)), ahead, read])
    }

    /// ROs just read from Xtime: their spiff totals, their lines in your hours (to flag, approvals as they come in),
    /// and for open ROs that aren't on your list (added by hand, or gone from it) their stage, parts and flags
    fn took_views(&mut self, found: Vec<(String, Option<xtime::ro::RoView>)>) -> Task<Message> {
        let now = clock::now();
        let mut changed = false;
        let mut notes = Vec::new();
        for (ro, view) in found {
            self.spiff_checked.insert(ro.clone(), now);
            let Some(v) = view else { continue };
            let news = self.watch_lines(&ro, &v);
            notes.extend(news);
            self.spiff_totals.insert(ro.clone(), v.spiff_total());
            self.hours.set_spiff_total(&ro, v.spiff_total());
            // the hours keep each RO's lines (to flag them, and their hours)
            let lines = timecard::lines_of(&v).into_iter().map(|(_, l)| l).collect();
            self.hours.update_lines(&ro, lines, &timecard::ymm(&v), timecard::now_ms());
            changed = true;
            let on_list = self.my_ros.iter().any(|r| r.ro == ro);
            if let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == ro).filter(|_| !on_list) {
                let mut r = xtime_ui::my_ros::from_view(&v);
                // (what Xtime left out, as it was)
                for (new, old) in [
                    (&mut r.vin, &ws.ro.vin),
                    (&mut r.year, &ws.ro.year),
                    (&mut r.make, &ws.ro.make),
                    (&mut r.model, &ws.ro.model),
                    (&mut r.tag, &ws.ro.tag),
                ] {
                    if new.is_empty() {
                        *new = old.clone();
                    }
                }
                ws.ro = r;
            }
        }
        if changed {
            let _ = timecard::save(&self.hours);
        }
        self.notify(notes)
    }

    /// An RO's lines, for the notifications (approved / declined since the last look)
    fn watch_lines(&mut self, ro: &str, v: &xtime::ro::RoView) -> Vec<crate::notes::New> {
        let lines: Vec<crate::notes::LineNow> = v
            .lines
            .iter()
            .filter(|l| !l.is_inspection())
            .map(|l| crate::notes::LineNow {
                key: l.key(),
                label: [l.dms.as_str(), l.name.as_str()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" "),
                approved: l.approved,
                declined: l.declined,
            })
            .collect();
        self.watch.lines(ro, &lines)
    }

    /// New notifications: kept in the list, the newest popped up in the corner, the taskbar button flashed
    fn notify(&mut self, news: Vec<crate::notes::New>) -> Task<Message> {
        if news.is_empty() || self.sample_data {
            return Task::none();
        }
        let ids = self.notes.add(news, timecard::now_ms());
        self.notes.save();
        let Some(&newest) = ids.last() else { return Task::none() };
        self.toast = Some((newest, Phase::Entering(std::time::Instant::now())));
        let gone = Task::perform(tokio::time::sleep(std::time::Duration::from_secs(7)), move |_| {
            Message::ToastGone(newest)
        });
        // (Windows flashes the taskbar button until the app's looked at; nothing when it's in front already)
        let flash = iced::window::get_oldest()
            .and_then(|id| iced::window::request_user_attention(id, Some(iced::window::UserAttention::Informational)));
        Task::batch([gone, flash])
    }

    /// A notification clicked: read, and to what it's about (its RO's Xtime page; a message: its conversation)
    fn open_note(&mut self, id: u64) -> Task<Message> {
        let Some(n) = self.notes.get(id).cloned() else {
            return Task::none();
        };
        self.notes.mark_read(id);
        self.notes.save();
        self.notes_open = false;
        if self.toast.is_some_and(|(t, _)| t == id) {
            self.toast = None;
        }
        let mut tasks = Vec::new();
        if !n.channel.is_empty() {
            self.chat = true;
            tasks.push(Task::done(Message::Chat(xtime_ui::chat::ChatMsg::Open(n.channel.clone()))));
        }
        if !n.ro.is_empty() {
            let known = self.open.iter().any(|w| w.ro.ro == n.ro) || self.my_ros.iter().any(|r| r.ro == n.ro);
            if known {
                // RO news shows on its Xtime page (a message: the vehicle as it was, the chat beside it)
                if n.kind != crate::notes::Kind::Message {
                    if let Some(ws) = self.open.iter_mut().find(|w| w.ro.ro == n.ro) {
                        ws.tab = Tab::XtimeView;
                    } else {
                        tasks.push(Task::done(Message::NoteShowXtime(n.ro.clone())));
                    }
                }
                tasks.insert(0, self.update(Message::OpenRo(n.ro.clone())));
            } else {
                // not open and not yours (any more): looked up in Xtime, as Add an RO does
                self.add_ro = Some(AddRo {
                    ro: n.ro.clone(),
                    ..Default::default()
                });
                tasks.push(self.submit_add_ro());
            }
        }
        Task::batch(tasks)
    }

    /// Reads the ROs on your list that haven't been read in the last 5 minutes, one after another, for their spiff
    /// totals (My ROs shows which have a spiff to turn in); and open ROs that aren't on your list (added by hand) every
    /// minute, as nothing else keeps them up to date
    fn check_spiffs(&mut self) -> Task<Message> {
        if self.checking_spiffs || self.sample_data {
            return Task::none();
        }
        let now = clock::now();
        // (ROs waiting on approval every minute, so approvals are noticed quickly; the rest every 5 minutes)
        let mut due: Vec<String> = self
            .my_ros
            .iter()
            .filter(|r| {
                let every = if matches!(r.mode.map(|m| m as i64), Some(4 | 5)) {
                    50
                } else {
                    5 * 60
                };
                self.spiff_checked.get(&r.ro).is_none_or(|at| now - at >= every)
            })
            .map(|r| r.ro.clone())
            .collect();
        due.extend(
            self.open
                .iter()
                .filter(|w| !w.is_sample() && !self.my_ros.iter().any(|r| r.ro == w.ro.ro))
                .map(|w| w.ro.ro.clone())
                .filter(|ro| self.spiff_checked.get(ro).is_none_or(|at| now - at >= 50)),
        );
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
            // the samples' vehicles were never real: close them (vehicles reopened from last time stay)
            self.sample_data = false;
            self.open.retain(|w| !w.is_sample());
            if !self.selected.as_ref().is_some_and(|s| self.open.iter().any(|w| &w.ro.ro == s)) {
                self.selected = None;
            }
        }
        // open vehicles get their RO's latest (mode, waiting…); one that's gone from your list stays open as it was
        for ws in &mut self.open {
            if let Some(r) = loaded.ros.iter().find(|r| r.ro == ws.ro.ro) {
                ws.ro = r.clone();
            }
        }
        self.my_ros = loaded.ros;
        self.dispatch = loaded.dispatch;
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
            // a change to your hours: synced a few seconds later
            if self.sync.due.is_some() {
                iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::SyncDue)
            } else {
                Subscription::none()
            },
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
            // the chat, every 30 s while it's open (otherwise with your ROs, every minute)
            if self.chat {
                iced::time::every(std::time::Duration::from_secs(30)).map(|_| Message::RefreshChat)
            } else {
                Subscription::none()
            },
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

/// Xtime's parts status as its number (2 waiting at parts, 3 working, 4 complete; 0 none): a number, or its words
fn parts_status_number(v: Option<&serde_json::Value>) -> i64 {
    match xtime_ui::my_ros::parts_label(v).map(|(label, _)| label) {
        Some("Parts: waiting at parts") => 2,
        Some("Parts: being worked on") => 3,
        Some("Parts: complete") => 4,
        _ => 0,
    }
}
