//! What the local Claude Code session logs say about the last seven days.
//!
//! Claude Code appends one JSON object per line to
//! `~/.claude/projects/<project>/<session>.jsonl`. The lines that matter here
//! are the `"type":"assistant"` ones: they carry the model, the token usage and
//! the tool calls. An assistant message is appended *several times* while it
//! streams, each copy a little longer than the last, so the same
//! `(requestId, message.id)` pair shows up repeatedly and only the final copy
//! has the true totals — hence the dedupe map below.
//!
//! [`scan`] is pure over a directory so the tests can point it at a temp dir;
//! [`scan_default`] is the one the app calls. Both are linear and streaming: we
//! never hold a file in memory, only one small record per distinct message.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::Deserialize;

use crate::model::{DayStats, LocalStats};

/// How many days the popover shows, today included.
pub const DAYS: usize = 7;

/// Files untouched for longer than this cannot contribute to the seven-day
/// window, so we never open them. One day of slack covers time zones and
/// clocks that disagree slightly.
const MTIME_WINDOW: Duration = Duration::from_secs(8 * 24 * 60 * 60);

/// One assistant line, with everything we do not use ignored. Every field is
/// optional because a malformed or unusual line should be skipped, not fatal.
#[derive(Deserialize)]
struct AssistantLine {
    #[serde(rename = "type")]
    kind: Option<String>,
    /// RFC 3339 as a string: chrono's `serde` feature is not enabled, and
    /// parsing it by hand keeps the dependency list as the spec lists it.
    timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    #[serde(default)]
    usage: Usage,
    #[serde(default)]
    content: Vec<ContentBlock>,
}

/// The four token counters the spec sums. Missing counters are zero: older
/// lines and synthetic messages do not always carry the cache fields.
#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
}

impl Usage {
    fn total(&self) -> u64 {
        self.input_tokens
            + self.output_tokens
            + self.cache_read_input_tokens
            + self.cache_creation_input_tokens
    }
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: Option<String>,
}

/// The one row we keep per distinct assistant message. Keeping this instead of
/// the line itself is what bounds memory: a busy week is a few hundred MB of
/// JSON but only tens of thousands of these.
struct Record {
    date: NaiveDate,
    session_id: String,
    model: String,
    tokens: u64,
    tool_calls: usize,
}

/// Scan `root` (`~/.claude/projects`) and fold it into seven days ending at
/// `today`. `now_utc` is only used to decide which files are too old to open,
/// so tests can pin it.
pub fn scan(root: &Path, today: NaiveDate, now_utc: DateTime<Utc>) -> LocalStats {
    let oldest = today - chrono::Duration::days(DAYS as i64 - 1);
    let mut records: HashMap<(String, String), Record> = HashMap::new();

    for path in session_files(root, now_utc) {
        collect_file(&path, oldest, today, &mut records);
    }

    fold(records, today)
}

/// The app's entry point: the real log directory and the real clock. A missing
/// home directory or a missing `projects` directory yields seven empty days,
/// which is exactly what a machine that has never run Claude Code should show.
pub fn scan_default(today: NaiveDate) -> LocalStats {
    let root = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("projects");
    scan(&root, today, Utc::now())
}

/// Every `root/*/*.jsonl` whose mtime is recent enough to matter. Unreadable
/// entries are skipped: a directory we cannot list is not an error worth
/// surfacing in a menu bar popover.
fn session_files(root: &Path, now_utc: DateTime<Utc>) -> Vec<PathBuf> {
    let cutoff = SystemTime::UNIX_EPOCH + Duration::from_secs(now_utc.timestamp().max(0) as u64)
        - MTIME_WINDOW;
    let mut files = Vec::new();
    let Ok(projects) = std::fs::read_dir(root) else {
        return files;
    };
    for project in projects.flatten() {
        let Ok(sessions) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for session in sessions.flatten() {
            let path = session.path();
            if path.extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }
            let recent = session
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|mtime| mtime >= cutoff);
            if recent {
                files.push(path);
            }
        }
    }
    files
}

/// Stream one file into `records`. Lines that are not JSON, are not assistant
/// messages, or lack the fields we key on are skipped silently — the logs are
/// another program's private format and we would rather under-report than
/// crash on a shape we have not seen.
fn collect_file(
    path: &Path,
    oldest: NaiveDate,
    today: NaiveDate,
    records: &mut HashMap<(String, String), Record>,
) {
    let Ok(file) = File::open(path) else {
        return;
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        // Parsing every line of a few hundred MB of JSON would dominate the
        // scan, and only a fraction of the lines are assistant messages, so
        // reject the rest with a substring test first. The test is on the bare
        // word rather than on `"type":"assistant"` because the logs are another
        // program's format: if it ever writes its JSON with spaces around the
        // colons we would silently report nothing at all, and the few extra
        // lines that slip through are rejected by the parse below anyway.
        if !line.contains("assistant") {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<AssistantLine>(&line) else {
            continue;
        };
        if parsed.kind.as_deref() != Some("assistant") {
            continue;
        }
        let (Some(timestamp), Some(message)) = (parsed.timestamp, parsed.message) else {
            continue;
        };
        let Ok(timestamp) = DateTime::parse_from_rfc3339(&timestamp) else {
            continue;
        };
        let Some(message_id) = message.id else {
            continue;
        };
        // Days are local dates: a message at 11pm UTC belongs to the previous
        // day for anyone west of Greenwich.
        let date = timestamp.with_timezone(&Local).date_naive();
        if date < oldest || date > today {
            continue;
        }
        let tool_calls = message
            .content
            .iter()
            .filter(|block| block.kind.as_deref() == Some("tool_use"))
            .count();
        let record = Record {
            date,
            session_id: parsed.session_id.unwrap_or_default(),
            model: message.model.unwrap_or_default(),
            tokens: message.usage.total(),
            tool_calls,
        };
        // The last copy of a streamed message is the complete one, so a later
        // occurrence always replaces an earlier one.
        let request_id = parsed.request_id.unwrap_or_default();
        records.insert((request_id, message_id), record);
    }
}

/// Fold the deduped records into exactly seven zero-filled days, oldest first.
fn fold(records: HashMap<(String, String), Record>, today: NaiveDate) -> LocalStats {
    let mut sessions: HashMap<NaiveDate, HashSet<String>> = HashMap::new();
    let mut tool_calls: HashMap<NaiveDate, usize> = HashMap::new();
    let mut by_model: HashMap<NaiveDate, HashMap<String, u64>> = HashMap::new();

    for record in records.into_values() {
        if !record.session_id.is_empty() {
            sessions
                .entry(record.date)
                .or_default()
                .insert(record.session_id);
        }
        *tool_calls.entry(record.date).or_default() += record.tool_calls;
        *by_model
            .entry(record.date)
            .or_default()
            .entry(record.model)
            .or_default() += record.tokens;
    }

    let days = (0..DAYS)
        .map(|i| {
            let date = today - chrono::Duration::days(DAYS as i64 - 1 - i as i64);
            let mut tokens_by_model: Vec<(String, u64)> = by_model
                .remove(&date)
                .unwrap_or_default()
                .into_iter()
                .collect();
            // Largest first, then by name so equal totals order predictably.
            tokens_by_model.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            DayStats {
                date,
                sessions: sessions.get(&date).map_or(0, HashSet::len),
                tool_calls: tool_calls.get(&date).copied().unwrap_or(0),
                tokens: tokens_by_model.iter().map(|(_, n)| n).sum(),
                tokens_by_model,
            }
        })
        .collect();

    LocalStats { days }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A private directory per test, without pulling in the `tempfile` crate.
    fn temp_root(name: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = format!(
            "claudebar-local-{}-{}-{}",
            name,
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let root = std::env::temp_dir().join(unique).join("projects");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir");
        root
    }

    fn write_session(root: &Path, project: &str, file: &str, lines: &[String]) {
        let dir = root.join(project);
        std::fs::create_dir_all(&dir).expect("project dir");
        std::fs::write(dir.join(file), lines.join("\n")).expect("session file");
    }

    /// An assistant line shaped like the real ones, with the timestamp given as
    /// a local time so the test does not depend on the machine's zone.
    fn assistant(
        session: &str,
        request: &str,
        message: &str,
        date: NaiveDate,
        model: &str,
        output_tokens: u64,
        tool_use: bool,
    ) -> String {
        let timestamp = date
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap()
            .with_timezone(&Utc)
            .to_rfc3339();
        let content = if tool_use {
            r#"{"type":"text","text":"hi"},{"type":"tool_use","name":"Bash"}"#
        } else {
            r#"{"type":"text","text":"hi"}"#
        };
        format!(
            r#"{{"type":"assistant","sessionId":"{session}","requestId":"{request}","timestamp":"{timestamp}","message":{{"id":"{message}","model":"{model}","content":[{content}],"usage":{{"input_tokens":10,"output_tokens":{output_tokens},"cache_read_input_tokens":100,"cache_creation_input_tokens":1000}}}}}}"#
        )
    }

    #[test]
    fn folds_two_sessions_over_two_days_and_keeps_the_last_stream_copy() {
        let today = Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let root = temp_root("fold");

        // One session yesterday, and one today whose single message was
        // appended three times as it streamed.
        write_session(
            &root,
            "-Users-me-alpha",
            "aaa.jsonl",
            &[assistant(
                "s-alpha",
                "req-1",
                "msg-1",
                yesterday,
                "claude-opus-5",
                5,
                false,
            )],
        );
        write_session(
            &root,
            "-Users-me-beta",
            "bbb.jsonl",
            &[
                assistant(
                    "s-beta",
                    "req-2",
                    "msg-2",
                    today,
                    "claude-fable-5-1",
                    1,
                    true,
                ),
                assistant(
                    "s-beta",
                    "req-2",
                    "msg-2",
                    today,
                    "claude-fable-5-1",
                    7,
                    true,
                ),
                assistant(
                    "s-beta",
                    "req-2",
                    "msg-2",
                    today,
                    "claude-fable-5-1",
                    50,
                    true,
                ),
                assistant("s-beta", "req-3", "msg-3", today, "claude-opus-5", 3, false),
            ],
        );

        let stats = scan(&root, today, Utc::now());
        assert_eq!(stats.days.len(), DAYS);
        assert_eq!(
            stats.days.first().unwrap().date,
            today - chrono::Duration::days(6)
        );
        assert_eq!(stats.today().unwrap().date, today);

        // The five days before yesterday are present and empty.
        for day in &stats.days[..5] {
            assert_eq!(
                day,
                &DayStats {
                    date: day.date,
                    ..Default::default()
                }
            );
        }

        let yday = &stats.days[5];
        assert_eq!(yday.sessions, 1);
        assert_eq!(yday.tool_calls, 0);
        assert_eq!(yday.tokens, 10 + 5 + 100 + 1000);

        let day = stats.today().unwrap();
        assert_eq!(day.sessions, 1, "both messages belong to one session");
        assert_eq!(day.tool_calls, 1, "only the surviving copy is counted");
        assert_eq!(day.tokens, (10 + 50 + 100 + 1000) + (10 + 3 + 100 + 1000));
        assert_eq!(
            day.tokens_by_model,
            vec![
                ("claude-fable-5-1".to_string(), 1160),
                ("claude-opus-5".to_string(), 1113),
            ],
            "models are largest first"
        );

        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn junk_and_non_assistant_lines_are_ignored() {
        let today = Local::now().date_naive();
        let root = temp_root("junk");
        write_session(
            &root,
            "-Users-me-alpha",
            "aaa.jsonl",
            &[
                "not json at all".to_string(),
                r#"{"type":"user","message":{"role":"user","content":"hello"}}"#.to_string(),
                r#"{"type":"assistant","message":{"model":"claude-opus-5"}}"#.to_string(),
                assistant("s", "req-1", "msg-1", today, "claude-opus-5", 5, false),
            ],
        );

        let stats = scan(&root, today, Utc::now());
        let day = stats.today().unwrap();
        assert_eq!(day.sessions, 1);
        assert_eq!(day.tokens, 10 + 5 + 100 + 1000);
        assert_eq!(stats.days.iter().map(|d| d.tokens).sum::<u64>(), day.tokens);

        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    /// The prefilter must not depend on how the writer spaces its JSON: a line
    /// with spaces around the colons is still an assistant message.
    #[test]
    fn a_spaced_out_line_is_still_counted() {
        let today = Local::now().date_naive();
        let root = temp_root("spacing");
        let timestamp = today
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap()
            .with_timezone(&Utc)
            .to_rfc3339();
        let line = format!(
            r#"{{"type": "assistant", "sessionId": "s", "requestId": "r", "timestamp": "{timestamp}", "message": {{"id": "m", "model": "claude-opus-5", "content": [], "usage": {{"output_tokens": 7}}}}}}"#
        );
        write_session(&root, "-Users-me-alpha", "aaa.jsonl", &[line]);

        let stats = scan(&root, today, Utc::now());
        assert_eq!(stats.today().unwrap().tokens, 7);

        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn a_missing_root_is_seven_empty_days() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 12).unwrap();
        let stats = scan(
            Path::new("/nonexistent/claudebar/projects"),
            today,
            Utc::now(),
        );
        assert_eq!(stats.days.len(), DAYS);
        assert!(stats.days.iter().all(|d| d.tokens == 0 && d.sessions == 0));
        assert_eq!(stats.today().unwrap().date, today);
    }
}
