//! Bounded tail reading of log files.
//!
//! The dae data-plane log grows without bound on a busy host: a single day of
//! per-connection `INFO` lines reaches tens of megabytes. Callers that only
//! need the newest lines must therefore not read the whole file — they read a
//! trailing byte window instead, and these helpers turn that window into
//! lines. That keeps per-request memory and I/O proportional to the requested
//! line count rather than to the total log size.

/// Lower bound on the byte window, so a quiet log still yields whole lines.
const MIN_WINDOW_BYTES: u64 = 256 * 1024;

/// Upper bound on the byte window, so one request can never buffer an
/// unbounded amount of a runaway log.
const MAX_WINDOW_BYTES: u64 = 16 * 1024 * 1024;

/// Assumed average line length when sizing a window for `max_lines`. dae
/// connection lines run ~230 bytes, so this leaves generous headroom.
const ASSUMED_LINE_BYTES: u64 = 1024;

/// Byte window to read from the end of a log file to recover `max_lines` lines.
pub fn tail_window_bytes(max_lines: usize) -> u64 {
    (max_lines as u64)
        .saturating_mul(ASSUMED_LINE_BYTES)
        .clamp(MIN_WINDOW_BYTES, MAX_WINDOW_BYTES)
}

/// Turn a trailing byte window into at most `max_lines` lines.
///
/// `window_truncated` must be true when the window does not begin at offset 0
/// of the file. The leading fragment is then a partial line — and may also cut
/// a multi-byte UTF-8 sequence, which decoding would turn into a replacement
/// character — so it is discarded. Discarding one possibly-whole line is
/// harmless for a tail view and keeps the result free of corrupt output.
pub fn tail_lines_from_bytes(
    bytes: &[u8],
    max_lines: usize,
    window_truncated: bool,
) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }

    let text = String::from_utf8_lossy(bytes);
    let mut lines: Vec<&str> = text.lines().collect();

    if window_truncated && !lines.is_empty() {
        lines.remove(0);
    }

    if lines.len() > max_lines {
        lines.drain(..lines.len() - max_lines);
    }

    lines.into_iter().map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_scales_with_requested_lines_and_is_bounded() {
        // Small requests get the floor, huge requests get the ceiling.
        assert_eq!(tail_window_bytes(1), MIN_WINDOW_BYTES);
        assert_eq!(tail_window_bytes(100), MIN_WINDOW_BYTES);
        assert_eq!(tail_window_bytes(500), 500 * ASSUMED_LINE_BYTES);
        assert_eq!(tail_window_bytes(5000), 5000 * ASSUMED_LINE_BYTES);
        assert_eq!(tail_window_bytes(usize::MAX), MAX_WINDOW_BYTES);
    }

    #[test]
    fn returns_trailing_lines_when_window_covers_whole_file() {
        let bytes = b"one\ntwo\nthree\n";
        assert_eq!(
            tail_lines_from_bytes(bytes, 2, false),
            vec!["two".to_string(), "three".to_string()]
        );
    }

    #[test]
    fn drops_partial_leading_line_when_window_is_truncated() {
        // Window started mid-line, so "o" is a fragment of a longer line.
        let bytes = b"o\ntwo\nthree\n";
        assert_eq!(
            tail_lines_from_bytes(bytes, 10, true),
            vec!["two".to_string(), "three".to_string()]
        );
    }

    #[test]
    fn truncation_flag_drops_utf8_splitting_fragment() {
        // A multi-byte character split by the window boundary must not leak a
        // replacement character into the output.
        let full = "节点\ndone\n".as_bytes();
        let chopped = &full[1..];
        let lines = tail_lines_from_bytes(chopped, 10, true);
        assert_eq!(lines, vec!["done".to_string()]);
        assert!(!lines.iter().any(|l| l.contains('\u{FFFD}')));
    }

    #[test]
    fn handles_missing_trailing_newline_and_empty_input() {
        assert_eq!(
            tail_lines_from_bytes(b"a\nb", 10, false),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(tail_lines_from_bytes(b"", 10, false).is_empty());
        assert!(tail_lines_from_bytes(b"a\nb", 0, false).is_empty());
    }
}
