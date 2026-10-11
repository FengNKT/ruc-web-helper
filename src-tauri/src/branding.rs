use std::path::Path;

pub const CHINESE_NAME: &str = "人大校园网助手";
pub const ENGLISH_NAME: &str = "RUC Web Helper";

fn legacy_command_matches(command: &str, executable: &Path) -> bool {
    let Some(directory) = executable.parent() else {
        return false;
    };
    let command = command
        .trim()
        .strip_suffix(" --autostart")
        .unwrap_or(command.trim())
        .trim_matches('"')
        .to_lowercase();
    let directory = directory.to_string_lossy().to_lowercase();
    // Old names are retained only to migrate this application's own registration.
    for parent in [
        directory.clone(),
        directory.replace("\\ruc-web-helper\\", "\\campus-assistant\\"),
    ] {
        for name in ["校园网助手.exe", "ruc-campus-assistant.exe"] {
            if command == format!("{parent}\\{name}") {
                return true;
            }
        }
    }
    false
}

pub fn migrate_startup(executable: &Path) -> Result<(), String> {
    use windows_registry::CURRENT_USER;
    let run_path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let approved_path = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    let Ok(run) = CURRENT_USER.open(run_path) else {
        return Ok(());
    };
    for old_name in ["校园网助手", "ruc-campus-assistant"] {
        let Ok(command) = run.get_string(old_name) else {
            continue;
        };
        if !legacy_command_matches(&command, executable) {
            continue;
        }
        let approved = CURRENT_USER
            .open(approved_path)
            .ok()
            .and_then(|key| key.get_value(old_name).ok());
        let writable = CURRENT_USER
            .options()
            .write()
            .open(run_path)
            .map_err(|_| "无法迁移本软件的自启动项")?;
        writable
            .set_string(
                CHINESE_NAME,
                format!("\"{}\" --autostart", executable.display()),
            )
            .map_err(|_| "无法更新本软件的自启动路径")?;
        if let Some(value) = approved {
            let key = CURRENT_USER
                .create(approved_path)
                .map_err(|_| "无法迁移自启动启用状态")?;
            key.set_bytes(CHINESE_NAME, value.ty(), &value)
                .map_err(|_| "无法保留自启动启用状态")?;
            key.remove_value(old_name)
                .map_err(|_| "无法清理旧版自启动状态")?;
        }
        writable
            .remove_value(old_name)
            .map_err(|_| "无法清理旧版自启动项")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_migrates_own_legacy_executable_paths() {
        let current = Path::new(r"C:\Projects\ruc-web-helper\交付\人大校园网助手.exe");
        assert!(legacy_command_matches(
            r"C:\Projects\campus-assistant\交付\校园网助手.exe --autostart",
            current
        ));
        assert!(legacy_command_matches(
            r#""C:\Projects\ruc-web-helper\交付\校园网助手.exe" --autostart"#,
            current
        ));
        assert!(!legacy_command_matches(
            r"C:\Other\校园网助手.exe --autostart",
            current
        ));
        assert!(!legacy_command_matches(
            r"C:\Projects\campus-assistant\交付\校园网助手.exe.extra --autostart",
            current
        ));
    }
}
