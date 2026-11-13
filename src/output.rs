//! Conservative, local, model-facing presentation; not reward evidence.
use serde::Serialize;
use serde_json::Value;
use std::fmt::Write as _;
use std::io;

/// Compute the compact JSON byte length of `value`, aborting as soon as it exceeds `limit`.
pub(crate) fn json_len_within<T: Serialize + ?Sized>(
    value: &T,
    limit: usize,
) -> Result<Option<usize>, String> {
    struct LimitedCounter {
        count: usize,
        limit: usize,
        exceeded: bool,
    }
    impl io::Write for LimitedCounter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let next = self.count.saturating_add(buf.len());
            if next > self.limit {
                self.exceeded = true;
                return Err(io::Error::other("byte limit exceeded"));
            }
            self.count = next;
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = LimitedCounter {
        count: 0,
        limit,
        exceeded: false,
    };
    match serde_json::to_writer(&mut counter, value) {
        Ok(()) => Ok(Some(counter.count)),
        Err(_) if counter.exceeded => Ok(None),
        Err(_) => Err("invalid JSON".into()),
    }
}

/// Model-facing projection only. The caller must durably retain the full result
/// before handing us its audit ID. Keep both opening findings and closing caveats.
pub fn worker_handoff(index: usize, audit_event: &str, text: &str, tool_calls: usize) -> Value {
    const LIMIT: usize = 8192;
    let (shown, omitted) = if text.len() <= LIMIT {
        (text.to_owned(), 0)
    } else {
        let mut head = LIMIT * 3 / 4;
        let mut tail = text.len() - LIMIT / 4;
        while !text.is_char_boundary(head) {
            head -= 1;
        }
        while !text.is_char_boundary(tail) {
            tail += 1;
        }
        // Separate fragments structurally with an explicit omission marker.
        // Reserve its bytes inside the same limit.
        let marker = "\n[zeus: middle omitted]\n";
        head -= marker.len();
        while !text.is_char_boundary(head) {
            head -= 1;
        }
        (
            format!("{}{}{}", &text[..head], marker, &text[tail..]),
            tail - head,
        )
    };
    serde_json::json!({
        "index": index,
        "audit_event": audit_event,
        "tool_calls": tool_calls,
        "original_bytes": text.len(),
        "omitted_bytes": omitted,
        "text": shown,
    })
}

/// Clone a completed successful verifier result and compact strictly recognized
/// libtest stdout. Call only AFTER storing the raw audit evidence. All other
/// fields (including stderr) and all unsupported/malformed output are unchanged.
pub fn compact_verifier_result(value: &Value) -> Value {
    if let Some(obj) = value.as_object() {
        if obj.get("success").and_then(Value::as_bool) == Some(true)
            && obj.get("exit_code").and_then(Value::as_i64) == Some(0)
            && obj.get("timed_out").and_then(Value::as_bool) == Some(false)
            && obj.get("output_limit").and_then(Value::as_bool) == Some(false)
            && obj.get("stderr").and_then(Value::as_str).is_some()
        {
            if let Some(compact) = obj
                .get("stdout")
                .and_then(Value::as_str)
                .and_then(compact_libtest)
            {
                let mut replacement = Some(compact);
                let mut map = serde_json::Map::with_capacity(obj.len());
                for (k, v) in obj {
                    let val = if k == "stdout" {
                        Value::String(replacement.take().unwrap_or_default())
                    } else {
                        v.clone()
                    };
                    map.insert(k.clone(), val);
                }
                return Value::Object(map);
            }
        }
    }
    value.clone()
}

fn compact_libtest(raw: &str) -> Option<String> {
    if !raw.trim_start_matches('\n').starts_with("running ") {
        return None;
    }
    let mut lines = raw.split_inclusive('\n');
    let mut raw_cursor = 0usize;
    let mut out = String::with_capacity(256);
    while let Some(line) = lines.next() {
        raw_cursor += line.len();
        if line == "\n" {
            out.push('\n');
            continue;
        }
        let header = line;
        let rest = header.strip_prefix("running ")?;
        let (num_str, tail) = rest.split_once(' ')?;
        if num_str.is_empty()
            || (num_str != "0" && num_str.starts_with('0'))
            || !num_str.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let count: usize = num_str.parse().ok()?;
        let expected_tail = if count == 1 { "test\n" } else { "tests\n" };
        if tail != expected_tail {
            return None;
        }
        out.push_str(header);
        let block_start = raw_cursor;
        for _ in 0..count {
            let test_line = lines.next()?;
            raw_cursor += test_line.len();
            let name = test_line.strip_prefix("test ")?.strip_suffix(" ... ok\n")?;
            let valid_name = if name.is_ascii() {
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b':')
            } else {
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
            };
            if !valid_name {
                return None;
            }
        }
        let omitted_bytes = raw_cursor - block_start;
        // Exact marker byte length: static parts (47) + decimal(count) + decimal(omitted_bytes).
        let marker_len = "[zeus: omitted  passing test lines;  UTF-8 bytes]\n".len()
            + num_str.len()
            + omitted_bytes.checked_ilog10().map_or(1, |d| d as usize + 1);
        if marker_len < omitted_bytes {
            let _ = writeln!(
                out,
                "[zeus: omitted {count} passing test lines; {omitted_bytes} UTF-8 bytes]"
            );
        } else {
            out.push_str(&raw[block_start..raw_cursor]);
        }
        let blank = lines.next()?;
        raw_cursor += blank.len();
        if blank != "\n" {
            return None;
        }
        out.push('\n');
        let summary = lines.next()?;
        raw_cursor += summary.len();
        let after_prefix = summary
            .strip_suffix('\n')
            .unwrap_or(summary)
            .strip_prefix("test result: ok. ")?;
        let after_num = after_prefix.strip_prefix(num_str)?;
        let seconds = after_num
            .strip_prefix(" passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ")?
            .strip_suffix('s')?;
        let (whole, fraction) = seconds.split_once('.')?;
        if whole.is_empty()
            || fraction.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        out.push_str(summary);
    }
    (out.len() < raw.len()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_len_within_returns_exact_length_and_stops_at_limit() {
        let value = serde_json::json!({
            "alpha": "hello world",
            "numbers": [1, 2, 3, 4],
        });
        let expected = value.to_string().len();
        assert_eq!(json_len_within(&value, expected).unwrap(), Some(expected));
        assert_eq!(json_len_within(&value, expected - 1).unwrap(), None);
        assert_eq!(json_len_within(&value, 0).unwrap(), None);
    }
}
