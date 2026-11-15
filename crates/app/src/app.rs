//! The app's state, the messages its screens send, and what each one changes (iced's state → view → update).

use iced::keyboard::{self, Key, key::Named};
use iced::{Element, Subscription, Task, Theme};
use workbench_net::Http;
use xtime::dashboard::AssignedRo;

use crate::clock::{self, Clock};
use crate::reports::{ReportMsg, Reports};
use crate::settings::{Settings, ThemeChoice};
use crate::signin::{self, Outcome, Site};
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
    /// The shift clock
    pub clock: Clock,
    /// The time, for the clock (seconds since 1970; moves on every half minute)
    pub now: i64,
    /// The page shown (screen, mode, vehicle) and its transition: it slides out before the next one slides in
    pub outer: Transition<OuterKey>,
    /// The Feedback screen
    pub reports: Reports,
    /// The card over everything (Settings or Feedback), and where it is in coming or going
    pub overlay: Option<(Overlay, Phase)>,
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
    OpenSettings,
    CloseSettings,
    Edit(Field, String),
    SetTheme(ThemeChoice),
    SaveSettings,
    DismissNotice,
    /// Open a site's sign-in window
    SignIn(Site),
    /// Its window closed: signed in (with the cookies), closed first, or failed
    SignInDone(Site, Outcome),
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
    TechId,
    Name,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let settings = Settings::load();
        let screen = if settings.needs_setup() { Screen::Setup } else { Screen::Main };
        let outer = Transition::new((screen.clone(), Mode::Workbench, None));
        let http = Http::with_saved_cookies(USER_AGENT, &cookie_file())
            .or_else(|_| Http::new(USER_AGENT)) // saved sign-ins unreadable: start signed out
            .expect("the app's web requests");
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
            search: String::new(),
            open: Vec::new(),
            selected: None,
            sidebar: true,
            chat: false,
            notice: None,
            http,
            reports,
            signing_in: None,
            clock: Clock::load(),
            now: clock::now(),
            outer,
            overlay: None,
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
        (app, Task::batch([focus, count]))
    }

    pub fn title(&self) -> String {
        match self.workspace() {
            Some(ws) => format!("RO {} · {} · PTS Workbench", ws.ro.ro, ws.vehicle()),
            None => "PTS Workbench".into(),
        }
    }

    pub fn theme(&self) -> Theme {
        ui::style::theme(self.settings.theme)
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
                    self.selected = Some(ro);
                    self.mode = Mode::Workbench;
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
            }
            Message::SetManualTab(t) => {
                if let Some(ws) = self.workspace_mut() {
                    ws.tab = Tab::ServiceManual;
                    ws.manual = t;
                }
            }
            Message::ToggleSidebar => self.sidebar = !self.sidebar,
            Message::ToggleChat => self.chat = !self.chat,
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
                    Field::TechId => &mut self.draft.tech_id,
                    Field::Name => &mut self.draft.name,
                };
                *f = value;
            }
            Message::SetTheme(t) => {
                self.draft.theme = t;
                self.settings.theme = t; // shown right away; kept with Save
            }
            Message::SaveSettings => {
                let next = self.draft.clone().cleaned();
                if next.needs_setup() {
                    self.notice = Some("The store ID and dealer code are needed".into());
                    return Task::none();
                }
                match next.save() {
                    Ok(()) => {
                        self.settings = next.clone();
                        self.draft = next;
                        self.notice = Some("Settings saved".into());
                        // first setup goes on to the app; Settings closes
                        if self.screen == Screen::Setup {
                            self.screen = Screen::Main;
                        } else {
                            return self.close_overlay();
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
                return Task::perform(signin::run(site, web_dir()), move |o| Message::SignInDone(site, o));
            }
            Message::SignInDone(site, outcome) => {
                self.signing_in = None;
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
            }
            Message::ToggleClock => {
                self.now = clock::now();
                self.clock.toggle(self.now);
                if let Err(e) = self.clock.save() {
                    self.notice = Some(format!("Couldn't keep the clock for next time: {e}"));
                }
            }
            Message::Tick => self.now = clock::now(),
            Message::Arrive(_) => {} // handled in update
            Message::OpenFeedback => {
                let task = self.reports.open(self.screen_name(), self.is_admin());
                self.show_overlay(Overlay::Feedback);
                return task.map(Message::Report);
            }
            Message::CloseFeedback => return self.close_overlay(),
            Message::WindowOpened => launcher::window_ready(),
            Message::OverlayGone => {
                if matches!(self.overlay, Some((_, Phase::Leaving(_)))) {
                    self.overlay = None;
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
        ])
    }

    pub fn view(&self) -> Element<'_, Message> {
        ui::view(self)
    }
}
