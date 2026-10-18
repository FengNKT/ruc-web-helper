use std::{fs, io::Write, os::windows::process::CommandExt, process::Command};

pub fn schedule() -> Result<(), String> {
    let script = std::env::temp_dir().join(format!(
        "ruc-webhelper-cleanup-{}-{}.ps1",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let ready = script.with_extension("ready");
    let marker = crate::storage::reset_marker(&crate::storage::directory()?);
    let previous = match fs::read(&marker) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err("数据正在清除，请稍后重试".into()),
    };
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&script)
            .map_err(|_| "无法准备清除任务")?;
        // Windows PowerShell 5 requires a BOM for Chinese text.
        file.write_all(b"\xef\xbb\xbf")
            .and_then(|_| file.write_all(include_bytes!("cleanup.ps1")))
            .map_err(|_| "无法保存清除任务")?;
        drop(file);
        let powershell = std::env::var_os("SystemRoot")
            .map(std::path::PathBuf::from)
            .ok_or("无法定位 Windows 系统目录")?
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut child = Command::new(powershell)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-File",
            ])
            .arg(&script)
            .arg("-ParentProcessId")
            .arg(std::process::id().to_string())
            .arg("-ReadyFile")
            .arg(&ready)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|_| "无法启动数据清除任务")?;
        let started = std::time::Instant::now();
        while started.elapsed() < std::time::Duration::from_secs(10) {
            if fs::read(&ready).ok().as_deref() == Some(b"ready") {
                return Ok(());
            }
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        match &previous {
            Some(bytes) => {
                let _ = fs::write(&marker, bytes);
            }
            None => {
                let _ = fs::remove_file(&marker);
            }
        }
        Err("无法准备数据清除任务，未执行清除".into())
    })();
    let _ = fs::remove_file(ready);
    if result.is_err() {
        let _ = fs::remove_file(script);
    }
    result
}
