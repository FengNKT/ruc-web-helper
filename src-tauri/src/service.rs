use crate::{
    credentials,
    portal::{Portal, Snapshot},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
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
    pub notice: bool,
    pub onboarding_done: bool,
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
            startup: true,
            auto: true,
            notice: true,
            onboarding_done: false,
            theme: ThemePreference::System,
        }
    }
}

fn configure_initial_startup(
    settings_path: &Path,
    settings: &Settings,
    enable: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    // Migrated/existing settings (including an explicit opt-out) never register
    // again. Do this before saving first-use settings so failures can be retried.
    if !settings_path.exists() && settings.startup {
        enable()?;
    }
    Ok(())
}

#[cfg(test)]
mod theme_tests {
    use super::*;
    #[test]
    fn theme_settings_preserve_old_configuration() {
        assert!(Settings::default().startup);
        assert!(
            !serde_json::from_str::<Settings>(r#"{"startup":false}"#)
                .unwrap()
                .startup
        );
        assert!(Settings::default().auto);
        assert!(serde_json::from_str::<Settings>("{}").unwrap().auto);
        assert!(
            !serde_json::from_str::<Settings>(r#"{"auto":false}"#)
                .unwrap()
                .auto
        );
        let old: Settings =
            serde_json::from_str(r#"{"startup":true,"auto":true,"wake":false,"notice":true}"#)
                .unwrap();
        assert_eq!(old.theme, ThemePreference::System);
        assert!(old.startup && old.auto && old.notice);
        assert!(!old.onboarding_done);
        for (text, expected) in [
            ("light", ThemePreference::Light),
            ("dark", ThemePreference::Dark),
            ("system", ThemePreference::System),
        ] {
            let settings: Settings =
                serde_json::from_value(serde_json::json!({"theme":text})).unwrap();
            assert_eq!(settings.theme, expected);
            assert_eq!(serde_json::to_value(&settings).unwrap()["theme"], text);
        }
        assert!(
            serde_json::from_value::<Settings>(serde_json::json!({"theme":"invalid"})).is_err()
        );
    }
    #[test]
    fn first_launch_registers_startup_but_existing_choices_are_respected() {
        let root = std::env::temp_dir().join(format!("ruc-startup-default-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        let calls = std::cell::Cell::new(0);
        let enable = || {
            calls.set(calls.get() + 1);
            Ok(())
        };
        let settings = Settings::default();
        configure_initial_startup(&path, &settings, enable).unwrap();
        assert_eq!(calls.get(), 1);
        // Existing installs and Windows-level disablement are not overwritten.
        std::fs::write(&path, br#"{"startup":false}"#).unwrap();
        configure_initial_startup(&path, &settings, enable).unwrap();
        assert_eq!(calls.get(), 1);
        std::fs::remove_file(&path).unwrap();
        assert!(
            configure_initial_startup(&path, &settings, || Err("registration denied".into()))
                .is_err()
        );
        assert!(
            !path.exists(),
            "A failed registration does not consume the first-run attempt"
        );
        configure_initial_startup(&path, &settings, enable).unwrap();
        assert_eq!(calls.get(), 2);
        std::fs::remove_dir(root).unwrap();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub settings: Settings,
    pub username: String,
    pub has_credentials: bool,
    pub accounts: Vec<credentials::AccountSummary>,
    pub selected_id: Option<String>,
    pub automatic_id: Option<String>,
    pub credential_error: String,
    pub automatic_error: String,
}

pub struct Service {
    pub portal: tokio::sync::Mutex<Portal>,
    settings: Mutex<Settings>,
    path: PathBuf,
    credential_path: PathBuf,
    device_names: crate::device_names::DeviceNames,
    startup_pending: AtomicBool,
    blocked: Mutex<String>,
}

impl Service {
    pub fn new(
        directory: PathBuf,
        legacy: PathBuf,
        enable_startup: impl FnOnce() -> Result<(), String>,
    ) -> Result<Self, String> {
        let path = directory.join("settings.json");
        let import_legacy = crate::storage::legacy_import_allowed(&directory);
        if import_legacy && !path.exists() && legacy.exists() {
            std::fs::copy(&legacy, &path).map_err(|_| "无法迁移设置，请将软件放在可写文件夹")?;
        }
        let credential_path = directory.join("account.enc");
        let migration_error = if import_legacy {
            credentials::migrate_legacy(&credential_path)
        } else {
            Ok(())
        }
        .err()
        .unwrap_or_default();
        let mut settings: Settings = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "设置文件损坏，请检查用户数据目录中的 settings.json")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(_) => return Err("无法读取本机设置文件".into()),
        };
        // Existing configured installations do not need the first-use introduction.
        if credentials::load_book(&credential_path).is_ok_and(|b| !b.accounts.is_empty()) {
            settings.onboarding_done = true;
        }
        configure_initial_startup(&path, &settings, enable_startup)?;
        let startup_pending = AtomicBool::new(
            settings.auto
                && credentials::load_automatic(&credential_path).is_ok_and(|a| a.is_some()),
        );
        let service = Self {
            portal: tokio::sync::Mutex::new(Portal::new()?),
            settings: Mutex::new(settings),
            path,
            credential_path,
            device_names: crate::device_names::DeviceNames::new(directory.join("devices.json")),
            startup_pending,
            blocked: Mutex::new(migration_error),
        };
        service.write(&service.settings())?;
        Ok(service)
    }
    pub(crate) fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
    fn record_login_result(
        &self,
        account: &credentials::Account,
        password_status: Option<bool>,
        result: Result<Snapshot, String>,
    ) -> Result<Snapshot, String> {
        if let Some(valid) = password_status {
            if let Err(error) =
                credentials::record_password_status(&self.credential_path, account, valid)
            {
                return match result {
                    Ok(mut snapshot) => {
                        snapshot.warnings.push(format!("无法保存账号状态：{error}"));
                        Ok(snapshot)
                    }
                    Err(original) => Err(format!("{original}；无法保存账号状态：{error}")),
                };
            }
        }
        result
    }
    fn name_snapshot(&self, mut snapshot: Snapshot) -> Snapshot {
        if snapshot.authenticated {
            if let Err(error) = credentials::align_online(&self.credential_path, &snapshot.username)
            {
                snapshot
                    .warnings
                    .push(format!("无法保存当前账号选择：{error}"));
            }
        }
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
            .map_err(|_| "无法写入用户数据目录，请检查当前 Windows 用户的权限")?;
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
        let (book, credential_error) = match credentials::load_book(&self.credential_path) {
            Ok(book) => (book, String::new()),
            Err(error) => (credentials::AccountBook::default(), error),
        };
        let account = book.selected();
        Ok(Preferences {
            settings,
            username: account
                .as_ref()
                .map(|a| a.account.username.clone())
                .unwrap_or_default(),
            has_credentials: account.is_some(),
            accounts: book
                .accounts
                .iter()
                .map(credentials::AccountSummary::from)
                .collect(),
            selected_id: book.selected_id.clone(),
            automatic_id: book.automatic_id.clone(),
            credential_error,
            automatic_error: self.blocked.lock().unwrap().clone(),
        })
    }
}

#[tauri::command]
pub async fn clear_all_data(
    app: tauri::AppHandle,
    state: State<'_, Service>,
    updater: State<'_, crate::updater::Updater>,
) -> Result<(), String> {
    updater.begin_data_clear()?;
    state.startup_pending.store(false, Ordering::SeqCst);
    let _guard = state.portal.lock().await;
    let result = (|| {
        let enabled = app
            .autolaunch()
            .is_enabled()
            .map_err(|_| "无法读取启动项状态")?;
        app.autolaunch()
            .disable()
            .map_err(|_| "无法关闭开机自启，未执行清除")?;
        if let Err(error) = crate::cleanup::schedule() {
            if enabled {
                let _ = app.autolaunch().enable();
            }
            return Err(error);
        }
        Ok(())
    })();
    if result.is_err() {
        updater.abort_data_clear();
        return result;
    }
    app.exit(0);
    Ok(())
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
    if !settings.auto {
        state.startup_pending.store(false, Ordering::SeqCst);
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
    id: Option<String>,
    name: String,
) -> Result<Preferences, String> {
    state.startup_pending.store(false, Ordering::SeqCst);
    let _guard = state.portal.lock().await;
    credentials::upsert(&state.credential_path, id, name, username, password)?;
    state.blocked.lock().unwrap().clear();
    state.preferences(&app)
}

#[tauri::command]
pub async fn set_automatic_account(
    app: tauri::AppHandle,
    state: State<'_, Service>,
    id: String,
) -> Result<Preferences, String> {
    let _guard = state.portal.lock().await;
    credentials::select_automatic(&state.credential_path, &id)?;
    state.preferences(&app)
}

#[tauri::command]
pub async fn forget_account(
    app: tauri::AppHandle,
    state: State<'_, Service>,
    id: String,
) -> Result<Preferences, String> {
    state.startup_pending.store(false, Ordering::SeqCst);
    let _guard = state.portal.lock().await;
    credentials::remove(&state.credential_path, &id)?;
    state.blocked.lock().unwrap().clear();
    state.preferences(&app)
}

#[tauri::command]
pub async fn get_snapshot(state: State<'_, Service>) -> Result<Snapshot, String> {
    Ok(state.name_snapshot(state.portal.lock().await.snapshot().await?))
}

#[tauri::command]
pub async fn reset_accounts(
    app: tauri::AppHandle,
    state: State<'_, Service>,
) -> Result<Preferences, String> {
    state.startup_pending.store(false, Ordering::SeqCst);
    let _guard = state.portal.lock().await;
    credentials::reset_with_backup(&state.credential_path)?;
    state.blocked.lock().unwrap().clear();
    state.preferences(&app)
}

#[tauri::command]
pub async fn connect_network(state: State<'_, Service>) -> Result<Snapshot, String> {
    state.startup_pending.store(false, Ordering::SeqCst);
    state.blocked.lock().unwrap().clear();
    let mut portal = state.portal.lock().await;
    let account =
        credentials::load(&state.credential_path)?.ok_or("请先在设置中保存校园账号和密码")?;
    let result = portal.login(&account).await;
    let snapshot = state.record_login_result(&account, portal.password_status, result)?;
    state.blocked.lock().unwrap().clear();
    Ok(state.name_snapshot(snapshot))
}

#[tauri::command]
pub async fn disconnect_network(state: State<'_, Service>) -> Result<Snapshot, String> {
    // Cancel pending startup retries before waiting for an in-flight login.
    state.startup_pending.store(false, Ordering::SeqCst);
    state.blocked.lock().unwrap().clear();
    Ok(state.name_snapshot(state.portal.lock().await.logout().await?))
}

#[tauri::command]
pub async fn switch_account(state: State<'_, Service>, id: String) -> Result<Snapshot, String> {
    state.startup_pending.store(false, Ordering::SeqCst);
    let mut portal = state.portal.lock().await;
    let account = credentials::select(&state.credential_path, &id)?;
    state.blocked.lock().unwrap().clear();
    let result = portal.switch_account(&account).await;
    let result = state.record_login_result(&account, portal.password_status, result);
    result.map(|snapshot| state.name_snapshot(snapshot))
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

#[tauri::command]
pub fn open_privacy_feedback(app: tauri::AppHandle) -> Result<(), String> {
    app.opener()
        .open_url(
            "https://github.com/FengNKT/ruc-web-helper/issues",
            None::<String>,
        )
        .map_err(|_| "无法打开反馈页面".into())
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

mod worker;
pub use worker::start_worker;
