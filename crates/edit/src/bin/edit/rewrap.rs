// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use edit::helpers::*;
use edit::tui::*;

use crate::state::*;

const DEFAULT_WRAP_COLUMN: usize = 80;

/// Known comment markers for a given file extension.
struct CommentMarkers {
    /// Line comment markers, ordered longest-first (e.g., `///` before `//`).
    line: &'static [&'static str],
}

fn comment_markers_for_extension(ext: &str) -> CommentMarkers {
    match ext {
        "rs" => CommentMarkers { line: &["///", "//!", "//"] },
        "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx" | "cs" | "java" | "js" | "jsx"
        | "ts" | "tsx" | "go" | "swift" | "scala" | "kt" | "kts" | "d" | "dart" | "zig"
        | "v" | "sv" | "groovy" | "gradle" | "php" | "m" | "mm" | "proto" | "jsonc" | "css"
        | "scss" | "less" | "sass" => CommentMarkers { line: &["///", "//"] },
        "py" | "pyi" | "rb" | "sh" | "bash" | "zsh" | "fish" | "yml" | "yaml" | "toml"
        | "cfg" | "ini" | "conf" | "pl" | "pm" | "r" | "jl" | "gd" | "nim" | "cr"
        | "dockerfile" | "cmake" | "makefile" | "mk" | "tf" | "hcl" | "nix" | "ps1"
        | "psm1" | "elixir" | "ex" | "exs" => CommentMarkers { line: &["##", "#"] },
        "lua" | "hs" | "sql" | "ada" | "adb" | "ads" | "elm" | "vhdl" => {
            CommentMarkers { line: &["--"] }
        }
        "el" | "lisp" | "clj" | "cljs" | "scm" | "rkt" | "asm" | "s" => {
            CommentMarkers { line: &[";;", ";"] }
        }
        "bat" | "cmd" => CommentMarkers { line: &["rem ", "::"] },
        "tex" | "sty" | "cls" | "bib" | "dtx" | "ins" | "erl" | "hrl" => {
            CommentMarkers { line: &["%"] }
        }
        "f" | "f90" | "f95" | "f03" | "f08" | "for" => CommentMarkers { line: &["!"] },
        _ => CommentMarkers { line: &["///", "//", "##", "#", "--", ";;", ";", "%", "!"] },
    }
}

/// Detected prefix for a line: the full prefix string and the content after it.
#[derive(Clone, Debug)]
struct LinePrefix {
    /// The full prefix including leading whitespace, comment marker, and trailing space.
    prefix: String,
    /// The content after the prefix.
    content: String,
}

/// Try to detect a comment prefix on the given line.
/// Returns the prefix (whitespace + marker + optional space) and remaining content.
fn detect_prefix(line: &str, markers: &CommentMarkers) -> LinePrefix {
    let trimmed = line.trim_start();
    let leading_ws = &line[..line.len() - trimmed.len()];

    for &marker in markers.line {
        if trimmed.starts_with(marker) {
            let after_marker = &trimmed[marker.len()..];
            // Include one space after marker if present.
            let space = if after_marker.starts_with(' ') { " " } else { "" };
            let prefix = format!("{}{}{}", leading_ws, marker, space);
            let content = &after_marker[space.len()..];
            return LinePrefix { prefix, content: content.to_string() };
        }
    }

    // No comment marker found. Use leading whitespace as the prefix.
    LinePrefix { prefix: leading_ws.to_string(), content: trimmed.to_string() }
}

/// Check if a line is "blank" for paragraph boundary purposes.
/// A line is blank if, after stripping the prefix, the content is empty or only whitespace.
fn is_blank_content(line: &str, markers: &CommentMarkers) -> bool {
    let parsed = detect_prefix(line, markers);
    parsed.content.trim().is_empty()
}

/// Check if two prefixes are compatible (same paragraph).
/// They must have the same comment marker and indentation.
fn prefixes_compatible(a: &LinePrefix, b: &LinePrefix) -> bool {
    a.prefix == b.prefix
}

/// Find the paragraph boundaries around `cursor_line`.
/// Returns (first_line, last_line) inclusive.
fn find_paragraph(
    lines: &[String],
    cursor_line: usize,
    markers: &CommentMarkers,
) -> (usize, usize) {
    if cursor_line >= lines.len() {
        return (cursor_line, cursor_line);
    }

    let cursor_prefix = detect_prefix(&lines[cursor_line], markers);

    // If cursor is on a blank line, just return that single line.
    if cursor_prefix.content.trim().is_empty() {
        return (cursor_line, cursor_line);
    }

    // Scan upward.
    let mut first = cursor_line;
    while first > 0 {
        let prev = first - 1;
        if is_blank_content(&lines[prev], markers) {
            break;
        }
        let prev_prefix = detect_prefix(&lines[prev], markers);
        if !prefixes_compatible(&cursor_prefix, &prev_prefix) {
            break;
        }
        first = prev;
    }

    // Scan downward.
    let mut last = cursor_line;
    while last + 1 < lines.len() {
        let next = last + 1;
        if is_blank_content(&lines[next], markers) {
            break;
        }
        let next_prefix = detect_prefix(&lines[next], markers);
        if !prefixes_compatible(&cursor_prefix, &next_prefix) {
            break;
        }
        last = next;
    }

    (first, last)
}

/// Returns the visual column width of a character.
fn char_column_width(c: char, tab_width: usize, current_column: usize) -> usize {
    match c as u32 {
        0x0009 => {
            // Tab: advance to next tab stop.
            if tab_width == 0 {
                1
            } else {
                tab_width - (current_column % tab_width)
            }
        }
        0x0000..0x0020 => 0, // control chars
        // CJK Unified Ideographs and related blocks.
        0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF01..=0xFF60
        | 0xFFE0..=0xFFE6 | 0x1F000..=0x1FFFF | 0x20000..=0x2FA1F => 2,
        _ => 1,
    }
}

/// Check if a character is CJK (Chinese/Japanese/Korean) for line-breaking purposes.
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x9FFF |
        0xF900..=0xFAFF |
        0xFE30..=0xFE4F |
        0xFF01..=0xFF60 |
        0xFFE0..=0xFFE6 |
        0x20000..=0x2FA1F
    )
}

/// Characters that should not start a line (CJK closing punctuation, etc.)
fn is_cjk_no_start(c: char) -> bool {
    matches!(
        c,
        ')' | ']'
            | '}'
            | '>'
            | '）'
            | '】'
            | '》'
            | '」'
            | '』'
            | '〉'
            | '〕'
            | '〗'
            | '〙'
            | '〛'
            | '。'
            | '、'
            | '，'
            | '．'
            | '：'
            | '；'
            | '！'
            | '？'
            | 'ー'
    )
}

/// Characters that should not end a line (CJK opening punctuation).
fn is_cjk_no_end(c: char) -> bool {
    matches!(
        c,
        '(' | '[' | '{' | '<' | '（' | '【' | '《' | '「' | '『' | '〈' | '〔' | '〖' | '〘' | '〚'
    )
}

/// Check if we can break between two characters.
fn can_break_between(before: char, after: char) -> bool {
    if before.is_whitespace() || after.is_whitespace() {
        return true;
    }
    // Allow breaks around CJK characters (with restrictions).
    if is_cjk(before) || is_cjk(after) {
        if is_cjk_no_start(after) {
            return false;
        }
        if is_cjk_no_end(before) {
            return false;
        }
        return true;
    }
    false
}

/// Concatenate stripped content lines into a single string for re-wrapping.
/// Joins with a space between lines (unless CJK adjacency).
fn concatenate_lines(contents: &[&str]) -> String {
    let mut result = String::new();
    for (i, content) in contents.iter().enumerate() {
        let trimmed = content.trim_end();
        if i > 0 && !trimmed.is_empty() {
            // Check if we need a space between the previous line's end and this line's start.
            let last_char = result.chars().last();
            let first_char = trimmed.chars().next();
            if let (Some(lc), Some(fc)) = (last_char, first_char) {
                if !(is_cjk(lc) && is_cjk(fc)) {
                    result.push(' ');
                }
            }
        }
        result.push_str(trimmed);
    }
    result
}

/// Break a string into lines that fit within `max_width` columns.
/// Returns the broken lines (without prefixes).
fn break_string(text: &str, max_width: usize, tab_width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let chars: Vec<char> = text.chars().collect();
    let mut lines = Vec::new();
    let mut line_start = 0;
    let mut column = 0;
    let mut last_break_pos = None; // Index AFTER the break character.

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        // Explicit newline forces a line break.
        if c == '\n' {
            let line: String = chars[line_start..i].iter().collect();
            lines.push(line.trim_end().to_string());
            line_start = i + 1;
            column = 0;
            last_break_pos = None;
            i += 1;
            continue;
        }

        let w = char_column_width(c, tab_width, column);
        let new_column = column + w;

        // Track potential break positions.
        if i > line_start {
            let prev = chars[i - 1];
            if can_break_between(prev, c) {
                last_break_pos = Some(i);
            }
        }

        if new_column > max_width && !c.is_whitespace() {
            // We've exceeded the limit. Try to break at the last known break position.
            if let Some(bp) = last_break_pos {
                let line: String = chars[line_start..bp].iter().collect();
                lines.push(line.trim_end().to_string());
                // Skip whitespace at the break point.
                line_start = bp;
                while line_start < chars.len() && chars[line_start].is_whitespace() {
                    if chars[line_start] == '\n' {
                        break;
                    }
                    line_start += 1;
                }
                // Recalculate column from new line_start.
                column = 0;
                for j in line_start..=i {
                    if j < chars.len() {
                        column += char_column_width(chars[j], tab_width, column);
                    }
                }
                last_break_pos = None;
                i += 1;
                continue;
            }
            // No break position found. Keep going until we find one (long word).
        }

        column = new_column;
        i += 1;
    }

    // Emit the last line.
    if line_start < chars.len() {
        let line: String = chars[line_start..].iter().collect();
        lines.push(line.trim_end().to_string());
    } else if line_start == chars.len() && !lines.is_empty() {
        // Text ended exactly at a break.
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Rewrap a set of lines (with their original prefixes) to fit within `max_width`.
fn rewrap_lines(
    lines: &[String],
    markers: &CommentMarkers,
    max_width: usize,
    tab_width: usize,
) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }

    // Detect prefix from the first non-blank line.
    let first_prefix = detect_prefix(&lines[0], markers);
    let prefix = &first_prefix.prefix;

    // Strip prefixes and collect content.
    let contents: Vec<&str> = lines
        .iter()
        .map(|l| {
            let p = detect_prefix(l, markers);
            // Return the content portion. We need to handle the lifetime carefully.
            // Since `detect_prefix` returns owned strings, we need a different approach.
            l.get(p.prefix.len()..).unwrap_or("")
        })
        .collect();

    // Concatenate all content into one string.
    let joined = concatenate_lines(&contents);

    // Calculate available width for content (after the prefix).
    let prefix_width: usize = prefix.chars().fold(0, |col, c| col + char_column_width(c, tab_width, col));
    let content_width = if max_width > prefix_width { max_width - prefix_width } else { 1 };

    // Break into lines.
    let broken = break_string(&joined, content_width, tab_width);

    // Re-add prefix.
    broken.iter().map(|line| format!("{}{}", prefix, line.trim_start())).collect()
}

/// Main entry point: execute the rewrap command.
pub fn execute(ctx: &mut Context, state: &mut State) {
    let Some(doc) = state.documents.active() else {
        return;
    };
    let mut tb = doc.buffer.borrow_mut();
    let tab_width = tb.tab_size() as usize;
    let max_width = DEFAULT_WRAP_COLUMN;
    let line_count = tb.logical_line_count();

    // Determine file extension for comment marker detection.
    let ext = doc
        .filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let markers = comment_markers_for_extension(&ext);

    // Determine the range of lines to rewrap.
    let (start_line, end_line_exclusive) = if tb.has_selection() {
        let (beg, end) = tb.selection_range().unwrap();
        let mut end_y = end.logical_pos.y;
        // If selection ends at column 0, don't include that line.
        if end.logical_pos.x == 0 && end_y > beg.logical_pos.y {
            end_y -= 1;
        }
        (beg.logical_pos.y, end_y + 1)
    } else {
        // Find paragraph around cursor.
        // We need to read lines around the cursor to detect boundaries.
        let cursor_y = tb.cursor_logical_pos().y;

        // Read a window of lines around the cursor for paragraph detection.
        let window_start = cursor_y.saturating_sub(500).max(0);
        let window_end = (cursor_y + 501).min(line_count);

        let saved_cursor = tb.cursor_logical_pos();

        // Read all lines in the window by selecting the range.
        tb.cursor_move_to_logical(Point { x: 0, y: window_start });
        tb.start_selection();
        tb.selection_update_logical(Point { x: 0, y: window_end });
        let text_bytes = tb.extract_user_selection(false).unwrap_or_default();
        tb.clear_selection();

        let text = String::from_utf8_lossy(&text_bytes);
        let window_lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();

        let cursor_idx = (cursor_y - window_start) as usize;

        let (first, last) = find_paragraph(&window_lines, cursor_idx, &markers);

        // Restore cursor.
        tb.cursor_move_to_logical(saved_cursor);

        (window_start + first as CoordType, window_start + last as CoordType + 1)
    };

    if start_line >= end_line_exclusive || start_line >= line_count {
        return;
    }

    // Read the lines to rewrap.
    let saved_cursor = tb.cursor_logical_pos();
    tb.cursor_move_to_logical(Point { x: 0, y: start_line });
    tb.start_selection();
    let select_end = if end_line_exclusive >= line_count {
        // Last line: select to end of last line.
        // Move to a very large x on the last line.
        Point { x: CoordType::MAX, y: end_line_exclusive - 1 }
    } else {
        Point { x: 0, y: end_line_exclusive }
    };
    tb.selection_update_logical(select_end);
    let text_bytes = tb.extract_user_selection(false).unwrap_or_default();

    let text = String::from_utf8_lossy(&text_bytes);
    let lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();

    // Remove trailing empty line from the split (artifact of trailing \n).
    let lines: Vec<String> = if lines.last().is_some_and(|l| l.is_empty()) && lines.len() > 1 {
        lines[..lines.len() - 1].to_vec()
    } else {
        lines
    };

    // Rewrap the lines.
    let rewrapped = rewrap_lines(&lines, &markers, max_width, tab_width);

    // Build replacement text.
    let mut replacement = rewrapped.join("\n");
    // If the original text ended with a newline, preserve it.
    let original_had_trailing_newline = text_bytes.last() == Some(&b'\n');
    if original_had_trailing_newline {
        replacement.push('\n');
    }

    // Check if anything actually changed.
    let original_text = String::from_utf8_lossy(&text_bytes);
    if replacement == original_text.as_ref() {
        tb.clear_selection();
        tb.cursor_move_to_logical(saved_cursor);
        return;
    }

    // The selection is still active from extract_user_selection(false).
    // write_raw replaces the selection.
    tb.write_raw(replacement.as_bytes());

    // Restore cursor to a reasonable position.
    let new_cursor_y = saved_cursor.y.min(start_line + rewrapped.len() as CoordType - 1);
    tb.cursor_move_to_logical(Point { x: 0, y: new_cursor_y });

    ctx.needs_rerender();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_break_string_basic() {
        let result = break_string("hello world foo bar baz", 10, 4);
        assert_eq!(result, vec!["hello", "world foo", "bar baz"]);
    }

    #[test]
    fn test_break_string_long_word() {
        let result = break_string("superlongword short", 10, 4);
        assert_eq!(result, vec!["superlongword", "short"]);
    }

    #[test]
    fn test_break_string_exact_fit() {
        let result = break_string("1234567890", 10, 4);
        assert_eq!(result, vec!["1234567890"]);
    }

    #[test]
    fn test_break_string_empty() {
        let result = break_string("", 80, 4);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn test_concatenate_lines_basic() {
        let lines = vec!["hello", "world"];
        assert_eq!(concatenate_lines(&lines), "hello world");
    }

    #[test]
    fn test_concatenate_lines_trim() {
        let lines = vec!["hello  ", "  world"];
        assert_eq!(concatenate_lines(&lines), "hello   world");
    }

    #[test]
    fn test_detect_prefix_rust_comment() {
        let markers = comment_markers_for_extension("rs");
        let p = detect_prefix("    // some text", &markers);
        assert_eq!(p.prefix, "    // ");
        assert_eq!(p.content, "some text");
    }

    #[test]
    fn test_detect_prefix_rust_doc_comment() {
        let markers = comment_markers_for_extension("rs");
        let p = detect_prefix("    /// some text", &markers);
        assert_eq!(p.prefix, "    /// ");
        assert_eq!(p.content, "some text");
    }

    #[test]
    fn test_detect_prefix_python() {
        let markers = comment_markers_for_extension("py");
        let p = detect_prefix("    # some text", &markers);
        assert_eq!(p.prefix, "    # ");
        assert_eq!(p.content, "some text");
    }

    #[test]
    fn test_detect_prefix_plain_text() {
        let markers = comment_markers_for_extension("txt");
        let p = detect_prefix("    some text", &markers);
        assert_eq!(p.prefix, "    ");
        assert_eq!(p.content, "some text");
    }

    #[test]
    fn test_find_paragraph() {
        let markers = comment_markers_for_extension("rs");
        let lines: Vec<String> = vec![
            "// first paragraph line one",
            "// first paragraph line two",
            "//",
            "// second paragraph line one",
            "// second paragraph line two",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        assert_eq!(find_paragraph(&lines, 0, &markers), (0, 1));
        assert_eq!(find_paragraph(&lines, 1, &markers), (0, 1));
        assert_eq!(find_paragraph(&lines, 3, &markers), (3, 4));
        assert_eq!(find_paragraph(&lines, 4, &markers), (3, 4));
    }

    #[test]
    fn test_rewrap_lines_basic() {
        let markers = comment_markers_for_extension("rs");
        let lines: Vec<String> = vec![
            "// This is a long line that should be wrapped because it exceeds the column limit for this test",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let result = rewrap_lines(&lines, &markers, 40, 4);
        for line in &result {
            assert!(line.starts_with("// "));
            assert!(line.len() <= 40, "Line too long: {:?} ({})", line, line.len());
        }
        assert!(result.len() > 1);
    }

    #[test]
    fn test_rewrap_lines_join_short() {
        let markers = comment_markers_for_extension("rs");
        let lines: Vec<String> = vec!["// hello", "// world"]
            .into_iter()
            .map(String::from)
            .collect();

        let result = rewrap_lines(&lines, &markers, 80, 4);
        assert_eq!(result, vec!["// hello world"]);
    }

    #[test]
    fn test_char_column_width_cjk() {
        assert_eq!(char_column_width('a', 4, 0), 1);
        assert_eq!(char_column_width('中', 4, 0), 2);
        assert_eq!(char_column_width('\t', 4, 0), 4);
        assert_eq!(char_column_width('\t', 4, 2), 2);
    }
}
