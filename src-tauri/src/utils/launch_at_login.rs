use crate::error::AppError;
use std::path::{Path, PathBuf};

const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE_NAME: &str = "DevNest";
const STARTUP_SHORTCUT_NAME: &str = "DevNest.lnk";

fn is_cargo_target_executable_path(path: &Path) -> bool {
    let mut saw_target = false;

    for component in path.components() {
        let value = component.as_os_str().to_string_lossy();
        if saw_target
            && (value.eq_ignore_ascii_case("debug") || value.eq_ignore_ascii_case("release"))
        {
            return true;
        }
        saw_target = value.eq_ignore_ascii_case("target");
    }

    false
}

fn extract_executable_from_command(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }

    if let Some(remainder) = command.strip_prefix('"') {
        let end = remainder.find('"')?;
        return Some(PathBuf::from(&remainder[..end]));
    }

    let lower = command.to_ascii_lowercase();
    let exe_end = lower.find(".exe").map(|index| index + ".exe".len())?;
    Some(PathBuf::from(command[..exe_end].trim()))
}

fn utf16le_contains_case_insensitive(bytes: &[u8], needle: &str) -> bool {
    let mut pattern = Vec::with_capacity(needle.len() * 2);
    for unit in needle.encode_utf16() {
        pattern.extend_from_slice(&unit.to_le_bytes());
    }

    bytes.windows(pattern.len()).any(|window| {
        window
            .chunks_exact(2)
            .zip(pattern.chunks_exact(2))
            .all(|(actual, expected)| {
                let actual = u16::from_le_bytes([actual[0], actual[1]]);
                let expected = u16::from_le_bytes([expected[0], expected[1]]);
                char::from_u32(actual as u32)
                    .zip(char::from_u32(expected as u32))
                    .map(|(left, right)| left.eq_ignore_ascii_case(&right))
                    .unwrap_or(actual == expected)
            })
    })
}

fn startup_shortcut_references_cargo_target(bytes: &[u8]) -> bool {
    bytes
        .windows(r"\src-tauri\target\".len())
        .any(|window| window.eq_ignore_ascii_case(r"\src-tauri\target\".as_bytes()))
        || bytes
            .windows(r"\target\debug\devnest.exe".len())
            .any(|window| window.eq_ignore_ascii_case(r"\target\debug\devnest.exe".as_bytes()))
        || bytes
            .windows(r"\target\release\devnest.exe".len())
            .any(|window| window.eq_ignore_ascii_case(r"\target\release\devnest.exe".as_bytes()))
        || utf16le_contains_case_insensitive(bytes, r"\src-tauri\target\")
        || utf16le_contains_case_insensitive(bytes, r"\target\debug\devnest.exe")
        || utf16le_contains_case_insensitive(bytes, r"\target\release\devnest.exe")
}

fn startup_shortcut_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| {
        PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
            .join(STARTUP_SHORTCUT_NAME)
    })
}

#[cfg(windows)]
pub fn ensure_launch_at_login(app_path: &std::path::Path) -> Result<(), AppError> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run_key, _) = hkcu
        .create_subkey_with_flags(RUN_KEY_PATH, KEY_READ | KEY_SET_VALUE)
        .map_err(|error| {
            AppError::with_details(
                "LAUNCH_AT_LOGIN_SYNC_FAILED",
                "DevNest could not open the Windows startup registry key.",
                error.to_string(),
            )
        })?;

    if is_cargo_target_executable_path(app_path) {
        if let Ok(existing_command) = run_key.get_value::<String, _>(RUN_VALUE_NAME) {
            if extract_executable_from_command(&existing_command)
                .as_deref()
                .is_some_and(is_cargo_target_executable_path)
            {
                let _ = run_key.delete_value(RUN_VALUE_NAME);
            }
        }

        cleanup_cargo_target_startup_shortcut();
        return Ok(());
    }

    let command = format!("\"{}\"", app_path.display());
    run_key
        .set_value(RUN_VALUE_NAME, &command)
        .map_err(|error| {
            AppError::with_details(
                "LAUNCH_AT_LOGIN_SYNC_FAILED",
                "DevNest could not register itself to launch when Windows starts.",
                error.to_string(),
            )
        })?;

    Ok(())
}

#[cfg(windows)]
fn cleanup_cargo_target_startup_shortcut() {
    let Some(shortcut_path) = startup_shortcut_path() else {
        return;
    };

    let Ok(bytes) = std::fs::read(&shortcut_path) else {
        return;
    };

    if startup_shortcut_references_cargo_target(&bytes) {
        let _ = std::fs::remove_file(shortcut_path);
    }
}

#[cfg(not(windows))]
pub fn ensure_launch_at_login(_app_path: &std::path::Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(not(windows))]
fn cleanup_cargo_target_startup_shortcut() {}

#[cfg(test)]
mod tests {
    use super::{
        RUN_VALUE_NAME, extract_executable_from_command, is_cargo_target_executable_path,
        startup_shortcut_references_cargo_target,
    };
    use std::path::Path;

    #[test]
    fn uses_stable_windows_run_value_name() {
        assert_eq!(RUN_VALUE_NAME, "DevNest");
    }

    #[test]
    fn detects_cargo_target_executable_paths() {
        assert!(is_cargo_target_executable_path(Path::new(
            r"D:\Aetherone\devnest\src-tauri\target\debug\devnest.exe"
        )));
        assert!(is_cargo_target_executable_path(Path::new(
            r"D:\Aetherone\devnest\src-tauri\target\release\devnest.exe"
        )));
        assert!(!is_cargo_target_executable_path(Path::new(
            r"C:\Users\phuvn\AppData\Local\DevNest\devnest.exe"
        )));
    }

    #[test]
    fn extracts_executable_from_startup_command() {
        assert_eq!(
            extract_executable_from_command(
                r#""D:\Aetherone\devnest\src-tauri\target\debug\devnest.exe" --startup"#
            )
            .as_deref(),
            Some(Path::new(
                r"D:\Aetherone\devnest\src-tauri\target\debug\devnest.exe"
            ))
        );
        assert_eq!(
            extract_executable_from_command(
                r"D:\Aetherone\devnest\src-tauri\target\debug\devnest.exe --startup"
            )
            .as_deref(),
            Some(Path::new(
                r"D:\Aetherone\devnest\src-tauri\target\debug\devnest.exe"
            ))
        );
    }

    #[test]
    fn detects_startup_shortcut_pointing_at_cargo_target() {
        assert!(startup_shortcut_references_cargo_target(
            br"D:\Aetherone\devnest\src-tauri\target\debug\devnest.exe"
        ));

        let utf16_bytes = r"D:\Aetherone\devnest\src-tauri\target\release\devnest.exe"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert!(startup_shortcut_references_cargo_target(&utf16_bytes));

        assert!(!startup_shortcut_references_cargo_target(
            br"C:\Users\phuvn\AppData\Local\DevNest\devnest.exe"
        ));
    }
}
