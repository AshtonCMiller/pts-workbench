//! QR codes: made here (ISO/IEC 18004: byte mode, versions 1 to 10, error correction L or M) and drawn with iced's
//! canvas. Enough for a web address with a pairing code after it (the phone's "Pair phone" code), with no crate to
//! add for it.

use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};

/// How much of the code can be damaged and still read: L (7%) makes the smallest code, M (15%) a sturdier one
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    L,
    M,
}

/// A QR code: `size` × `size` modules, dark or light
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qr {
    pub size: usize,
    dark: Vec<bool>,
}

impl Qr {
    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        self.dark[y * self.size + x]
    }
}

/// (total codewords, EC codewords per block, blocks: (count, data codewords each)) for L and M, versions 1 to 10
type Blocks = (usize, &'static [(usize, usize)]);
const TABLE: [(usize, Blocks, Blocks); 10] = [
    (26, (7, &[(1, 19)]), (10, &[(1, 16)])),
    (44, (10, &[(1, 34)]), (16, &[(1, 28)])),
    (70, (15, &[(1, 55)]), (26, &[(1, 44)])),
    (100, (20, &[(1, 80)]), (18, &[(2, 32)])),
    (134, (26, &[(1, 108)]), (24, &[(2, 43)])),
    (172, (18, &[(2, 68)]), (16, &[(4, 27)])),
    (196, (20, &[(2, 78)]), (18, &[(4, 31)])),
    (242, (24, &[(2, 97)]), (22, &[(2, 38), (2, 39)])),
    (292, (30, &[(2, 116)]), (22, &[(3, 36), (2, 37)])),
    (346, (18, &[(2, 68), (2, 69)]), (26, &[(4, 43), (1, 44)])),
];
const ALIGN: [&[usize]; 10] = [
    &[],
    &[6, 18],
    &[6, 22],
    &[6, 26],
    &[6, 30],
    &[6, 34],
    &[6, 22, 38],
    &[6, 24, 42],
    &[6, 26, 46],
    &[6, 28, 50],
];

/// The smallest code that holds `data` (none: too long for version 10)
pub fn encode(data: &[u8], level: Level) -> Option<Qr> {
    for v in 1..=10 {
        let (_, blocks) = blocks_of(v, level);
        let capacity: usize = blocks.1.iter().map(|(n, d)| n * d).sum();
        let count_bits = if v < 10 { 8 } else { 16 };
        if 4 + count_bits + data.len() * 8 <= capacity * 8 {
            return Some(build(data, v, level, count_bits));
        }
    }
    None
}

fn blocks_of(v: usize, level: Level) -> (usize, Blocks) {
    let (total, l, m) = TABLE[v - 1];
    (total, if level == Level::L { l } else { m })
}

// ---------- the codewords ----------

struct Bits(Vec<u8>, usize);

impl Bits {
    fn push(&mut self, value: u32, n: usize) {
        for i in (0..n).rev() {
            if self.1 % 8 == 0 {
                self.0.push(0);
            }
            if (value >> i) & 1 == 1 {
                let last = self.0.len() - 1;
                self.0[last] |= 0x80 >> (self.1 % 8);
            }
            self.1 += 1;
        }
    }
}

/// GF(256) (x⁸ + x⁴ + x³ + x² + 1): its powers of 2 and their logs
fn gf() -> ([u8; 256], [u8; 256]) {
    let (mut exp, mut log) = ([0u8; 256], [0u8; 256]);
    let mut x: u16 = 1;
    for i in 0..255 {
        exp[i] = x as u8;
        log[x as usize] = i as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= 0x11d;
        }
    }
    exp[255] = exp[0];
    (exp, log)
}

/// Reed–Solomon: the `n` error correction codewords for `data`
fn ec_codewords(data: &[u8], n: usize) -> Vec<u8> {
    let (exp, log) = gf();
    let mul = |a: u8, b: u8| -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            exp[(log[a as usize] as usize + log[b as usize] as usize) % 255]
        }
    };
    // the generator: (x - 2⁰)(x - 2¹)…(x - 2ⁿ⁻¹), highest power first
    let mut g = vec![1u8];
    for i in 0..n {
        let mut next = vec![0u8; g.len() + 1];
        for (j, &c) in g.iter().enumerate() {
            next[j] ^= c;
            next[j + 1] ^= mul(c, exp[i]);
        }
        g = next;
    }
    let mut rem = vec![0u8; n];
    for &d in data {
        let factor = d ^ rem[0];
        rem.remove(0);
        rem.push(0);
        for (j, r) in rem.iter_mut().enumerate() {
            *r ^= mul(g[j + 1], factor);
        }
    }
    rem
}

/// The data and error correction codewords, interleaved as they're placed
fn codewords(data: &[u8], v: usize, level: Level, count_bits: usize) -> Vec<u8> {
    let (total, (ec, groups)) = blocks_of(v, level);
    let capacity: usize = groups.iter().map(|(n, d)| n * d).sum();
    let mut bits = Bits(Vec::new(), 0);
    bits.push(0b0100, 4);
    bits.push(data.len() as u32, count_bits);
    for &b in data {
        bits.push(b as u32, 8);
    }
    let room = capacity * 8 - bits.1;
    bits.push(0, room.min(4));
    while bits.1 % 8 != 0 {
        bits.push(0, 1);
    }
    let mut cw = bits.0;
    for pad in [0xec, 0x11].iter().cycle() {
        if cw.len() >= capacity {
            break;
        }
        cw.push(*pad);
    }
    // split into blocks, each with its own error correction
    let mut blocks: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut at = 0;
    for &(n, d) in groups {
        for _ in 0..n {
            let part = cw[at..at + d].to_vec();
            at += d;
            let e = ec_codewords(&part, ec);
            blocks.push((part, e));
        }
    }
    let mut out = Vec::with_capacity(total);
    let longest = blocks.iter().map(|b| b.0.len()).max().unwrap_or(0);
    for i in 0..longest {
        for b in &blocks {
            if let Some(&x) = b.0.get(i) {
                out.push(x);
            }
        }
    }
    for i in 0..ec {
        for b in &blocks {
            out.push(b.1[i]);
        }
    }
    out
}

// ---------- the modules ----------

struct Grid {
    size: usize,
    dark: Vec<bool>,
    /// Taken by a pattern (not data, not masked)
    fixed: Vec<bool>,
}

impl Grid {
    fn set(&mut self, x: usize, y: usize, dark: bool) {
        let i = y * self.size + x;
        self.dark[i] = dark;
        self.fixed[i] = true;
    }

    fn finder(&mut self, cx: isize, cy: isize) {
        for dy in -4..=4isize {
            for dx in -4..=4isize {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x >= self.size as isize || y >= self.size as isize {
                    continue;
                }
                let d = dx.abs().max(dy.abs());
                self.set(x as usize, y as usize, d != 2 && d != 4);
            }
        }
    }
}

fn bch(value: u32, poly: u32, poly_bits: u32) -> u32 {
    let mut v = value << (poly_bits - 1);
    while v != 0 && 32 - v.leading_zeros() >= poly_bits {
        v ^= poly << (32 - v.leading_zeros() - poly_bits);
    }
    v
}

fn mask_bit(mask: u8, x: usize, y: usize) -> bool {
    match mask {
        0 => (x + y) % 2 == 0,
        1 => y % 2 == 0,
        2 => x % 3 == 0,
        3 => (x + y) % 3 == 0,
        4 => (y / 2 + x / 3) % 2 == 0,
        5 => (x * y) % 2 + (x * y) % 3 == 0,
        6 => ((x * y) % 2 + (x * y) % 3) % 2 == 0,
        _ => ((x + y) % 2 + (x * y) % 3) % 2 == 0,
    }
}

fn format_bits(level: Level, mask: u8) -> u32 {
    let ec: u32 = if level == Level::L { 0b01 } else { 0b00 };
    let data = (ec << 3) | mask as u32;
    ((data << 10) | bch(data, 0x537, 11)) ^ 0x5412
}

fn build(data: &[u8], v: usize, level: Level, count_bits: usize) -> Qr {
    let size = 17 + 4 * v;
    let mut g = Grid {
        size,
        dark: vec![false; size * size],
        fixed: vec![false; size * size],
    };
    let s = size as isize;
    g.finder(3, 3);
    g.finder(s - 4, 3);
    g.finder(3, s - 4);
    for i in 8..size - 8 {
        g.set(i, 6, i % 2 == 0);
        g.set(6, i, i % 2 == 0);
    }
    let al = ALIGN[v - 1];
    for &ay in al {
        for &ax in al {
            // (not over the finders)
            if (ax == 6 && ay == 6) || (ax == 6 && ay == size - 7) || (ax == size - 7 && ay == 6) {
                continue;
            }
            for dy in -2..=2isize {
                for dx in -2..=2isize {
                    let d = dx.abs().max(dy.abs());
                    g.set((ax as isize + dx) as usize, (ay as isize + dy) as usize, d != 1);
                }
            }
        }
    }
    // the format's places (filled in after the mask is picked), and the dark module
    for i in (0..9).filter(|i| *i != 6) {
        g.set(8, i, false);
        g.set(i, 8, false);
    }
    for i in 0..8 {
        g.set(size - 1 - i, 8, false);
        g.set(8, size - 1 - i, false);
    }
    g.set(8, size - 8, true);
    // the version (7 and up)
    if v >= 7 {
        let bits = ((v as u32) << 12) | bch(v as u32, 0x1f25, 13);
        for i in 0..18 {
            let dark = (bits >> i) & 1 == 1;
            let (a, b) = (i / 3, size - 11 + i % 3);
            g.set(a, b, dark);
            g.set(b, a, dark);
        }
    }
    // the codewords, up and down in two-module columns from the bottom right
    let cw = codewords(data, v, level, count_bits);
    let mut bit = 0usize;
    let total_bits = cw.len() * 8;
    let mut x = size as isize - 1;
    let mut upward = true;
    while x > 0 {
        if x == 6 {
            x -= 1;
        }
        for k in 0..size {
            let y = if upward { size - 1 - k } else { k };
            for dx in 0..2 {
                let xx = (x - dx) as usize;
                let i = y * size + xx;
                if g.fixed[i] {
                    continue;
                }
                let dark = bit < total_bits && (cw[bit / 8] >> (7 - bit % 8)) & 1 == 1;
                g.dark[i] = dark;
                bit += 1;
            }
        }
        upward = !upward;
        x -= 2;
    }
    // the mask that leaves the fewest confusing patterns
    let mut best: Option<(u32, Vec<bool>)> = None;
    for mask in 0..8u8 {
        let mut d = g.dark.clone();
        for y in 0..size {
            for x in 0..size {
                if !g.fixed[y * size + x] && mask_bit(mask, x, y) {
                    d[y * size + x] ^= true;
                }
            }
        }
        place_format(&mut d, size, format_bits(level, mask));
        let p = penalty(&d, size);
        if best.as_ref().is_none_or(|(bp, _)| p < *bp) {
            best = Some((p, d));
        }
    }
    Qr {
        size,
        dark: best.map(|b| b.1).unwrap_or(g.dark),
    }
}

fn place_format(d: &mut [bool], size: usize, bits: u32) {
    let bit = |i: u32| (bits >> i) & 1 == 1;
    // around the top left finder
    for i in 0..6 {
        d[i * size + 8] = bit(i as u32);
    }
    d[7 * size + 8] = bit(6);
    d[8 * size + 8] = bit(7);
    d[8 * size + 7] = bit(8);
    for i in 9..15 {
        d[8 * size + (14 - i)] = bit(i as u32);
    }
    // beside the other two
    for i in 0..8 {
        d[8 * size + (size - 1 - i)] = bit(i as u32);
    }
    for i in 8..15 {
        d[(size - 15 + i) * size + 8] = bit(i as u32);
    }
    d[(size - 8) * size + 8] = true;
}

fn penalty(d: &[bool], size: usize) -> u32 {
    let at = |x: usize, y: usize| d[y * size + x];
    let mut p = 0u32;
    // runs of five or more alike, across and down
    for horizontal in [true, false] {
        for a in 0..size {
            let mut run = 1;
            for b in 1..size {
                let (cur, prev) = if horizontal {
                    (at(b, a), at(b - 1, a))
                } else {
                    (at(a, b), at(a, b - 1))
                };
                if cur == prev {
                    run += 1;
                } else {
                    if run >= 5 {
                        p += 3 + (run - 5);
                    }
                    run = 1;
                }
            }
            if run >= 5 {
                p += 3 + (run - 5);
            }
        }
    }
    // 2×2 blocks alike
    for y in 0..size - 1 {
        for x in 0..size - 1 {
            let c = at(x, y);
            if at(x + 1, y) == c && at(x, y + 1) == c && at(x + 1, y + 1) == c {
                p += 3;
            }
        }
    }
    // finder-like patterns (1:1:3:1:1 with four light beside)
    const A: [bool; 11] = [true, false, true, true, true, false, true, false, false, false, false];
    const B: [bool; 11] = [false, false, false, false, true, false, true, true, true, false, true];
    for horizontal in [true, false] {
        for a in 0..size {
            for b in 0..size.saturating_sub(10) {
                let get = |k: usize| if horizontal { at(b + k, a) } else { at(a, b + k) };
                if (0..11).all(|k| get(k) == A[k]) || (0..11).all(|k| get(k) == B[k]) {
                    p += 40;
                }
            }
        }
    }
    // how far from half dark
    let darks = d.iter().filter(|x| **x).count();
    let pct = darks * 100 / d.len();
    p += (pct.abs_diff(50) / 5) as u32 * 10;
    p
}

// ---------- drawn ----------

struct Drawing {
    qr: Qr,
}

impl<Message> canvas::Program<Message> for Drawing {
    type State = ();

    fn draw(&self, _state: &(), renderer: &Renderer, _theme: &Theme, bounds: Rectangle, _cursor: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        // always dark on white (with room around it), whatever the theme: phones read it best that way
        let quiet = 4.0;
        let cells = self.qr.size as f32 + quiet * 2.0;
        let unit = (bounds.width.min(bounds.height) / cells).floor().max(1.0);
        let side = unit * cells;
        let origin = Point::new((bounds.width - side) / 2.0, (bounds.height - side) / 2.0);
        frame.fill(
            &Path::rounded_rectangle(origin, Size::new(side, side), (unit * 2.0).into()),
            Color::WHITE,
        );
        let ink = Color::from_rgb8(0x11, 0x18, 0x27);
        for y in 0..self.qr.size {
            let mut x = 0;
            while x < self.qr.size {
                if !self.qr.is_dark(x, y) {
                    x += 1;
                    continue;
                }
                // a run of dark modules as one rectangle
                let start = x;
                while x < self.qr.size && self.qr.is_dark(x, y) {
                    x += 1;
                }
                frame.fill_rectangle(
                    Point::new(origin.x + (start as f32 + quiet) * unit, origin.y + (y as f32 + quiet) * unit),
                    Size::new((x - start) as f32 * unit, unit),
                    ink,
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

/// The code drawn `side` pixels square
pub fn view<'a, Message: 'a>(qr: &Qr, side: f32) -> Element<'a, Message> {
    Canvas::new(Drawing { qr: qr.clone() })
        .width(Length::Fixed(side))
        .height(Length::Fixed(side))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(encode(b"hello", Level::M).map(|q| q.size), Some(21));
        let link = format!(
            "https://pts-sync.parkourdev.workers.dev/phone/#pair=v1.{}.{}",
            "a".repeat(64),
            "b".repeat(64)
        );
        assert_eq!(encode(link.as_bytes(), Level::L).map(|q| q.size), Some(49));
        assert!(encode(&[0u8; 400], Level::L).is_none());
    }

    #[test]
    fn format_and_ec() {
        // ISO 18004's example: level M, mask 5 → 100000011001110
        assert_eq!(format_bits(Level::M, 5), 0b100000011001110);
        // the version 7 block: 000111110010010100
        assert_eq!((7 << 12) | bch(7, 0x1f25, 13), 0b000111110010010100);
        // "01234567" (version 1-M), as in the standard's annex
        let data = [
            0x10, 0x20, 0x0c, 0x56, 0x61, 0x80, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11,
        ];
        assert_eq!(
            ec_codewords(&data, 10),
            [0xa5, 0x24, 0xd4, 0xc1, 0xed, 0x36, 0xc7, 0x87, 0x2c, 0x55]
        );
    }
}