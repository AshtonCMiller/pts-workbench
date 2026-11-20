//! Sync between your computers: the Electron app's `sync.js`, the same way, so computers on either app sync together.
//!
//! What syncs: flags, hours and your hours for lines, renamed lines, RO lines and Review / Completed, the shift
//! clock, and which ROs are open in the Workbench (RO numbers and year / make / model only; each computer finds the
//! VIN itself). Never: VINs, customer details, sign-ins, settings or tab layouts.
//!
//! Spiffs sync too, in a document of their own beside the hours ([`Keys::spiffs`]): the Electron app doesn't know
//! spiffs, and would take them for deleted if they were in its document. Only this app reads that one.
//!
//! How: each piece is a record with a time; when two computers changed things, the records are merged one by one
//! and the newest change to each wins, so nothing else is lost. Deleted things leave a "deleted" marker. Everything
//! is compressed and encrypted (AES-256-GCM) here before it's sent: the server sees a hashed ID and unreadable text,
//! both from the store ID and employee number. A computer joining counts what it already has as oldest, so it only
//! fills gaps and never overrules what other computers changed.
//!
//! The server (sync-server/ in the repository): `GET /v1/store/<id>` → `{ version, data }`;
//! `PUT /v1/store/<id> { base, data }` → `{ version }`, or 409 with the newer copy (merged, and tried again).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use hours::{Flag, Hours, OpenRo, RoRec, Shift, Spiff, TLine};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use workbench_net::{Http, Method, Url};

/// The sync server (the Electron app's `ptsWorkbench.syncUrl`)
pub const URL: &str = "https://prodealer.works";

const DAY: i64 = 24 * 60 * 60 * 1000;
/// How long a "deleted" marker is kept
const KEEP_DELETED: i64 = 90 * DAY;
const FORMAT: i64 = 1;

// ---------- identity & encryption ----------

/// Who you are on the server (a hashed ID) and the key your data is encrypted with
#[derive(Clone)]
pub struct Keys {
    pub id: String,
    key: [u8; 32],
}

impl Keys {
    /// The spiffs' document: its own ID on the server (from yours), the same key
    pub fn spiffs(&self) -> Keys {
        Keys {
            id: hex(&Sha256::digest(format!("spiffs|{}", self.id).as_bytes())),
            key: self.key,
        }
    }

    /// Your RO list, as this computer's sidebar has it (for your phone): its own document, written whole by the app
    pub fn ros(&self) -> Keys {
        Keys {
            id: hex(&Sha256::digest(format!("ros|{}", self.id).as_bytes())),
            key: self.key,
        }
    }

    /// What pairs a phone (the page at `<server>/phone/` reads it after `#pair=`): this ID and the key, so the phone
    /// can work out the same push ID and token, and later read your synced flags itself. Shown only as a QR code.
    pub fn pair_code(&self) -> String {
        format!("v1.{}.{}", self.id, hex(&self.key))
    }

    /// For phone notifications (sync-server/src/push.js): whose phones (a hash of the sync ID) and the token that
    /// sends to them (from the key: the server keeps only a hash of it). The phone page works out the same two.
    pub fn push_ids(&self) -> (String, String) {
        (
            hex(&Sha256::digest(format!("pts-push|{}", self.id).as_bytes())),
            hex(&Sha256::digest(format!("pts-push-token|{}", hex(&self.key)).as_bytes())),
        )
    }
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Keys(…)") // (the key is never printed)
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// From the store ID and employee number (leading zeros of the number don't count); none without both
pub fn derive_keys(store_id: &str, employee_number: &str) -> Option<Keys> {
    let emp = employee_number.trim();
    // (as /^0+(?=\d)/: zeros before another digit go)
    let emp = {
        let z = emp.len() - emp.trim_start_matches('0').len();
        let keep = if z == emp.len() { z.saturating_sub(1) } else { z };
        if emp[keep..].starts_with(|c: char| c.is_ascii_digit()) {
            &emp[keep..]
        } else {
            emp
        }
    };
    let store = store_id.trim();
    if emp.is_empty() || store.is_empty() {
        return None;
    }
    let material = format!("pts-workbench-sync-v1|{store}|{emp}");
    let id = hex(&Sha256::digest(format!("id|{material}").as_bytes()));
    let mut key = [0u8; 32];
    hkdf::Hkdf::<Sha256>::new(Some(&b"pts-workbench"[..]), material.as_bytes())
        .expand(b"encrypt", &mut key)
        .ok()?;
    Some(Keys { id, key })
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut n, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let v = B64.iter().position(|x| *x == c)? as u32;
        n = (n << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
            n &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Compressed (raw deflate), encrypted, "v1." and base64 of IV + tag + ciphertext (as `sync.js` `encrypt`)
pub fn encrypt(keys: &Keys, obj: &Value) -> Result<String, String> {
    let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(obj.to_string().as_bytes()).map_err(|e| e.to_string())?;
    let plain = z.finish().map_err(|e| e.to_string())?;
    let mut iv = [0u8; 12];
    getrandom::getrandom(&mut iv).map_err(|e| e.to_string())?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&keys.key));
    let sealed = cipher
        .encrypt(Nonce::from_slice(&iv), plain.as_ref())
        .map_err(|_| "couldn't encrypt".to_string())?;
    // (aes-gcm puts the tag last; sync.js wants it right after the IV)
    let (ct, tag) = sealed.split_at(sealed.len() - 16);
    let mut out = Vec::with_capacity(12 + sealed.len());
    out.extend_from_slice(&iv);
    out.extend_from_slice(tag);
    out.extend_from_slice(ct);
    Ok(format!("v1.{}", b64_encode(&out)))
}

pub fn decrypt(keys: &Keys, text: &str) -> Result<Value, String> {
    let raw = text.strip_prefix("v1.").and_then(b64_decode).ok_or("unknown format")?;
    if raw.len() < 28 {
        return Err("unknown format".into());
    }
    let (iv, tag, ct) = (&raw[..12], &raw[12..28], &raw[28..]);
    let mut sealed = ct.to_vec();
    sealed.extend_from_slice(tag);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&keys.key));
    let plain = cipher
        .decrypt(Nonce::from_slice(iv), sealed.as_ref())
        .map_err(|_| "couldn't authenticate the synced data".to_string())?;
    let mut json = String::new();
    flate2::read::DeflateDecoder::new(&plain[..])
        .read_to_string(&mut json)
        .map_err(|_| "unknown format".to_string())?;
    serde_json::from_str(&json).map_err(|_| "unknown format".into())
}

// ---------- records ----------

/// One record: when it changed (ms), on which computer, and its value (or that it was deleted)
#[derive(Debug, Clone, PartialEq)]
pub struct Rec {
    pub t: i64,
    pub d: String,
    pub v: Value,
    pub del: bool,
}

impl Rec {
    fn to_value(&self) -> Value {
        if self.del {
            json!({ "t": self.t, "d": self.d, "del": 1 })
        } else {
            json!({ "t": self.t, "d": self.d, "v": self.v })
        }
    }

    fn from_value(v: &Value) -> Option<Rec> {
        let t = v.get("t")?.as_f64()? as i64;
        let d = match v.get("d") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        };
        let del = v.get("del").is_some_and(truthy);
        Some(Rec {
            t,
            d,
            v: v.get("v").cloned().unwrap_or(Value::Null),
            del,
        })
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// A number as JavaScript writes it (2 rather than 2.0)
fn js_number(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    let f = n.as_f64().unwrap_or(0.0);
    if f.fract() == 0.0 && f.abs() < 1e21 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

/// A value written with its keys in order (so the same value always reads the same)
pub fn stable(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => js_number(n),
        Value::String(s) => serde_json::to_string(s).unwrap_or_default(),
        Value::Array(a) => format!("[{}]", a.iter().map(stable).collect::<Vec<_>>().join(",")),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{}:{}", serde_json::to_string(k).unwrap_or_default(), stable(&o[k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

/// What a value's record checks against (to see whether it changed)
pub fn hash_of(v: &Value) -> String {
    b64_encode(&Sha1::digest(stable(v).as_bytes()))
}

fn s_of(m: &Map<String, Value>, k: &str) -> Value {
    m.get(k).filter(|v| truthy(v)).cloned().unwrap_or(Value::Null)
}

/// The hours as records (`sync.js` `recordsFrom`). Keys: f/RO/LINE flag · r/RO status · l/RO lines · o/RO/LINE your
/// hours · n/RO/LINE renamed line · s/START shift · c/DEVICE when that computer last closed the app · w/RO an RO
/// open in the Workbench · v/DEVICE this computer syncs open ROs
pub fn records_from(h: &Hours, device: &str) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for f in &h.flags {
        out.insert(
            format!("f/{}/{}", f.ro, f.key),
            json!({ "dms": f.dms, "label": f.label, "ymm": f.ymm, "hours": f.hours, "flaggedAt": f.flagged_at, "confirmed": f.confirmed }),
        );
    }
    for (ro, rec) in &h.ros {
        let tag = rec.other.get("tag").and_then(Value::as_str).unwrap_or("");
        out.insert(
            format!("r/{ro}"),
            json!({
                "ymm": rec.ymm,
                "tag": tag,
                "review": rec.review.clone().filter(truthy).unwrap_or(Value::Null),
                "completedAt": rec.completed_at.filter(|t| *t != 0),
                "onList": s_of(&rec.other, "onList"),
            }),
        );
        let mut lines = Map::new();
        for (k, l) in &rec.lines {
            let mut lv = serde_json::to_value(l).unwrap_or_else(|_| json!({}));
            if let Some(o) = lv.as_object_mut() {
                o.remove("override");
                o.remove("name");
            }
            lines.insert(k.clone(), lv);
            if let Some(over) = l.override_hours {
                out.insert(format!("o/{ro}/{k}"), json!(over));
            }
            if let Some(name) = l.name.as_ref().filter(|n| !n.is_empty()) {
                out.insert(format!("n/{ro}/{k}"), json!(name));
            }
        }
        out.insert(format!("l/{ro}"), json!({ "order": rec.order, "lines": lines }));
    }
    for (ro, w) in &h.open_ros {
        out.insert(format!("w/{ro}"), json!({ "ymm": w.ymm }));
    }
    if !device.is_empty() {
        out.insert(format!("v/{device}"), json!({ "openROs": 1 }));
    }
    for s in &h.shifts {
        out.insert(format!("s/{}", s.start), json!({ "start": s.start, "end": s.end, "auto": s.auto }));
    }
    if let (false, Some(c)) = (device.is_empty(), h.last_closed.filter(|t| *t != 0)) {
        out.insert(format!("c/{device}"), json!(c));
    }
    out
}

/// The spiffs turned in as records: p/RO (when it was marked, what it came to, the tag and year / make / model)
pub fn spiff_records(spiffs: &BTreeMap<String, Spiff>, _device: &str) -> BTreeMap<String, Value> {
    spiffs
        .iter()
        .map(|(ro, s)| (format!("p/{ro}"), serde_json::to_value(s).unwrap_or(Value::Null)))
        .collect()
}

/// Merged spiff records → the spiffs
pub fn apply_spiffs(spiffs: &mut BTreeMap<String, Spiff>, recs: &BTreeMap<String, Rec>) {
    *spiffs = recs
        .iter()
        .filter(|(_, r)| !r.del)
        .filter_map(|(k, r)| Some((k.strip_prefix("p/")?.to_string(), serde_json::from_value(r.v.clone()).ok()?)))
        .collect();
}

fn newer(a: &Rec, b: &Rec) -> bool {
    if a.t != b.t { a.t > b.t } else { a.d > b.d }
}

/// Picks between two copies of the same record: the newer, except a shift ended for real beats one the app ended
/// ("forgot to clock out")
fn pick<'a>(key: &str, a: Option<&'a Rec>, b: Option<&'a Rec>) -> Option<&'a Rec> {
    let (a, b) = match (a, b) {
        (None, b) => return b,
        (a, None) => return a,
        (Some(a), Some(b)) => (a, b),
    };
    if key.starts_with("s/") && !a.del && !b.del {
        let ended = |r: &Rec| !r.v["end"].is_null();
        let auto = |r: &Rec| truthy(&r.v["auto"]);
        let real_a = ended(a) && !auto(a);
        let real_b = ended(b) && !auto(b);
        if auto(a) && real_b {
            return Some(b);
        }
        if auto(b) && real_a {
            return Some(a);
        }
    }
    Some(if newer(a, b) { a } else { b })
}

pub fn merge(local: &BTreeMap<String, Rec>, remote: &BTreeMap<String, Rec>) -> BTreeMap<String, Rec> {
    let keys: BTreeSet<&String> = local.keys().chain(remote.keys()).collect();
    keys.into_iter()
        .filter_map(|k| pick(k, local.get(k), remote.get(k)).map(|r| (k.clone(), r.clone())))
        .collect()
}

/// Shifts from all your computers as one list: two computers clocked in separately → one shift; a shift left open
/// on one computer is ended by a clock-out made later on another (`sync.js` `normalizeShifts`)
pub fn normalize_shifts(entries: Vec<(Value, i64)>) -> Vec<Shift> {
    #[derive(Clone)]
    struct M {
        start: i64,
        end: Option<i64>,
        auto: bool,
        t: i64,
    }
    let mut list: Vec<M> = entries
        .into_iter()
        .map(|(v, t)| M {
            start: v["start"].as_f64().unwrap_or(0.0) as i64,
            end: v["end"].as_f64().map(|e| e as i64),
            auto: truthy(&v["auto"]),
            t,
        })
        .collect();
    list.sort_by_key(|m| m.start);
    let close = |start: i64, members: &[M]| -> Shift {
        let open: Vec<&M> = members.iter().filter(|m| m.end.is_none()).collect();
        let closed: Vec<&M> = members.iter().filter(|m| m.end.is_some()).collect();
        let (mut end, mut auto) = (None, false);
        if open.is_empty() {
            if let Some(last) = closed.iter().max_by_key(|m| m.end) {
                end = last.end;
                auto = closed.iter().all(|m| m.auto);
            }
        } else {
            let open_t = open.iter().map(|m| m.t).max().unwrap_or(0);
            let start_open = open.iter().map(|m| m.start).max().unwrap_or(0);
            let closers: Vec<&&M> = closed.iter().filter(|m| m.t > open_t && m.end.unwrap_or(0) >= start_open).collect();
            if let Some(last) = closers.iter().max_by_key(|m| m.end) {
                end = last.end;
                auto = last.auto;
            }
        }
        Shift { start, end, auto }
    };
    let mut out = Vec::new();
    let mut group: Option<(i64, Vec<M>)> = None;
    for s in list {
        if let Some((start, members)) = &group {
            let sofar = close(*start, members);
            if sofar.end.is_some_and(|e| s.start > e) {
                out.push(sofar);
                group = None;
            }
        }
        match &mut group {
            None => group = Some((s.start, vec![s])),
            Some((_, members)) => members.push(s),
        }
    }
    if let Some((start, members)) = &group {
        out.push(close(*start, members));
    }
    out
}

/// Merged records → the hours (`sync.js` `applyRecords`). What's only this computer's (an RO's VIN, when its lines
/// were last read, spiffs) stays.
pub fn apply_records(h: &mut Hours, recs: &BTreeMap<String, Rec>) {
    let live: BTreeMap<&String, &Rec> = recs.iter().filter(|(_, r)| !r.del).collect();
    let ro_ids: BTreeSet<String> = live
        .keys()
        .filter(|k| k.starts_with("r/") || k.starts_with("l/"))
        .map(|k| k[2..].to_string())
        .collect();
    let get = |k: String| live.get(&k).map(|r| r.v.clone());

    let mut ros = BTreeMap::new();
    for ro in ro_ids {
        let local = h.ros.get(&ro);
        let r = get(format!("r/{ro}")).unwrap_or_else(|| json!({}));
        let l = get(format!("l/{ro}")).unwrap_or_else(|| json!({ "order": [], "lines": {} }));
        let mut other = Map::new();
        if let Some(local) = local {
            for k in ["vin", "seenInXtime"] {
                if let Some(v) = local.other.get(k) {
                    other.insert(k.into(), v.clone());
                }
            }
        }
        other.insert("tag".into(), json!(r["tag"].as_str().unwrap_or("")));
        other.insert("onList".into(), s_of(r.as_object().unwrap_or(&Map::new()), "onList"));
        let mut lines = BTreeMap::new();
        if let Some(obj) = l["lines"].as_object() {
            for (k, line) in obj {
                let mut lv = if line.is_object() { line.clone() } else { json!({}) };
                lv["override"] = get(format!("o/{ro}/{k}")).unwrap_or(Value::Null);
                lv["name"] = get(format!("n/{ro}/{k}")).unwrap_or(Value::Null);
                lines.insert(k.clone(), serde_json::from_value::<TLine>(lv).unwrap_or_default());
            }
        }
        let rec = RoRec {
            lines,
            order: l["order"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
            ymm: r["ymm"].as_str().unwrap_or("").to_string(),
            touched: local.and_then(|x| x.touched),
            completed_at: r["completedAt"].as_f64().map(|t| t as i64).filter(|t| *t != 0),
            review: Some(r["review"].clone()).filter(truthy),
            spiff_total: local.and_then(|x| x.spiff_total),
            other,
        };
        ros.insert(ro, rec);
    }
    h.ros = ros;

    let existing: BTreeMap<(String, String), u64> = h.flags.iter().map(|f| ((f.ro.clone(), f.key.clone()), f.id)).collect();
    let mut next = h.next_flag_id.max(1);
    let mut flags = Vec::new();
    for (k, r) in &live {
        let Some(rest) = k.strip_prefix("f/") else { continue };
        let Some((ro, key)) = rest.split_once('/') else { continue };
        let id = match existing.get(&(ro.to_string(), key.to_string())) {
            Some(id) => *id,
            None => {
                next += 1;
                next - 1
            }
        };
        let mut v = if r.v.is_object() { r.v.clone() } else { json!({}) };
        v["id"] = json!(id);
        v["ro"] = json!(ro);
        v["key"] = json!(key);
        if let Ok(f) = serde_json::from_value::<Flag>(v) {
            flags.push(f);
        }
    }
    h.flags = flags;
    h.next_flag_id = next;

    h.open_ros = live
        .iter()
        .filter_map(|(k, r)| {
            let ro = k.strip_prefix("w/")?;
            Some((
                ro.to_string(),
                OpenRo {
                    ymm: r.v["ymm"].as_str().unwrap_or("").to_string(),
                },
            ))
        })
        .collect();
    // who closed each RO: only closes from computers whose app syncs open ROs count (an older version drops them all)
    let aware: BTreeSet<&str> = live.keys().filter_map(|k| k.strip_prefix("v/")).collect();
    h.open_closed_by = recs
        .iter()
        .filter(|(k, r)| k.starts_with("w/") && r.del)
        .map(|(k, r)| {
            (
                k[2..].to_string(),
                if aware.contains(r.d.as_str()) { "app" } else { "old" }.to_string(),
            )
        })
        .collect();

    h.shifts = normalize_shifts(
        live.iter()
            .filter(|(k, _)| k.starts_with("s/"))
            .map(|(_, r)| (r.v.clone(), r.t))
            .collect(),
    );
    let closed = live
        .iter()
        .filter(|(k, _)| k.starts_with("c/"))
        .filter_map(|(_, r)| r.v.as_f64().map(|x| x as i64))
        .max()
        .unwrap_or(0);
    h.last_closed_all = (closed > 0).then_some(closed);
}

// ---------- this computer's side ----------

/// What this computer keeps about sync (`sync-state.json`): its ID, the server's version it last saw, and when each
/// record last changed here
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    pub device_id: String,
    pub version: i64,
    pub journal: BTreeMap<String, Entry>,
    pub max_t: i64,
    /// Whose data this is (the hashed ID): another person here starts afresh
    pub who: Option<String>,
    /// Joining: what's here counts as oldest
    pub joining: bool,
    pub dirty: bool,
    pub last_sync: Option<i64>,
    /// The spiffs' document (its own version and history; the same computer ID)
    pub spiffs: Option<Box<State>>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub t: i64,
    pub d: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub h: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub del: bool,
}

impl Default for State {
    fn default() -> Self {
        let mut id = [0u8; 8];
        let _ = getrandom::getrandom(&mut id);
        State {
            device_id: hex(&id),
            version: 0,
            journal: BTreeMap::new(),
            max_t: 0,
            who: None,
            joining: true,
            dirty: false,
            last_sync: None,
            spiffs: None,
        }
    }
}

impl State {
    fn stamp_t(&mut self, now: i64) -> i64 {
        let t = now.max(self.max_t + 1);
        self.max_t = t;
        t
    }

    /// Compares the hours with the journal and times whatever changed here; the records as they are now
    pub fn stamp(&mut self, h: &Hours, now: i64) -> BTreeMap<String, Value> {
        let current = records_from(h, &self.device_id);
        self.stamp_records(current, now)
    }

    /// Compares records as they are now with the journal and times whatever changed here
    fn stamp_records(&mut self, current: BTreeMap<String, Value>, now: i64) -> BTreeMap<String, Value> {
        let mut changed = false;
        for (k, v) in &current {
            let hh = hash_of(v);
            let same = self.journal.get(k).is_some_and(|e| !e.del && e.h == hh);
            if !same {
                // joining: what's here already counts as oldest (it only fills gaps)
                let t = if self.joining { 1 } else { self.stamp_t(now) };
                self.journal.insert(
                    k.clone(),
                    Entry {
                        t,
                        d: self.device_id.clone(),
                        h: hh,
                        del: false,
                    },
                );
                changed = true;
            }
        }
        let gone: Vec<String> = self
            .journal
            .iter()
            .filter(|(k, e)| !e.del && !current.contains_key(*k))
            .map(|(k, _)| k.clone())
            .collect();
        for k in gone {
            let t = self.stamp_t(now);
            self.journal.insert(
                k,
                Entry {
                    t,
                    d: self.device_id.clone(),
                    h: String::new(),
                    del: true,
                },
            );
            changed = true;
        }
        self.joining = false;
        let cutoff = now - KEEP_DELETED;
        self.journal.retain(|_, e| !(e.del && e.t < cutoff));
        if changed {
            self.dirty = true;
        }
        current
    }

    /// This computer's records (timed)
    pub fn local_records(&mut self, h: &Hours, now: i64) -> BTreeMap<String, Rec> {
        let current = records_from(h, &self.device_id);
        self.timed(current, now)
    }

    /// Records as they are now, timed from the journal (and the journal brought up to date)
    fn timed(&mut self, current: BTreeMap<String, Value>, now: i64) -> BTreeMap<String, Rec> {
        let current = self.stamp_records(current, now);
        self.journal
            .iter()
            .map(|(k, e)| {
                let r = Rec {
                    t: e.t,
                    d: e.d.clone(),
                    v: if e.del {
                        Value::Null
                    } else {
                        current.get(k).cloned().unwrap_or(Value::Null)
                    },
                    del: e.del,
                };
                (k.clone(), r)
            })
            .collect()
    }

    /// Sync turned on (again): this computer joins afresh, filling gaps without overruling the server's copy
    pub fn rejoin(&mut self) {
        self.who = None;
        self.version = 0;
        self.journal.clear();
        self.joining = true;
        self.dirty = true;
        self.spiffs = None;
    }

    /// The spiffs' side (made the first time, with this computer's ID)
    fn spiff_side(&mut self) -> &mut State {
        let device = self.device_id.clone();
        self.spiffs.get_or_insert_with(|| {
            Box::new(State {
                device_id: device,
                ..State::default()
            })
        })
    }
}

/// How a sync went: the hours with what came from your other computers (when anything did), this computer's side,
/// and whether it worked
#[derive(Debug, Clone)]
pub struct Outcome {
    pub hours: Option<Hours>,
    pub state: State,
    pub result: Result<(), String>,
}

#[derive(Deserialize)]
struct Doc {
    #[serde(default)]
    version: i64,
    #[serde(default)]
    data: Option<String>,
}

async fn request(http: &Http, method: Method, url: &str, body: Option<String>) -> Result<(u16, Doc), String> {
    let u = Url::parse(url).map_err(|_| "the sync server's address".to_string())?;
    let mut req = Http::request(method, u).header("Accept", "application/json");
    if let Some(b) = body {
        req = req.header("Content-Type", "application/json").body(b);
    }
    let res = http.send(req).await.map_err(|_| "Can't reach the sync server".to_string())?;
    let status = res.status.as_u16();
    let doc = serde_json::from_str(&res.body).unwrap_or(Doc { version: 0, data: None });
    Ok((status, doc))
}

fn records_of(keys: &Keys, doc: &Doc) -> Result<BTreeMap<String, Rec>, String> {
    let Some(data) = doc.data.as_deref().filter(|d| !d.is_empty()) else {
        return Ok(BTreeMap::new());
    };
    let v = decrypt(keys, data).map_err(|_| "Couldn't read the synced data".to_string())?;
    Ok(v["records"]
        .as_object()
        .map(|o| o.iter().filter_map(|(k, r)| Some((k.clone(), Rec::from_value(r)?))).collect())
        .unwrap_or_default())
}

/// Saves a document whole (one only this app writes, like the RO list for your phone): written over the server's
/// copy, again over a newer one if another of your computers saved in between
pub async fn put_whole(http: &Http, url: &str, keys: &Keys, value: &Value) -> Result<(), String> {
    let at = format!("{}/v1/store/{}", url.trim_end_matches('/'), keys.id);
    let sealed = encrypt(keys, value)?;
    let (status, mut got) = request(http, Method::GET, &at, None).await?;
    if status != 200 {
        return Err(format!("the sync server said {status}"));
    }
    for _ in 0..3 {
        let body = json!({ "base": got.version, "data": sealed }).to_string();
        let (status, put) = request(http, Method::PUT, &at, Some(body)).await?;
        match status {
            200 => return Ok(()),
            409 => got = put,
            s => return Err(format!("the sync server said {s}")),
        }
    }
    Err("the sync server was busy".into())
}

/// One sync (`sync.js` `run`): the server's copy merged with this computer's, this computer brought up to date,
/// and what changed here sent (when someone else saved first, merged again and tried again). Then the spiffs'
/// document the same way.
pub async fn run(http: Http, url: String, keys: Keys, hours: Hours, mut st: State, now: i64) -> Outcome {
    let mut hours = hours;
    let (applied, result) = sync_doc(&http, &url, &keys, &mut hours, &mut st, now, records_from, apply_records).await;
    let mut applied_spiffs = false;
    let result = match result {
        Ok(()) => {
            let mut spiffs = hours.spiffs.clone();
            let side = st.spiff_side();
            let (got, r) = sync_doc(&http, &url, &keys.spiffs(), &mut spiffs, side, now, spiff_records, apply_spiffs).await;
            if got {
                hours.spiffs = spiffs;
                applied_spiffs = true;
            }
            r
        }
        e => e,
    };
    if result.is_ok() {
        st.dirty = false;
        st.last_sync = Some(now);
    }
    Outcome {
        hours: (applied || applied_spiffs).then_some(hours),
        state: st,
        result,
    }
}

/// One document synced: `to_records` makes its records from `data`, `apply` puts merged records back into it.
/// Whether anything came from elsewhere (`data` changed), and how it went.
#[allow(clippy::too_many_arguments)]
async fn sync_doc<T>(
    http: &Http,
    url: &str,
    keys: &Keys,
    data: &mut T,
    st: &mut State,
    now: i64,
    to_records: fn(&T, &str) -> BTreeMap<String, Value>,
    apply: fn(&mut T, &BTreeMap<String, Rec>),
) -> (bool, Result<(), String>) {
    // a different person on this computer: a fresh history for them (their data is merged in)
    if st.who.as_deref() != Some(keys.id.as_str()) {
        st.who = Some(keys.id.clone());
        st.version = 0;
        st.journal.clear();
        st.dirty = true;
        st.joining = true;
    }
    let mut applied = false;
    let base = format!("{}/v1/store/{}", url.trim_end_matches('/'), keys.id);
    let result: Result<(), String> = async {
        let recs = to_records(data, &st.device_id);
        let mut local = st.timed(recs, now);
        let (mut status, mut got) = request(http, Method::GET, &base, None).await?;
        for _ in 0..5 {
            if status != 200 && status != 409 {
                return Err(format!("the sync server said {status}"));
            }
            let remote = records_of(keys, &got)?;
            let merged = merge(&local, &remote);
            // anything from elsewhere: this computer brought up to date
            if merged.iter().any(|(k, r)| local.get(k) != Some(r)) {
                apply(data, &merged);
                st.journal = merged
                    .iter()
                    .map(|(k, r)| {
                        let e = Entry {
                            t: r.t,
                            d: r.d.clone(),
                            h: if r.del { String::new() } else { hash_of(&r.v) },
                            del: r.del,
                        };
                        (k.clone(), e)
                    })
                    .collect();
                st.max_t = st.max_t.max(merged.values().map(|r| r.t).max().unwrap_or(0));
                applied = true;
            }
            // (with anything the merge itself tidied up: joined shifts…)
            let recs = to_records(data, &st.device_id);
            local = st.timed(recs, now);
            let needs_upload = local.iter().any(|(k, r)| remote.get(k).is_none_or(|x| x.t != r.t || x.d != r.d))
                || remote.keys().any(|k| !local.contains_key(k));
            if !needs_upload {
                st.version = got.version;
                break;
            }
            let records: Map<String, Value> = local.iter().map(|(k, r)| (k.clone(), r.to_value())).collect();
            let sealed = encrypt(keys, &json!({ "format": FORMAT, "records": records }))?;
            let body = json!({ "base": got.version, "data": sealed }).to_string();
            let (put_status, put) = request(http, Method::PUT, &base, Some(body)).await?;
            match put_status {
                200 => {
                    st.version = put.version;
                    break;
                }
                // someone else saved first: their copy merged in, and tried again
                409 => {
                    status = 409;
                    got = put;
                }
                s => return Err(format!("the sync server said {s}")),
            }
        }
        Ok(())
    }
    .await;
    if result.is_ok() {
        st.dirty = false;
        st.last_sync = Some(now);
    }
    (applied, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_electron_app_encrypted() {
        // made by sync.js's deriveKeys / encrypt (with a fixed IV)
        let keys = derive_keys(" 3590 ", "001234").unwrap();
        assert_eq!(keys.id, "63d4ddc6002fc2804c51631b864f4ba969870cadf93d1f804ad016ffc89f5602");
        assert_eq!(hex(&keys.key), "b93d71dfc4a77fbcfc90bc9b5aec77ba2fc9243d7092cdc7706596c1ac5f28d7");
        let data =
            "v1.BwcHBwcHBwcHBwcHqyBy/vawBaay0Z+ibq2XtFJONkDKhXtgPKuIroNBneGN3nLYsQOi3sbyNhATJDIAE3UBOCJuF4QKSl3Af+hJvY9mxqpW2/O5EGzRMw==";
        let v = decrypt(&keys, data).unwrap();
        assert_eq!(
            v,
            json!({ "format": 1, "records": { "w/1": { "t": 5, "d": "ab", "v": { "ymm": "X" } } } })
        );
        // and what's encrypted here reads back
        let back = decrypt(&keys, &encrypt(&keys, &v).unwrap()).unwrap();
        assert_eq!(back, v);
        assert!(derive_keys("3590", "").is_none());
        assert_eq!(derive_keys("3590", "0").unwrap().id, derive_keys("3590", "000").unwrap().id);
    }

    #[test]
    fn the_phone_works_out_the_same() {
        // (the same as sync-server/site/phone/app.js works out from the store ID and employee number)
        let k = derive_keys(" 12345 ", "00987").unwrap();
        assert_eq!(k.id, "bb452fd48e0d937395cc279ab8b649bfd02449aea1e2677aff49c77fc64c1a07");
        assert_eq!(
            k.pair_code(),
            "v1.bb452fd48e0d937395cc279ab8b649bfd02449aea1e2677aff49c77fc64c1a07.5a73714deab4eacc024e01c12ba737036e194e4415e4032322e112a3f2ae4ad6"
        );
        let (pid, token) = k.push_ids();
        assert_eq!(pid, "4e4793f85c371e165adaaadecefe18edd7ebbef88237c774e147f0fbfc6efcc9");
        assert_eq!(token, "15e32b65a0c37f6fcd1187d2abc814dd6abb2fb4b85383004bc86da35e943513");
    }

    #[test]
    fn base64_both_ways() {
        for s in ["", "f", "fo", "foo", "foob", "fooba", "foobar"] {
            assert_eq!(b64_decode(&b64_encode(s.as_bytes())).unwrap(), s.as_bytes());
        }
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
    }

    #[test]
    fn stable_values() {
        assert_eq!(stable(&json!({ "b": 2.0, "a": [1, "x", null] })), r#"{"a":[1,"x",null],"b":2}"#);
        assert_eq!(hash_of(&json!({ "a": 1 })), hash_of(&json!({ "a": 1.0 })));
    }

    fn rec(t: i64, d: &str, v: Value) -> Rec {
        Rec {
            t,
            d: d.into(),
            v,
            del: false,
        }
    }

    #[test]
    fn the_newest_change_wins_but_a_real_clock_out_beats_an_automatic_one() {
        let a = rec(5, "a", json!(1));
        let b = rec(6, "b", json!(2));
        assert_eq!(pick("f/1/A", Some(&a), Some(&b)), Some(&b));
        let real = rec(5, "a", json!({ "start": 1, "end": 9 }));
        let auto = rec(9, "b", json!({ "start": 1, "end": 4, "auto": true }));
        assert_eq!(pick("s/1", Some(&real), Some(&auto)), Some(&real));
    }

    #[test]
    fn shifts_from_two_computers_become_one() {
        let s = normalize_shifts(vec![
            (json!({ "start": 100, "end": null }), 10),
            (json!({ "start": 120, "end": 500, "auto": false }), 20),
            (json!({ "start": 1000, "end": 1200 }), 30),
        ]);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].start, s[0].end, s[0].auto), (100, Some(500), false));
        assert_eq!((s[1].start, s[1].end), (1000, Some(1200)));
    }

    #[test]
    fn hours_round_trip_through_records() {
        let mut h = Hours::default();
        let line = TLine {
            key: "A".into(),
            dms: "A".into(),
            complaint: "Noise".into(),
            hours: 1.5,
            approved: true,
            ..Default::default()
        };
        h.update_lines("820367", vec![line], "2023 Ford Bronco Sport", 10);
        h.set_override("820367", "A", Some(2.0));
        h.flag_line("820367", "A", 20);
        h.open_ros.insert(
            "820367".into(),
            OpenRo {
                ymm: "2023 Ford Bronco Sport".into(),
            },
        );
        h.clock_in(1000);
        let recs = records_from(&h, "dev1");
        for k in ["f/820367/A", "r/820367", "l/820367", "o/820367/A", "w/820367", "v/dev1", "s/1000"] {
            assert!(recs.contains_key(k), "{k}");
        }
        assert!(!recs["l/820367"]["lines"]["A"].as_object().unwrap().contains_key("override"));
        let timed: BTreeMap<String, Rec> = recs.into_iter().map(|(k, v)| (k, rec(1, "dev1", v))).collect();
        let mut back = Hours::default();
        apply_records(&mut back, &timed);
        assert_eq!(back.ros["820367"].lines["A"].override_hours, Some(2.0));
        assert_eq!(back.ros["820367"].ymm, "2023 Ford Bronco Sport");
        assert_eq!(back.flags.len(), 1);
        assert_eq!(back.flags[0].hours, 2.0);
        assert_eq!(back.open_ros["820367"].ymm, "2023 Ford Bronco Sport");
        assert_eq!(back.shifts.len(), 1);
    }

    #[test]
    fn spiffs_round_trip_in_their_own_document() {
        let keys = derive_keys("3590", "1234").unwrap();
        let s = keys.spiffs();
        assert_eq!(s.id.len(), 64);
        assert_ne!(s.id, keys.id);
        let mut spiffs = BTreeMap::new();
        spiffs.insert(
            "820367".to_string(),
            Spiff {
                at: 5,
                total: 189.5,
                tag: "T12".into(),
                vehicle: "2026 Ford Bronco".into(),
            },
        );
        let recs = spiff_records(&spiffs, "dev1");
        assert_eq!(recs.keys().collect::<Vec<_>>(), ["p/820367"]);
        let mut timed: BTreeMap<String, Rec> = recs.into_iter().map(|(k, v)| (k, rec(1, "dev1", v))).collect();
        timed.insert(
            "p/1".into(),
            Rec {
                t: 2,
                d: "x".into(),
                v: Value::Null,
                del: true,
            },
        );
        let mut back = BTreeMap::new();
        apply_spiffs(&mut back, &timed);
        assert_eq!(back, spiffs);
        // the spiffs' side keeps this computer's ID
        let mut st = State::default();
        let id = st.device_id.clone();
        assert_eq!(st.spiff_side().device_id, id);
    }

    #[test]
    fn joining_counts_whats_here_as_oldest() {
        let mut h = Hours::default();
        h.clock_in(1000);
        let mut st = State::default();
        let recs = st.local_records(&h, 5000);
        assert!(recs.values().all(|r| r.t == 1));
        // a change after joining is timed now
        h.clock_out(2000);
        let recs = st.local_records(&h, 6000);
        assert_eq!(recs["s/1000"].t, 6000);
        // and a record that's gone is marked deleted
        h.shifts.clear();
        let recs = st.local_records(&h, 7000);
        assert!(recs["s/1000"].del);
    }
}
