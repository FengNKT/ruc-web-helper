use std::io::{Read, Seek, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const IDENTIFIER: &str = "cn.ruc.webhelper";
const FILES: &[&str] = &[
    "settings.json",
    "account.enc",
    "devices.json",
    "device-names.json",
    "updater.json",
];

pub fn directory() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .map(|root| root.join(IDENTIFIER))
        .ok_or_else(|| "无法定位当前 Windows 用户的数据目录".into())
}

pub(crate) fn reset_marker(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(".reset");
    PathBuf::from(name)
}
pub fn legacy_import_allowed(target: &Path) -> bool {
    !reset_marker(target).exists()
}

fn remove_data_tree(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_attributes() & 0x400 != 0 {
        return Err(io::Error::other("数据目录不能是链接或联接点"));
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_attributes() & 0x400 != 0 {
            if metadata.file_attributes() & 0x10 != 0 {
                fs::remove_dir(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
        } else if metadata.is_dir() {
            remove_data_tree(&entry.path())?;
        } else {
            if metadata.permissions().readonly() {
                let mut permissions = metadata.permissions();
                permissions.set_readonly(false);
                fs::set_permissions(entry.path(), permissions)?;
            }
            fs::remove_file(entry.path())?;
        }
    }
    fs::remove_dir(path)
}

fn finish_pending_clear(target: &Path) -> Result<(), String> {
    let marker = reset_marker(target);
    let mut file = match fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&marker)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("数据正在清除，请稍后重新打开软件".into()),
    };
    let mut state = String::new();
    file.read_to_string(&mut state)
        .map_err(|_| "无法读取数据清除状态")?;
    if state == "pending" {
        if target.exists() {
            remove_data_tree(target).map_err(|_| "数据清除未完成，请退出其他助手后重试")?;
        }
        file.set_len(0)
            .and_then(|_| file.rewind())
            .and_then(|_| file.write_all(b"complete"))
            .and_then(|_| file.sync_all())
            .map_err(|_| "无法保存数据清除状态")?;
    }
    Ok(())
}

fn copy_entry(source: &Path, target: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(source)?;
    if meta.file_attributes() & 0x400 != 0 {
        return Err(io::Error::other("旧数据中存在链接或联接点"));
    }
    if meta.is_dir() {
        fs::create_dir(target)?;
        for item in fs::read_dir(source)? {
            let item = item?;
            copy_entry(&item.path(), &target.join(item.file_name()))?;
        }
    } else if meta.is_file() {
        // WebView processes can briefly retain file handles after the app exits.
        let mut result = fs::copy(source, target);
        for _ in 0..5 {
            if result.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
            result = fs::copy(source, target);
        }
        result?;
    }
    Ok(())
}

/// Import one coherent old installation, never merge it over existing user data.
/// Stage the entire copy before publishing it; old data is always retained.
pub fn prepare(source: &Path, target: &Path) -> Result<(), String> {
    finish_pending_clear(target)?;
    if !legacy_import_allowed(target) {
        fs::create_dir_all(target).map_err(|_| "无法创建用户数据目录")?;
        return Ok(());
    }
    if target.exists()
        && fs::read_dir(target)
            .map_err(|_| "无法读取用户数据目录")?
            .next()
            .is_some()
    {
        return Ok(());
    }
    let parent = target.parent().ok_or("用户数据目录无效")?;
    fs::create_dir_all(parent).map_err(|_| "无法创建用户数据目录")?;
    let staged = parent.join(format!(
        "{}.import-{}-{}",
        IDENTIFIER,
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    fs::create_dir(&staged).map_err(|_| "无法准备旧数据迁移")?;
    let result = (|| -> io::Result<()> {
        for name in FILES {
            let path = source.join(name);
            if path.exists() {
                copy_entry(&path, &staged.join(name))?;
            }
        }
        if source.is_dir() {
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("account.enc.recovery-")
                {
                    copy_entry(&entry.path(), &staged.join(entry.file_name()))?;
                }
            }
        }
        if source.join("webview-data").exists() {
            copy_entry(&source.join("webview-data"), &staged.join("webview-data"))?;
        }
        // Keep verified downloads and recovery backups, never old helper tasks.
        if source.join(".updates").is_dir() {
            fs::create_dir(staged.join(".updates"))?;
            for name in [
                "update.json",
                "update.json.sig",
                "new.exe",
                "previous.exe",
                "result.txt",
            ] {
                let path = source.join(".updates").join(name);
                if path.exists() {
                    copy_entry(&path, &staged.join(".updates").join(name))?;
                }
            }
        }
        if target.exists() {
            fs::remove_dir(target)?;
        } // only an empty directory
        fs::rename(&staged, target)
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staged);
        return Err(format!(
            "无法迁移旧数据，原文件已保留，请退出旧助手后重试：{error}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_clear_removes_all_user_data_without_reimporting_legacy_copies() {
        let root = std::env::temp_dir().join(format!("ruc-clear-storage-{}", std::process::id()));
        let target = root.join("cn.ruc.webhelper");
        let old = root.join("old");
        fs::create_dir_all(target.join("webview-data/Default")).unwrap();
        fs::create_dir_all(&old).unwrap();
        fs::write(target.join("account.enc"), b"fictional-account").unwrap();
        fs::write(target.join("account.enc.recovery-1"), b"fictional-backup").unwrap();
        fs::write(target.join("webview-data/Default/data"), b"cache").unwrap();
        fs::write(old.join("account.enc"), b"must-not-return").unwrap();
        fs::write(reset_marker(&target), b"pending").unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(reset_marker(&target))
            .unwrap();
        assert!(prepare(&old, &target).is_err());
        assert!(target.join("account.enc").exists());
        drop(lock);
        prepare(&old, &target).unwrap();
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
        assert!(!legacy_import_allowed(&target));
        assert_eq!(
            fs::read_to_string(reset_marker(&target)).unwrap(),
            "complete"
        );
        assert_eq!(
            fs::read(old.join("account.enc")).unwrap(),
            b"must-not-return"
        );
        prepare(&old, &target).unwrap();
        assert!(!target.join("account.enc").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn imports_without_deleting_originals_and_never_overwrites_user_data() {
        let root = std::env::temp_dir().join(format!("ruc-storage-{}", std::process::id()));
        let old = root.join("old");
        let new = root.join("user");
        fs::create_dir_all(old.join("webview-data/Default/Local Storage")).unwrap();
        fs::create_dir_all(old.join(".updates")).unwrap();
        for name in FILES {
            fs::write(old.join(name), format!("old-{name}")).unwrap();
        }
        fs::write(old.join("account.enc.recovery-test"), b"backup").unwrap();
        fs::write(
            old.join("webview-data/Default/Local Storage/state"),
            b"webview",
        )
        .unwrap();
        fs::write(old.join(".updates/task.json"), b"obsolete").unwrap();
        fs::write(old.join(".updates/new.exe"), b"download").unwrap();
        prepare(&old, &new).unwrap();
        for name in FILES {
            assert_eq!(
                fs::read(old.join(name)).unwrap(),
                fs::read(new.join(name)).unwrap()
            );
        }
        assert_eq!(
            fs::read(new.join("webview-data/Default/Local Storage/state")).unwrap(),
            b"webview"
        );
        assert!(new.join("account.enc.recovery-test").exists());
        assert!(!new.join(".updates/task.json").exists());
        assert!(new.join(".updates/new.exe").exists());
        fs::write(new.join("account.enc"), b"current").unwrap();
        prepare(&old, &new).unwrap();
        assert_eq!(fs::read(new.join("account.enc")).unwrap(), b"current");
        let empty = root.join("fresh");
        prepare(&root.join("missing"), &empty).unwrap();
        assert_eq!(fs::read_dir(empty).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_import_keeps_existing_data_and_can_be_retried() {
        let root = std::env::temp_dir().join(format!("ruc-storage-retry-{}", std::process::id()));
        let old = root.join("old");
        let new = root.join("user");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("account.enc"), b"preserved").unwrap();
        // An invalid destination is never replaced or removed.
        fs::write(&new, b"existing file").unwrap();
        assert!(prepare(&old, &new).is_err());
        assert_eq!(fs::read(&new).unwrap(), b"existing file");
        assert_eq!(fs::read(old.join("account.enc")).unwrap(), b"preserved");
        fs::remove_file(&new).unwrap();
        prepare(&old, &new).unwrap();
        assert_eq!(fs::read(new.join("account.enc")).unwrap(), b"preserved");
        fs::remove_dir_all(root).unwrap();
    }
}
