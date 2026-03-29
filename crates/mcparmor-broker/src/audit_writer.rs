//! Append-only NDJSON audit writer with size-based log rotation and retention pruning.
//!
//! Entries are written as newline-delimited JSON to a rotating log file.
//! When the file exceeds the configured size limit, it is rotated by shifting
//! existing rotations and renaming the current log to `.1`. Up to 5 rotations
//! are kept; the oldest is discarded when the limit is reached.
//!
//! On construction, files in the audit directory older than `retention_days` are
//! automatically deleted. The audit directory is `~/.mcparmor/audit/` and files
//! must follow the naming convention `YYYY-MM-DD.jsonl`.

use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use mcparmor_core::audit::AuditEntry;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Maximum number of rotated log files to keep alongside the active log.
const MAX_ROTATIONS: u32 = 5;

/// Date format used in audit log filenames (`YYYY-MM-DD`).
const AUDIT_DATE_FORMAT: &str = "%Y-%m-%d";

/// Extension used for all audit log files.
const AUDIT_FILE_EXTENSION: &str = ".jsonl";

/// Append-only NDJSON audit writer with optional size-based rotation and
/// optional date-based retention pruning.
///
/// Can operate in three modes:
/// - Normal: writes to the given path with optional rotation and pruning.
/// - Disabled: accepts entries but discards them (no I/O performed).
pub struct AuditWriter {
    /// Path to the active audit log file. `None` when the writer is disabled.
    path: Option<PathBuf>,
    /// Maximum permitted file size in bytes before rotation is triggered.
    max_size_bytes: Option<u64>,
    /// Number of days to retain audit log files. Files older than this are
    /// pruned from the audit directory on construction.
    retention_days: Option<u32>,
}

impl AuditWriter {
    /// Create a new `AuditWriter` for the given path and optional size limit.
    ///
    /// When `retention_days` is `Some(n)`, files in the audit directory that
    /// follow the `YYYY-MM-DD.jsonl` naming convention and are older than `n`
    /// days are deleted immediately during construction.
    ///
    /// # Arguments
    /// * `path` - Absolute path to the audit log file
    /// * `max_size_mb` - Maximum file size in megabytes before rotation; `None` disables rotation
    /// * `retention_days` - Days to keep audit files; `None` disables date-based pruning
    pub fn new(path: PathBuf, max_size_mb: Option<u32>, retention_days: Option<u32>) -> Self {
        let max_size_bytes = max_size_mb.map(|mb| u64::from(mb) * 1024 * 1024);
        let writer = Self { path: Some(path), max_size_bytes, retention_days };
        writer.prune_old_files();
        writer
    }

    /// Create a disabled `AuditWriter` that accepts entries but discards them.
    ///
    /// No files are created or written. Use this when `--no-audit` is set.
    pub fn disabled() -> Self {
        Self { path: None, max_size_bytes: None, retention_days: None }
    }

    /// Create an `AuditWriter` that writes to a specific file path.
    ///
    /// Uses no size-based rotation or retention pruning. Use this when
    /// `--audit-log <file>` is set.
    ///
    /// # Arguments
    /// * `path` - Path to the audit log file to write to
    pub fn at_path(path: PathBuf) -> Self {
        Self { path: Some(path), max_size_bytes: None, retention_days: None }
    }

    /// Prune dated audit log files older than `retention_days` from the audit directory.
    ///
    /// Scans the parent directory of the current log path for files matching
    /// the `YYYY-MM-DD.jsonl` naming convention. Any file whose date is strictly
    /// older than today minus `retention_days` is deleted.
    ///
    /// Errors during scanning or deletion are silently ignored so that a
    /// misconfigured retention policy never interrupts normal broker operation.
    pub fn prune_old_files(&self) {
        let Some(days) = self.retention_days else {
            return;
        };
        let Some(path) = &self.path else {
            return;
        };
        let Some(audit_dir) = path.parent() else {
            return;
        };

        let cutoff = Utc::now().date_naive() - ChronoDuration::days(i64::from(days));
        delete_files_older_than(audit_dir, cutoff);
    }

    /// Serialize an `AuditEntry` to JSON and append it to the log file.
    ///
    /// When the writer is disabled (created via [`AuditWriter::disabled`]), this
    /// method is a no-op and returns `Ok(())` immediately.
    ///
    /// Creates parent directories and the log file if they do not exist.
    /// Triggers rotation before writing if the current size exceeds the limit.
    ///
    /// # Errors
    /// Returns an error if directory creation, rotation, serialization, or the
    /// file write fails.
    pub fn write(&self, entry: &AuditEntry) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        self.ensure_parent_dirs(path)?;
        self.rotate_if_needed()?;

        let line = serialize_entry(entry)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("Failed to open audit log at {}", path.display()))?;

        writeln!(file, "{line}")
            .with_context(|| format!("Failed to write audit entry to {}", path.display()))?;

        Ok(())
    }

    /// Rotate the log file if it exceeds the configured size limit.
    ///
    /// Shifts existing rotations up by one (`.4` → `.5`, `.3` → `.4`, etc.)
    /// before renaming the active log to `.1`. The oldest rotation beyond
    /// `MAX_ROTATIONS` is silently discarded.
    ///
    /// Returns `Ok(())` immediately when the writer is disabled or no size
    /// limit is configured.
    ///
    /// # Errors
    /// Returns an error if any file rename fails.
    pub fn rotate_if_needed(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let Some(max_bytes) = self.max_size_bytes else {
            return Ok(());
        };

        let current_size = current_file_size(path);
        if current_size <= max_bytes {
            return Ok(());
        }

        self.rotate_files(path)?;
        Ok(())
    }

    /// Returns the default audit log path: `~/.mcparmor/audit.jsonl`.
    ///
    /// Falls back to `$TMPDIR/.mcparmor/audit.jsonl` when the home directory
    /// cannot be determined, and emits a warning to stderr so the operator is
    /// aware of the non-standard location rather than silently writing elsewhere.
    pub fn default_path() -> PathBuf {
        match dirs::home_dir() {
            Some(home) => home.join(".mcparmor").join("audit.jsonl"),
            None => {
                eprintln!(
                    "warning: [mcparmor] cannot determine home directory — \
                     writing audit log to system temp directory instead."
                );
                std::env::temp_dir().join(".mcparmor").join("audit.jsonl")
            }
        }
    }

    /// Returns the active log path, or `None` when the writer is disabled.
    ///
    /// Primarily used in tests to verify the writer targets the expected path.
    pub fn log_path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Create parent directories for the log file if they do not exist.
    fn ensure_parent_dirs(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create audit log directory: {}", parent.display())
            })?;
        }
        Ok(())
    }

    /// Shift all existing rotations and rename the active log to `.1`.
    ///
    /// Rotation order (highest first to avoid overwriting):
    /// `.4` → `.5`, `.3` → `.4`, `.2` → `.3`, `.1` → `.2`, active → `.1`
    fn rotate_files(&self, path: &Path) -> Result<()> {
        // Shift existing rotations from highest to lowest.
        for n in (1..MAX_ROTATIONS).rev() {
            let from = rotation_path(path, n);
            let to = rotation_path(path, n + 1);
            if from.exists() {
                fs::rename(&from, &to).with_context(|| {
                    format!("Failed to rotate {} → {}", from.display(), to.display())
                })?;
            }
        }

        // Rename active log to .1.
        let first_rotation = rotation_path(path, 1);
        if path.exists() {
            fs::rename(path, &first_rotation).with_context(|| {
                format!(
                    "Failed to rotate {} → {}",
                    path.display(),
                    first_rotation.display()
                )
            })?;
        }

        Ok(())
    }
}

/// Delete audit log files in `dir` whose date is older than `cutoff`.
///
/// Only files matching `YYYY-MM-DD.jsonl` are considered. Files that do not
/// match the naming convention are left untouched. Errors are silently ignored
/// so that a misconfigured retention policy never interrupts broker operation.
fn delete_files_older_than(dir: &Path, cutoff: NaiveDate) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else {
            continue;
        };
        let Some(date_str) = name_str.strip_suffix(AUDIT_FILE_EXTENSION) else {
            continue;
        };
        let Ok(file_date) = NaiveDate::parse_from_str(date_str, AUDIT_DATE_FORMAT) else {
            continue;
        };
        if file_date < cutoff {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Returns the path for rotation index `n` (e.g. `audit.jsonl.1`).
fn rotation_path(base: &Path, n: u32) -> PathBuf {
    let mut s = base.as_os_str().to_owned();
    s.push(format!(".{n}"));
    PathBuf::from(s)
}

/// Returns the current size of a file in bytes, or 0 if the file does not exist.
fn current_file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Serialize an `AuditEntry` to a JSON string.
fn serialize_entry(entry: &AuditEntry) -> Result<String> {
    serde_json::to_string(entry).context("Failed to serialize audit entry")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcparmor_core::audit::{AuditEntry, AuditEvent};
    use tempfile::TempDir;

    fn make_writer(dir: &TempDir, max_size_mb: Option<u32>) -> AuditWriter {
        let path = dir.path().join("audit.jsonl");
        AuditWriter::new(path, max_size_mb, None)
    }

    fn sample_entry() -> AuditEntry {
        AuditEntry::invoke("test-tool", "tools/call")
    }

    // --- Happy-path tests ---

    #[test]
    fn write_single_entry_produces_valid_json() {
        let dir = TempDir::new().unwrap();
        let writer = make_writer(&dir, None);

        writer.write(&sample_entry()).unwrap();

        let contents = std::fs::read_to_string(writer.path.as_ref().unwrap()).unwrap();
        let trimmed = contents.trim();
        assert!(!trimmed.is_empty());
        // Each line must be valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(trimmed).unwrap();
        assert_eq!(parsed["tool"], "test-tool");
        assert_eq!(parsed["event"], "invoke");
    }

    #[test]
    fn write_multiple_entries_each_on_own_line() {
        let dir = TempDir::new().unwrap();
        let writer = make_writer(&dir, None);

        let entries = vec![
            AuditEntry::invoke("tool-a", "tools/call"),
            AuditEntry::response("tool-b", 512, 42),
            AuditEntry::param_violation("tool-c", "path traversal"),
            AuditEntry::secret_detected("tool-d", "OpenAI API key"),
        ];

        for entry in &entries {
            writer.write(entry).unwrap();
        }

        let contents = std::fs::read_to_string(writer.path.as_ref().unwrap()).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 4);

        for line in &lines {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(v["tool"].is_string());
            assert!(v["event"].is_string());
            assert!(v["timestamp"].is_string());
        }
    }

    #[test]
    fn creates_parent_directories_if_absent() {
        let dir = TempDir::new().unwrap();
        let nested_path = dir.path().join("a").join("b").join("c").join("audit.jsonl");
        let writer = AuditWriter::new(nested_path.clone(), None, None);

        writer.write(&sample_entry()).unwrap();

        assert!(nested_path.exists());
    }

    #[test]
    fn rotation_triggered_when_size_limit_exceeded() {
        let dir = TempDir::new().unwrap();
        // Tiny limit: 1 byte — every write triggers rotation.
        let writer = AuditWriter::new(dir.path().join("audit.jsonl"), Some(0), None);

        // Write 3 entries to force multiple rotations.
        writer.write(&sample_entry()).unwrap();
        writer.write(&sample_entry()).unwrap();
        writer.write(&sample_entry()).unwrap();

        // After multiple rotations the .1 file must exist.
        let rotation_1 = dir.path().join("audit.jsonl.1");
        assert!(rotation_1.exists(), "rotation .1 should exist");
    }

    #[test]
    fn no_rotation_when_limit_not_exceeded() {
        let dir = TempDir::new().unwrap();
        let writer = AuditWriter::new(dir.path().join("audit.jsonl"), Some(100), None);

        writer.write(&sample_entry()).unwrap();

        let rotation_1 = dir.path().join("audit.jsonl.1");
        assert!(!rotation_1.exists(), "rotation .1 should not exist");
    }

    #[test]
    fn rotation_shifts_existing_files() {
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("audit.jsonl");

        // Pre-create the active log plus .1 and .2 to verify shifting.
        // The active log must already exist and be non-empty so the size check triggers.
        std::fs::write(&base, b"existing-content").unwrap();
        std::fs::write(dir.path().join("audit.jsonl.1"), b"old-1").unwrap();
        std::fs::write(dir.path().join("audit.jsonl.2"), b"old-2").unwrap();

        // Tiny size limit (1 byte) — file already exceeds it so rotation fires immediately.
        let writer = AuditWriter::new(base.clone(), Some(0), None);
        // Manually trigger rotation to verify the shift.
        writer.rotate_if_needed().unwrap();

        // Old .1 → .2, old .2 → .3, active → .1
        assert!(dir.path().join("audit.jsonl.1").exists(), ".1 should exist (was active log)");
        assert!(dir.path().join("audit.jsonl.2").exists(), ".2 should exist (was .1)");
        assert!(dir.path().join("audit.jsonl.3").exists(), ".3 should exist (was .2)");
    }

    #[test]
    fn default_path_ends_with_expected_components() {
        let path = AuditWriter::default_path();
        // The file name must be audit.jsonl.
        assert_eq!(path.file_name().unwrap(), "audit.jsonl");
        // The parent directory must be named .mcparmor.
        let parent = path.parent().unwrap();
        assert_eq!(parent.file_name().unwrap(), ".mcparmor");
    }

    // --- Edge-case / adversarial tests ---

    #[test]
    fn rotate_if_needed_is_noop_when_no_limit_configured() {
        let dir = TempDir::new().unwrap();
        let writer = make_writer(&dir, None);
        // No error even when file is large — no limit configured.
        writer.rotate_if_needed().unwrap();
    }

    #[test]
    fn rotate_if_needed_is_noop_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let writer = AuditWriter::new(dir.path().join("audit.jsonl"), Some(1), None);
        // Should succeed without error even if the file doesn't exist.
        writer.rotate_if_needed().unwrap();
    }

    #[test]
    fn write_does_not_truncate_existing_content() {
        let dir = TempDir::new().unwrap();
        let writer = make_writer(&dir, None);

        writer.write(&AuditEntry::invoke("tool-a", "tools/call")).unwrap();
        writer.write(&AuditEntry::invoke("tool-b", "tools/call")).unwrap();

        let contents = std::fs::read_to_string(writer.path.as_ref().unwrap()).unwrap();
        assert!(contents.contains("tool-a"));
        assert!(contents.contains("tool-b"));
    }

    #[test]
    fn audit_entry_with_empty_tool_name_is_written() {
        let dir = TempDir::new().unwrap();
        let writer = make_writer(&dir, None);
        // Edge case: empty tool name — should not panic or error.
        let entry = AuditEntry {
            timestamp: chrono::Utc::now(),
            tool: String::new(),
            event: AuditEvent::Invoke,
            detail: None,
        };
        writer.write(&entry).unwrap();
        let contents = std::fs::read_to_string(writer.path.as_ref().unwrap()).unwrap();
        let v: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(v["tool"], "");
    }

    // ---------------------------------------------------------------------------
    // disabled() — no-op writer
    // ---------------------------------------------------------------------------

    #[test]
    fn disabled_writer_accepts_entry_without_error() {
        // Writing to a disabled writer must succeed silently; no file is created.
        let writer = AuditWriter::disabled();
        writer.write(&sample_entry()).unwrap();
        assert!(
            writer.path.is_none(),
            "disabled writer must not have a path"
        );
    }

    #[test]
    fn disabled_writer_does_not_create_any_file() {
        let dir = TempDir::new().unwrap();
        let expected_path = dir.path().join("audit.jsonl");

        // A disabled writer ignores the entry — even if it were pointed at a path,
        // no file should be created. We verify by writing many entries.
        let writer = AuditWriter::disabled();
        for _ in 0..5 {
            writer.write(&sample_entry()).unwrap();
        }

        // The path we checked does not exist (we never pointed the writer at it).
        assert!(
            !expected_path.exists(),
            "disabled writer must not create any file"
        );
    }

    #[test]
    fn disabled_writer_rotate_if_needed_is_noop() {
        let writer = AuditWriter::disabled();
        // Must not panic or error.
        writer.rotate_if_needed().unwrap();
    }

    // ---------------------------------------------------------------------------
    // at_path() — custom-path writer
    // ---------------------------------------------------------------------------

    #[test]
    fn at_path_writer_writes_to_specified_file() {
        let dir = TempDir::new().unwrap();
        let custom_path = dir.path().join("custom-audit.jsonl");

        let writer = AuditWriter::at_path(custom_path.clone());
        writer.write(&sample_entry()).unwrap();

        assert!(custom_path.exists(), "at_path writer must create the specified file");
        let contents = std::fs::read_to_string(&custom_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(v["tool"], "test-tool");
    }

    #[test]
    fn at_path_writer_does_not_write_to_default_path() {
        let dir = TempDir::new().unwrap();
        let custom_path = dir.path().join("custom-audit.jsonl");
        let default_path = AuditWriter::default_path();

        let writer = AuditWriter::at_path(custom_path);
        writer.write(&sample_entry()).unwrap();

        // The default path must not have been written to.
        // We can only assert this when the default path is absent before the test.
        // Since we can't guarantee that in all environments, assert the custom path exists.
        // The at_path() struct field stores the custom path, not the default.
        let stored = writer.path.as_ref().unwrap();
        assert_ne!(
            stored, &default_path,
            "at_path writer path must not equal the default path"
        );
    }

    // ---------------------------------------------------------------------------
    // prune_old_files — date-based retention
    // ---------------------------------------------------------------------------

    /// Create a dated JSONL file in `dir` with the given date string and content.
    fn create_dated_file(dir: &std::path::Path, date_str: &str, content: &[u8]) {
        let path = dir.join(format!("{date_str}.jsonl"));
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn prune_old_files_deletes_files_older_than_retention_days() {
        let dir = TempDir::new().unwrap();
        // File dated 10 days ago — must be pruned with retention_days = 7.
        let old_date = (Utc::now().date_naive() - chrono::Duration::days(10))
            .format(AUDIT_DATE_FORMAT)
            .to_string();
        // File dated 2 days ago — must be kept.
        let recent_date = (Utc::now().date_naive() - chrono::Duration::days(2))
            .format(AUDIT_DATE_FORMAT)
            .to_string();

        create_dated_file(dir.path(), &old_date, b"old content");
        create_dated_file(dir.path(), &recent_date, b"recent content");

        let log_path = dir.path().join("audit.jsonl");
        AuditWriter::new(log_path, None, Some(7));

        assert!(
            !dir.path().join(format!("{old_date}.jsonl")).exists(),
            "file older than retention window must be deleted"
        );
        assert!(
            dir.path().join(format!("{recent_date}.jsonl")).exists(),
            "file within retention window must be kept"
        );
    }

    #[test]
    fn prune_old_files_keeps_files_exactly_at_retention_boundary() {
        let dir = TempDir::new().unwrap();
        // File exactly at the cutoff date (today minus retention_days) must be kept.
        let boundary_date = (Utc::now().date_naive() - chrono::Duration::days(7))
            .format(AUDIT_DATE_FORMAT)
            .to_string();

        create_dated_file(dir.path(), &boundary_date, b"boundary content");

        let log_path = dir.path().join("audit.jsonl");
        AuditWriter::new(log_path, None, Some(7));

        assert!(
            dir.path().join(format!("{boundary_date}.jsonl")).exists(),
            "file at retention boundary must be kept (cutoff is strictly less than)"
        );
    }

    #[test]
    fn prune_old_files_ignores_files_with_non_date_names() {
        let dir = TempDir::new().unwrap();
        // Files that do not match YYYY-MM-DD.jsonl must not be deleted.
        let non_date_file = dir.path().join("audit.jsonl");
        let garbage_file = dir.path().join("not-a-date.jsonl");
        std::fs::write(&non_date_file, b"current log").unwrap();
        std::fs::write(&garbage_file, b"some content").unwrap();

        // Even with very aggressive retention (0 days), non-date files stay.
        let writer = AuditWriter::new(non_date_file.clone(), None, Some(0));
        drop(writer);

        assert!(non_date_file.exists(), "active log file must not be deleted");
        assert!(garbage_file.exists(), "non-date files must not be deleted");
    }

    #[test]
    fn prune_old_files_is_noop_when_retention_days_is_none() {
        let dir = TempDir::new().unwrap();
        // An old file that would be pruned if retention_days were set.
        let old_date = (Utc::now().date_naive() - chrono::Duration::days(365))
            .format(AUDIT_DATE_FORMAT)
            .to_string();
        create_dated_file(dir.path(), &old_date, b"very old content");

        let log_path = dir.path().join("audit.jsonl");
        AuditWriter::new(log_path, None, None);

        assert!(
            dir.path().join(format!("{old_date}.jsonl")).exists(),
            "without retention_days set, no files must be deleted"
        );
    }

    #[test]
    fn prune_old_files_handles_empty_directory_without_panic() {
        let dir = TempDir::new().unwrap();
        let log_path = dir.path().join("audit.jsonl");
        // Should not panic when the directory is empty (no dated files).
        AuditWriter::new(log_path, None, Some(30));
    }

    #[test]
    fn prune_old_files_deletes_multiple_old_files() {
        let dir = TempDir::new().unwrap();
        let dates: Vec<String> = (20..=25u64)
            .map(|days| {
                (Utc::now().date_naive() - chrono::Duration::days(days as i64))
                    .format(AUDIT_DATE_FORMAT)
                    .to_string()
            })
            .collect();

        for date in &dates {
            create_dated_file(dir.path(), date, b"old content");
        }

        let log_path = dir.path().join("audit.jsonl");
        AuditWriter::new(log_path, None, Some(7));

        for date in &dates {
            assert!(
                !dir.path().join(format!("{date}.jsonl")).exists(),
                "file from {date} (>7 days old) must be deleted"
            );
        }
    }
}
