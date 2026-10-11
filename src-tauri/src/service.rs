use crate::{
    credentials,
    portal::{Portal, Snapshot},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub startup: bool,
    pub auto: bool,
    pub wake: bool,
    pub notice: bool,
    pub theme: ThemePreference,
}
#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    System,
}
impl ThemePreference {
    pub fn native(self) -> Option<tauri::Theme> {
        match self {
            Self::Light => Some(tauri::Theme::Light),
            Self::Dark => Some(tauri::Theme::Dark),
            Self::System => None,
        }
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            startup: false,
            auto: false,
            wake: true,
            notice: true,
            theme: ThemePreference::System,
        }
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;
    #[test]
    fn theme_settings_preserve_old_configuration() {
        let old: Settings = serde_json::from_str(r#"{"startup":true,"auto":true,"wake":false,"notice":true}"#).unwrap();
        assert_eq!(old.theme, ThemePreference::System);
        assert!(old.startup && old.auto && !old.wake && old.notice);
        for (text, expected) in [("light", ThemePreference::Light), ("dark", ThemePreference::Dark), ("system", ThemePreference::System)] {
            let settings: Settings = serde_json::from_value(serde_json::json!({"theme":text})).unwrap();
            assert_eq!(settings.theme, expected);
            assert_eq!(serde_json::to_value(&settings).unwrap()["theme"], text);
        }
        assert!(serde_json::from_value::<Settings>(serde_json::json!({"theme":"invalid"})).is_err());
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub settings: Settings,
    pub username: String,
    pub has_credentials: bool,
    pub paused: bool,
    pub automatic_error: String,
}

pub struct Service {
    pub portal: tokio::sync::Mutex<Portal>,
    settings: Mutex<Settings>,
    path: PathBuf,
    credential_path: PathBuf,
    device_names: crate::device_names::DeviceNames,
    paused: AtomicBool,
    blocked: Mutex<String>,
}

impl Service {
    pub fn new(directory: PathBuf, legacy: PathBuf) -> Result<Self, String> {
        let path = directory.join("settings.json");
        if !path.exists() && legacy.exists() {
            std::fs::copy(&legacy, &path).map_err(|_| "无法迁移设置，请将软件放在可写文件夹")?;
        }
        let credential_path = directory.join("account.enc");
        let migration_error = credentials::migrate_legacy(&credential_path)
            .err()
            .unwrap_or_default();
        let settings = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "设置文件损坏，请检查程序目录中的 settings.json")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(_) => return Err("无法读取本机设置文件".into()),
        };
        let service = Self {
            portal: tokio::sync::Mutex::new(Portal::new()?),
            settings: Mutex::new(settings),
            path,
            credential_path,
            device_names: crate::device_names::DeviceNames::new(directory.join("devices.json")),
            paused: AtomicBool::new(false),
            blocked: Mutex::new(migration_error),
        };
        service.write(&service.settings())?;
        Ok(service)
    }
    pub(crate) fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
    fn name_snapshot(&self, mut snapshot: Snapshot) -> Snapshot {
        match self.device_names.merge(
            &snapshot.username,
            &snapshot.devices,
            &[],
            None,
            snapshot.warnings.is_empty(),
        ) {
            Ok((devices, months)) => {
                snapshot.devices = devices;
                snapshot.device_months = months;
            }
            Err(error) => {
                snapshot.warnings.push(error);
            }
        }
        snapshot
    }
    fn write(&self, settings: &Settings) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| "无法创建设置目录")?;
        }
        let bytes = serde_json::to_vec_pretty(settings).map_err(|_| "无法编码设置")?;
        let temporary = self.path.with_extension("tmp");
        std::fs::write(&temporary, bytes)
            .map_err(|_| "无法写入程序目录，请将软件放在可写文件夹")?;
        std::fs::rename(&temporary, &self.path).map_err(|_| "无法更新设置文件")?;
        *self.settings.lock().unwrap() = settings.clone();
        Ok(())
    }
    fn preferences(&self, app: &tauri::AppHandle) -> Result<Preferences, String> {
        let mut settings = self.settings();
        settings.startup = app
            .autolaunch()
            .is_enabled()
            .map_err(|_| "无法读取启动项状态")?;
        let (account, credential_error) = match credentials::load(&self.credential_path) {
            Ok(account) => (account, String::new()),
            Err(error) => (None, error),
        };
        settings.auto &= account.is_some();
        Ok(Preferences {
            settings,
            username: account
                .as_ref()
                .map(|a| a.username.clone())
                .unwrap_or_default(),
            has_credentials: account.is_some(),
            paused: self.paused.load(Ordering::SeqCst),
            automatic_error: if credential_error.is_empty() {
                self.blocked.lock().unwrap().clone()
            } else {
                credential_error
            },
        })
    }
}

#[tauri::command]
pub fn get_preferences(
    app: tauri::AppHandle,
    state: State<'_, Service>,
) -> Result<Preferences, String> {
    state.preferences(&app)
}

#[tauri::command]
pub async fn set_preferences(
    app: tauri::AppHandle,
    state: State<'_, Service>,
    settings: Settings,
) -> Result<Preferences, String> {
    let _guard = state.portal.lock().await;
    let old_settings = state.settings();
    if settings.auto && credentials::load(&state.credential_path)?.is_none() {
        return Err("请先在设置中保存校园账号和密码".into());
    }
    let previous = app
        .autolaunch()
        .is_enabled()
        .map_err(|_| "无法读取启动项状态")?;
    if settings.startup != previous {
        if settings.startup {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        }
        .map_err(|_| "无法修改 Windows 启动项")?;
    }
    if let Err(error) = state.write(&settings) {
        if previous {
            let _ = app.autolaunch().enable();
        } else {
            let _ = app.autolaunch().disable();
        }
        return Err(error);
    }
    if old_settings.auto != settings.auto {
        state.paused.store(!settings.auto, Ordering::SeqCst);
        state.blocked.lock().unwrap().clear();
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_theme(settings.theme.native());
    }
    state.preferences(&app)
}

#[tauri::command]
pub async fn save_account(
    app: tauri::AppHandle,
    state: State<'_, Service>,
    username: String,
    password: String,
) -> Result<Preferences, String> {
    let _guard = state.portal.lock().await;
    credentials::update(&state.credential_path, username, password)?;
    state.blocked.lock().unwrap().clear();
    state.preferences(&app)
}

#[tauri::command]
pub async fn forget_account(
    app: tauri::AppHandle,
    state: State<'_, Service>,
) -> Result<Preferences, String> {
    state.paused.store(true, Ordering::SeqCst);
    let _guard = state.portal.lock().await;
    credentials::delete(&state.credential_path)?;
    let mut settings = state.settings();
    settings.auto = false;
    state.write(&settings)?;
    state.blocked.lock().unwrap().clear();
    state.preferences(&app)
}

#[tauri::command]
pub async fn get_snapshot(state: State<'_, Service>) -> Result<Snapshot, String> {
    Ok(state.name_snapshot(state.portal.lock().await.snapshot().await?))
}

#[tauri::command]
pub async fn connect_network(state: State<'_, Service>) -> Result<Snapshot, String> {
    let mut portal = state.portal.lock().await;
    let account =
        credentials::load(&state.credential_path)?.ok_or("请先在设置中保存校园账号和密码")?;
    let snapshot = portal.login(&account).await?;
    state.paused.store(false, Ordering::SeqCst);
    state.blocked.lock().unwrap().clear();
    Ok(state.name_snapshot(snapshot))
}

#[tauri::command]
pub async fn disconnect_network(state: State<'_, Service>) -> Result<Snapshot, String> {
    // Pause before waiting for an in-flight request, so the worker cannot reconnect.
    state.paused.store(true, Ordering::SeqCst);
    Ok(state.name_snapshot(state.portal.lock().await.logout().await?))
}

#[tauri::command]
pub async fn drop_device(state: State<'_, Service>, id: i64) -> Result<Snapshot, String> {
    Ok(state.name_snapshot(state.portal.lock().await.drop_device(id).await?))
}

#[tauri::command]
pub async fn get_records(
    state: State<'_, Service>,
    year: u32,
    month: u32,
    page: u32,
) -> Result<serde_json::Value, String> {
    let mut portal = state.portal.lock().await;
    let mut data = portal.records(year, month, page).await?;
    let username = data["username"].as_str().ok_or("未取得当前账号")?;
    let (devices, months) = state.device_names.merge(
        username,
        data["devices"].as_array().ok_or("设备数据格式无效")?,
        data["items"].as_array().ok_or("记录数据格式无效")?,
        None,
        data["warnings"].as_array().is_some_and(|a| a.is_empty()),
    )?;
    data["devices"] = serde_json::json!(devices);
    data["deviceMonths"] = serde_json::json!(months);
    Ok(data)
}

#[tauri::command]
pub async fn get_traffic_data(
    state: State<'_, Service>,
    year: u32,
    month: u32,
) -> Result<serde_json::Value, String> {
    let mut portal = state.portal.lock().await;
    let mut data = portal.traffic_data(year, month).await?;
    let username = data["username"].as_str().ok_or("未取得当前账号")?;
    let (devices, months) = state.device_names.merge(
        username,
        data["devices"].as_array().ok_or("设备数据格式无效")?,
        data["items"].as_array().ok_or("记录数据格式无效")?,
        Some(format!("{year:04}-{month:02}")),
        data["warnings"].as_array().is_some_and(|a| a.is_empty()),
    )?;
    let names = state.device_names.get(username)?;
    data["deviceNames"] = serde_json::json!(names);
    data["devices"] = serde_json::json!(devices);
    data["deviceMonths"] = serde_json::json!(months);
    Ok(data)
}

#[tauri::command]
pub async fn set_device_ignored(
    state: State<'_, Service>,
    mac: String,
    ignored: bool,
    username: String,
) -> Result<Snapshot, String> {
    let mut portal = state.portal.lock().await;
    let snapshot = portal.snapshot().await?;
    if snapshot.username.is_empty() || snapshot.username != username {
        return Err("当前账号已变化，请刷新后重试".into());
    }
    state.device_names.set_ignored(&username, &mac, ignored)?;
    Ok(state.name_snapshot(snapshot))
}

#[tauri::command]
pub async fn set_device_name(
    state: State<'_, Service>,
    mac: String,
    name: String,
    username: String,
) -> Result<serde_json::Value, String> {
    let mut portal = state.portal.lock().await;
    let snapshot = portal.snapshot().await?;
    if snapshot.username.is_empty() || snapshot.username != username {
        return Err("当前账号已变化，请刷新后重新命名".into());
    }
    let names = state.device_names.set(&snapshot.username, &mac, &name)?;
    Ok(serde_json::json!({"snapshot":state.name_snapshot(snapshot),"deviceNames":names}))
}

#[tauri::command]
pub fn open_official(app: tauri::AppHandle) -> Result<(), String> {
    app.opener()
        .open_url("https://go.ruc.edu.cn/", None::<&str>)
        .map_err(|_| "无法打开系统浏览器".into())
}

fn report(app: &tauri::AppHandle, message: &str) {
    let _ = app.emit("network-attention", message);
    let _ = app
        .notification()
        .builder()
        .title("人大校园网助手")
        .body(message)
        .show();
}

pub fn start_worker(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last = Instant::now();
        let mut delay = 15u64;
        let mut low_notified = false;
        loop {
            tokio::time::sleep(Duration::from_secs(delay)).await;
            let state = app.state::<Service>();
            let settings = state.settings();
            let resumed = last.elapsed() > Duration::from_secs(delay + 60);
            last = Instant::now();
            if resumed && !settings.wake && settings.auto {
                state.paused.store(true, Ordering::SeqCst);
                let _ = app.emit(
                    "network-attention",
                    "已从休眠恢复，自动恢复连接已关闭，请手动连接",
                );
            }
            if !settings.auto
                || state.paused.load(Ordering::SeqCst)
                || !state.blocked.lock().unwrap().is_empty()
            {
                delay = 15;
                continue;
            }
            let account = match credentials::load(&state.credential_path) {
                Ok(Some(account)) => account,
                Ok(None) => {
                    delay = 30;
                    continue;
                }
                Err(error) => {
                    *state.blocked.lock().unwrap() = error.clone();
                    report(&app, &error);
                    continue;
                }
            };
            let mut portal = state.portal.lock().await;
            if !state.settings().auto || state.paused.load(Ordering::SeqCst) {
                continue;
            }
            match portal.snapshot().await {
                Ok(snapshot) => {
                    let snapshot = state.name_snapshot(snapshot);
                    delay = 30;
                    if !snapshot.username.is_empty() && snapshot.username != account.username {
                        let error = "当前在线账号与保存账号不同，已暂停自动登录";
                        *state.blocked.lock().unwrap() = error.into();
                        report(&app, error);
                        continue;
                    }
                    if snapshot.authenticated {
                        if settings.notice
                            && snapshot
                                .remain_bytes
                                .is_some_and(|n| (0.0..5_000_000_000.0).contains(&n))
                            && !low_notified
                        {
                            report(&app, "剩余可用流量不足 5 GB");
                            low_notified = true;
                        }
                        if snapshot
                            .remain_bytes
                            .is_some_and(|n| n >= 5_000_000_000.0 || n < 0.0)
                        {
                            low_notified = false;
                        }
                        let _ = app.emit("network-snapshot", &snapshot);
                    } else if snapshot.status != "offline" {
                        *state.blocked.lock().unwrap() = snapshot.message.clone();
                        report(&app, &snapshot.message);
                        let _ = app.emit("network-snapshot", &snapshot);
                    } else if !state.paused.load(Ordering::SeqCst) {
                        match portal.login(&account).await {
                            Ok(snapshot) => {
                                let snapshot = state.name_snapshot(snapshot);
                                let _ = app.emit("network-snapshot", &snapshot);
                            }
                            Err(error) if error.starts_with("网络") => {
                                delay = (delay * 2).min(300);
                            }
                            Err(error) => {
                                *state.blocked.lock().unwrap() = error.clone();
                                report(&app, &error);
                            }
                        }
                    }
                }
                Err(_) => {
                    delay = (delay * 2).min(300);
                }
            }
        }
    });
}
