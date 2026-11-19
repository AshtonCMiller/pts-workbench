//! The diagnostic requests (UDS, ISO 14229) the network test and vehicle ID make, and reading their answers.
//! Only reads: ReadDataByIdentifier (22) and OBD's vehicle information (09).

/// ReadDataByIdentifier for a data identifier
pub fn read(did: u16) -> Vec<u8> {
    vec![0x22, (did >> 8) as u8, did as u8]
}

/// The VIN
pub const VIN: u16 = 0xF190;
/// The module's assembly part number (on its label, e.g. "NZ6T-14G371-ABC")
pub const PART: u16 = 0xF113;
/// Its software (Ford's "strategy")
pub const SOFTWARE: u16 = 0xF188;

/// OBD Mode 09 PID 02: the VIN, the way every OBD-II vehicle gives it
pub const OBD_VIN: [u8; 2] = [0x09, 0x02];

/// What a module answered
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The data asked for
    Data(Vec<u8>),
    /// It refused (UDS's negative response: its code)
    Refused(u8),
    /// Something else
    Odd(Vec<u8>),
}

/// Reads an answer to ReadDataByIdentifier `did`
pub fn answer_to_read(did: u16, r: &[u8]) -> Answer {
    match r {
        [0x62, hi, lo, rest @ ..] if u16::from(*hi) << 8 | u16::from(*lo) == did => Answer::Data(rest.to_vec()),
        [0x7F, 0x22, code, ..] => Answer::Refused(*code),
        _ => Answer::Odd(r.to_vec()),
    }
}

/// Text in an answer: printable characters, without the padding modules fill with (spaces, 00, FF)
pub fn text(data: &[u8]) -> String {
    let s: String = data
        .iter()
        .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { ' ' })
        .collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A VIN from an answer: 22 F190's, or OBD 09 02's (49 02 01 + the VIN)
pub fn vin_from(r: &[u8]) -> Option<String> {
    let data = match r {
        [0x62, 0xF1, 0x90, rest @ ..] => rest,
        [0x49, 0x02, _count, rest @ ..] => rest,
        _ => return None,
    };
    let v: String = text(data)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_uppercase();
    (v.len() == 17 && check_digit_ok(&v)).then_some(v)
}

/// A VIN's check digit (the 9th): North American VINs have one; it catches a garbled read
pub fn check_digit_ok(vin: &str) -> bool {
    let value = |c: char| -> Option<u32> {
        match c {
            '0'..='9' => c.to_digit(10),
            'A' | 'J' => Some(1),
            'B' | 'K' | 'S' => Some(2),
            'C' | 'L' | 'T' => Some(3),
            'D' | 'M' | 'U' => Some(4),
            'E' | 'N' | 'V' => Some(5),
            'F' | 'W' => Some(6),
            'G' | 'P' | 'X' => Some(7),
            'H' | 'Y' => Some(8),
            'R' | 'Z' => Some(9),
            _ => None, // I, O, Q never appear
        }
    };
    const WEIGHTS: [u32; 17] = [8, 7, 6, 5, 4, 3, 2, 10, 0, 9, 8, 7, 6, 5, 4, 3, 2];
    let chars: Vec<char> = vin.chars().collect();
    if chars.len() != 17 {
        return false;
    }
    let mut sum = 0;
    for (c, w) in chars.iter().zip(WEIGHTS) {
        match value(*c) {
            Some(v) => sum += v * w,
            None => return false,
        }
    }
    let check = match sum % 11 {
        10 => 'X',
        n => char::from_digit(n, 10).unwrap_or('?'),
    };
    chars[8] == check
}

/// A refusal's meaning
pub fn refusal(code: u8) -> &'static str {
    match code {
        0x10 => "general reject",
        0x11 => "service not supported",
        0x12 => "sub-function not supported",
        0x13 => "wrong message length",
        0x14 => "answer too long",
        0x21 => "busy, ask again",
        0x22 => "conditions not right",
        0x24 => "request out of sequence",
        0x31 => "doesn't have that",
        0x33 => "security access needed",
        0x78 => "still working on it",
        0x7E => "not in this session",
        0x7F => "not in this session",
        _ => "refused",
    }
}

/// Bytes as hex ("22 F1 90")
pub fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers() {
        assert_eq!(read(VIN), [0x22, 0xF1, 0x90]);
        let mut r = vec![0x62, 0xF1, 0x13];
        r.extend(b"NZ6T-14G371-ABC\0\0\xFF");
        match answer_to_read(PART, &r) {
            Answer::Data(d) => assert_eq!(text(&d), "NZ6T-14G371-ABC"),
            other => panic!("{other:?}"),
        }
        assert_eq!(answer_to_read(PART, &[0x7F, 0x22, 0x31]), Answer::Refused(0x31));
        assert_eq!(refusal(0x31), "doesn't have that");
        assert!(matches!(answer_to_read(PART, &[0x62, 0xF1, 0x88]), Answer::Odd(_)));
        assert_eq!(hex(&[0x22, 0xF1, 0x90]), "22 F1 90");
    }

    #[test]
    fn vins() {
        // (made up: the check digit, 9th, works out)
        assert!(check_digit_ok("11111111111111111"));
        assert!(!check_digit_ok("11111111211111111"));
        let mut r = vec![0x62, 0xF1, 0x90];
        r.extend(b"11111111111111111");
        assert_eq!(vin_from(&r).as_deref(), Some("11111111111111111"));
        let mut obd = vec![0x49, 0x02, 0x01];
        obd.extend(b"11111111111111111");
        assert_eq!(vin_from(&obd).as_deref(), Some("11111111111111111"));
        r[5] = b'2';
        assert_eq!(vin_from(&r), None);
    }
}
