//! Your Xtime technician ID from your employee number (the Electron app's `employee.js`).
//! Only that person's ID and name are taken from the store's staff list; nobody's email or phone number is kept.

use serde::Serialize;
use serde_json::Value;
use workbench_net::Method;

use crate::json::{get, text};
use crate::{Error, Result, Xtime};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Employee {
    /// Xtime's technician ID (what the dashboard's ROs carry as TechnicianID)
    pub id: String,
    pub name: String,
    pub active: bool,
}

/// The staff list from Xtime's answer: JSON, or JSON inside a little script (`var Employees = [...];`)
pub fn parse_staff(body: &str) -> Option<Vec<Value>> {
    let data: Option<Value> = serde_json::from_str(body).ok().or_else(|| {
        let (a, b) = (body.find('[')?, body.rfind(']')?);
        if b > a { serde_json::from_str(&body[a..=b]).ok() } else { None }
    });
    match data? {
        Value::Array(list) => Some(list),
        Value::Object(map) => map.into_iter().find_map(|(_, v)| match v {
            Value::Array(list) => Some(list),
            _ => None,
        }),
        _ => None,
    }
}

/// The person with this employee number (leading zeros don't matter). If the number is there more than once: an
/// active technician first, then anyone active, then the first.
pub fn find(list: &[Value], number: &str, store: &str) -> Result<Employee> {
    let norm = |v: Option<&Value>| crate::json::no_leading_zeros(&text(v));
    let want = crate::json::no_leading_zeros(number);
    let matches: Vec<&Value> = list.iter().filter(|e| e.is_object() && norm(get(e, "Number")) == want).collect();
    if matches.is_empty() {
        return Err(Error::NotFound(format!("Employee number {number} isn't in store {store}'s list")));
    }
    let active = |e: &&Value| get(e, "Active") != Some(&Value::Bool(false));
    let is_tech = |e: &&Value| {
        text(get(e, "EmployeeType").and_then(|t| get(t, "Name")))
            .to_lowercase()
            .contains("tech")
    };
    let pick = matches
        .iter()
        .find(|e| active(e) && is_tech(e))
        .or_else(|| matches.iter().find(|e| active(e)))
        .unwrap_or(&matches[0]);
    let id = match get(pick, "ID") {
        None | Some(Value::Null) => return Err(Error::Format("That employee has no technician ID in Xtime".into())),
        id => text(id),
    };
    let name = ["FirstName", "LastName"]
        .iter()
        .filter_map(|k| get(pick, k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(Employee {
        id,
        name,
        active: active(pick),
    })
}

impl Xtime {
    /// Looks up an employee number in the store's staff list. `path`: normally [`crate::paths::EMPLOYEES`]
    pub async fn employee(&self, number: &str, store: &str, path: &str) -> Result<Employee> {
        let req = self
            .request(Method::GET, &path.replace("{STORE}", &encode(store)))?
            .header("Accept", "application/json, text/javascript, */*");
        let res = self.send(req).await?;
        if matches!(res.status.as_u16(), 401 | 403) || res.redirected {
            return Err(Error::SessionExpired);
        }
        if !res.ok() {
            return Err(Error::Status(res.status.as_u16()));
        }
        let list =
            parse_staff(&res.body).ok_or_else(|| Error::Format("The employee list wasn't in a format the app understands".into()))?;
        find(&list, number, store)
    }
}

/// encodeURIComponent, for the store ID in an address
pub(crate) fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"var Employees = [
        {"ID": 70, "Number": "0456", "FirstName": "Old", "LastName": "Record", "Active": false, "EmployeeType": {"Name": "Technician"}},
        {"ID": 77, "Number": "456", "FirstName": "Ashton", "LastName": "M", "Active": true, "EmployeeType": {"Name": "Technician"}, "Email": "x@y"},
        {"ID": 9, "Number": "123", "FirstName": "Tony", "LastName": " ", "EmployeeType": {"Name": "Advisor"}}
    ];"#;

    #[test]
    fn reads_the_script_wrapped_list() {
        assert_eq!(parse_staff(LIST).map(|l| l.len()), Some(3));
        assert_eq!(parse_staff(r#"{"data": [{"ID": 1}]}"#).map(|l| l.len()), Some(1));
        assert_eq!(parse_staff("<html>sign in</html>"), None);
    }

    #[test]
    fn prefers_the_active_technician() {
        let list = parse_staff(LIST).unwrap();
        assert_eq!(
            find(&list, "00456", "3590").unwrap(),
            Employee {
                id: "77".into(),
                name: "Ashton M".into(),
                active: true
            }
        );
        assert_eq!(find(&list, "123", "3590").unwrap().name, "Tony");
        assert!(matches!(find(&list, "999", "3590"), Err(Error::NotFound(_))));
    }

    #[test]
    fn encodes_like_encode_uri_component() {
        assert_eq!(encode("3590"), "3590");
        assert_eq!(encode("a b&c"), "a%20b%26c");
    }
}