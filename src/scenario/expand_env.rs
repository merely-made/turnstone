// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Replace every `${NAME}` in `line` with that process environment variable.
/// A name with no value set is a parse error (named by line), so a typo'd or
/// forgotten export fails loudly instead of leaving a literal `${NAME}` to be
/// matched against by accident.
pub(crate) fn expand_env(line: &str, line_no: usize) -> Result<String, String> {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find("${") {
        let Some(end) = rest[start + 2..].find('}') else {
            out.push_str(&rest[..start + 2]);
            rest = &rest[start + 2..];
            continue;
        };
        let name = &rest[start + 2..start + 2 + end];
        let value =
            std::env::var(name).map_err(|_| format!("line {line_no}: ${{{name}}} is not set"))?;
        out.push_str(&rest[..start]);
        out.push_str(&value);
        rest = &rest[start + 2 + end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}
