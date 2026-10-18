use super::*;

// A single startup task may wait for campus networking for up to five minutes.
// Once connected, cancelled, or rejected, only observation and reminders remain.
pub fn start_worker(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let deadline = Instant::now() + Duration::from_secs(300);
        let mut delay = 1;
        let mut low_account = String::new();
        loop {
            tokio::time::sleep(Duration::from_secs(delay)).await;
            let state = app.state::<Service>();
            let settings = state.settings();
            if state.startup_pending.load(Ordering::SeqCst) && Instant::now() >= deadline {
                state.startup_pending.store(false, Ordering::SeqCst);
                let error = "启动登录未完成：等待校园网络超时，请检查网络后手动登录";
                *state.blocked.lock().unwrap() = error.into();
                report(&app, error);
            }
            let mut portal = state.portal.lock().await;
            let pending = state.startup_pending.load(Ordering::SeqCst) && state.settings().auto;
            let snapshot = match portal.snapshot().await {
                Ok(snapshot) => snapshot,
                Err(_) => {
                    delay = if pending { 10 } else { 30 };
                    continue;
                }
            };
            delay = 30;
            if snapshot.authenticated {
                if pending {
                    state.startup_pending.store(false, Ordering::SeqCst);
                    state.blocked.lock().unwrap().clear();
                }
                if settings.notice
                    && snapshot
                        .remain_bytes
                        .is_some_and(|n| (0.0..5_000_000_000.0).contains(&n))
                {
                    if low_account != snapshot.username {
                        report(&app, "剩余可用流量不足 5 GB");
                        low_account = snapshot.username.clone();
                    }
                } else {
                    low_account.clear();
                }
            } else if pending && state.startup_pending.load(Ordering::SeqCst) {
                let account = match credentials::load_automatic(&state.credential_path) {
                    Ok(Some(account)) => Some(account),
                    Ok(None) => {
                        state.startup_pending.store(false, Ordering::SeqCst);
                        None
                    }
                    Err(error) => {
                        state.startup_pending.store(false, Ordering::SeqCst);
                        *state.blocked.lock().unwrap() = error.clone();
                        report(&app, &error);
                        None
                    }
                };
                if let Some(account) = account {
                    // A manual disconnect may have arrived during the status query.
                    if state.startup_pending.load(Ordering::SeqCst) && Instant::now() < deadline {
                        let result = portal.login(&account).await;
                        let result =
                            state.record_login_result(&account, portal.password_status, result);
                        match result {
                            Ok(after) => {
                                state.startup_pending.store(false, Ordering::SeqCst);
                                state.blocked.lock().unwrap().clear();
                                let _ = app.emit("network-snapshot", state.name_snapshot(after));
                                let _ = app.emit("preferences-changed", ());
                                continue;
                            }
                            Err(error) if error.starts_with("网络") => delay = 10,
                            Err(error) => {
                                state.startup_pending.store(false, Ordering::SeqCst);
                                *state.blocked.lock().unwrap() = error.clone();
                                report(&app, &error);
                            }
                        }
                    }
                }
            }
            let _ = app.emit("network-snapshot", state.name_snapshot(snapshot));
        }
    });
}
