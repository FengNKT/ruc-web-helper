use crate::update_manifest::{verify, verify_file};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    fs::create_dir_all(&directory).map_err(|_| "无法创建更新目录，请检查用户数据目录写入权限")?;
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
pub fn prepare(root: &Path, executable: &Path, version: &str) -> Result<(), String> {
    let directory = area(root)?;
    let raw = fs::read(directory.join("update.json")).map_err(|_| "请先下载更新")?;
    let sig = fs::read(directory.join("update.json.sig")).map_err(|_| "请先下载更新")?;
    let manifest = verify(&raw, &sig, version)?;
    verify_file(&directory.join("new.exe"), &manifest.windows_x86_64)?;
    let executable = fs::canonicalize(executable).map_err(|_| "无法定位原程序")?;
    if !valid_target_path(&executable) {
        return Err("更新目标无效".into());
    }
    let target = executable.to_str().ok_or("程序路径无效")?.to_owned();
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
    fs::copy(&executable, directory.join("helper.exe"))
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
fn valid_target_path(target: &Path) -> bool {
    target.is_absolute()
        && !target
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        && target
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(filename)
}
fn validate_target(target: &Path, helper: &Path) -> Result<(), String> {
    if !valid_target_path(target) {
        return Err("更新目标无效".into());
    }
    let meta = fs::symlink_metadata(target).map_err(|_| "找不到原程序")?;
    let helper_meta = fs::metadata(helper).map_err(|_| "无法读取更新助手")?;
    if !meta.is_file() || meta.file_attributes() & 0x400 != 0 || meta.len() != helper_meta.len() {
        return Err("更新目标与原助手不匹配".into());
    }
    let target_hash = Sha256::digest(fs::read(target).map_err(|_| "无法校验原程序")?);
    let helper_hash = Sha256::digest(fs::read(helper).map_err(|_| "无法校验更新助手")?);
    if target_hash != helper_hash {
        return Err("更新目标与原助手不匹配".into());
    }
    Ok(())
}
// ReplaceFile requires files on the same volume. Only this short-lived staging
// file lives beside the EXE; downloads, backups and helper stay in user data.
fn install_file(target: &Path, replacement: &Path) -> Result<(), String> {
    let parent = target.parent().ok_or("程序路径无效")?;
    let staged = parent.join(format!(
        ".ruc-update-{}-{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let result = (|| {
        let mut input = fs::File::open(replacement).map_err(|_| "无法读取更新文件")?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
            .map_err(|_| "无法写入程序所在文件夹，请移动 EXE 到可写位置后重试")?;
        std::io::copy(&mut input, &mut output).map_err(|_| "无法准备程序替换文件")?;
        output.sync_all().map_err(|_| "无法保存程序替换文件")?;
        drop(output);
        if !target.exists() {
            return fs::rename(&staged, target).map_err(|_| "无法恢复程序文件".into());
        }
        let mut result = replace(target, &staged, None);
        for _ in 0..10 {
            if result.is_ok() || !staged.exists() || !target.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
            result = replace(target, &staged, None);
        }
        result
    })();
    let _ = fs::remove_file(staged);
    result
}
fn apply() -> Result<(), String> {
    let helper = std::env::current_exe().map_err(|_| "无法定位更新助手")?;
    let root = crate::storage::directory()?;
    let directory = area(&root)?;
    if fs::canonicalize(helper.parent().ok_or("无法定位更新目录")?).ok()
        != fs::canonicalize(&directory).ok()
        || helper.file_name().and_then(|p| p.to_str()) != Some("helper.exe")
    {
        return Err("更新助手目录无效".into());
    }
    let task: Task = serde_json::from_slice(
        &fs::read(directory.join("task.json")).map_err(|_| "无法读取更新任务")?,
    )
    .map_err(|_| "更新任务无效")?;
    let target = PathBuf::from(&task.target);
    validate_target(&target, &helper)?;
    let executable_directory = target.parent().ok_or("程序路径无效")?;
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
    // Check again after waiting for the old process, before replacing anything.
    validate_target(&target, &helper)?;
    let backup = directory.join("previous.exe");
    if backup.exists() {
        fs::remove_file(&backup).map_err(|_| "无法清理旧更新备份")?;
    }
    let ack = directory.join("ack.txt");
    let _ = fs::remove_file(&ack);
    fs::copy(&target, &backup).map_err(|_| "无法备份原程序，更新已取消")?;
    if let Err(error) = install_file(&target, &directory.join("new.exe")) {
        // Some ReplaceFile errors may have moved the original; recover it first.
        if !target.exists() {
            install_file(&target, &backup)?;
        }
        return Err(error);
    }
    let launched = Command::new(&target)
        .args(["--update-complete", &task.nonce])
        .current_dir(executable_directory)
        .creation_flags(HIDDEN)
        .spawn();
    let mut child = match launched {
        Ok(child) => child,
        Err(_) => {
            install_file(&target, &backup)?;
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
    install_file(&target, &backup)?;
    let _ = fs::write(directory.join("result.txt"), "新版启动未完成，已恢复原版本");
    let _ = Command::new(&target)
        .current_dir(executable_directory)
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
pub fn acknowledge(root: &Path, executable: &Path) {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--update-complete") {
        if let Some(nonce) = args.get(2).filter(|n| n.len() < 100) {
            acknowledge_nonce(root, executable, nonce);
        }
    }
}
fn acknowledge_nonce(root: &Path, executable: &Path, nonce: &str) {
    let _ = fs::write(root.join(".updates/ack.txt"), nonce);
    // A 1.1 updater still waits next to the EXE. Acknowledge its exact task so
    // the first upgrade to user-directory storage does not roll back.
    if let Some(legacy) = executable.parent().map(|p| p.join(".updates")) {
        if let Ok(bytes) = fs::read(legacy.join("task.json")) {
            if let Ok(task) = serde_json::from_slice::<Task>(&bytes) {
                if task.nonce == nonce
                    && filename(&task.target)
                    && executable.file_name().and_then(|s| s.to_str()) == Some(task.target.as_str())
                {
                    let _ = fs::write(legacy.join("ack.txt"), nonce);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_directory_updates_stage_and_restore_beside_the_target_without_leaving_files() {
        let root = std::env::temp_dir().join(format!("ruc-user-update-{}", std::process::id()));
        let user = root.join("user/.updates");
        let program = root.join("program");
        fs::create_dir_all(&user).unwrap();
        fs::create_dir_all(&program).unwrap();
        let target = program.join("助手.exe");
        let helper = user.join("helper.exe");
        let downloaded = user.join("new.exe");
        fs::write(&target, b"original").unwrap();
        fs::write(&helper, b"original").unwrap();
        fs::write(&downloaded, b"updated").unwrap();
        validate_target(&target, &helper).unwrap();
        let other = program.join("other.exe");
        fs::write(&other, b"not-ours").unwrap();
        assert!(validate_target(&other, &helper).is_err());
        assert!(!valid_target_path(Path::new("relative.exe")));
        assert!(!valid_target_path(&program.join("../other.exe")));
        fs::remove_file(other).unwrap();
        let backup = user.join("previous.exe");
        fs::copy(&target, &backup).unwrap();
        install_file(&target, &downloaded).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"updated");
        assert_eq!(fs::read(&downloaded).unwrap(), b"updated");
        assert_eq!(fs::read_dir(&program).unwrap().count(), 1);
        install_file(&target, &backup).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read_dir(&program).unwrap().count(), 1);
        // Also recover a missing target after a failed Windows replacement.
        fs::remove_file(&target).unwrap();
        install_file(&target, &backup).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn acknowledges_the_exact_legacy_update_task_during_first_upgrade() {
        let root = std::env::temp_dir().join(format!("ruc-legacy-ack-{}", std::process::id()));
        let user = root.join("user");
        let program = root.join("program");
        fs::create_dir_all(user.join(".updates")).unwrap();
        fs::create_dir_all(program.join(".updates")).unwrap();
        let task = Task {
            target: "助手.exe".into(),
            version: "1.2.0".into(),
            parent: 1,
            nonce: "expected".into(),
        };
        fs::write(
            program.join(".updates/task.json"),
            serde_json::to_vec(&task).unwrap(),
        )
        .unwrap();
        let exe = program.join("助手.exe");
        acknowledge_nonce(&user, &exe, "wrong");
        assert!(!program.join(".updates/ack.txt").exists());
        acknowledge_nonce(&user, &exe, "expected");
        assert_eq!(
            fs::read(user.join(".updates/ack.txt")).unwrap(),
            b"expected"
        );
        assert_eq!(
            fs::read(program.join(".updates/ack.txt")).unwrap(),
            b"expected"
        );
        fs::remove_dir_all(root).unwrap();
    }
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
