//! Amazon's event stream, put together here: what decodes it has no part in it. Shared with the engine's
//! and the host's tests.
#![allow(dead_code)]

/// Text headers, payload.
pub fn message(headers: &[(&str, &str)], payload: &[u8]) -> Vec<u8> {
    let mut head = Vec::new();
    for (name, value) in headers {
        head.extend(text(name, value));
    }
    framed(&head, payload)
}
pub fn text(name: &str, value: &str) -> Vec<u8> {
    let mut out = vec![u8::try_from(name.len()).unwrap()];
    out.extend(name.as_bytes());
    out.push(7);
    out.extend(u16::try_from(value.len()).unwrap().to_be_bytes());
    out.extend(value.as_bytes());
    out
}
pub fn framed(head: &[u8], payload: &[u8]) -> Vec<u8> {
    let total = u32::try_from(16 + head.len() + payload.len()).unwrap();
    let headers = u32::try_from(head.len()).unwrap();
    let mut out = [total.to_be_bytes(), headers.to_be_bytes()].concat();
    out.extend(crc(&out).to_be_bytes());
    out.extend(head);
    out.extend(payload);
    out.extend(crc(&out).to_be_bytes());
    out
}
pub fn event(name: &str, json: &str) -> Vec<u8> {
    let headers =
        [(":event-type", name), (":content-type", "application/json"), (":message-type", "event")];
    message(&headers, json.as_bytes())
}
pub fn exception(name: &str, json: &str) -> Vec<u8> {
    let headers = [
        (":exception-type", name),
        (":content-type", "application/json"),
        (":message-type", "exception"),
    ];
    message(&headers, json.as_bytes())
}
pub fn error(code: &str, words: &str) -> Vec<u8> {
    let headers = [(":error-code", code), (":error-message", words), (":message-type", "error")];
    message(&headers, b"")
}
/// Events as the documentation prints them: an object with one member, a line each.
pub fn stream(lines: &str) -> Vec<u8> {
    let lines = lines.lines().map(str::trim).filter(|l| !l.is_empty());
    lines
        .flat_map(|l| {
            let all: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str(l).unwrap_or_else(|e| panic!("{l}: {e}"));
            let [(name, _)] = all.iter().collect::<Vec<_>>()[..] else { panic!("{l}") };
            // Its own text, not what a map made of it.
            let json = l.split_once(':').unwrap().1.trim().strip_suffix('}').unwrap();
            event(name, json)
        })
        .collect()
}
pub fn crc(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for b in bytes {
        for bit in 0..8 {
            let out = (crc ^ u32::from(b >> bit)) & 1;
            crc = (crc >> 1) ^ (out * 0xEDB8_8320);
        }
    }
    !crc
}
