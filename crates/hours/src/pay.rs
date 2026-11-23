//! Predictive Pay: a guess at a pay period's check from its flagged hours (spiffs aren't counted).
//!
//! It's set up once from two pay stubs: what each paid (hours, and the rate or what it came to), what came out for
//! benefits and other fees, and the taxes. From the two, each deduction is worked out as a fixed amount, a percentage
//! of the check, or some of each ([`Learned`]). Taxes are figured the way payroll figures them, from your federal
//! and Missouri W-4s (IRS Publication 15-T's percentage method; Missouri's Form 4282), then checked against the
//! stubs: a difference the stubs show every time is added to (or taken off) each guess.
//!
//! The same math is in the phone app (sync-server/web/src/phone/lib/pay.js): the two have to agree. The setup syncs
//! between your computers and phone encrypted, in a document of its own (crates/sync `Keys::pay`).
//!
//! 2026's figures. Each new year: the tables in [`federal`] and [`missouri`] (and the Social Security wage base).

use serde::{Deserialize, Serialize};

/// Checks a year: twice a month (the 1st–15th and the 16th–end, as [`crate::pay_period`])
pub const PERIODS: f64 = 24.0;
/// Social Security: 6.2% (of wages up to $184,500 a year, 2026: far more than a check here comes to, so the cap
/// isn't figured, which would need the year's total so far)
pub const SOCIAL: f64 = 0.062;
/// Medicare: 1.45% (and 0.9% more over $200,000 a year, not figured either)
pub const MEDICARE: f64 = 0.0145;

fn cents(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

// ---------- the setup ----------

/// Federal W-4, Step 1(c)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FedStatus {
    /// Single or married filing separately
    #[default]
    Single,
    /// Married filing jointly
    Married,
    /// Head of household
    Head,
}

impl FedStatus {
    pub const ALL: [FedStatus; 3] = [FedStatus::Single, FedStatus::Married, FedStatus::Head];

    pub fn label(self) -> &'static str {
        match self {
            FedStatus::Single => "Single",
            FedStatus::Married => "Married, filing jointly",
            FedStatus::Head => "Head of household",
        }
    }
}

impl std::fmt::Display for FedStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Missouri W-4 (Form MO W-4), line 1
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoStatus {
    #[default]
    Single,
    /// Married, and your spouse works too
    SpouseWorks,
    /// Married, and your spouse doesn't work
    SpouseHome,
    /// Married filing separately
    Separate,
    /// Head of household
    Head,
}

impl MoStatus {
    pub const ALL: [MoStatus; 5] = [
        MoStatus::Single,
        MoStatus::SpouseWorks,
        MoStatus::SpouseHome,
        MoStatus::Separate,
        MoStatus::Head,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MoStatus::Single => "Single",
            MoStatus::SpouseWorks => "Married, spouse works",
            MoStatus::SpouseHome => "Married, spouse doesn't work",
            MoStatus::Separate => "Married, filing separately",
            MoStatus::Head => "Head of household",
        }
    }

    /// Missouri's standard deduction (2026)
    fn deduction(self) -> f64 {
        match self {
            MoStatus::Single | MoStatus::SpouseWorks | MoStatus::Separate => 16_100.0,
            MoStatus::SpouseHome => 32_200.0,
            MoStatus::Head => 24_150.0,
        }
    }
}

impl std::fmt::Display for MoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// What your federal W-4 says
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct W4 {
    pub status: FedStatus,
    /// Step 2(c): the box for two jobs (or a spouse who works) is checked
    pub two_jobs: bool,
    /// Step 3: dependents (a year)
    pub dependents: f64,
    /// Step 4(a): other income (a year)
    pub other_income: f64,
    /// Step 4(b): deductions (a year)
    pub deductions: f64,
    /// Step 4(c): extra taken out each check
    pub extra: f64,
}

/// What your Missouri W-4 says
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MoW4 {
    pub status: MoStatus,
    /// Line 2: extra taken out each check
    pub extra: f64,
}

/// One pay stub, as typed in. Gross: what it came to, or the hours times the rate.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Stub {
    /// Hours paid
    pub hours: f64,
    /// The hourly rate (none: from what it came to)
    pub rate: Option<f64>,
    /// What it came to before anything came out (none: hours × rate)
    pub gross: Option<f64>,
    /// The taxes it shows (to check the W-4s against: none, not checked)
    pub federal: Option<f64>,
    pub social: Option<f64>,
    pub medicare: Option<f64>,
    pub state: Option<f64>,
    /// Take-home (to check the whole thing against)
    pub net: Option<f64>,
}

impl Stub {
    pub fn gross(&self) -> Option<f64> {
        match (self.gross, self.rate) {
            (Some(g), _) if g > 0.0 => Some(g),
            (_, Some(r)) if r > 0.0 && self.hours > 0.0 => Some(cents(r * self.hours)),
            _ => None,
        }
    }

    pub fn rate(&self) -> Option<f64> {
        match (self.rate, self.gross) {
            (Some(r), _) if r > 0.0 => Some(r),
            (_, Some(g)) if g > 0.0 && self.hours > 0.0 => Some(g / self.hours),
            _ => None,
        }
    }
}

/// A benefit or a fee, and what it was on each stub
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Line {
    pub name: String,
    /// Taken out before taxes (health, dental and vision insurance usually are: a cafeteria plan)
    pub pretax: bool,
    /// On each stub, in the stubs' order (none: not on that one)
    pub amounts: Vec<Option<f64>>,
}

impl Line {
    fn on(&self, stub: usize) -> f64 {
        self.amounts.get(stub).copied().flatten().unwrap_or(0.0)
    }
}

/// A new hourly rate (a raise), from a pay period on: the periods before it keep the rate they had
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RateChange {
    /// The first pay period it's for: when it starts (ms, as [`crate::Period`]'s `start`)
    pub from: i64,
    pub rate: f64,
}

/// Everything Predictive Pay is set up with
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PaySetup {
    /// When it was last changed (ms): the newest wins between your computers and phone
    pub at: i64,
    pub w4: W4,
    pub mo: MoW4,
    /// The retirement plan: a percentage of each check (6 for 6%)
    pub retirement: f64,
    /// It's a Roth (taken out after taxes)
    pub roth: bool,
    /// The two stubs, newest first
    pub stubs: Vec<Stub>,
    pub lines: Vec<Line>,
    /// New hourly rates since the stubs (the stubs' own rate is the one before the first of them)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rates: Vec<RateChange>,
}

impl PaySetup {
    /// The hourly rate for the pay period starting at `start` (ms): the newest rate change from it or before it
    /// (none: the stubs')
    pub fn rate_from(&self, start: i64) -> Option<f64> {
        self.rates
            .iter()
            .filter(|r| r.from <= start && r.rate > 0.0)
            .max_by_key(|r| r.from)
            .map(|r| r.rate)
    }

    /// A new hourly rate from the pay period starting at `from` on (one already from then: replaced)
    pub fn set_rate(&mut self, from: i64, rate: f64) {
        self.rates.retain(|r| r.from != from);
        self.rates.push(RateChange { from, rate });
        self.rates.sort_by_key(|r| r.from);
    }
}

// ---------- the taxes ----------

/// A row of a withholding table: from this much a year, this much plus this rate of what's over it
type Row = (f64, f64, f64);

fn by_table(rows: &[Row], annual: f64) -> f64 {
    let (from, base, rate) = rows.iter().rev().find(|r| annual >= r.0).copied().unwrap_or((0.0, 0.0, 0.0));
    base + (annual - from) * rate
}

// Publication 15-T (2026), Worksheet 1A's annual tables: the standard ones, and the ones for W-4s with Step 2 checked
const MARRIED: [Row; 8] = [
    (0.0, 0.0, 0.0),
    (19_300.0, 0.0, 0.10),
    (44_100.0, 2_480.0, 0.12),
    (120_100.0, 11_600.0, 0.22),
    (230_700.0, 35_932.0, 0.24),
    (422_850.0, 82_048.0, 0.32),
    (531_750.0, 116_896.0, 0.35),
    (788_000.0, 206_583.5, 0.37),
];
const MARRIED_2: [Row; 8] = [
    (0.0, 0.0, 0.0),
    (16_100.0, 0.0, 0.10),
    (28_500.0, 1_240.0, 0.12),
    (66_500.0, 5_800.0, 0.22),
    (121_800.0, 17_966.0, 0.24),
    (217_875.0, 41_024.0, 0.32),
    (272_325.0, 58_448.0, 0.35),
    (400_450.0, 103_291.75, 0.37),
];
const SINGLE: [Row; 8] = [
    (0.0, 0.0, 0.0),
    (7_500.0, 0.0, 0.10),
    (19_900.0, 1_240.0, 0.12),
    (57_900.0, 5_800.0, 0.22),
    (113_200.0, 17_966.0, 0.24),
    (209_275.0, 41_024.0, 0.32),
    (263_725.0, 58_448.0, 0.35),
    (648_100.0, 192_979.25, 0.37),
];
const SINGLE_2: [Row; 8] = [
    (0.0, 0.0, 0.0),
    (8_050.0, 0.0, 0.10),
    (14_250.0, 620.0, 0.12),
    (33_250.0, 2_900.0, 0.22),
    (60_900.0, 8_983.0, 0.24),
    (108_938.0, 20_512.0, 0.32),
    (136_163.0, 29_224.0, 0.35),
    (328_350.0, 96_489.63, 0.37),
];
const HEAD: [Row; 8] = [
    (0.0, 0.0, 0.0),
    (15_550.0, 0.0, 0.10),
    (33_250.0, 1_770.0, 0.12),
    (83_000.0, 7_740.0, 0.22),
    (121_250.0, 16_155.0, 0.24),
    (217_300.0, 39_207.0, 0.32),
    (271_750.0, 56_631.0, 0.35),
    (656_150.0, 191_171.0, 0.37),
];
const HEAD_2: [Row; 8] = [
    (0.0, 0.0, 0.0),
    (12_075.0, 0.0, 0.10),
    (20_925.0, 885.0, 0.12),
    (45_800.0, 3_870.0, 0.22),
    (64_925.0, 8_077.5, 0.24),
    (112_950.0, 19_603.5, 0.32),
    (140_175.0, 28_315.5, 0.35),
    (332_375.0, 95_585.5, 0.37),
];

/// Federal income tax on a check's taxable wages (Publication 15-T 2026, Worksheet 1A)
pub fn federal(w4: &W4, wages: f64) -> f64 {
    let mut annual = wages * PERIODS + w4.other_income - w4.deductions;
    if !w4.two_jobs {
        annual -= if w4.status == FedStatus::Married { 12_900.0 } else { 8_600.0 };
    }
    let table: &[Row] = match (w4.status, w4.two_jobs) {
        (FedStatus::Married, false) => &MARRIED,
        (FedStatus::Married, true) => &MARRIED_2,
        (FedStatus::Single, false) => &SINGLE,
        (FedStatus::Single, true) => &SINGLE_2,
        (FedStatus::Head, false) => &HEAD,
        (FedStatus::Head, true) => &HEAD_2,
    };
    let tentative = by_table(table, annual.max(0.0)) / PERIODS;
    cents((tentative - w4.dependents / PERIODS).max(0.0) + w4.extra.max(0.0))
}

// Form 4282 (2026): $1,348 brackets, from 2% up to 4.7%
const MISSOURI: [Row; 8] = [
    (0.0, 0.0, 0.0),
    (1_348.0, 0.0, 0.02),
    (2_696.0, 27.0, 0.025),
    (4_044.0, 61.0, 0.03),
    (5_392.0, 101.0, 0.035),
    (6_740.0, 148.0, 0.04),
    (8_088.0, 202.0, 0.045),
    (9_436.0, 263.0, 0.047),
];

/// Missouri income tax on a check's taxable wages (Form 4282 2026: rounded to the dollar, then the MO W-4's extra)
pub fn missouri(mo: &MoW4, wages: f64) -> f64 {
    let annual = (wages * PERIODS - mo.status.deduction()).max(0.0);
    (by_table(&MISSOURI, annual) / PERIODS).round() + mo.extra.max(0.0)
}

/// The four taxes on a check
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Taxes {
    pub federal: f64,
    pub social: f64,
    pub medicare: f64,
    pub state: f64,
}

impl Taxes {
    pub fn total(&self) -> f64 {
        self.federal + self.social + self.medicare + self.state
    }
}

/// The taxes on a check of `gross`, with `pretax` out of it before any taxes (a cafeteria plan's insurance) and
/// `retirement` out of it before income taxes (not before Social Security and Medicare; a Roth's: none)
pub fn taxes(s: &PaySetup, gross: f64, pretax: f64, retirement: f64) -> Taxes {
    let fica = (gross - pretax).max(0.0);
    let income = (fica - if s.roth { 0.0 } else { retirement }).max(0.0);
    Taxes {
        federal: federal(&s.w4, income),
        social: cents(fica * SOCIAL),
        medicare: cents(fica * MEDICARE),
        state: missouri(&s.mo, income),
    }
}

// ---------- learning from the stubs ----------

/// A benefit or fee as worked out from the stubs: this much each check, plus this part of what the check comes to
#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    pub name: String,
    pub pretax: bool,
    pub fixed: f64,
    /// A fraction (0.02: 2%)
    pub part: f64,
}

impl Fit {
    pub fn on(&self, gross: f64) -> f64 {
        cents(self.fixed + self.part * gross).max(0.0)
    }

    /// "$42.10 each check", "2% of the check", "$5.00 + 1.5% of the check"
    pub fn describe(&self) -> String {
        let pct = |p: f64| {
            let s = format!("{:.2}", p * 100.0);
            format!("{}%", s.trim_end_matches('0').trim_end_matches('.'))
        };
        match (self.fixed.abs() >= 0.005, self.part.abs() >= 0.000_05) {
            (_, false) => format!("{} each check", money(self.fixed)),
            (false, true) => format!("{} of the check", pct(self.part)),
            (true, true) => format!("{} + {} of the check", money(self.fixed), pct(self.part)),
        }
    }
}

/// A fixed amount and a part of the check that make these (check, amount) pairs (the best fit for more than two)
fn fit(points: &[(f64, f64)]) -> (f64, f64) {
    match points {
        [] => (0.0, 0.0),
        [(_, a)] => (*a, 0.0),
        _ => {
            let n = points.len() as f64;
            let (mg, ma) = (
                points.iter().map(|p| p.0).sum::<f64>() / n,
                points.iter().map(|p| p.1).sum::<f64>() / n,
            );
            // the same every time, or checks too alike to tell: a fixed amount
            let spread = points.iter().map(|p| (p.1 - ma).abs()).fold(0.0, f64::max);
            let gs = points.iter().map(|p| (p.0 - mg).powi(2)).sum::<f64>();
            if spread < 0.005 || gs < 1.0 {
                return (cents(ma), 0.0);
            }
            // a part of the check, the same every time (to the cent): that alone
            let parts: Vec<f64> = points.iter().filter(|p| p.0 > 0.0).map(|p| p.1 / p.0).collect();
            if parts.len() == points.len() {
                let mean = parts.iter().sum::<f64>() / n;
                if points.iter().all(|p| (cents(p.0 * mean) - p.1).abs() <= 0.011) {
                    return (0.0, mean);
                }
            }
            let part = points.iter().map(|p| (p.0 - mg) * (p.1 - ma)).sum::<f64>() / gs;
            let fixed = ma - part * mg;
            if part < 0.0 {
                (cents(ma), 0.0) // (less on a bigger check: not a part of it)
            } else if fixed < -0.005 {
                (0.0, parts.iter().sum::<f64>() / parts.len().max(1) as f64)
            } else {
                (fixed, part)
            }
        }
    }
}

/// One tax as the W-4s figure it against what the stubs show
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Check {
    pub name: &'static str,
    /// On average, what the stubs show more than the W-4s figure (each check)
    pub off: f64,
    /// How many stubs showed it
    pub stubs: usize,
}

impl Check {
    /// Close enough to say it matches
    pub fn matches(&self) -> bool {
        self.off.abs() < 1.0
    }
}

/// What the stubs taught it
#[derive(Debug, Clone, PartialEq)]
pub struct Learned {
    /// Your hourly rate (the newest stub's)
    pub rate: Option<f64>,
    pub lines: Vec<Fit>,
    /// Added to each tax so the guess matches the stubs
    pub adjust: Taxes,
    /// Each tax checked against the stubs (only ones a stub showed)
    pub checks: Vec<Check>,
    /// The stubs' take-home against what's figured for them (each check, on average): none, no take-home typed
    pub net_off: Option<f64>,
}

impl Learned {
    pub fn ready(&self) -> bool {
        self.rate.is_some()
    }
}

/// Works out the deductions and checks the taxes from the stubs
pub fn learn(s: &PaySetup) -> Learned {
    let stubs: Vec<(usize, f64)> = s.stubs.iter().enumerate().filter_map(|(i, st)| Some((i, st.gross()?))).collect();
    let rate = s.stubs.iter().find_map(Stub::rate);
    let lines: Vec<Fit> = s
        .lines
        .iter()
        .filter(|l| !l.name.trim().is_empty() || l.amounts.iter().any(Option::is_some))
        .map(|l| {
            let points: Vec<(f64, f64)> = stubs.iter().map(|&(i, g)| (g, l.on(i))).collect();
            let (fixed, part) = fit(&points);
            Fit {
                name: if l.name.trim().is_empty() {
                    "Deduction".into()
                } else {
                    l.name.trim().to_string()
                },
                pretax: l.pretax,
                fixed,
                part,
            }
        })
        .collect();

    // each stub's taxes as the W-4s figure them, with what really came out of that stub
    let mut sums = [(0.0, 0usize); 4];
    let mut net = (0.0, 0usize);
    for &(i, gross) in &stubs {
        let st = &s.stubs[i];
        let pretax: f64 = s.lines.iter().filter(|l| l.pretax).map(|l| l.on(i)).sum();
        let after: f64 = s.lines.iter().filter(|l| !l.pretax).map(|l| l.on(i)).sum();
        let retirement = cents(gross * s.retirement / 100.0);
        let t = taxes(s, gross, pretax, retirement);
        for (k, (shown, figured)) in [
            (st.federal, t.federal),
            (st.social, t.social),
            (st.medicare, t.medicare),
            (st.state, t.state),
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(v) = shown {
                sums[k].0 += v - figured;
                sums[k].1 += 1;
            }
        }
        if let Some(n) = st.net {
            // (with the stub's own taxes where it shows them)
            let tax = st.federal.unwrap_or(t.federal)
                + st.social.unwrap_or(t.social)
                + st.medicare.unwrap_or(t.medicare)
                + st.state.unwrap_or(t.state);
            net.0 += n - (gross - pretax - after - retirement - tax);
            net.1 += 1;
        }
    }
    let avg = |(sum, n): (f64, usize)| if n == 0 { 0.0 } else { cents(sum / n as f64) };
    let names = ["Federal income tax", "Social Security", "Medicare", "Missouri income tax"];
    let checks = (0..4)
        .filter(|&k| sums[k].1 > 0)
        .map(|k| Check {
            name: names[k],
            off: avg(sums[k]),
            stubs: sums[k].1,
        })
        .collect();
    Learned {
        rate,
        lines,
        adjust: Taxes {
            federal: avg(sums[0]),
            social: avg(sums[1]),
            medicare: avg(sums[2]),
            state: avg(sums[3]),
        },
        checks,
        net_off: (net.1 > 0).then(|| avg(net)),
    }
}

// ---------- the guess ----------

/// A check, as it's guessed
#[derive(Debug, Clone, PartialEq)]
pub struct Breakdown {
    pub hours: f64,
    pub rate: f64,
    pub gross: f64,
    pub retirement: f64,
    /// Each benefit or fee: its name, what comes out, and whether before taxes
    pub lines: Vec<(String, f64, bool)>,
    pub taxes: Taxes,
    /// Take-home
    pub net: f64,
}

/// The check for `hours` flagged in the pay period starting at `start` (ms), at that period's rate (none: not set
/// up enough yet: no rate)
pub fn predict_for(s: &PaySetup, l: &Learned, hours: f64, start: i64) -> Option<Breakdown> {
    predict_at(s, l, hours, s.rate_from(start).or(l.rate)?)
}

/// The check for `hours` flagged, at the stubs' rate (none: no rate)
pub fn predict(s: &PaySetup, l: &Learned, hours: f64) -> Option<Breakdown> {
    predict_at(s, l, hours, l.rate?)
}

/// The check for `hours` flagged at `rate` an hour
pub fn predict_at(s: &PaySetup, l: &Learned, hours: f64, rate: f64) -> Option<Breakdown> {
    let gross = cents(hours.max(0.0) * rate);
    let retirement = cents(gross * s.retirement.max(0.0) / 100.0);
    let lines: Vec<(String, f64, bool)> = l.lines.iter().map(|f| (f.name.clone(), f.on(gross), f.pretax)).collect();
    let pretax: f64 = lines.iter().filter(|x| x.2).map(|x| x.1).sum();
    let t = taxes(s, gross, pretax, retirement);
    let a = &l.adjust;
    // (nothing's taken out of a check of nothing)
    let adj = |v: f64, by: f64| if gross > 0.0 { cents((v + by).max(0.0)) } else { 0.0 };
    let taxes = Taxes {
        federal: adj(t.federal, a.federal),
        social: adj(t.social, a.social),
        medicare: adj(t.medicare, a.medicare),
        state: adj(t.state, a.state),
    };
    let out: f64 = lines.iter().map(|x| x.1).sum();
    Some(Breakdown {
        hours,
        rate,
        gross,
        retirement,
        lines,
        taxes,
        net: cents(gross - retirement - out - taxes.total()),
    })
}

/// $1,234.50
pub fn money(n: f64) -> String {
    let c = (n * 100.0).round() as i64;
    let (whole, part) = (c.abs() / 100, c.abs() % 100);
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}${grouped}.{part:02}", if c < 0 { "-" } else { "" })
}

/// A typed amount: "$1,234.50", "6%", "42" (blank: none; not a number: an error)
pub fn parse_amount(s: &str) -> Result<Option<f64>, ()> {
    let t: String = s.chars().filter(|c| !matches!(c, '$' | ',' | '%' | ' ')).collect();
    if t.is_empty() {
        return Ok(None);
    }
    match t.parse::<f64>() {
        Ok(v) if v.is_finite() && v >= 0.0 => Ok(Some(v)),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.006
    }

    #[test]
    fn federal_by_the_book() {
        // single, standard: $2,000 a check → 48,000 − 8,600 = 39,400 → 1,240 + 12% of 19,500 = 3,580 → 149.17
        let w = W4::default();
        assert!(near(federal(&w, 2000.0), 149.17));
        // married jointly: 48,000 − 12,900 = 35,100 → 10% of 15,800 = 1,580 → 65.83
        let m = W4 {
            status: FedStatus::Married,
            ..W4::default()
        };
        assert!(near(federal(&m, 2000.0), 65.83));
        // Step 2 checked (single): 48,000 → 8,983 + 0 ... = 2,900 + 22% of 14,750 = 6,145 → 256.04
        let two = W4 {
            two_jobs: true,
            ..W4::default()
        };
        assert!(near(federal(&two, 2000.0), 256.04));
        // dependents take theirs off a check at a time, never below nothing; then the extra
        let kids = W4 {
            status: FedStatus::Married,
            dependents: 4400.0,
            extra: 10.0,
            ..W4::default()
        };
        assert!(near(federal(&kids, 2000.0), 10.0));
        assert_eq!(federal(&w, 0.0), 0.0);
    }

    #[test]
    fn missouri_by_the_book() {
        // Form 4282's example: married, spouse works, $35,000 a year → 18,900 taxable → 707.81 → $59 a month; a
        // half-month check of the same pay: 707.81 / 24 = 29.49 → $29
        let mo = MoW4 {
            status: MoStatus::SpouseWorks,
            extra: 0.0,
        };
        assert_eq!(missouri(&mo, 35_000.0 / 24.0), 29.0);
        assert_eq!(missouri(&MoW4 { extra: 5.0, ..mo.clone() }, 35_000.0 / 24.0), 34.0);
        // under the standard deduction: nothing
        assert_eq!(missouri(&mo, 600.0), 0.0);
        let home = MoW4 {
            status: MoStatus::SpouseHome,
            extra: 0.0,
        };
        assert!(missouri(&home, 2000.0) < missouri(&mo, 2000.0));
    }

    fn setup() -> PaySetup {
        PaySetup {
            at: 1,
            rates: Vec::new(),
            w4: W4::default(),
            mo: MoW4::default(),
            retirement: 5.0,
            roth: false,
            stubs: vec![
                Stub {
                    hours: 80.0,
                    rate: Some(25.0),
                    ..Stub::default()
                },
                Stub {
                    hours: 64.0,
                    gross: Some(1600.0),
                    ..Stub::default()
                },
            ],
            lines: vec![
                // the same both times: fixed
                Line {
                    name: "Health".into(),
                    pretax: true,
                    amounts: vec![Some(95.5), Some(95.5)],
                },
                // 1% both times: a part of the check
                Line {
                    name: "Union dues".into(),
                    pretax: false,
                    amounts: vec![Some(20.0), Some(16.0)],
                },
                // some of each: $5 + 2%
                Line {
                    name: "Uniforms".into(),
                    pretax: false,
                    amounts: vec![Some(45.0), Some(37.0)],
                },
            ],
        }
    }

    #[test]
    fn learns_fixed_and_percentages() {
        let s = setup();
        let l = learn(&s);
        assert_eq!(l.rate, Some(25.0));
        assert_eq!(l.lines[0].describe(), "$95.50 each check");
        assert_eq!(l.lines[1].describe(), "1% of the check");
        assert_eq!(l.lines[2].describe(), "$5.00 + 2% of the check");
        assert!(near(l.lines[2].on(1000.0), 25.0));
        // no taxes typed: nothing checked, nothing added
        assert!(l.checks.is_empty() && l.adjust == Taxes::default() && l.net_off.is_none());
        // one stub only: what it showed, each check
        let one = PaySetup {
            stubs: vec![s.stubs[0].clone()],
            ..s.clone()
        };
        assert_eq!(learn(&one).lines[1].describe(), "$20.00 each check");
        // a line only on one of the two: nothing on the other
        let mut gap = s.clone();
        gap.lines[0].amounts = vec![Some(10.0), None];
        assert!(learn(&gap).lines[0].on(2000.0) >= 0.0);
    }

    #[test]
    fn guesses_a_check() {
        let s = setup();
        let l = learn(&s);
        let b = predict(&s, &l, 80.0).unwrap();
        assert!(near(b.gross, 2000.0));
        assert!(near(b.retirement, 100.0));
        // FICA on 2,000 − 95.50 (health is before all taxes); income taxes after retirement too
        assert!(near(b.taxes.social, cents(1904.5 * 0.062)));
        assert!(near(b.taxes.medicare, cents(1904.5 * 0.0145)));
        assert!(near(b.taxes.federal, federal(&s.w4, 1804.5)));
        assert!(near(b.taxes.state, missouri(&s.mo, 1804.5)));
        let out = 100.0 + 95.5 + 20.0 + 45.0 + b.taxes.total();
        assert!(near(b.net, 2000.0 - out));
        // a Roth comes out after taxes
        let roth = PaySetup { roth: true, ..s.clone() };
        let r = predict(&roth, &learn(&roth), 80.0).unwrap();
        assert!(r.taxes.federal > b.taxes.federal && r.net < b.net);
        // nothing flagged: nothing
        let z = predict(&s, &l, 0.0).unwrap();
        assert_eq!((z.gross, z.taxes.total()), (0.0, 0.0));
        assert!(near(z.net, -95.5 - 5.0)); // (the fixed ones still come out)
        // no rate yet: no guess
        assert!(predict(&PaySetup::default(), &learn(&PaySetup::default()), 10.0).is_none());
    }

    #[test]
    fn checks_against_the_stubs() {
        let mut s = setup();
        // the stubs show $12 more federal than the W-4 figures each time, and Medicare to the cent
        for i in 0..2 {
            let g = s.stubs[i].gross().unwrap();
            let pretax = 95.5;
            let ret = cents(g * 0.05);
            let t = taxes(&s, g, pretax, ret);
            s.stubs[i].federal = Some(t.federal + 12.0);
            s.stubs[i].medicare = Some(t.medicare);
            let after = s.lines.iter().filter(|l| !l.pretax).map(|l| l.on(i)).sum::<f64>();
            s.stubs[i].net = Some(cents(g - pretax - after - ret - (t.total() + 12.0)));
        }
        let l = learn(&s);
        assert_eq!(l.checks.len(), 2);
        assert!(near(l.adjust.federal, 12.0) && !l.checks[0].matches());
        assert!(l.checks[1].matches());
        assert!(near(l.net_off.unwrap(), 0.0));
        let b = predict(&s, &l, 80.0).unwrap();
        let plain = predict(
            &s,
            &Learned {
                adjust: Taxes::default(),
                ..l.clone()
            },
            80.0,
        )
        .unwrap();
        assert!(near(b.taxes.federal, plain.taxes.federal + 12.0));
    }

    #[test]
    fn new_rates_from_a_pay_period_on() {
        let mut s = setup();
        let l = learn(&s);
        // none yet: the stubs' $25
        assert_eq!(predict_for(&s, &l, 80.0, 1000).unwrap().rate, 25.0);
        s.set_rate(1000, 27.5);
        s.set_rate(2000, 30.0);
        // before the first: still the stubs'; from each on: its own
        assert_eq!(predict_for(&s, &l, 80.0, 999).unwrap().rate, 25.0);
        assert_eq!(predict_for(&s, &l, 80.0, 1000).unwrap().rate, 27.5);
        assert_eq!(predict_for(&s, &l, 80.0, 1500).unwrap().gross, 2200.0);
        assert_eq!(predict_for(&s, &l, 80.0, 5000).unwrap().rate, 30.0);
        // the same period again: replaced, not added
        s.set_rate(2000, 31.0);
        assert_eq!((s.rates.len(), s.rate_from(2000)), (2, Some(31.0)));
        // (the stubs, and what's learned from them, untouched)
        assert_eq!(learn(&s), l);
    }

    #[test]
    fn reads_amounts_and_round_trips() {
        assert_eq!(parse_amount(" $1,234.50 "), Ok(Some(1234.5)));
        assert_eq!(parse_amount("6%"), Ok(Some(6.0)));
        assert_eq!(parse_amount(""), Ok(None));
        assert!(parse_amount("abc").is_err() && parse_amount("-4").is_err());
        assert_eq!(money(1234.5), "$1,234.50");
        let s = setup();
        let back: PaySetup = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        // a field added later just takes its default
        let old: PaySetup = serde_json::from_str(r#"{"retirement":4}"#).unwrap();
        assert_eq!((old.retirement, old.w4.status), (4.0, FedStatus::Single));
    }
}
