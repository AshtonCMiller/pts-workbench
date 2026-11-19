//! A Quick Lane's maintenance card: the few specs a quick service needs.
//!
//! First, PTS's own Specification Sheet for the vehicle (Workshop › Maintenance Specifications: 2015 and newer): oil
//! capacity, its type, the oil filter, lug nut torque, the transmission's capacity and fluid, and the parts a quick
//! service sells (air and cabin filters, wipers, battery, spark plugs, coolant). Its part numbers are kept, with what
//! they are ("5W-20", "MERCON LV").
//!
//! Then the Workshop Manual's Specifications pages, for what the sheet doesn't say (the oil's service fill with the
//! filter and without, the drain plug), or everything when there's no sheet (older vehicles):
//!
//!   Engine oil      the service fill (with the filter, without), the oil's grade (5W-30), the drain plug's torque
//!   Wheels          the lug nuts' torque
//!   Transmission    its fill capacity, its fluid (MERCON ULV)
//!   Transfer case,  their capacity, their fluid or gear oil (75W-85)
//!   axles
//!
//! Those pages are read (the sections above, by their titles), and from their tables, only the rows that say these:
//! what's around them (oil pressure, the pump's clearances, part numbers, coolant) is left out. Each group from them
//! keeps its page, so it can be opened.

use crate::doc::{Block, ProcLink};
use crate::manual::{TocKind, TocNode};
use crate::{Error, Ford, Result};

/// The sections looked in (how their title starts, without its number, lowercase), in the order they're shown
const SECTIONS: &[&str] = &[
    "engine",
    // (read for nothing: only so an Engine Cooling section isn't taken for the engine's)
    "engine cooling",
    "wheels and tires",
    "automatic transmission",
    "manual transmission",
    "transfer case",
    "front drive axle",
    "rear drive axle",
];

/// At most this many pages read
const MOST_PAGES: usize = 8;

/// What a section's specs are about
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Engine,
    Wheels,
    Transmission,
    TransferCase,
    FrontAxle,
    RearAxle,
}

impl Part {
    fn of(section: &str) -> Option<Part> {
        Some(match rank_of(section) {
            0 => Part::Engine,
            2 => Part::Wheels,
            3 | 4 => Part::Transmission,
            5 => Part::TransferCase,
            6 => Part::FrontAxle,
            7 => Part::RearAxle,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Part::Engine => "Engine oil",
            Part::Wheels => "Wheels",
            Part::Transmission => "Transmission",
            Part::TransferCase => "Transfer case",
            Part::FrontAxle => "Front axle",
            Part::RearAxle => "Rear axle",
        }
    }
}

/// One group of the card ("Engine oil"): its lines (what, the value), and the Workshop Manual page they're from
/// (none: the Specification Sheet)
#[derive(Debug, Clone, PartialEq)]
pub struct SpecGroup {
    pub name: String,
    pub lines: Vec<(String, String)>,
    /// (its section, its title, where it is)
    pub page: Option<(String, String, ProcLink)>,
}

/// An engine's displacement in some text ("303-01D Engine - 5.0L" → "5.0L"; "5.0 L" too)
pub fn displacement(text: &str) -> Option<String> {
    let b = text.as_bytes();
    for i in 0..b.len() {
        // d.d then L (a space between allowed), not inside a longer number
        if i + 3 <= b.len()
            && b[i].is_ascii_digit()
            && b[i + 1] == b'.'
            && b[i + 2].is_ascii_digit()
            && (i == 0 || !(b[i - 1].is_ascii_digit() || b[i - 1] == b'.'))
        {
            let mut j = i + 3;
            if j < b.len() && b[j] == b' ' {
                j += 1;
            }
            if j < b.len() && (b[j] == b'L' || b[j] == b'l') && !b.get(j + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
                return Some(format!("{}L", &text[i..i + 3]));
            }
        }
    }
    None
}

/// Whether some text (a section's title, a row) is about the vehicle's engine: it names no engine, or this one (its
/// displacement, and when the text says, the same kind: EcoBoost, Ti-VCT, diesel…). The engine not known: anything.
pub fn same_engine(engine: Option<&str>, text: &str) -> bool {
    let (Some(engine), Some(d)) = (engine, displacement(text)) else {
        return true;
    };
    if displacement(engine).is_some_and(|e| e != d) {
        return false;
    }
    // the same displacement, two engines (a 3.5L EcoBoost and a 3.5L Ti-VCT): their kind, where both say it
    let kind = |t: &str| {
        let t: String = t.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        [
            "ecoboost",
            "tivct",
            "diesel",
            "powerboost",
            "hybrid",
            "duratec",
            "cyclone",
            "coyote",
            "godzilla",
        ]
        .into_iter()
        .find(|k| t.contains(k))
    };
    match (kind(engine), kind(text)) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

// ---------- the Specification Sheet ----------

/// The sheet: for each engine it lists (the sheet's heading: "5.0L TiVCT"), its items (what, its lines)
pub type Sheet = Vec<(String, Vec<(String, Vec<String>)>)>;

/// Text without its tags, entities decoded, spaces tidied
fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => {
                inside = false;
                out.push(' ');
            }
            c if !inside => out.push(c),
            _ => {}
        }
    }
    crate::html::clean(&crate::html::decode(&out))
}

fn find_ci(hay: &str, needle: &str, from: usize) -> Option<usize> {
    hay.get(from..)?.to_ascii_lowercase().find(needle).map(|i| i + from)
}

/// The sheet's page, read (each engine's heading, then a table row per item: its name in bold, its lines split by
/// line breaks)
pub fn parse_sheet(html: &str) -> Sheet {
    let mut out: Sheet = Vec::new();
    let lower = html.to_ascii_lowercase();
    // each engine: its <h6> heading, up to the next one
    let mut starts: Vec<usize> = Vec::new();
    let mut at = 0;
    while let Some(i) = find_ci(&lower, "<h6", at) {
        starts.push(i);
        at = i + 3;
    }
    if starts.is_empty() {
        starts.push(0);
    }
    for (n, &start) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(html.len());
        let part = &html[start..end];
        let pl = &lower[start..end];
        let engine = match (pl.find('>'), pl.find("</h6")) {
            (Some(a), Some(b)) if a < b => plain(&part[a + 1..b]),
            _ => String::new(),
        };
        let mut items = Vec::new();
        let mut at = 0;
        while let Some(r) = find_ci(pl, "<tr", at) {
            let r_end = find_ci(pl, "</tr", r).unwrap_or(pl.len());
            let row = &part[r..r_end];
            let rl = &pl[r..r_end];
            at = r_end;
            // its cells
            let mut cells = Vec::new();
            let mut c = 0;
            while let Some(td) = find_ci(rl, "<td", c) {
                let open = rl[td..].find('>').map(|o| td + o + 1).unwrap_or(rl.len());
                let close = find_ci(rl, "</td", open).unwrap_or(rl.len());
                cells.push(&row[open..close]);
                c = close;
            }
            if cells.len() < 2 {
                continue;
            }
            let name = plain(cells[0]).trim_end_matches(':').trim().to_string();
            let mut lines = Vec::new();
            let v = cells[1];
            let vl = v.to_ascii_lowercase();
            let mut from = 0;
            loop {
                let next = find_ci(&vl, "<br", from);
                let piece = plain(&v[from..next.unwrap_or(v.len())]);
                if !piece.is_empty() {
                    lines.push(piece);
                }
                match next {
                    Some(b) => from = vl[b..].find('>').map_or(v.len(), |o| b + o + 1),
                    None => break,
                }
            }
            if !name.is_empty() && !lines.is_empty() {
                items.push((name, lines));
            }
        }
        if !items.is_empty() {
            out.push((engine, items));
        }
    }
    out
}

/// What a Motorcraft transmission fluid's part number is ("XT-12-QULV" → "MERCON ULV")
pub fn fluid_of_part(part: &str) -> Option<&'static str> {
    let p = part.to_ascii_uppercase();
    [
        ("XT-12-QULV", "MERCON ULV"),
        ("XT-10-QLV", "MERCON LV"),
        ("XT-6-QSP", "MERCON SP"),
        ("XT-5-QMC", "MERCON V"),
        ("XT-2-QDX", "MERCON (Dexron III)"),
    ]
    .iter()
    .find(|(k, _)| p.contains(k))
    .map(|(_, v)| *v)
}

/// The sheet as the card's groups: the oil change, the wheels, the transmission, then the rest (parts a quick
/// service sells). More than one engine: each engine's oil change its own group.
pub fn sheet_groups(sheet: &Sheet) -> Vec<SpecGroup> {
    let mut oil: Vec<SpecGroup> = Vec::new();
    let (mut wheels, mut trans, mut other) = (Vec::new(), Vec::new(), Vec::new());
    let many = sheet.len() > 1;
    for (engine, items) in sheet {
        let mut lines = Vec::new();
        for (name, values) in items {
            let n = name.to_lowercase();
            let joined = values.join(" · ");
            let push = |list: &mut Vec<(String, String)>, l: String, v: String| {
                if !list.contains(&(l.clone(), v.clone())) {
                    list.push((l, v));
                }
            };
            if n.contains("oil") && n.contains("capacit") {
                push(&mut lines, "Oil capacity".into(), joined);
            } else if n.contains("oil type") || (n.contains("oil") && n.contains("spec")) {
                // the grade, then the bottle's part number
                let v = values
                    .iter()
                    .map(|p| match grade_in(p) {
                        Some(g) => format!("{g} ({p})"),
                        None => p.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(" · ");
                push(&mut lines, "Oil type".into(), v);
            } else if n.contains("oil filter") {
                push(&mut lines, "Oil filter".into(), joined);
            } else if n.contains("lug nut") || n.contains("wheel nut") {
                for v in values {
                    match v.split_once(':') {
                        Some((what, val)) if what.to_lowercase().contains("bolt size") => {
                            push(&mut wheels, "Lug nut size".into(), val.trim().to_string())
                        }
                        Some((_, val)) => push(&mut wheels, "Lug nut torque".into(), val.trim().to_string()),
                        None => push(&mut wheels, "Lug nut torque".into(), v.clone()),
                    }
                }
            } else if n.contains("transmission") && n.contains("capacit") {
                push(&mut trans, "Capacity".into(), joined);
            } else if n.contains("transmission") && (n.contains("fluid") || n.contains("spec")) {
                for v in values {
                    // "6R80: XT-10-QLVC": which transmission, its fluid
                    let (which, part) = match v.split_once(':') {
                        Some((w, p)) => (format!("Fluid ({})", w.trim()), p.trim().to_string()),
                        None => ("Fluid".to_string(), v.clone()),
                    };
                    let shown = match fluid_of_part(&part) {
                        Some(f) => format!("{f} ({part})"),
                        None => part,
                    };
                    push(&mut trans, which, shown);
                }
            } else if n.contains("transmission") {
                // ("Transmission Filter" → "Filter": it's under Transmission)
                let short = name.trim_start_matches(|c: char| c.is_alphabetic()).trim();
                push(&mut trans, if short.is_empty() { name.clone() } else { short.to_string() }, joined);
            } else {
                push(&mut other, name.clone(), joined);
            }
        }
        if !lines.is_empty() {
            oil.push(SpecGroup {
                name: if many && !engine.is_empty() {
                    format!("Engine oil · {engine}")
                } else {
                    "Engine oil".into()
                },
                lines,
                page: None,
            });
        }
    }
    let mut out = oil;
    for (name, lines) in [("Wheels", wheels), ("Transmission", trans), ("Parts", other)] {
        if !lines.is_empty() {
            out.push(SpecGroup {
                name: name.into(),
                lines,
                page: None,
            });
        }
    }
    out
}

/// The Specifications pages of the sections wanted: (the section's title, the page's title, where it is)
pub fn spec_pages(roots: &[TocNode]) -> Vec<(String, String, ProcLink)> {
    fn walk(nodes: &[TocNode], section: &str, out: &mut Vec<(usize, String, String, ProcLink)>) {
        for n in nodes {
            if n.na {
                continue;
            }
            match &n.kind {
                TocKind::Folder => {
                    // a section ("303-01 Engine"): its title without its number decides
                    let name = name_of(&n.title);
                    let here = if SECTIONS.iter().any(|s| name.starts_with(s)) {
                        n.title.as_str()
                    } else {
                        section
                    };
                    walk(&n.children, here, out);
                }
                TocKind::Doc(link) if !section.is_empty() && n.title.to_lowercase().contains("specification") => {
                    let rank = rank_of(section);
                    out.push((rank, section.to_string(), n.title.clone(), link.clone()));
                }
                _ => walk(&n.children, section, out),
            }
        }
    }
    let mut out = Vec::new();
    walk(roots, "", &mut out);
    out.sort_by_key(|(rank, ..)| *rank);
    out.dedup_by(|a, b| a.3 == b.3);
    out.into_iter().take(MOST_PAGES).map(|(_, s, t, l)| (s, t, l)).collect()
}

/// A section's title without its number, lowercase ("303-01A Engine - 2.7L" → "engine - 2.7l")
fn name_of(title: &str) -> String {
    let t = title.trim();
    let t = match t.split_once(' ') {
        Some((num, rest)) if num.starts_with(|c: char| c.is_ascii_digit()) => rest,
        _ => t,
    };
    t.trim().to_lowercase()
}

fn rank_of(section: &str) -> usize {
    let name = name_of(section);
    // the longest match ("engine cooling" before "engine")
    SECTIONS
        .iter()
        .enumerate()
        .filter(|(_, s)| name.starts_with(*s))
        .max_by_key(|(_, s)| s.len())
        .map_or(usize::MAX, |(i, _)| i)
}

/// Every table row of a page, as its cells' text (header rows left out)
fn rows_of(blocks: &[Block]) -> Vec<Vec<String>> {
    fn walk(bs: &[Block], out: &mut Vec<Vec<String>>) {
        for b in bs {
            match b {
                Block::Table { rows, .. } => {
                    for r in rows {
                        let cells: Vec<String> = r
                            .iter()
                            .map(|c| Block::text(&c.blocks).split_whitespace().collect::<Vec<_>>().join(" "))
                            .filter(|t| !t.is_empty())
                            .collect();
                        if !cells.is_empty() && !r.iter().all(|c| c.header) {
                            out.push(cells);
                        }
                        for c in r {
                            walk(&c.blocks, out);
                        }
                    }
                }
                Block::List { items, .. } => items.iter().for_each(|i| walk(i, out)),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(blocks, &mut out);
    out
}

fn has(t: &str, words: &[&str]) -> bool {
    words.iter().any(|w| t.contains(w))
}

/// A viscosity grade in some text ("Motorcraft® SAE 5W-30 Full Synthetic…" → "5W-30"; "75W85" → "75W-85")
pub fn grade_in(text: &str) -> Option<String> {
    let b = text.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c != b'W' && c != b'w' {
            continue;
        }
        // digits just before (one or two, not part of a longer number)
        let mut s = i;
        while s > 0 && b[s - 1].is_ascii_digit() && i - s < 2 {
            s -= 1;
        }
        if s == i || (s > 0 && b[s - 1].is_ascii_alphanumeric()) {
            continue;
        }
        // then a dash (or not) and two or three digits
        let mut e = i + 1;
        if e < b.len() && b[e] == b'-' {
            e += 1;
        }
        let d0 = e;
        while e < b.len() && b[e].is_ascii_digit() && e - d0 < 3 {
            e += 1;
        }
        if e - d0 < 2 || (e < b.len() && b[e].is_ascii_alphanumeric()) {
            continue;
        }
        return Some(format!("{}W-{}", &text[s..i], &text[d0..e]));
    }
    None
}

/// A transmission fluid's name in some text ("Motorcraft® MERCON® ULV Automatic Transmission Fluid; XT-12-QULV" →
/// "MERCON ULV")
pub fn fluid_in(text: &str) -> Option<String> {
    let clean = text.replace(['®', '™'], " ");
    let words: Vec<&str> = clean.split_whitespace().collect();
    let i = words.iter().position(|w| w.eq_ignore_ascii_case("mercon"))?;
    let mut name = "MERCON".to_string();
    if let Some(next) = words
        .get(i + 1)
        .map(|w| w.trim_matches(|c: char| !c.is_ascii_alphanumeric()).to_uppercase())
    {
        if ["ULV", "LV", "V", "SP", "C"].contains(&next.as_str()) {
            name = format!("{name} {next}");
        }
    }
    Some(name)
}

/// What a row says (its first cell, tidied: "Engine oil capacity — with filter" stays as it is), and its value (the
/// rest of its cells)
fn label_value(cells: &[String]) -> Option<(String, String)> {
    let (label, rest) = cells.split_first()?;
    if rest.is_empty() {
        return None;
    }
    Some((label.trim_end_matches(':').to_string(), rest.join(" · ")))
}

/// The lines a section's rows give, for what it's about
fn lines_for(part: Part, rows: &[Vec<String>]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |l: String, v: String| {
        if !out.iter().any(|(a, b)| *a == l && *b == v) {
            out.push((l, v));
        }
    };
    let mut grades: Vec<String> = Vec::new();
    let mut fluids: Vec<String> = Vec::new();
    for cells in rows {
        let all = cells.join(" ").to_lowercase();
        let first = cells[0].to_lowercase();
        let fill = has(&first, &["capacit", "fill", "refill"]) && !has(&first, &["pressure", "pump", "cooler", "level", "plug"]);
        match part {
            Part::Engine => {
                if fill && first.contains("oil") {
                    if let Some((l, v)) = label_value(cells) {
                        add(l, v);
                    }
                } else if has(&first, &["drain plug"]) {
                    if let Some((_, v)) = label_value(cells) {
                        add("Drain plug torque".into(), v);
                    }
                }
                if all.contains("oil") && !has(&all, &["transmission", "gear", "axle"]) {
                    grades.extend(grade_in(&cells.join(" ")));
                }
            }
            Part::Wheels => {
                if has(&first, &["lug nut", "wheel nut", "lug bolt", "wheel bolt", "wheel lug"]) {
                    if let Some((_, v)) = label_value(cells) {
                        add("Lug nut torque".into(), v);
                    }
                }
            }
            Part::Transmission | Part::TransferCase | Part::FrontAxle | Part::RearAxle => {
                if fill && !has(&first, &["oil pan", "engine"]) {
                    if let Some((l, v)) = label_value(cells) {
                        add(l, v);
                    }
                }
                if has(&all, &["fluid", "lubricant", "oil"]) {
                    fluids.extend(fluid_in(&cells.join(" ")));
                    if part != Part::Transmission || !all.contains("mercon") {
                        grades.extend(grade_in(&cells.join(" ")));
                    }
                }
            }
        }
    }
    grades.dedup();
    let mut unique = Vec::new();
    for g in grades.into_iter().chain(fluids) {
        if !unique.contains(&g) {
            unique.push(g);
        }
    }
    if !unique.is_empty() {
        let what = match part {
            Part::Engine => "Oil type",
            Part::Transmission => "Fluid",
            _ => "Fluid / gear oil",
        };
        out.push((what.into(), unique.join(", ")));
    }
    out
}

impl Ford {
    /// PTS's Specification Sheet for a VIN (2015 and newer: none for older ones)
    pub async fn spec_sheet(&self, vin: &str) -> Result<Sheet> {
        let book = self.book(vin).await?;
        let e = crate::vehicle::enc;
        let url = format!(
            "{}/VDIRSNET/applicationservices/ExpressService?flavor=DEALERS&country=USA&language=EN-US&vin={}&vehicleId={}",
            crate::paths::PTS,
            e(&book.vin),
            e(book.param("vehicleId")),
        );
        let html = self.text(self.url(&url)?).await?;
        Ok(parse_sheet(&html))
    }

    /// The maintenance card's specs for a VIN (see the top of this file)
    pub async fn maintenance_specs(&self, vin: &str) -> Result<Vec<SpecGroup>> {
        // the Specification Sheet (none: an older vehicle, or it didn't load)
        let sheet = match self.spec_sheet(vin).await {
            Ok(sheet) => sheet,
            Err(Error::NeedsLogin) => return Err(Error::NeedsLogin),
            Err(_) => Vec::new(),
        };
        let mut out = sheet_groups(&sheet);
        let had_sheet = !out.is_empty();
        // the vehicle's engine (the sheet's heading: "5.0L TiVCT"), so the manual's other engines' pages are left out
        // (it has a section for each engine the model came with; their oil fills differ)
        let engine = (sheet.len() == 1).then(|| sheet[0].0.clone()).filter(|e| displacement(e).is_some());
        // the Workshop Manual: with a sheet, only the engine oil's pages (its service fills, the drain plug)
        let toc = match self.toc(vin).await {
            Ok(t) => t,
            Err(e) if !had_sheet => return Err(e),
            Err(_) => return Ok(out),
        };
        for (section, title, link) in spec_pages(&toc.roots) {
            let Some(part) = Part::of(&section) else { continue };
            if had_sheet && part != Part::Engine {
                continue;
            }
            if part == Part::Engine && !same_engine(engine.as_deref(), &section) {
                continue;
            }
            let page = match self.procedure(vin, &link).await {
                Ok(p) => p,
                Err(e) if !had_sheet => return Err(e),
                Err(_) => continue,
            };
            // (a row naming another engine left out too, by what it says: "Oil capacity — 2.7L EcoBoost"; not its
            // values, which are liters too)
            let rows: Vec<Vec<String>> = rows_of(&page.blocks)
                .into_iter()
                .filter(|r| part != Part::Engine || same_engine(engine.as_deref(), &r[0]))
                .collect();
            let mut lines = lines_for(part, &rows);
            if had_sheet {
                // (the sheet's oil type stands)
                lines.retain(|(l, _)| l != "Oil type");
            }
            if lines.is_empty() {
                continue;
            }
            // (one group per part: lines from another page join it; the engine not known, each engine's its own)
            let name = match (part, displacement(&section)) {
                (Part::Engine, Some(d)) if engine.is_none() => format!("{} · {d}", part.name()),
                _ => part.name().to_string(),
            };
            match out.iter_mut().find(|g| g.name == name) {
                Some(g) => {
                    for l in lines {
                        if !g.lines.contains(&l) {
                            g.lines.push(l);
                        }
                    }
                    if g.page.is_none() {
                        g.page = Some((section, title, link));
                    }
                }
                None => out.push(SpecGroup {
                    name,
                    lines,
                    page: Some((section, title, link)),
                }),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(r: &[&[&str]]) -> Vec<Vec<String>> {
        r.iter().map(|c| c.iter().map(|s| s.to_string()).collect()).collect()
    }

    #[test]
    fn engine_oil() {
        let r = rows(&[
            &["Engine oil capacity with filter", "5.7 L (6.0 qt)"],
            &["Engine oil capacity without filter", "5.2 L (5.5 qt)"],
            &["Oil pressure at 2,000 rpm", "207-345 kPa"],
            &["Oil pump bolts", "10 Nm", "89 lb-in"],
            &["Oil pan drain plug", "27 Nm", "20 lb-ft"],
            &["Motorcraft® SAE 5W-30 Full Synthetic Motor Oil (US); XO-5W30-Q1FS", "WSS-M2C961-A1"],
            &[
                "Motorcraft® SAE 5W-30 Synthetic Blend Motor Oil (Canada); CXO-5W30-LSP6",
                "WSS-M2C961-A1",
            ],
            &["Engine coolant level", "Between MIN and MAX"],
        ]);
        let l = lines_for(Part::Engine, &r);
        assert_eq!(
            l,
            vec![
                ("Engine oil capacity with filter".to_string(), "5.7 L (6.0 qt)".to_string()),
                ("Engine oil capacity without filter".into(), "5.2 L (5.5 qt)".into()),
                ("Drain plug torque".into(), "27 Nm · 20 lb-ft".into()),
                ("Oil type".into(), "5W-30".into()),
            ]
        );
    }

    #[test]
    fn transmission_and_wheels() {
        let t = lines_for(
            Part::Transmission,
            &rows(&[
                &["Transmission fluid capacity (dry fill)", "13.1 L (13.8 qt)"],
                &["Transmission fluid pump bolts", "12 Nm"],
                &["Motorcraft® MERCON® ULV Automatic Transmission Fluid; XT-12-QULV", "WSS-M2C949-A"],
            ]),
        );
        assert_eq!(t[0].1, "13.1 L (13.8 qt)");
        assert_eq!(t[1], ("Fluid".to_string(), "MERCON ULV".to_string()));
        assert_eq!(t.len(), 2);
        let w = lines_for(
            Part::Wheels,
            &rows(&[&["Wheel lug nuts", "204 Nm", "150 lb-ft"], &["Tire pressure", "See label"]]),
        );
        assert_eq!(w, vec![("Lug nut torque".to_string(), "204 Nm · 150 lb-ft".to_string())]);
        let a = lines_for(
            Part::RearAxle,
            &rows(&[
                &["Lubricant capacity", "2.4 L"],
                &["Motorcraft SAE 75W85 Premium Synthetic Hypoid Gear Lubricant", "WSS-M2C942-A"],
            ]),
        );
        assert_eq!(a[1].1, "75W-85");
    }

    #[test]
    fn spec_sheet() {
        let html = r#"<h1>Specification Sheet For: 2017 F-150</h1><div id="accordion">
 <h6 class="accordion-header"><span class="ui-icon"></span>
 5.0L TiVCT
 </h6>
 <div><ul>
 <li><table><tbody><tr><td colspan="3" valign="top"><b>Oil Filter:</b></td><td colspan="3" valign="top">FL-500-S</td></tr></tbody></table></li>
 <li><table><tbody><tr><td colspan="3"><b>Oil Capacity:</b></td><td colspan="3">7.7qt(7.3L)</td></tr></tbody></table></li>
 <li><table><tbody><tr><td colspan="3"><b>Oil Type:</b></td><td colspan="3">XO-5W20-QSP</td></tr></tbody></table></li>
 <li><table><tbody><tr><td colspan="3"><b>Battery:</b></td><td colspan="3">BXT-48H6-610
<br>BXT-94RH7-730: King Ranch, Lariat, Premium</td></tr></tbody></table></li>
 <li><table><tbody><tr><td colspan="3"><b>Transmission Fluid Spec:</b></td><td colspan="3">6R80: XT-10-QLVC
<br>10R80: XT-12-QULV</td></tr></tbody></table></li>
 <li><table><tbody><tr><td colspan="3"><b>Transmission Capacity:</b></td><td colspan="3">13.1qt(12.4L)</td></tr></tbody></table></li>
 <li><table><tbody><tr><td colspan="3"><b>Lug Nut Torque:</b></td><td colspan="3">Bolt Size: M14 x 1.5
<br>lb.ft (Nm): 150 lb.ft (204 Nm)</td></tr></tbody></table></li>
 <li><table><tbody><tr><td colspan="3"><b>Coolant Grade:</b></td><td colspan="3">VC-3DIL-B</td></tr></tbody></table></li>
 </ul></div></div>"#;
        let sheet = parse_sheet(html);
        assert_eq!(sheet.len(), 1);
        assert_eq!(sheet[0].0, "5.0L TiVCT");
        assert_eq!(sheet[0].1[3].1, vec!["BXT-48H6-610", "BXT-94RH7-730: King Ranch, Lariat, Premium"]);
        let g = sheet_groups(&sheet);
        let names: Vec<&str> = g.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Engine oil", "Wheels", "Transmission", "Parts"]);
        assert_eq!(
            g[0].lines,
            vec![
                ("Oil filter".to_string(), "FL-500-S".to_string()),
                ("Oil capacity".into(), "7.7qt(7.3L)".into()),
                ("Oil type".into(), "5W-20 (XO-5W20-QSP)".into()),
            ]
        );
        assert_eq!(
            g[1].lines,
            vec![
                ("Lug nut size".to_string(), "M14 x 1.5".to_string()),
                ("Lug nut torque".into(), "150 lb.ft (204 Nm)".into())
            ]
        );
        assert_eq!(g[2].lines[0], ("Fluid (6R80)".to_string(), "MERCON LV (XT-10-QLVC)".to_string()));
        assert_eq!(g[2].lines[1], ("Fluid (10R80)".to_string(), "MERCON ULV (XT-12-QULV)".to_string()));
        assert_eq!(g[2].lines[2].0, "Capacity");
        assert_eq!(g[3].lines.len(), 2);
    }

    #[test]
    fn only_this_engine() {
        assert_eq!(displacement("303-01D Engine - 5.0L").as_deref(), Some("5.0L"));
        assert_eq!(displacement("2.7 L EcoBoost").as_deref(), Some("2.7L"));
        assert_eq!(displacement("7.75qt (7.33 L)"), None);
        assert_eq!(displacement("XO-5W20 12.4L"), None);
        let e = Some("5.0L TiVCT");
        assert!(same_engine(e, "303-01D Engine - 5.0L"));
        assert!(!same_engine(e, "303-01A Engine - 2.7L EcoBoost"));
        assert!(!same_engine(e, "303-01B Engine - 3.5L EcoBoost"));
        assert!(same_engine(e, "303-00 Engine System - General Information"));
        assert!(same_engine(None, "303-01A Engine - 2.7L EcoBoost"));
        assert!(!same_engine(Some("3.5L EcoBoost"), "303-01E Engine - 3.5L Ti-VCT"));
        assert!(same_engine(Some("3.5L EcoBoost"), "303-01B Engine - 3.5L EcoBoost"));
    }

    #[test]
    fn grades() {
        assert_eq!(grade_in("SAE 0W-20").as_deref(), Some("0W-20"));
        assert_eq!(grade_in("15W40 diesel").as_deref(), Some("15W-40"));
        assert_eq!(grade_in("XO-5W30-Q1FS").as_deref(), Some("5W-30"));
        assert_eq!(grade_in("WSS-M2C961-A1"), None);
        assert_eq!(fluid_in("Motorcraft® MERCON® LV Automatic").as_deref(), Some("MERCON LV"));
    }

    #[test]
    fn finds_the_sections_spec_pages() {
        let link = |p: &str| ProcLink {
            proc_uid: p.into(),
            ..Default::default()
        };
        let doc = |t: &str, p: &str| TocNode {
            title: t.into(),
            kind: TocKind::Doc(link(p)),
            na: false,
            children: vec![],
        };
        let folder = |t: &str, c: Vec<TocNode>| TocNode {
            title: t.into(),
            kind: TocKind::Folder,
            na: false,
            children: c,
        };
        let roots = vec![
            folder("204-04 Wheels and Tires", vec![doc("Specifications", "W")]),
            folder(
                "303-01A Engine - 2.7L EcoBoost",
                vec![doc("Specifications", "E"), doc("Oil Pan", "O")],
            ),
            folder("501-05 Interior Trim", vec![doc("Specifications", "T")]),
            folder("303-03A Engine Cooling", vec![doc("Specifications", "C")]),
        ];
        let pages = spec_pages(&roots);
        let parts: Vec<Option<Part>> = pages.iter().map(|(s, ..)| Part::of(s)).collect();
        assert_eq!(parts, [Some(Part::Engine), None, Some(Part::Wheels)]);
    }
}
