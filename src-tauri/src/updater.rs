use crate::{update_install, update_manifest, update_network};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

const API: &str = "https://api.github.com/repos/FengNKT/ruc-web-helper/releases/latest";
fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
fn yes() -> bool {
    true
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Network {
    #[default]
    System,
    Direct,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default = "yes")]
    pub automatic: bool,
    #[serde(default)]
    pub network: Network,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            automatic: true,
            network: Network::System,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub published_at: String,
    pub size: u64,
    pub signed: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Stored {
    preferences: Preferences,
    checked: i64,
    next_attempt: i64,
    rate_limited_until: i64,
    etag: Option<String>,
    release: Option<Release>,
    skipped: String,
    notified: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub preferences: Preferences,
    pub status: String,
    pub message: String,
    pub release: Option<Release>,
    pub checked: i64,
    pub downloaded: u64,
    pub skipped: bool,
}
struct Data {
    stored: Stored,
    view: View,
}
struct Inner {
    directory: PathBuf,
    data: Mutex<Data>,
    busy: AtomicBool,
    cancel: AtomicBool,
}
#[derive(Clone)]
pub struct Updater(Arc<Inner>);
impl Updater {
    pub fn begin_data_clear(&self) -> Result<(), String> {
        self.0
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| ())
            .map_err(|_| "正在检查或下载更新，请完成或取消后再清除数据".into())
    }
    pub fn abort_data_clear(&self) {
        self.0.busy.store(false, Ordering::SeqCst);
    }

    pub fn new(directory: PathBuf) -> Self {
        let stored: Stored = fs::read(directory.join("updater.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut view = View {
            preferences: stored.preferences.clone(),
            status: "idle".into(),
            message: "尚未检查更新".into(),
            release: stored.release.clone(),
            checked: stored.checked,
            downloaded: 0,
            skipped: stored
                .release
                .as_ref()
                .is_some_and(|r| r.version == stored.skipped),
        };
        if let Some(release) = stored.release.as_ref().filter(|r| {
            update_manifest::stable(&r.version).ok()
                > update_manifest::stable(env!("CARGO_PKG_VERSION")).ok()
        }) {
            view.status = "available".into();
            view.message = format!("发现新版本 {}", release.version);
            if let Ok(raw) = fs::read(directory.join(".updates/update.json")) {
                if let Ok(sig) = fs::read(directory.join(".updates/update.json.sig")) {
                    if let Ok(manifest) = update_manifest::verify(&raw, &sig, &release.version) {
                        if update_manifest::verify_file(
                            &directory.join(".updates/new.exe"),
                            &manifest.windows_x86_64,
                        )
                        .is_ok()
                        {
                            view.status = "ready".into();
                            view.message = "更新已下载，可重启更新".into();
                            view.downloaded = manifest.windows_x86_64.size;
                        }
                    }
                }
            }
        } else {
            view.release = None;
            if stored.checked > 0 {
                view.status = "current".into();
                view.message = "当前已是最新版本".into();
            }
        }
        if let Ok(result) = fs::read_to_string(directory.join(".updates/result.txt")) {
            if result != "更新成功" {
                view.message = result;
                view.status = "error".into();
            }
            let _ = fs::remove_file(directory.join(".updates/result.txt"));
        }
        Self(Arc::new(Inner {
            directory,
            data: Mutex::new(Data { stored, view }),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
        }))
    }
    fn view(&self) -> View {
        self.0.data.lock().unwrap().view.clone()
    }
    fn emit(&self, app: &tauri::AppHandle) {
        let _ = app.emit("update-state", self.view());
    }
    fn set_status(&self, app: &tauri::AppHandle, status: &str, message: impl Into<String>) {
        {
            let mut data = self.0.data.lock().unwrap();
            data.view.status = status.into();
            data.view.message = message.into();
        }
        self.emit(app);
    }
    fn persist(&self) -> Result<(), String> {
        let data = self.0.data.lock().unwrap();
        let bytes = serde_json::to_vec_pretty(&data.stored).map_err(|_| "无法保存更新设置")?;
        let temp = self.0.directory.join("updater.tmp");
        fs::write(&temp, bytes)
            .and_then(|_| fs::rename(&temp, self.0.directory.join("updater.json")))
            .map_err(|_| "无法保存更新设置，请检查用户数据目录权限".into())
    }
    fn request(
        &self,
        url: &str,
        limit: u64,
        etag: Option<&str>,
    ) -> Result<(update_network::Response, Vec<u8>), String> {
        let direct = matches!(self.view().preferences.network, Network::Direct);
        let mut bytes = Vec::new();
        let response =
            update_network::get(url, direct, etag, limit, &self.0.cancel, &mut bytes, |_| {})?;
        Ok((response, bytes))
    }
    fn bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        let (response, bytes) = self.request(url, limit, None)?;
        if response.status != 200 {
            return Err(format!(
                "更新文件下载失败（HTTP {}），请稍后重试",
                response.status
            ));
        }
        Ok(bytes)
    }
    fn check_inner(&self, app: &tauri::AppHandle, manual: bool) -> Result<(), String> {
        self.set_status(app, "checking", "正在检查更新…");
        let etag = self.0.data.lock().unwrap().stored.etag.clone();
        let (response, raw) = self.request(API, 4 * 1024 * 1024, etag.as_deref())?;
        if response.status == 403 || response.status == 429 {
            let next = retry_time(
                response.retry_after.as_deref(),
                response.reset.as_deref(),
                now(),
            );
            self.0.data.lock().unwrap().stored.next_attempt = next;
            self.0.data.lock().unwrap().stored.rate_limited_until = next;
            let _ = self.persist();
            return Err("GitHub 暂时限制了请求，请稍后再检查".into());
        }
        let release = match response.status {
            304 => self.0.data.lock().unwrap().stored.release.clone(),
            200 => parse_release(&raw)?,
            404 => return Err("尚未找到正式发布版本".into()),
            code => return Err(format!("检查更新失败（HTTP {code}），请稍后重试")),
        };
        let available = release
            .as_ref()
            .filter(|r| {
                update_manifest::stable(&r.version).ok()
                    > update_manifest::stable(env!("CARGO_PKG_VERSION")).ok()
            })
            .cloned();
        {
            let mut data = self.0.data.lock().unwrap();
            data.stored.release = release;
            data.stored.checked = now();
            data.stored.next_attempt = now() + 86400;
            if response.status == 200 {
                data.stored.etag = response.etag;
            }
            data.view.release = available.clone();
            data.view.checked = data.stored.checked;
            data.view.downloaded = 0;
            data.view.skipped = !manual
                && available
                    .as_ref()
                    .is_some_and(|r| r.version == data.stored.skipped);
        }
        self.persist()?;
        if let Some(release) = available {
            self.set_status(app, "available", format!("发现新版本 {}", release.version));
            let notify = !manual
                && !self.view().skipped
                && self.0.data.lock().unwrap().stored.notified != release.version;
            if notify {
                let _ = app
                    .notification()
                    .builder()
                    .title("人大校园网助手有新版本")
                    .body(format!("{} 已发布，打开设置即可更新", release.version))
                    .show();
                self.0.data.lock().unwrap().stored.notified = release.version;
                let _ = self.persist();
            }
        } else {
            self.set_status(app, "current", "当前已是最新版本");
        }
        Ok(())
    }
    fn check(&self, app: &tauri::AppHandle, manual: bool) -> Result<View, String> {
        if self.0.data.lock().unwrap().stored.rate_limited_until > now() {
            return Err("GitHub 暂时限制了请求，请稍后再检查".into());
        }
        if self
            .0
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("更新操作正在进行中".into());
        }
        if self.view().status == "ready" {
            self.0.busy.store(false, Ordering::SeqCst);
            return Ok(self.view());
        }
        self.0.cancel.store(false, Ordering::SeqCst);
        let result = self.check_inner(app, manual);
        if let Err(error) = &result {
            {
                let mut data = self.0.data.lock().unwrap();
                data.stored.next_attempt = data.stored.next_attempt.max(now() + 1800);
            }
            let _ = self.persist();
            self.set_status(app, "error", error.clone());
        }
        self.0.busy.store(false, Ordering::SeqCst);
        result.map(|_| self.view())
    }
    fn download_inner(&self, app: &tauri::AppHandle) -> Result<(), String> {
        let release = self.view().release.ok_or("请先检查更新")?;
        if !release.signed {
            return Err("此版本没有签名更新文件，请从发布页面手动下载".into());
        }
        let directory = update_install::area(&self.0.directory)?;
        self.set_status(app, "downloading", "正在下载更新清单…");
        let raw = self.bytes(
            &update_manifest::release_url(&release.version, "update.json"),
            65536,
        )?;
        let signature = self.bytes(
            &update_manifest::release_url(&release.version, "update.json.sig"),
            256,
        )?;
        let manifest = update_manifest::verify(&raw, &signature, &release.version)?;
        if manifest.windows_x86_64.size != release.size {
            return Err("发布文件与签名清单不匹配".into());
        }
        fs::write(directory.join("update.json"), &raw)
            .and_then(|_| fs::write(directory.join("update.json.sig"), &signature))
            .map_err(|_| "无法保存更新清单")?;
        let part = directory.join("download.part");
        let mut file =
            File::create(&part).map_err(|_| "无法保存更新文件，请检查目录权限和磁盘空间")?;
        self.set_status(app, "downloading", "正在下载更新…");
        let direct = matches!(self.view().preferences.network, Network::Direct);
        let mut last = Instant::now();
        let response = update_network::get(
            &manifest.windows_x86_64.url,
            direct,
            None,
            manifest.windows_x86_64.size,
            &self.0.cancel,
            &mut file,
            |received| {
                self.0.data.lock().unwrap().view.downloaded = received;
                if last.elapsed() > Duration::from_millis(150) {
                    self.emit(app);
                    last = Instant::now();
                }
            },
        )?;
        if response.status != 200 {
            return Err(format!("下载失败（HTTP {}）", response.status));
        }
        file.flush()
            .and_then(|_| file.sync_all())
            .map_err(|_| "无法保存更新文件")?;
        drop(file);
        if self.0.cancel.load(Ordering::Relaxed) {
            return Err("下载已取消".into());
        }
        self.set_status(app, "verifying", "正在校验更新文件…");
        update_manifest::verify_file(&part, &manifest.windows_x86_64)?;
        fs::rename(part, directory.join("new.exe")).map_err(|_| "无法保存已校验的更新文件")?;
        self.set_status(app, "ready", "更新已下载，可重启更新");
        Ok(())
    }
    fn download(&self, app: &tauri::AppHandle) -> Result<View, String> {
        if self
            .0
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("更新操作正在进行中".into());
        }
        self.0.cancel.store(false, Ordering::SeqCst);
        let result = self.download_inner(app);
        if let Err(error) = &result {
            let _ = fs::remove_file(self.0.directory.join(".updates/download.part"));
            if self.0.cancel.load(Ordering::Relaxed) {
                self.set_status(app, "available", "下载已取消，可重新下载");
            } else {
                self.set_status(app, "error", error.clone());
            }
        }
        self.0.busy.store(false, Ordering::SeqCst);
        if self.0.cancel.load(Ordering::Relaxed) {
            Ok(self.view())
        } else {
            result.map(|_| self.view())
        }
    }
}

fn retry_time(after: Option<&str>, reset: Option<&str>, timestamp: i64) -> i64 {
    let seconds = after.and_then(|s| s.parse::<i64>().ok()).unwrap_or(1800);
    (timestamp + seconds)
        .max(reset.and_then(|s| s.parse().ok()).unwrap_or(0))
        .clamp(timestamp + 60, timestamp + 86400)
}
fn parse_release(raw: &[u8]) -> Result<Option<Release>, String> {
    let value: serde_json::Value =
        serde_json::from_slice(raw).map_err(|_| "无法读取 GitHub 版本信息")?;
    if value["draft"] != false || value["prerelease"] != false {
        return Err("GitHub 返回的不是正式版本".into());
    }
    let version = value["tag_name"]
        .as_str()
        .and_then(|v| v.strip_prefix('v'))
        .ok_or("发布版本号无效")?;
    update_manifest::stable(version)?;
    let assets = value["assets"].as_array().ok_or("发布附件无效")?;
    let has = |name: &str| {
        assets.iter().any(|a| {
            a["name"] == name
                && a["state"] == "uploaded"
                && a["browser_download_url"] == update_manifest::release_url(version, name)
        })
    };
    let exe_name = format!("ruc-web-helper_{version}_windows-x86_64.exe");
    let size = assets
        .iter()
        .find(|a| a["name"] == exe_name)
        .and_then(|a| a["size"].as_u64())
        .unwrap_or(0);
    let signed = has("update.json")
        && has("update.json.sig")
        && has(&exe_name)
        && size <= update_manifest::MAX_EXE_SIZE
        && size >= 1024;
    Ok(Some(Release {
        version: version.into(),
        notes: value["body"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(16000)
            .collect(),
        published_at: value["published_at"].as_str().unwrap_or("").into(),
        size,
        signed,
    }))
}

#[tauri::command]
pub fn get_update_state(updater: tauri::State<'_, Updater>) -> View {
    updater.view()
}
#[tauri::command]
pub fn set_update_preferences(
    app: tauri::AppHandle,
    updater: tauri::State<'_, Updater>,
    preferences: Preferences,
) -> Result<View, String> {
    if updater.0.busy.load(Ordering::SeqCst) {
        return Err("请等待当前更新操作完成".into());
    }
    let previous = updater.0.data.lock().unwrap().stored.preferences.clone();
    {
        let mut data = updater.0.data.lock().unwrap();
        data.stored.preferences = preferences.clone();
        data.view.preferences = preferences;
    }
    if let Err(error) = updater.persist() {
        let mut data = updater.0.data.lock().unwrap();
        data.stored.preferences = previous.clone();
        data.view.preferences = previous;
        return Err(error);
    }
    updater.emit(&app);
    Ok(updater.view())
}
#[tauri::command]
pub async fn check_update(
    app: tauri::AppHandle,
    updater: tauri::State<'_, Updater>,
) -> Result<View, String> {
    let updater = updater.inner().clone();
    tauri::async_runtime::spawn_blocking(move || updater.check(&app, true))
        .await
        .map_err(|_| "检查更新任务失败".to_owned())?
}
#[tauri::command]
pub async fn download_update(
    app: tauri::AppHandle,
    updater: tauri::State<'_, Updater>,
) -> Result<View, String> {
    let updater = updater.inner().clone();
    tauri::async_runtime::spawn_blocking(move || updater.download(&app))
        .await
        .map_err(|_| "下载更新任务失败".to_owned())?
}
#[tauri::command]
pub fn cancel_update(updater: tauri::State<'_, Updater>) {
    updater.0.cancel.store(true, Ordering::SeqCst);
}
#[tauri::command]
pub fn skip_update(
    app: tauri::AppHandle,
    updater: tauri::State<'_, Updater>,
) -> Result<View, String> {
    {
        let mut data = updater.0.data.lock().unwrap();
        data.stored.skipped = data
            .view
            .release
            .as_ref()
            .ok_or("没有可忽略的更新")?
            .version
            .clone();
        data.view.skipped = true;
    }
    updater.persist()?;
    updater.emit(&app);
    Ok(updater.view())
}
#[tauri::command]
pub fn install_update(
    app: tauri::AppHandle,
    updater: tauri::State<'_, Updater>,
) -> Result<(), String> {
    if updater.view().status != "ready" {
        return Err("请先下载并校验更新".into());
    }
    if updater
        .0
        .busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("更新操作正在进行中".into());
    }
    let result = (|| {
        let executable = std::env::current_exe().map_err(|_| "无法定位程序")?;
        let release = updater.view().release.ok_or("更新信息已失效")?;
        update_install::prepare(&updater.0.directory, &executable, &release.version)
    })();
    match result {
        Ok(()) => {
            updater.set_status(&app, "installing", "正在重启并更新…");
            app.exit(0);
            Ok(())
        }
        Err(error) => {
            updater.0.busy.store(false, Ordering::SeqCst);
            updater.set_status(&app, "error", error.clone());
            Err(error)
        }
    }
}
#[tauri::command]
pub fn open_update_release(
    app: tauri::AppHandle,
    updater: tauri::State<'_, Updater>,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let version = updater
        .view()
        .release
        .map(|r| r.version)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into());
    update_manifest::stable(&version)?;
    app.opener()
        .open_url(
            format!(
                "https://github.com/{}/releases/tag/v{version}",
                update_manifest::REPOSITORY
            ),
            None::<&str>,
        )
        .map_err(|_| "无法打开发布页面".into())
}
pub fn start_worker(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(30));
        loop {
            let updater = app.state::<Updater>();
            let due = {
                let data = updater.0.data.lock().unwrap();
                data.stored.preferences.automatic && data.stored.next_attempt <= now()
            };
            if due && !matches!(updater.view().status.as_str(), "ready" | "installing") {
                let _ = updater.check(&app, false);
            }
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_rate_limits() {
        let stored: Stored = serde_json::from_str("{}").unwrap();
        assert!(stored.preferences.automatic);
        assert!(matches!(stored.preferences.network, Network::System));
        assert_eq!(retry_time(Some("120"), None, 1000), 1120);
        assert_eq!(retry_time(None, Some("5000"), 1000), 5000);
        assert_eq!(retry_time(Some("999999"), None, 1000), 87400);
    }
    #[test]
    fn old_unsigned_releases_do_not_enable_downloads() {
        let raw =
            br#"{"draft":false,"prerelease":false,"tag_name":"v1.0.0","assets":[],"body":"old"}"#;
        assert!(!parse_release(raw).unwrap().unwrap().signed);
        let raw = br#"{"draft":false,"prerelease":true,"tag_name":"v1.2.0-beta.1","assets":[]}"#;
        assert!(parse_release(raw).is_err());
    }
}
