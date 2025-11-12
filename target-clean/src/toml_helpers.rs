// Small helpers for TOML/parse error message extraction shared across modules.
// Extracted from multiple files to avoid duplication.

/// Look for patterns like "line N column M" in parser error messages and
/// return (line, column) when found.
#[must_use]
pub fn extract_line_col_from_msg(msg: &str) -> Option<(usize, usize)> {
    if let Some(pos) = msg.find("line ") {
        let after = &msg[pos + 5..];
        let mut digits = String::new();
        for ch in after.chars() {
            if ch.is_ascii_digit() {
                digits.push(ch);
            } else {
                break;
            }
        }
        if !digits.is_empty() {
            if let Ok(line) = digits.parse::<usize>() {
                if let Some(pos2) = after.find("column ") {
                    let after2 = &after[pos2 + 7..];
                    let mut digs2 = String::new();
                    for ch in after2.chars() {
                        if ch.is_ascii_digit() {
                            digs2.push(ch);
                        } else {
                            break;
                        }
                    }
                    if !digs2.is_empty() {
                        if let Ok(col) = digs2.parse::<usize>() {
                            return Some((line, col));
                        }
                    }
                }
            }
        }
    }
    None
}
