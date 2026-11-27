//! Standalone Windows/desktop startup diagnostics, OS-native application directory
//! resolution, local crash reporting, and visible native error dialogs.
//!
//! Ensures double-clicking `HYDRA-Leviathan-Windows-x64.exe` on a clean Windows
//! user profile (with no repository clone, config files, CLI arguments, or prior
//! database) always initializes safely into a dormant, non-trading state or
//! presents a readable error dialog + local crash report instead of silently exiting.

use chrono::Utc;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

/// Resolve the OS-native writable Hydra application root directory:
/// - Windows: `%LOCALAPPDATA%\Hydra`
/// - Unix: `$XDG_STATE_HOME/Hydra` or `$HOME/.local/state/Hydra`
pub fn app_data_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("Hydra"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .map(|p| p.join("Hydra"))
    }
}

/// Resolve the default read-only history database path when no explicit `--db` is supplied.
/// If a local `data/hydra.db` already exists beside the working directory (developer/portable
/// usage), preserve it; otherwise use the writable OS user data directory and initialize an
/// empty local history database if needed so clean first launches never error out.
pub fn resolve_default_database(explicit_db: Option<PathBuf>) -> PathBuf {
    if let Some(path) = explicit_db {
        return path;
    }
    let cwd_candidate = PathBuf::from("data").join("hydra.db");
    if cwd_candidate.is_file() {
        return cwd_candidate;
    }
    if let Some(root) = app_data_root() {
        let data_dir = root.join("data");
        let db_path = data_dir.join("hydra.db");
        let _ = fs::create_dir_all(&data_dir);
        if !db_path.exists() {
            if let Err(err) = crate::local_setup::initialize_history(&db_path) {
                log_startup(&format!(
                    "Deferred empty history initialization at {}: {err:#}",
                    db_path.display()
                ));
            } else {
                log_startup(&format!(
                    "Initialized clean dormant local history at {}",
                    db_path.display()
                ));
            }
        }
        return db_path;
    }
    cwd_candidate
}

/// Non-destructive read-only SQLite quick integrity check logged during startup
/// so damaged or inaccessible local history files are diagnosed cleanly.
pub fn verify_local_database_health(db_path: &Path) -> Result<String, String> {
    if !db_path.is_file() {
        return Ok("database file not yet created".into());
    }
    let flags =
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = rusqlite::Connection::open_with_flags(db_path, flags)
        .map_err(|e| format!("open read-only failed: {e}"))?;
    let _ = conn.busy_timeout(std::time::Duration::from_millis(500));
    let check: String = conn
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|e| format!("PRAGMA quick_check failed: {e}"))?;
    if check.eq_ignore_ascii_case("ok") {
        Ok("ok".into())
    } else {
        Err(format!("SQLite quick_check reported: {check}"))
    }
}

pub fn logs_dir() -> Option<PathBuf> {
    let dir = app_data_root()?.join("logs");
    let _ = fs::create_dir_all(&dir);
    Some(dir)
}

pub fn startup_log_path() -> Option<PathBuf> {
    Some(logs_dir()?.join("cockpit-startup.log"))
}

/// Append a timestamped diagnostic line to `%LOCALAPPDATA%\Hydra\logs\cockpit-startup.log`.
pub fn log_startup(message: &str) {
    let Some(path) = startup_log_path() else {
        return;
    };
    let line = format!("[{}] {}\n", Utc::now().to_rfc3339(), message);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Write a readable local crash report file and return its path.
pub fn write_crash_report(title: &str, detail: &str) -> Option<PathBuf> {
    let dir = logs_dir()?;
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("crash-{stamp}.txt"));
    let body = format!(
        "HYDRA — Project Leviathan Startup / Runtime Report\n\
         Version: {}\n\
         Timestamp (UTC): {}\n\
         OS: {} ({})\n\
         Working Dir: {}\n\
         \n\
         Summary: {}\n\
         \n\
         Details:\n{}\n\
         \n\
         Troubleshooting:\n\
         - Try launching with OpenGL fallback: HYDRA-Leviathan-Windows-x64.exe --renderer glow\n\
         - Try software WGPU fallback: HYDRA-Leviathan-Windows-x64.exe --renderer software\n\
         - Headless/terminal CLI remains available via hydra.exe\n",
        env!("CARGO_PKG_VERSION"),
        Utc::now().to_rfc3339(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".into()),
        title,
        detail
    );
    fs::write(&path, body).ok()?;
    Some(path)
}

/// Install a global panic hook that logs a readable local crash report and displays
/// a visible OS error dialog on Windows rather than terminating silently.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".into());
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_owned()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "non-string panic payload".into()
        };
        let detail = format!("Panic at {location}: {payload}");
        log_startup(&format!("PANIC: {detail}"));
        let report = write_crash_report("Unexpected Cockpit Panic", &detail);
        show_startup_error_dialog("HYDRA — Project Leviathan", &detail, report.as_deref());
        previous(info);
    }));
}

/// Display a visible native Windows error dialog when startup fails.
/// On non-Windows targets, prints cleanly to stderr.
pub fn show_startup_error_dialog(title: &str, detail: &str, crash_report: Option<&Path>) {
    let mut body = format!(
        "HYDRA — Project Leviathan could not start its graphical cockpit.\n\n\
         Reason:\n{detail}\n\n\
         Try launching with the OpenGL compatibility fallback:\n\
         HYDRA-Leviathan-Windows-x64.exe --renderer glow"
    );
    if let Some(path) = crash_report {
        body.push_str(&format!(
            "\n\nDiagnostic report saved to:\n{}",
            path.display()
        ));
    }
    eprintln!("{title}: {body}");
    #[cfg(windows)]
    {
        win32_message_box(title, &body);
    }
}

#[cfg(windows)]
fn win32_message_box(title: &str, message: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    const MB_OK: u32 = 0x0000_0000;
    const MB_ICONERROR: u32 = 0x0000_0010;
    const MB_SETFOREGROUND: u32 = 0x0001_0000;

    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(
            h_wnd: *mut std::ffi::c_void,
            lp_text: *const u16,
            lp_caption: *const u16,
            u_type: u32,
        ) -> i32;
    }

    let wide_title: Vec<u16> = OsStr::new(title).encode_wide().chain(Some(0)).collect();
    let wide_msg: Vec<u16> = OsStr::new(message).encode_wide().chain(Some(0)).collect();
    // SAFETY: Null-terminated UTF-16 buffers valid for the duration of MessageBoxW.
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            wide_msg.as_ptr(),
            wide_title.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_database_path_is_preserved_verbatim() {
        let explicit = PathBuf::from("custom/my-hydra.db");
        assert_eq!(resolve_default_database(Some(explicit.clone())), explicit);
    }

    #[test]
    fn app_data_root_resolves_to_hydra_directory() {
        if let Some(root) = app_data_root() {
            assert_eq!(root.file_name().and_then(|s| s.to_str()), Some("Hydra"));
        }
    }

    #[test]
    fn database_health_check_validates_clean_sqlite_and_flags_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("clean.db");
        crate::local_setup::initialize_history(&db).unwrap();
        assert_eq!(verify_local_database_health(&db).unwrap(), "ok");

        let corrupt = dir.path().join("corrupt.db");
        fs::write(&corrupt, b"not-a-valid-sqlite-header").unwrap();
        assert!(verify_local_database_health(&corrupt).is_err());
    }
}
