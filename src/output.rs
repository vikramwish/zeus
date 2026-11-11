//! Conservative, local, model-facing presentation; not reward evidence.
use serde_json::Value;

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
    let mut shown = value.clone();
    if value.get("success").and_then(Value::as_bool) != Some(true)
        || value.get("exit_code").and_then(Value::as_i64) != Some(0)
        || value.get("timed_out").and_then(Value::as_bool) != Some(false)
        || value.get("output_limit").and_then(Value::as_bool) != Some(false)
        || value.get("stderr").and_then(Value::as_str).is_none()
    {
        return shown;
    }
    if let Some(raw) = value.get("stdout").and_then(Value::as_str) {
        if let Some(compact) = compact_libtest(raw) {
            shown["stdout"] = Value::String(compact);
        }
    }
    shown
}

fn compact_libtest(raw: &str) -> Option<String> {
    if raw.chars().any(|c| c.is_control() && c != '\n') {
        return None;
    }
    let lines: Vec<&str> = raw.split_inclusive('\n').collect();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < lines.len() {
        if lines[i] == "\n" {
            out.push_str(lines[i]);
            i += 1;
            continue;
        }
        let header = lines[i];
        let number = header.strip_prefix("running ")?;
        let count: usize = number
            .strip_suffix(" tests\n")
            .or_else(|| number.strip_suffix(" test\n"))?
            .parse()
            .ok()?;
        let noun = if count == 1 { "test" } else { "tests" };
        if header != format!("running {count} {noun}\n") {
            return None;
        }
        out.push_str(header);
        i += 1;
        let start = i;
        for _ in 0..count {
            let name = lines
                .get(i)?
                .strip_prefix("test ")?
                .strip_suffix(" ... ok\n")?;
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
            {
                return None;
            }
            i += 1;
        }
        let omitted_bytes: usize = lines[start..i].iter().map(|s| s.len()).sum();
        let marker =
            format!("[zeus: omitted {count} passing test lines; {omitted_bytes} UTF-8 bytes]\n");
        if marker.len() < omitted_bytes {
            out.push_str(&marker);
        } else {
            for line in &lines[start..i] {
                out.push_str(line);
            }
        }
        if *lines.get(i)? != "\n" {
            return None;
        }
        out.push('\n');
        i += 1;
        let summary = *lines.get(i)?;
        let prefix = format!("test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ");
        let seconds = summary
            .strip_suffix('\n')
            .unwrap_or(summary)
            .strip_prefix(&prefix)?
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
        i += 1;
    }
    (out.len() < raw.len()).then_some(out)
}
