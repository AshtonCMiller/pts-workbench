//! The Add Service window's state and what it does (the Electron app's xtime-view/service-editor.js): picking one
//! of the store's services, the CDK op code (searched as you type), colour, labor type, hours and prices, dispatch
//! code and duration, details and notes; also used to edit a line already on the RO. What's filled in starts the way
//! Xtime's own window starts it (the service's name in Details, Concern and Correction; its op code, or M), and
//! anything typed yourself is kept when another service or op code is picked. The window itself: [`card`].

use std::time::Duration;

use iced::Task;
use iced::widget::text_editor;
use serde_json::Value;
use xtime::Xtime;
use xtime::ro::Line;
use xtime::service::{CatService, Catalog, DispatchCode, LineInput, OpCode, PayType};

use crate::LoadError;

/// The window's text areas
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Complaint,
    Cause,
    Correction,
    Note,
}

/// The window's one-line boxes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Search,
    Hours,
    Labor,
    Parts,
    Total,
    Duration,
    Details,
}

#[derive(Debug, Clone)]
pub enum SvcMsg {
    CatalogLoaded(String, Result<Catalog, LoadError>),
    Type(Field, String),
    Edit(Area, text_editor::Action),
    Pick(CatService),
    OpInput(String),
    /// A moment after typing in the op code box (that search's number): search, unless more was typed since
    OpSearch(u64),
    OpFound(u64, Result<Vec<OpCode>, LoadError>),
    PickOp(OpCode),
    Color(String),
    Pay(PayType),
    Dispatch(DispatchCode),
    Save,
    Saved(Result<(), LoadError>),
    Close,
}

pub struct ServiceEditor {
    pub ro: String,
    /// Editing a line already on the RO (it, as read); none when adding
    pub line: Option<Line>,
    /// Adding a service for an inspection item: its name, and the services offered for it (SGIDs)
    pub for_item: Option<(String, Vec<String>)>,
    /// The colour a new line starts as (an inspection item's), when not the store's
    pub start_color: Option<String>,
    /// The store's services etc. (none while being read)
    pub catalog: Option<Catalog>,
    /// The services to pick from (all, or the item's)
    pub services: Vec<CatService>,
    pub error: Option<String>,
    pub search: String,
    pub service: Option<CatService>,
    pub op_code: String,
    /// The op code that came from the last service picked (replaced when another is picked; one typed isn't)
    pub op_auto: Option<String>,
    /// Op codes found for what's typed (shown under the box); none when not searching
    pub op_found: Option<Vec<OpCode>>,
    pub op_seq: u64,
    pub color: String,
    pub pay: Option<PayType>,
    pub hours: String,
    pub labor: String,
    pub parts: String,
    pub total: String,
    pub dispatch: Option<DispatchCode>,
    /// Dispatch codes to pick from (the store's, and the line's own if the store doesn't list it)
    pub dispatch_codes: Vec<DispatchCode>,
    pub duration: String,
    pub details: String,
    pub complaint: text_editor::Content,
    pub cause: text_editor::Content,
    pub correction: text_editor::Content,
    pub note: text_editor::Content,
    /// What was last filled in for you (Details etc. still showing it get the next one)
    pub auto: String,
    pub saving: bool,
}

fn content_text(c: &text_editor::Content) -> String {
    c.text().trim_end_matches('\n').to_string()
}

fn number(n: Option<f64>) -> String {
    n.map(|v| format!("{v}")).unwrap_or_default()
}

fn money(n: Option<f64>) -> String {
    n.map(|v| format!("{v:.2}")).unwrap_or_default()
}

impl ServiceEditor {
    /// The window for adding a service (for an inspection item: its name and services) or editing `line`
    pub fn new(ro: String, line: Option<Line>, for_item: Option<(String, Vec<String>)>) -> Self {
        Self {
            ro,
            line,
            for_item,
            start_color: None,
            catalog: None,
            services: Vec::new(),
            error: None,
            search: String::new(),
            service: None,
            op_code: String::new(),
            op_auto: None,
            op_found: None,
            op_seq: 0,
            color: String::new(),
            pay: None,
            hours: String::new(),
            labor: String::new(),
            parts: String::new(),
            total: String::new(),
            dispatch: None,
            dispatch_codes: Vec::new(),
            duration: String::new(),
            details: String::new(),
            complaint: text_editor::Content::new(),
            cause: text_editor::Content::new(),
            correction: text_editor::Content::new(),
            note: text_editor::Content::new(),
            auto: String::new(),
            saving: false,
        }
    }

    pub fn title(&self) -> String {
        match (&self.line, &self.for_item) {
            (Some(l), _) if !l.dms.is_empty() => format!("Edit Service · line {}", l.dms),
            (Some(_), _) => "Edit Service".into(),
            (None, Some((name, _))) => format!("Add Service · {name}"),
            (None, None) => "Add Service".into(),
        }
    }

    /// The services matching what's typed in the search box (up to 300)
    pub fn matching(&self) -> Vec<&CatService> {
        let q = self.search.trim().to_lowercase();
        // once a service is picked, the box shows its name: show them all again then
        let picked = self.service.as_ref().is_some_and(|s| s.name == self.search);
        self.services
            .iter()
            .filter(|s| q.is_empty() || picked || format!("{} {}", s.name, s.op_code).to_lowercase().contains(&q))
            .take(300)
            .collect()
    }

    /// The catalog's here: what the window starts with (the line's own, when editing)
    fn start(&mut self, catalog: Catalog) {
        let only = self.for_item.as_ref().map(|(_, sgids)| sgids.clone());
        self.services = catalog
            .services
            .iter()
            .filter(|s| only.as_ref().is_none_or(|o| o.contains(&xtime_text(&s.sgid))))
            .cloned()
            .collect();
        self.dispatch_codes = catalog.dispatch_codes.clone();
        self.pay = catalog.default_pay().cloned();
        self.dispatch = catalog.default_dispatch().cloned();
        self.duration = number(self.dispatch.as_ref().and_then(|d| d.duration));
        // an inspection item's colour when it's Yellow or Red; otherwise the store's
        self.color = self.start_color.clone().unwrap_or_else(|| catalog.default_color.clone());
        if let Some(l) = self.line.clone() {
            // editing: the line's own service, even when it isn't one the Add Service list offers (a customer concern)
            let sgcid = l.sgcid.clone().unwrap_or(Value::Null);
            if !sgcid.is_null() && !self.services.iter().any(|s| s.sgcid == sgcid) {
                self.services.insert(
                    0,
                    CatService {
                        sgcid: sgcid.clone(),
                        sgid: l.sgid.clone().unwrap_or(Value::Null),
                        name: if l.service_name.is_empty() {
                            l.name.clone()
                        } else {
                            l.service_name.clone()
                        },
                        op_code: l.op_code.clone(),
                        ..Default::default()
                    },
                );
            }
            self.service = self.services.iter().find(|s| s.sgcid == sgcid).cloned();
            self.search = self.service.as_ref().map(|s| s.name.clone()).unwrap_or_default();
            if let Some(id) = &l.pay_type_id {
                if let Some(p) = catalog.pay_types.iter().find(|p| xtime_text(&p.id) == xtime_text(id)) {
                    self.pay = Some(p.clone());
                }
            }
            self.hours = number(l.hours);
            self.labor = money(l.labor);
            self.parts = money(l.parts);
            self.total = money(l.total);
            self.details = if l.details.is_empty() { l.name.clone() } else { l.details.clone() };
            self.complaint = text_editor::Content::with_text(&l.concern);
            self.cause = text_editor::Content::with_text(&l.cause);
            self.correction = text_editor::Content::with_text(&l.correction);
            self.note = text_editor::Content::with_text(&l.tech_note);
            if !l.dispatch.is_empty() {
                self.set_dispatch(&l.dispatch);
            }
            if let Some(e) = l.estimate.filter(|e| *e > 0.0) {
                self.duration = number(Some(e));
            }
            self.op_code = l.op_code_dms.clone();
            if !l.color.is_empty() {
                self.color = l.color.clone();
            }
        }
        self.catalog = Some(catalog);
    }

    /// Picks a dispatch code by its code (added to the list when the store's list doesn't have it)
    fn set_dispatch(&mut self, code: &str) {
        if !self.dispatch_codes.iter().any(|c| c.code == code) {
            self.dispatch_codes.push(DispatchCode {
                code: code.to_string(),
                ..Default::default()
            });
        }
        self.dispatch = self.dispatch_codes.iter().find(|c| c.code == code).cloned();
    }

    /// A box still showing what was filled in for you (or empty) gets the next thing filled in
    fn is_auto(&self, v: &str) -> bool {
        v.trim().is_empty() || v == self.auto
    }

    /// As Xtime's window when a service is picked: Details, Concern and Correction start as its name (what you typed
    /// yourself stays); its op code (or M) unless you picked or typed one; its labor and price when adding
    fn pick(&mut self, s: CatService) {
        let plain = s.plain_name();
        self.search = s.name.clone();
        if self.is_auto(&self.details.clone()) {
            self.details = plain.clone();
        }
        if self.is_auto(&content_text(&self.complaint)) {
            self.complaint = text_editor::Content::with_text(&plain);
        }
        if self.is_auto(&content_text(&self.correction)) {
            self.correction = text_editor::Content::with_text(&plain);
        }
        self.auto = plain;
        let code = if s.op_code.is_empty() { "M".to_string() } else { s.op_code.clone() };
        if self.op_code.trim().is_empty() || Some(&self.op_code) == self.op_auto.as_ref() {
            self.op_code = code.clone();
            self.op_auto = Some(code);
            self.op_found = None;
        }
        if self.line.is_none() {
            if self.labor.is_empty() && s.labor > 0.0 {
                self.labor = format!("{:.2}", s.labor);
            }
            if self.total.is_empty() && s.price > 0.0 {
                self.total = format!("{:.2}", s.price);
            }
        }
        self.service = Some(s);
        self.error = None;
    }

    /// As Xtime's onOpCodeSelect: the op code's description, concern, dispatch code, duration, hours and labor fill in
    /// (what you typed yourself stays)
    fn pick_op(&mut self, o: OpCode) {
        self.op_code = o.id.clone();
        self.op_auto = None;
        self.op_found = None;
        let keep = |v: &str, auto: &str| !v.trim().is_empty() && v != auto;
        let auto = self.auto.clone();
        if !o.description.is_empty() && !keep(&self.details, &auto) {
            self.details = o.description.clone();
        }
        if !keep(&content_text(&self.correction), &auto) {
            let c = if o.description.is_empty() {
                self.details.clone()
            } else {
                o.description.clone()
            };
            self.correction = text_editor::Content::with_text(&c);
        }
        if !keep(&content_text(&self.complaint), &auto) {
            let c = [&o.complaint, &o.description].into_iter().find(|s| !s.is_empty()).cloned();
            if let Some(c) = c {
                self.complaint = text_editor::Content::with_text(&c);
            }
        }
        self.auto = self.details.clone();
        let default = self.catalog.as_ref().and_then(|c| c.default_dispatch()).map(|d| d.code.clone());
        let dc = if o.dispatch.is_empty() {
            default.unwrap_or_default()
        } else {
            o.dispatch.clone()
        };
        if !dc.is_empty() {
            self.set_dispatch(&dc);
        }
        if let Some(d) = o.duration.or_else(|| self.dispatch.as_ref().and_then(|d| d.duration)) {
            self.duration = number(Some(d));
        }
        if let Some(h) = o.hours {
            self.hours = number(Some(h));
        }
        if let Some(s) = o.sell.filter(|s| *s > 0.0) {
            self.labor = format!("{s:.2}");
        }
    }

    /// What's filled in, for Xtime
    pub fn input(&self) -> LineInput {
        let s = self.service.as_ref();
        LineInput {
            sgcid: s.map(|s| s.sgcid.clone()).unwrap_or(Value::Null),
            sgid: s.map(|s| s.sgid.clone()).unwrap_or(Value::Null),
            color: self.color.clone(),
            pay_type_id: self.pay.as_ref().map(|p| p.id.clone()),
            hours: self.hours.trim().to_string(),
            labor: self.labor.trim().to_string(),
            parts: self.parts.trim().to_string(),
            total: self.total.trim().to_string(),
            dispatch_code: self.dispatch.as_ref().map(|d| d.code.clone()).unwrap_or_default(),
            duration: self.duration.trim().to_string(),
            op_code_dms: self.op_code.trim().to_string(),
            details: self.details.trim().to_string(),
            complaint: content_text(&self.complaint).trim().to_string(),
            cause: content_text(&self.cause).trim().to_string(),
            correction: content_text(&self.correction).trim().to_string(),
            tech_note: content_text(&self.note).trim().to_string(),
        }
    }

    /// What a message does. `x` and `store`: to ask Xtime. Returns the work to do, and whether the window's done
    /// (saved: the RO should be read again).
    pub fn update(&mut self, msg: SvcMsg, x: &Xtime, store: &str) -> (Task<SvcMsg>, bool) {
        let none = (Task::none(), false);
        match msg {
            SvcMsg::CatalogLoaded(_, Ok(c)) => self.start(c),
            SvcMsg::CatalogLoaded(_, Err(e)) => self.error = Some(format!("Couldn't load the services ({})", plain(&e))),
            SvcMsg::Type(f, v) => match f {
                Field::Search => {
                    self.search = v;
                }
                Field::Hours => self.hours = v,
                Field::Labor => self.labor = v,
                Field::Parts => self.parts = v,
                Field::Total => self.total = v,
                Field::Duration => self.duration = v,
                Field::Details => self.details = v,
            },
            SvcMsg::Edit(a, action) => match a {
                Area::Complaint => self.complaint.perform(action),
                Area::Cause => self.cause.perform(action),
                Area::Correction => self.correction.perform(action),
                Area::Note => self.note.perform(action),
            },
            SvcMsg::Pick(s) => self.pick(s),
            SvcMsg::OpInput(v) => {
                self.op_code = v;
                self.op_auto = None; // typed yourself: not replaced when the service changes
                if self.op_code.trim().chars().count() < 2 {
                    self.op_found = None;
                    return none;
                }
                self.op_seq += 1;
                let seq = self.op_seq;
                return (
                    Task::perform(tokio::time::sleep(Duration::from_millis(300)), move |_| SvcMsg::OpSearch(seq)),
                    false,
                );
            }
            SvcMsg::OpSearch(seq) if seq == self.op_seq => {
                let (x, store, q) = (x.clone(), store.to_string(), self.op_code.clone());
                return (
                    Task::perform(async move { x.op_codes(&store, &q).await.map_err(LoadError::from) }, move |r| {
                        SvcMsg::OpFound(seq, r)
                    }),
                    false,
                );
            }
            SvcMsg::OpSearch(_) => {} // typed more since
            SvcMsg::OpFound(seq, r) if seq == self.op_seq => match r {
                Ok(found) => self.op_found = Some(found),
                Err(e) => self.error = Some(format!("Couldn't search the op codes ({})", plain(&e))),
            },
            SvcMsg::OpFound(..) => {}
            SvcMsg::PickOp(o) => self.pick_op(o),
            SvcMsg::Color(c) => self.color = c,
            SvcMsg::Pay(p) => self.pay = Some(p),
            SvcMsg::Dispatch(d) => {
                // a code with its own time fills it in; otherwise the time stays
                if let Some(t) = d.duration {
                    self.duration = number(Some(t));
                }
                self.dispatch = Some(d);
            }
            SvcMsg::Save => {
                if self.saving {
                    return none;
                }
                let input = self.input();
                if let Some(p) = input.problem() {
                    self.error = Some(p.into());
                    return none;
                }
                self.saving = true;
                self.error = None;
                let (x, store, ro) = (x.clone(), store.to_string(), self.ro.clone());
                let line_id = self.line.as_ref().map(|l| l.id.clone());
                return (
                    Task::perform(
                        async move {
                            match line_id {
                                Some(id) => x.edit_line(&store, &ro, &id, &input).await,
                                None => x.add_line(&store, &ro, &input).await,
                            }
                            .map_err(LoadError::from)
                        },
                        SvcMsg::Saved,
                    ),
                    false,
                );
            }
            SvcMsg::Saved(Ok(())) => {
                self.saving = false;
                return (Task::none(), true);
            }
            SvcMsg::Saved(Err(e)) => {
                self.saving = false;
                let what = if self.line.is_some() {
                    "Xtime didn't save the changes"
                } else {
                    "Xtime didn't add the service"
                };
                self.error = Some(format!("{what}: {}", plain(&e)));
            }
            SvcMsg::Close => {} // the app closes the window
        }
        none
    }

    /// What the bottom bar says once it's saved
    pub fn done_notice(&self) -> String {
        match (&self.line, &self.service) {
            (Some(l), _) if !l.dms.is_empty() => format!("Saved line {}", l.dms),
            (Some(_), Some(s)) => format!("Saved {}", s.name),
            (None, Some(s)) => format!("Added: {}", s.name),
            _ => "Saved".into(),
        }
    }
}

fn plain(e: &LoadError) -> String {
    e.text().trim_start_matches("Xtime: ").to_string()
}

fn xtime_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Reads the catalog for an RO's window
pub fn load_catalog(x: &Xtime, store: &str, ro: &str) -> Task<SvcMsg> {
    let (x, store, ro) = (x.clone(), store.to_string(), ro.to_string());
    Task::perform(
        async move {
            let r = x.catalog(&store, &ro).await.map_err(LoadError::from);
            (ro, r)
        },
        |(ro, r)| SvcMsg::CatalogLoaded(ro, r),
    )
}

impl ServiceEditor {
    /// Starts with a catalog read earlier (kept by the app per RO)
    pub fn start_with(&mut self, c: Catalog) {
        self.start(c);
    }
}

// ---------- the window ----------

use iced::widget::{Column, button, column, container, horizontal_space, pick_list, row, text, text_input};
// scrolling that glides (crates/ui-kit)
use iced::{Alignment, Border, Color, Element, Length, Theme};
use ui_kit::scroll::scrollable;
use ui_kit::style;

fn m(msg: SvcMsg) -> SvcMsg {
    msg
}

/// A label over its box (a red * when it's required)
fn field<'a>(label: &'a str, required: bool, input: impl Into<Element<'a, SvcMsg>>) -> Element<'a, SvcMsg> {
    let mut l = row![text(label).size(13).font(style::BOLD)].spacing(2);
    if required {
        l = l.push(text("*").size(13).style(text::danger));
    }
    column![l, input.into()].spacing(4).width(Length::Fill).into()
}

fn line_input<'a>(value: &str, placeholder: &str, f: Field) -> Element<'a, SvcMsg> {
    text_input(placeholder, value)
        .on_input(move |v| m(SvcMsg::Type(f, v)))
        .padding([5, 8])
        .size(13)
        .into()
}

fn area<'a>(content: &'a text_editor::Content, a: Area, height: f32) -> Element<'a, SvcMsg> {
    text_editor(content)
        .on_action(move |act| m(SvcMsg::Edit(a, act)))
        .height(height)
        .padding(8)
        .size(13)
        .into()
}

/// A list of choices under a box (services, op codes): the picked one highlighted
fn choice<'a>(label: String, picked: bool, msg: SvcMsg) -> Element<'a, SvcMsg> {
    button(text(label).size(13))
        .width(Length::Fill)
        .padding([4, 8])
        .style(style::list_item(picked))
        .on_press(msg)
        .into()
}

fn list_box<'a>(items: Column<'a, SvcMsg>, height: f32) -> Element<'a, SvcMsg> {
    container(scrollable(items.spacing(1).padding(4)).height(height))
        .width(Length::Fill)
        .style(|theme: &Theme| container::Style {
            border: Border {
                color: style::colors(theme).line,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Yellow / Red: the line's colour
fn color_button<'a>(ed: &ServiceEditor, name: &'static str, c: Color) -> Element<'a, SvcMsg> {
    let on = ed.color == name;
    button(text(name).size(13).font(style::BOLD))
        .padding([5, 14])
        .style(move |theme: &Theme, status| button::Style {
            background: Some(
                Color {
                    a: if on {
                        1.0
                    } else if matches!(status, button::Status::Hovered) {
                        0.2
                    } else {
                        0.08
                    },
                    ..c
                }
                .into(),
            ),
            text_color: if on { Color::WHITE } else { theme.palette().text },
            border: Border {
                color: c,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .on_press(m(SvcMsg::Color(name.to_string())))
        .into()
}

/// The window, as a card (the app shows it over everything)
pub fn card(ed: &ServiceEditor) -> Element<'_, SvcMsg> {
    let head = row![
        text(ed.title()).size(20).font(style::BOLD),
        horizontal_space(),
        button(text("Close"))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(m(SvcMsg::Close)),
    ]
    .align_y(Alignment::Center);

    let Some(cat) = &ed.catalog else {
        let msg = ed
            .error
            .clone()
            .unwrap_or_else(|| "Loading the store's services from Xtime…".into());
        return container(column![head, text(msg).style(style::muted)].spacing(16))
            .padding(24)
            .width(760)
            .style(style::floating_card)
            .into();
    };

    // the service: search, then pick
    let count = ed.services.len();
    let mut svc_list = Column::new();
    for s in ed.matching() {
        let mut label = s.name.clone();
        if !s.op_code.is_empty() {
            label.push_str(&format!("  ({})", s.op_code));
        }
        if s.price > 0.0 {
            label.push_str(&format!("  ${:.2}", s.price));
        }
        let picked = ed.service.as_ref().is_some_and(|p| p.sgcid == s.sgcid);
        svc_list = svc_list.push(choice(label, picked, m(SvcMsg::Pick(s.clone()))));
    }
    let search_hint = if count > 12 {
        format!("Search {count} services: name or op code")
    } else {
        "Search services".into()
    };
    let service = field(
        "Service Name",
        true,
        column![line_input(&ed.search, &search_hint, Field::Search), list_box(svc_list, 170.0)].spacing(6),
    );

    // the CDK op code: type to search the store's
    let mut op = column![
        text_input("Type to search the CDK op codes (code or description)", &ed.op_code)
            .on_input(|v| m(SvcMsg::OpInput(v)))
            .padding([5, 8])
            .size(13)
    ]
    .spacing(6);
    if let Some(found) = &ed.op_found {
        let mut list = Column::new();
        if found.is_empty() {
            list = list.push(text("No op codes match").size(13).style(style::muted));
        }
        for o in found {
            let mut label = o.id.clone();
            if !o.description.is_empty() {
                label.push_str(&format!("  ·  {}", o.description));
            }
            if let Some(h) = o.hours.filter(|h| *h > 0.0) {
                label.push_str(&format!("  ·  {h} hr"));
            }
            list = list.push(choice(label, o.id == ed.op_code, m(SvcMsg::PickOp(o.clone()))));
        }
        op = op.push(list_box(list, 130.0));
    }
    let op = field("CDK Op Code", true, op);

    // colour
    let note = if ed.color == "Yellow" {
        "May Require Future Attention"
    } else {
        "Requires Immediate Attention"
    };
    let colors = field(
        "Color",
        false,
        row![
            color_button(ed, "Yellow", Color::from_rgb8(0xd9, 0x9a, 0x06)),
            color_button(ed, "Red", Color::from_rgb8(0xb4, 0x23, 0x18)),
            text(note).size(12).style(style::muted),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

    // labor type, hours and prices
    let money_row = row![
        field(
            "Labor Type",
            true,
            pick_list(cat.pay_types.as_slice(), ed.pay.clone(), |p| m(SvcMsg::Pay(p)))
                .padding([5, 8])
                .text_size(13)
        ),
        field("Hours", false, line_input(&ed.hours, "—", Field::Hours)),
        field("Labor", false, line_input(&ed.labor, "—", Field::Labor)),
        field("Parts", false, line_input(&ed.parts, "—", Field::Parts)),
        field("Total", false, line_input(&ed.total, "—", Field::Total)),
    ]
    .spacing(10);
    let dispatch_row = row![
        field(
            "Dispatch Code",
            true,
            pick_list(ed.dispatch_codes.as_slice(), ed.dispatch.clone(), |d| m(SvcMsg::Dispatch(d)))
                .padding([5, 8])
                .text_size(13)
                .width(Length::Fill)
        ),
        field("Est. Duration", true, line_input(&ed.duration, "—", Field::Duration)),
    ]
    .spacing(10);

    let mut foot = row![].spacing(8).align_y(Alignment::Center);
    if let Some(e) = &ed.error {
        foot = foot.push(text(e.as_str()).size(13).style(text::danger).width(Length::Fill));
    } else {
        foot = foot.push(horizontal_space());
    }
    let save_label = match (ed.line.is_some(), ed.saving) {
        (true, true) => "Saving…",
        (true, false) => "Save",
        (false, true) => "Adding…",
        (false, false) => "Add",
    };
    foot = foot
        .push(
            button(text("Cancel"))
                .padding([8, 16])
                .style(style::ghost)
                .on_press(m(SvcMsg::Close)),
        )
        .push(
            button(text(save_label).font(style::BOLD))
                .padding([8, 22])
                .style(button::primary)
                .on_press_maybe((!ed.saving).then_some(m(SvcMsg::Save))),
        );

    let body = column![
        service,
        op,
        colors,
        money_row,
        dispatch_row,
        field("Details", true, line_input(&ed.details, "", Field::Details)),
        field("Concern / Service Request", false, area(&ed.complaint, Area::Complaint, 60.0)),
        field("Cause", false, area(&ed.cause, Area::Cause, 60.0)),
        field("Correction", false, area(&ed.correction, Area::Correction, 60.0)),
        field("Technician Note", false, area(&ed.note, Area::Note, 150.0)),
        text("Empty hours or prices use the service's own, like Xtime.")
            .size(12)
            .style(style::muted),
    ]
    .spacing(14);

    container(column![head, scrollable(container(body).padding([0, 6])).height(560), foot].spacing(16))
        .padding(24)
        .width(760)
        .style(style::floating_card)
        .into()
}
