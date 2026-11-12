//! Query-string parsing with repeated keys (`visible=a&visible=b`).

/// Decoded `key=value` pairs in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Params(Vec<(String, String)>);

impl Params {
    /// Parses a raw query string; `+` is a space, `%XX` is a byte.
    ///
    /// Example: `Params::parse(Some("id=clj%3A.%2F%2Fa&depth=2"))` gives
    /// `id = "clj:.//a"`, `depth = "2"`.
    pub fn parse(raw: Option<&str>) -> Params {
        let pairs = raw
            .unwrap_or("")
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                (decode(k), decode(v))
            })
            .collect();
        Params(pairs)
    }

    /// The first value of `key`.
    ///
    /// Example: `get("depth")` is `Some("2")`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Every value of `key`.
    ///
    /// Example: `all("visible")` for `visible=a&visible=b` is `["a", "b"]`.
    pub fn all(&self, key: &str) -> Vec<&str> {
        self.0
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

/// Percent-decodes one component; invalid escapes are kept as written and
/// invalid UTF-8 is replaced.
///
/// Example: `decode("a%20b+c")` is `"a b c"`.
fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The value of one hex digit.
///
/// Example: `hex(b'f')` is `Some(15)`; `hex(b'g')` is `None`.
fn hex(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_repeats() {
        let p = Params::parse(Some(
            "id=clj%3A.%2F%2Fshop.core%2Fcharge%21&visible=a&visible=b+c&flag",
        ));
        assert_eq!(p.get("id"), Some("clj:.//shop.core/charge!"));
        assert_eq!(p.all("visible"), ["a", "b c"]);
        assert_eq!(p.get("flag"), Some(""));
        assert_eq!(p.get("none"), None);
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%zz"), "%zz");
        assert_eq!(decode("%4"), "%4");
    }
}
