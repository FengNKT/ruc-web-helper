use super::*;

// Each startup/resume task waits for networking for up to five minutes.
// Ordinary observation never schedules a login or takes over an online session.
pub fn start_worker(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut delay = 1;
        let mut low_account = String::new();
        loop {
            let _ = tokio::time::timeout(
                Duration::from_secs(delay),
                crate::power::EVENTS.changed.notified(),
            )
            .await;
            let state = app.state::<Service>();
            let settings = state.settings();
            if crate::power::EVENTS.take()
                && settings.auto
                && credentials::load_automatic(&state.credential_path).is_ok_and(|a| a.is_some())
            {
                state
                    .automatic
                    .begin(automatic::Trigger::Resume, Instant::now());
                state.blocked.lock().unwrap().clear();
            }
            if !settings.auto {
                state.automatic.cancel();
            }
            if let Some(task) = state.automatic.current() {
                if Instant::now() >= task.deadline && state.automatic.finish(task) {
                    let error = task.trigger.timeout_message();
                    *state.blocked.lock().unwrap() = error.into();
                    report(&app, error);
                }
            }
            let mut portal = state.portal.lock().await;
            let pending = state.automatic.current();
            let snapshot = match portal.snapshot().await {
                Ok(snapshot) => snapshot,
                Err(_) => {
                    delay = if pending.is_some() { 10 } else { 30 };
                    continue;
                }
            };
            delay = 30;
            if snapshot.authenticated {
                if let Some(task) = pending {
                    if state.automatic.finish(task) {
                        state.blocked.lock().unwrap().clear();
                    }
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
            } else if let Some(task) = pending.filter(|task| state.automatic.is_current(*task)) {
                let account = match credentials::load_automatic(&state.credential_path) {
                    Ok(Some(account)) => Some(account),
                    Ok(None) => {
                        state.automatic.finish(task);
                        None
                    }
                    Err(error) => {
                        state.automatic.finish(task);
                        *state.blocked.lock().unwrap() = error.clone();
                        report(&app, &error);
                        None
                    }
                };
                if let Some(account) = account {
                    // A manual disconnect may have arrived during the status query.
                    if state.automatic.is_current(task) && Instant::now() < task.deadline {
                        let result = portal.login(&account).await;
                        let result =
                            state.record_login_result(&account, portal.password_status, result);
                        match result {
                            Ok(after) => {
                                state.automatic.finish(task);
                                state.blocked.lock().unwrap().clear();
                                let _ = app.emit("network-snapshot", state.name_snapshot(after));
                                let _ = app.emit("preferences-changed", ());
                                continue;
                            }
                            Err(error) if error.starts_with("网络") => delay = 10,
                            Err(error) => {
                                state.automatic.finish(task);
                                *state.blocked.lock().unwrap() = error.clone();
                                report(&app, &error);
                                let _ = app.emit("preferences-changed", ());
                            }
                        }
                    }
                }
            }
            let _ = app.emit("network-snapshot", state.name_snapshot(snapshot));
        }
    });
}
