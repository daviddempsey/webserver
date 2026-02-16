use std::collections::HashMap;

#[derive(Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query_string: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub params: HashMap<String, String>,
    pub body: Vec<u8>,
    pub extensions: HashMap<String, String>,
    pub remote_addr: Option<String>,
}

impl Request {
    pub fn new(method: String, raw_path: String) -> Self {
        let (path, query_string) = match raw_path.split_once('?') {
            Some((p, qs)) => (p.to_string(), parse_query_string(qs)),
            None => (raw_path, HashMap::new()),
        };

        Self {
            method,
            path,
            query_string,
            headers: HashMap::new(),
            params: HashMap::new(),
            body: Vec::new(),
            extensions: HashMap::new(),
            remote_addr: None,
        }
    }

    pub fn param(&self, name: &str) -> Option<&str> {
        self.params.get(name).map(|s| s.as_str())
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_lowercase()).map(|s| s.as_str())
    }

    pub fn body_str(&self) -> &str {
        std::str::from_utf8(&self.body).unwrap_or("")
    }

    pub fn query(&self, name: &str) -> Option<&str> {
        self.query_string.get(name).map(|s| s.as_str())
    }

    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }
}

fn parse_query_string(qs: &str) -> HashMap<String, String> {
    qs.split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((decode_percent(k), decode_percent(v)))
        })
        .collect()
}

fn decode_percent(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let mut bytes = s.as_bytes().iter();

    while let Some(&b) = bytes.next() {
        if b == b'%' {
            let hi = bytes.next().copied().unwrap_or(0);
            let lo = bytes.next().copied().unwrap_or(0);
            if let (Some(h), Some(l)) = (hex_val(hi), hex_val(lo)) {
                out.push(h << 4 | l);
                continue;
            }
        } else if b == b'+' {
            out.push(b' ');
            continue;
        }
        out.push(b);
    }

    String::from_utf8(out).unwrap_or_default()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
