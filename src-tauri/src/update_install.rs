use crate::update_manifest::{verify, verify_file};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::windows::{fs::MetadataExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, WAIT_OBJECT_0},
    Storage::FileSystem::ReplaceFileW,
    System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
};

const HIDDEN: u32 = 0x08000000;
#[derive(Serialize, Deserialize)]
struct Task {
    target: String,
    version: String,
    parent: u32,
    nonce: String,
}
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
/// Reject junctions/symlinks anywhere in the update area before touching executables.
pub fn area(root: &Path) -> Result<PathBuf, String> {
    let directory = root.join(".updates");
    fs::create_dir_all(&directory).map_err(|_| "无法创建更新目录，请检查程序目录写入权限")?;
    if fs::symlink_metadata(&directory)
        .map_err(|_| "无法读取更新目录")?
        .file_attributes()
        & 0x400
        != 0
    {
        return Err("更新目录不能是链接或联接点".into());
    }
    for entry in fs::read_dir(&directory).map_err(|_| "无法读取更新目录")? {
        let entry = entry.map_err(|_| "无法读取更新目录")?;
        let meta = fs::symlink_metadata(entry.path()).map_err(|_| "无法读取更新文件")?;
        if meta.file_attributes() & 0x400 != 0 || !meta.is_file() {
            return Err("更新目录中存在不支持的链接或目录".into());
        }
    }
    Ok(directory)
}
pub fn prepare(executable: &Path, version: &str) -> Result<(), String> {
    let directory = area(executable.parent().ok_or("无法定位程序目录")?)?;
    let raw = fs::read(directory.join("update.json")).map_err(|_| "请先下载更新")?;
    let sig = fs::read(directory.join("update.json.sig")).map_err(|_| "请先下载更新")?;
    let manifest = verify(&raw, &sig, version)?;
    verify_file(&directory.join("new.exe"), &manifest.windows_x86_64)?;
    let target = executable
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("程序文件名无效")?
        .to_owned();
    let task = Task {
        target,
        version: version.to_owned(),
        parent: std::process::id(),
        nonce: format!(
            "{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ),
    };
    fs::write(
        directory.join("task.json"),
        serde_json::to_vec(&task).map_err(|_| "无法创建更新任务")?,
    )
    .map_err(|_| "无法保存更新任务")?;
    fs::copy(executable, directory.join("helper.exe"))
        .map_err(|_| "无法准备更新助手，请检查目录权限")?;
    Command::new(directory.join("helper.exe"))
        .arg("--apply-update")
        .creation_flags(HIDDEN)
        .spawn()
        .map_err(|_| "无法启动更新助手")?;
    Ok(())
}
fn replace(target: &Path, replacement: &Path, backup: Option<&Path>) -> Result<(), String> {
    let target = wide(target);
    let replacement = wide(replacement);
    let backup = backup.map(wide);
    if unsafe {
        ReplaceFileW(
            target.as_ptr(),
            replacement.as_ptr(),
            backup
                .as_ref()
                .map(|v| v.as_ptr())
                .unwrap_or(std::ptr::null()),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(format!(
            "替换程序失败（{}），请关闭占用程序的窗口后重试",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}
fn filename(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['/', '\\', ':'])
        && value.ends_with(".exe")
        && value != "helper.exe"
        && Path::new(value).components().count() == 1
}
fn apply() -> Result<(), String> {
    let helper = std::env::current_exe().map_err(|_| "无法定位更新助手")?;
    let directory = helper.parent().ok_or("无法定位更新目录")?;
    let root = directory.parent().ok_or("无法定位程序目录")?;
    if directory.file_name().and_then(|p| p.to_str()) != Some(".updates")
        || helper.file_name().and_then(|p| p.to_str()) != Some("helper.exe")
    {
        return Err("更新助手目录无效".into());
    }
    area(root)?;
    let task: Task = serde_json::from_slice(
        &fs::read(directory.join("task.json")).map_err(|_| "无法读取更新任务")?,
    )
    .map_err(|_| "更新任务无效")?;
    if !filename(&task.target) {
        return Err("更新目标无效".into());
    }
    let raw = fs::read(directory.join("update.json")).map_err(|_| "更新清单不存在")?;
    let sig = fs::read(directory.join("update.json.sig")).map_err(|_| "更新签名不存在")?;
    let manifest = verify(&raw, &sig, &task.version)?;
    verify_file(&directory.join("new.exe"), &manifest.windows_x86_64)?;
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, task.parent) };
    if !process.is_null() {
        let status = unsafe { WaitForSingleObject(process, 60000) };
        unsafe {
            CloseHandle(process);
        }
        if status != WAIT_OBJECT_0 {
            return Err("原程序未能退出，更新已取消".into());
        }
    }
    let target = root.join(&task.target);
    if fs::symlink_metadata(&target)
        .map_err(|_| "找不到原程序")?
        .file_attributes()
        & 0x400
        != 0
    {
        return Err("原程序不能是链接".into());
    }
    let backup = directory.join("previous.exe");
    if backup.exists() {
        fs::remove_file(&backup).map_err(|_| "无法清理旧更新备份")?;
    }
    let ack = directory.join("ack.txt");
    let _ = fs::remove_file(&ack);
    // ReplaceFile atomically keeps the target name present and saves the original.
    replace(&target, &directory.join("new.exe"), Some(&backup))?;
    let launched = Command::new(&target)
        .args(["--update-complete", &task.nonce])
        .current_dir(root)
        .creation_flags(HIDDEN)
        .spawn();
    let mut child = match launched {
        Ok(child) => child,
        Err(_) => {
            replace(&target, &backup, None)?;
            let _ = fs::write(directory.join("result.txt"), "新版无法启动，已恢复原版本");
            let _ = Command::new(&target).creation_flags(HIDDEN).spawn();
            return Err("新版无法启动，已恢复原版本".into());
        }
    };
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        if fs::read_to_string(&ack).ok().as_deref() == Some(task.nonce.as_str()) {
            let _ = fs::remove_file(&backup);
            fs::write(directory.join("result.txt"), "更新成功").map_err(|_| "无法保存更新结果")?;
            return Ok(());
        }
        if child
            .try_wait()
            .map_err(|_| "无法检查新版启动状态")?
            .is_some()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let _ = child.kill();
    let _ = child.wait();
    replace(&target, &backup, None)?;
    let _ = fs::write(directory.join("result.txt"), "新版启动未完成，已恢复原版本");
    let _ = Command::new(&target)
        .current_dir(root)
        .creation_flags(HIDDEN)
        .spawn();
    Err("新版启动未完成，已恢复原版本".into())
}
pub fn helper_mode() -> bool {
    if std::env::args().nth(1).as_deref() != Some("--apply-update") {
        return false;
    }
    if let Err(error) = apply() {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let _ = fs::write(dir.join("result.txt"), error);
            }
        }
    }
    true
}
pub fn acknowledge(root: &Path) {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--update-complete") {
        if let Some(nonce) = args.get(2).filter(|n| n.len() < 100) {
            let _ = fs::write(root.join(".updates/ack.txt"), nonce);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restricts_targets_to_a_single_executable_filename() {
        for bad in [
            "../a.exe",
            "C:\\a.exe",
            "a/b.exe",
            "a.exe:stream",
            "helper.exe",
            "..",
            "a.txt",
        ] {
            assert!(!filename(bad));
        }
        assert!(filename("人大校园网助手.exe"));
    }
    #[test]
    fn atomic_replacement_preserves_backup_and_can_restore() {
        let root = std::env::temp_dir().join(format!("ruc-replace-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("app.exe");
        let new = root.join("new.exe");
        let backup = root.join("previous.exe");
        fs::write(&target, b"original").unwrap();
        fs::write(&new, b"updated").unwrap();
        replace(&target, &new, Some(&backup)).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"updated");
        assert_eq!(fs::read(&backup).unwrap(), b"original");
        replace(&target, &backup, None).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        fs::remove_file(target).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
