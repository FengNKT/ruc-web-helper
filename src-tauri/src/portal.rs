use crate::{credentials::Account, protocol};
use regex::Regex;
use reqwest::{Client, Method, Response};
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;

const BASE: &str = "https://go.ruc.edu.cn";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub authenticated: bool,
    pub username: String,
    pub ip: String,
    pub status: String,
    pub message: String,
    pub used_bytes: Option<f64>,
    pub remain_bytes: Option<f64>,
    pub package_remain_bytes: Option<f64>,
    pub balance: Option<f64>,
    pub checkout_date: Value,
    pub devices: Vec<Value>,
    pub device_months: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct Portal {
    pub password_status: Option<bool>,
    client: Client,
    ip: String,
    acid: String,
    xsrf: String,
    username: String,
    #[cfg(test)]
    test_base: String,
}

fn network_error(error: reqwest::Error) -> String {
    // Do not expose request URLs: SRun auth puts sensitive values in the query.
    if error.is_timeout() {
        "网络请求超时，请检查校园网连接".into()
    } else {
        "网络连接失败，请确认已接入校园网并检查代理/VPN及证书".into()
    }
}

pub fn parse_response(text: &str) -> Result<Value, String> {
    if let Ok(value) = serde_json::from_str(text) {
        return Ok(value);
    }
    let re = Regex::new(r"(?s)^\s*[A-Za-z_$][\w$]*\((.*)\);?\s*$").unwrap();
    let inner = re
        .captures(text)
        .and_then(|caps| caps.get(1))
        .ok_or("返回的数据格式无法识别")?;
    serde_json::from_str(inner.as_str()).map_err(|_| "未获取到有效数据".into())
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok())
}

pub fn successful(value: &Value) -> bool {
    value["error"].as_str() == Some("ok") || value["code"].as_i64() == Some(0)
}

pub fn error_message(value: &Value) -> String {
    let code = value["ecode"]
        .as_str()
        .or(value["status"].as_str())
        .unwrap_or("");
    match code {
        "E3001" => "流量已用尽，请前往官网充值".into(),
        "E2616" => "账号欠费，请前往官网充值".into(),
        "E2617" => "已达到消费保护限额，请在官网处理".into(),
        "E2606" => "账号尚未激活，请在官网处理".into(),
        _ => {
            // The school's password rejection can be reported by the LDAP
            // adapter without the word "password", and in any detail field.
            let password_rejected = ["error_msg", "message", "ploy_msg", "error"]
                .iter()
                .filter_map(|key| value[*key].as_str())
                .any(|detail| {
                    let detail = detail.to_ascii_lowercase();
                    detail.contains("ldap_bind error") || detail.contains("ldap_bind_error")
                });
            if password_rejected {
                return "数智人大密码错误，请在设置中修改该账号的密码后重试".into();
            }
            let detail = ["error_msg", "message", "ploy_msg", "error"]
                .iter()
                .filter_map(|key| value[*key].as_str())
                .find(|s| !s.is_empty())
                .unwrap_or("请求失败");
            if detail.to_ascii_lowercase().contains("password") || detail.contains("密码") {
                "学工号或数智人大密码错误，请在设置中检查后重试".into()
            } else {
                format!("{}", detail.chars().take(180).collect::<String>())
            }
        }
    }
}

fn require_success(value: &Value) -> Result<(), String> {
    if successful(value) {
        Ok(())
    } else {
        Err(error_message(value))
    }
}

fn offline_response(value: &Value) -> bool {
    ["error", "error_msg", "message"].iter().any(|key| {
        matches!(
            value[*key]
                .as_str()
                .map(|s| s
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_ascii_lowercase())
                .as_deref(),
            Some(
                "not_online"
                    | "notonlineerror"
                    | "not_online_error"
                    | "ip_not_online"
                    | "user not log in"
                    | "user not login"
            )
        )
    })
}

impl Portal {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .no_proxy()
            .cookie_store(true)
            .timeout(Duration::from_secs(12))
            .connect_timeout(Duration::from_secs(5))
            .user_agent(concat!("RUC-Web-Helper/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                let url = attempt.url();
                if attempt.previous().len() >= 5
                    || url.scheme() != "https"
                    || url.host_str() != Some("go.ruc.edu.cn")
                    || url.port_or_known_default() != Some(443)
                {
                    attempt.error("refused unsafe portal redirect")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| "无法初始化 HTTPS 网络客户端")?;
        Ok(Self {
            password_status: None,
            client,
            ip: String::new(),
            acid: String::new(),
            xsrf: String::new(),
            username: String::new(),
            #[cfg(test)]
            test_base: BASE.into(),
        })
    }

    fn url(&self, path: &str) -> String {
        #[cfg(test)]
        let base = &self.test_base;
        #[cfg(not(test))]
        let base = BASE;
        format!("{base}{path}")
    }

    async fn text(&mut self, response: Response) -> Result<String, String> {
        let response = response.error_for_status().map_err(network_error)?;
        if let Some(xsrf) = response
            .headers()
            .get("Set-Xsrf")
            .and_then(|v| v.to_str().ok())
        {
            self.xsrf = xsrf.into();
        }
        if response.content_length().unwrap_or(0) > 2_000_000 {
            return Err("返回的数据过大".into());
        }
        response.text().await.map_err(network_error)
    }

    async fn initialize(&mut self) -> Result<(), String> {
        let response = self
            .client
            .get(self.url("/index_6.html"))
            .send()
            .await
            .map_err(network_error)?;
        let acid = response
            .url()
            .query_pairs()
            .find(|(key, _)| key == "ac_id")
            .map(|(_, value)| value.into_owned());
        let html = self.text(response).await?;
        self.ip = Regex::new(r#"\bip\s*:\s*"([^"]+)""#)
            .unwrap()
            .captures(&html)
            .map(|v| v[1].to_string())
            .ok_or("无法读取校园网分配的 IP，请打开官网检查")?;
        if self.ip.parse::<std::net::IpAddr>().is_err() {
            return Err("登录页面返回了无效 IP".into());
        }
        self.acid = acid
            .filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()))
            .ok_or("登录入口参数已变化，请打开官网检查")?;
        self.xsrf = Regex::new(r#"<meta\s+name=["']_xsrf["']\s+content=["']([^"']+)"#)
            .unwrap()
            .captures(&html)
            .map(|v| v[1].to_string())
            .unwrap_or_default();
        Ok(())
    }

    async fn api(
        &mut self,
        method: Method,
        path: &str,
        params: &[(String, String)],
        authenticated: bool,
    ) -> Result<Value, String> {
        let mut request = self.client.request(method.clone(), self.url(path));
        if method == Method::GET {
            request = request.query(params);
            // Match the school's JSONP client for CGI authentication endpoints.
            // Some operations do not return a JSON body without an explicit callback.
            if path.starts_with("/cgi-bin/") {
                request = request.query(&[("callback", "rucWebHelperCallback")]);
            }
        } else {
            request = request.form(params);
        }
        if authenticated {
            if self.username.is_empty() || self.xsrf.is_empty() {
                return Err("请先登录校园网，再查询账号信息".into());
            }
            request = request
                .header("User-Auth", &self.username)
                .header("X-Xsrftoken", &self.xsrf);
        }
        let response = request.send().await.map_err(network_error)?;
        let body = self.text(response).await?;
        parse_response(&body).map_err(|error| {
            format!(
                "{}：{error}",
                match path {
                    "/cgi-bin/rad_user_info" => "在线状态响应",
                    "/cgi-bin/get_challenge" => "登录参数响应",
                    "/cgi-bin/srun_portal" => "登录操作响应",
                    _ => "账号查询响应",
                }
            )
        })
    }

    async fn initialize_account_session(&mut self) -> Result<(), String> {
        let response = self
            .client
            .get(self.url("/srun_portal_success"))
            .query(&[("ac_id", self.acid.as_str()), ("theme", "ruc")])
            .send()
            .await
            .map_err(network_error)?;
        let html = self.text(response).await?;
        self.xsrf = Regex::new(r#"<meta\s+name=["']_xsrf["']\s+content=["']([^"']+)"#)
            .unwrap()
            .captures(&html)
            .map(|v| v[1].to_string())
            .ok_or("未取得账号管理会话，请打开官网检查登录状态")?;
        Ok(())
    }

    pub async fn snapshot(&mut self) -> Result<Snapshot, String> {
        self.initialize().await?;
        let value = self
            .api(Method::GET, "/cgi-bin/rad_user_info", &[], false)
            .await?;
        let offline = offline_response(&value);
        if !successful(&value) && !offline {
            return Err(error_message(&value));
        }
        // These are normal pre-login responses, even if status uses another spelling
        // or the response still carries an old username. Never query account APIs then.
        self.username = if offline {
            String::new()
        } else {
            value["user_name"].as_str().unwrap_or("").into()
        };
        let status = if offline {
            "offline"
        } else {
            value["status"].as_str().unwrap_or("unknown")
        }
        .to_string();
        let authenticated = !offline && successful(&value) && status == "E0000";
        let mut snapshot = Snapshot {
            authenticated,
            username: self.username.clone(),
            ip: self.ip.clone(),
            status,
            message: if offline {
                "尚未登录校园网".into()
            } else if authenticated {
                "已登录校园网".into()
            } else if successful(&value) {
                error_message(&value)
            } else {
                "尚未登录校园网".into()
            },
            used_bytes: number(&value["sum_bytes"]),
            remain_bytes: number(&value["remain_bytes"]),
            package_remain_bytes: number(&value["package_remain_bytes"]),
            balance: number(&value["user_balance"]),
            checkout_date: value["checkout_date"].clone(),
            devices: vec![],
            warnings: vec![],
            device_months: vec![],
        };
        if !self.username.is_empty() {
            let devices = match self.initialize_account_session().await {
                Ok(()) => self.devices().await,
                Err(error) => Err(error),
            };
            match devices {
                Ok(devices) => snapshot.devices = devices,
                Err(error) => snapshot.warnings.push(error),
            }
        }
        Ok(snapshot)
    }

    async fn devices(&mut self) -> Result<Vec<Value>, String> {
        let params = vec![("user_name".into(), self.username.clone())];
        let value = self
            .api(Method::GET, "/v1/auth/device/get", &params, true)
            .await?;
        require_success(&value)?;
        Ok(value["data"].as_array().ok_or("设备数据格式无法识别")?.iter().map(|item|json!({
            "id":item["rad_online_id"],"name":item["device_name"],"os":item["os_name"],"mac":item["user_mac"],
            "ip":item["ip"],"ip6":item["ip6"],"online":item["is_online"],"since":item["add_time"],
            "current":item["ip"].as_str()==Some(self.ip.as_str()) || item["ip6"].as_str()==Some(self.ip.as_str())
        })).collect())
    }

    pub async fn switch_account(&mut self, account: &Account) -> Result<Snapshot, String> {
        self.password_status = None;
        let before = self.snapshot().await?;
        if before.authenticated && before.username == account.username {
            return Ok(before);
        }
        if before.authenticated {
            self.logout().await?;
        } else if before.status != "offline" {
            return Err(before.message);
        }
        // login verifies the final identity and refuses to overwrite another session.
        self.login(account).await
    }

    pub async fn login(&mut self, account: &Account) -> Result<Snapshot, String> {
        self.password_status = None;
        let before = self.snapshot().await?;
        if !before.username.is_empty() && before.username != account.username {
            return Err("当前网络已由其他账号登录，请先在官网处理后再连接".into());
        }
        if before.authenticated {
            return Ok(before);
        }
        if before.status != "offline" {
            return Err(before.message);
        }
        let params = vec![
            ("username".into(), account.username.clone()),
            ("ip".into(), self.ip.clone()),
        ];
        let challenge = self
            .api(Method::GET, "/cgi-bin/get_challenge", &params, false)
            .await?;
        require_success(&challenge)?;
        let token = challenge["challenge"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("未获取到登录参数")?;
        let params = protocol::login_parameters(
            &account.username,
            &account.password,
            &self.ip,
            &self.acid,
            token,
        );
        let response = self
            .api(Method::GET, "/cgi-bin/srun_portal", &params, false)
            .await?;
        if let Err(error) = require_success(&response) {
            if error.starts_with("数智人大密码错误")
                || error.starts_with("学工号或数智人大密码错误")
            {
                self.password_status = Some(false);
            }
            return Err(error);
        }
        let snapshot = self.snapshot().await?;
        if snapshot.authenticated && snapshot.username == account.username {
            self.password_status = Some(true);
            Ok(snapshot)
        } else if snapshot.authenticated {
            Err("认证后的在线账号与目标账号不一致，请刷新网络状态后重试".into())
        } else {
            Err(snapshot.message)
        }
    }

    pub async fn logout(&mut self) -> Result<Snapshot, String> {
        let before = self.snapshot().await?;
        if !before.authenticated {
            return Ok(before);
        }
        let params = vec![
            ("action".into(), "logout".into()),
            ("username".into(), self.username.clone()),
            ("ip".into(), self.ip.clone()),
            ("ac_id".into(), self.acid.clone()),
        ];
        let result = self
            .api(Method::GET, "/cgi-bin/srun_portal", &params, false)
            .await
            .and_then(|value| require_success(&value));
        // A logout can take effect before its response is returned or parsed.
        // Confirm the actual state instead of reporting a false failure (or
        // accepting an unparseable response as success without verification).
        match self.snapshot().await {
            Ok(after) if after.status == "offline" => Ok(after),
            Ok(after) => {
                result?;
                Err(if after.authenticated {
                    "本机仍在线，请稍后刷新确认".into()
                } else {
                    after.message
                })
            }
            Err(error) => Err(match result {
                Err(request_error) => format!("{request_error}；无法确认断开后的状态：{error}"),
                Ok(()) => format!("断开请求已提交，但无法确认当前状态：{error}"),
            }),
        }
    }

    pub async fn drop_device(&mut self, id: i64) -> Result<Snapshot, String> {
        let snapshot = self.snapshot().await?;
        let device = snapshot
            .devices
            .iter()
            .find(|d| d["id"].as_i64().or_else(|| d["id"].as_str()?.parse().ok()) == Some(id))
            .ok_or("设备列表已变化，请刷新后重试")?;
        if device["current"].as_bool() == Some(true) {
            return Err("请使用断开连接来下线本机".into());
        }
        if !matches!(&device["online"], Value::Bool(true)) && number(&device["online"]) != Some(1.0)
        {
            return Err("该设备当前未在线，请刷新后重试".into());
        }
        let params = vec![
            ("user_name".into(), self.username.clone()),
            ("drop_type".into(), "radius".into()),
            ("rad_online_id".into(), id.to_string()),
        ];
        let value = self
            .api(Method::POST, "/v1/auth/online_device/drop", &params, true)
            .await?;
        require_success(&value)?;
        self.snapshot().await
    }

    pub async fn records(&mut self, year: u32, month: u32, page: u32) -> Result<Value, String> {
        if !(2020..=2100).contains(&year)
            || !(1..=12).contains(&month)
            || !(1..=10000).contains(&page)
        {
            return Err("无效日期或页码".into());
        }
        let snapshot = self.snapshot().await?;
        if snapshot.username.is_empty() {
            return Err("请先登录校园网，再查看记录".into());
        }
        let (end_year, end_month) = shift_month(year, month, 1);
        let params = vec![
            ("user_name".into(), self.username.clone()),
            ("start_date".into(), format!("{year:04}-{month:02}-01")),
            (
                "end_date".into(),
                format!("{end_year:04}-{end_month:02}-01"),
            ),
            ("page".into(), page.to_string()),
            ("per_page".into(), "10".into()),
        ];
        let value = self
            .api(Method::GET, "/v1/auth/details", &params, true)
            .await?;
        require_success(&value)?;
        Ok(
            json!({"items":value["data"],"total":value["_meta"]["totalCount"],
            "username":snapshot.username,"devices":snapshot.devices,"warnings":snapshot.warnings}),
        )
    }

    pub async fn traffic_data(&mut self, year: u32, month: u32) -> Result<Value, String> {
        if !(2020..=2100).contains(&year) || !(1..=12).contains(&month) {
            return Err("无效统计月份".into());
        }
        let snapshot = self.snapshot().await?;
        if snapshot.username.is_empty() {
            return Err("请先登录校园网，再查看流量统计".into());
        }
        // Extend both sides: the server filters whole sessions, not daily usage.
        let (from_year, from_month) = shift_month(year, month, -1);
        let (until_year, until_month) = shift_month(year, month, 2);
        let from = format!("{from_year:04}-{from_month:02}-01");
        let until = format!("{until_year:04}-{until_month:02}-01");
        let mut items = Vec::new();
        let mut ids = std::collections::HashSet::new();
        use chrono::Datelike;
        let now = chrono::Utc::now() + chrono::Duration::hours(8);
        let current = (now.year() as u32, now.month());
        for offset in [-1, 0, 1] {
            let source = shift_month(year, month, offset);
            if source > current {
                break;
            }
            for row in self.history_month(source.0, source.1, &until).await? {
                let id = record_id(&row)?;
                if ids.insert(id) {
                    items.push(row);
                }
            }
        }
        Ok(
            json!({"items":items,"devices":snapshot.devices,"username":snapshot.username,
            "usedBytes":snapshot.used_bytes,"remainBytes":snapshot.remain_bytes,"checkoutDate":snapshot.checkout_date,
            "warnings":snapshot.warnings,"queryFrom":from,"queryUntil":until}),
        )
    }

    async fn history_month(
        &mut self,
        year: u32,
        month: u32,
        until: &str,
    ) -> Result<Vec<Value>, String> {
        // This endpoint selects a monthly partition from start_date. A single multi-month
        // request silently returns only the first month, so paginate each month separately.
        let from = format!("{year:04}-{month:02}-01");
        let mut items = Vec::new();
        let mut ids = std::collections::HashSet::new();
        let mut expected = None;
        for page in 1..=200 {
            let params = vec![
                ("user_name".into(), self.username.clone()),
                ("start_date".into(), from.clone()),
                ("end_date".into(), until.to_string()),
                ("page".into(), page.to_string()),
                ("per_page".into(), "100".into()),
            ];
            let value = self
                .api(Method::GET, "/v1/auth/details", &params, true)
                .await?;
            require_success(&value)?;
            let total = number(&value["_meta"]["totalCount"])
                .filter(|n| *n >= 0.0 && n.fract() == 0.0)
                .ok_or("未获取到有效记录总数")? as usize;
            if expected.is_some_and(|n| n != total) {
                return Err("记录正在更新，请刷新统计重试".into());
            }
            expected = Some(total);
            let rows = value["data"].as_array().ok_or("上网记录格式无法识别")?;
            let before_count = items.len();
            for row in rows {
                if ids.insert(record_id(row)?) {
                    items.push(row.clone());
                }
            }
            if items.len() == total {
                return Ok(items);
            }
            if items.len() == before_count || items.len() > total {
                return Err("获取的记录不完整，请刷新重试".into());
            }
        }
        Err("记录数量过多，暂时无法完成统计".into())
    }
}

fn record_id(row: &Value) -> Result<String, String> {
    row["detail_id"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| row["detail_id"].as_u64().map(|v| v.to_string()))
        .ok_or("记录缺少编号，无法确认统计完整性".into())
}

fn shift_month(year: u32, month: u32, offset: i32) -> (u32, u32) {
    let absolute = year as i32 * 12 + month as i32 - 1 + offset;
    ((absolute / 12) as u32, (absolute % 12 + 1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expanded_month_boundaries_cross_years() {
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 12, 2), (2027, 2));
    }
    #[test]
    fn accepts_json_and_strict_jsonp_without_evaluation() {
        assert_eq!(parse_response(r#"{"error":"ok"}"#).unwrap()["error"], "ok");
        assert_eq!(
            parse_response(r#"callback_123({"error":"ok"});"#).unwrap()["error"],
            "ok"
        );
        assert!(parse_response(r#"alert(1);callback({"error":"ok"})"#).is_err());
    }
    #[test]
    fn offline_is_not_any_error() {
        assert!(offline_response(&json!({"error":"not_online"})));
        assert!(offline_response(&json!({"error":"user not log in"})));
        assert!(offline_response(&json!({"error":"user not login"})));
        assert!(offline_response(&json!({"error_msg":"user not login"})));
        assert!(offline_response(&json!({"error_msg":" User  not log in "})));
        assert!(offline_response(&json!({"message":"user not log in"})));
        assert!(!offline_response(&json!({"error":"password_error"})));
        assert!(!offline_response(&json!({"error":"network_error"})));
        assert!(!offline_response(&json!({"error":"permission denied"})));
        assert!(!offline_response(
            &json!({"error":"unknown user not log in failure"})
        ));
    }

    #[test]
    fn school_ldap_password_rejection_has_an_actionable_message() {
        let expected = "数智人大密码错误，请在设置中修改该账号的密码后重试";
        for key in ["error_msg", "message", "ploy_msg", "error"] {
            let mut response = json!({"ecode":"E2901","error":"authentication_failed"});
            response[key] = json!("E2901: (Third party 1)bind_user2: ldap_bind error");
            assert_eq!(error_message(&response), expected);
        }
        assert_eq!(
            error_message(&json!({"status":"E2901","message":"LDAP_BIND_ERROR"})),
            expected
        );
        assert_eq!(
            error_message(&json!({"error":"password_error"})),
            "学工号或数智人大密码错误，请在设置中检查后重试"
        );
        // E2901 alone is not proof of a password rejection.
        assert_eq!(
            error_message(
                &json!({"ecode":"E2901","error_msg":"authentication service unavailable"})
            ),
            "authentication service unavailable"
        );
        assert_eq!(
            error_message(&json!({"error":"ldap_server_connect_error"})),
            "ldap_server_connect_error"
        );
    }

    // Exercise the actual HTTP login sequence with fictional credentials. The
    // production build has no endpoint override; no school requests occur here.
    fn fixture(responses: Vec<(&'static str, String)>) -> (Portal, std::thread::JoinHandle<()>) {
        fixture_for_users(responses, "fixture-student", "fixture-student")
    }
    fn fixture_for_users(
        responses: Vec<(&'static str, String)>,
        login_name: &'static str,
        logout_name: &'static str,
    ) -> (Portal, std::thread::JoinHandle<()>) {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            for (expected, response) in responses {
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((s, _)) => break s,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "Expected login request was never sent"
                            );
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => panic!("Fixture server failed"),
                    }
                };
                // Accepted sockets can inherit nonblocking mode on Windows.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let target = first.split_whitespace().nth(1).unwrap();
                let url = reqwest::Url::parse(&format!("http://localhost{target}")).unwrap();
                assert_eq!(url.path(), expected);
                if expected == "/cgi-bin/srun_portal" {
                    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
                    assert_eq!(
                        query.get("username").map(|s| s.as_ref()),
                        Some(
                            if query.get("action").map(|s| s.as_ref()) == Some("login") {
                                login_name
                            } else {
                                logout_name
                            }
                        )
                    );
                    if query.get("action").map(|s| s.as_ref()) == Some("login") {
                        assert!(query
                            .get("password")
                            .is_some_and(|s| s.starts_with("{MD5}")));
                        assert!(query.get("info").is_some_and(|s| s.starts_with("{SRBX1}")));
                        assert!(query.get("chksum").is_some_and(|s| s.len() == 40));
                    } else {
                        assert_eq!(query.get("action").map(|s| s.as_ref()), Some("logout"));
                        assert!(!query.contains_key("password"));
                    }
                }
                if expected.starts_with("/cgi-bin/") {
                    assert!(url
                        .query_pairs()
                        .any(|(key, value)| key == "callback" && value == "rucWebHelperCallback"));
                }
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }
                if expected == "/index_6.html" && url.query().is_none() {
                    write!(stream,"HTTP/1.1 302 Found\r\nLocation: /index_6.html?ac_id=1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    let response = if expected.starts_with("/cgi-bin/") && response.starts_with('{')
                    {
                        format!("rucWebHelperCallback({response});")
                    } else {
                        response
                    };
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.len(),
                        response
                    )
                    .unwrap();
                }
            }
        });
        let mut portal = Portal::new().unwrap();
        portal.test_base = base;
        portal.client = Client::builder()
            .no_proxy()
            .cookie_store(true)
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        (portal, worker)
    }
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }
    #[test]
    fn login_proceeds_from_school_not_logged_in_responses() {
        let html = r#"ip: "10.0.0.2" <meta name="_xsrf" content="fixture-xsrf">"#;
        for offline in [
            json!({"error":"user not login","status":"not_online_error"}),
            json!({"error":"user not log in"}),
            json!({"error":"not_online_error","status":"not_online_error","user_name":"old-account"}),
        ] {
            // Include ac_id in the initial URL just as the school's redirected entry does.
            let responses = vec![
                ("/index_6.html", String::new()),
                ("/index_6.html", html.into()),
                ("/cgi-bin/rad_user_info", offline.to_string()),
                (
                    "/cgi-bin/get_challenge",
                    json!({"error":"ok","challenge":"fictional-challenge"}).to_string(),
                ),
                ("/cgi-bin/srun_portal", json!({"error":"ok"}).to_string()),
                ("/index_6.html", String::new()),
                ("/index_6.html", html.into()),
                (
                    "/cgi-bin/rad_user_info",
                    json!({"error":"ok","status":"E0000","user_name":"fixture-student"})
                        .to_string(),
                ),
                ("/srun_portal_success", html.into()),
                (
                    "/v1/auth/device/get",
                    json!({"code":0,"data":[]}).to_string(),
                ),
            ];
            let (mut portal, worker) = fixture(responses);
            let account = Account {
                username: "fixture-student".into(),
                password: "fictional-password".into(),
            };
            let result = runtime().block_on(portal.login(&account));
            assert!(
                result.is_ok(),
                "The offline preflight must allow authentication: {:?}",
                result.err()
            );
            let snapshot = result.unwrap();
            assert!(snapshot.authenticated);
            assert_eq!(snapshot.username, "fixture-student");
            worker.join().unwrap();
        }
    }
    #[test]
    fn genuine_status_error_does_not_submit_credentials() {
        let (mut portal, worker) = fixture(vec![
            ("/index_6.html", String::new()),
            ("/index_6.html", r#"ip: "10.0.0.2""#.into()),
            (
                "/cgi-bin/rad_user_info",
                json!({"error":"password_error"}).to_string(),
            ),
        ]);
        let account = Account {
            username: "fixture-student".into(),
            password: "fictional-password".into(),
        };
        let result = runtime().block_on(portal.login(&account));
        assert!(result.is_err());
        worker.join().unwrap();
    }

    fn online_steps() -> Vec<(&'static str, String)> {
        let html = r#"ip: "10.0.0.2" <meta name="_xsrf" content="fixture-xsrf">"#;
        vec![
            ("/index_6.html", String::new()),
            ("/index_6.html", html.into()),
            (
                "/cgi-bin/rad_user_info",
                json!({"error":"ok","status":"E0000","user_name":"fixture-student"}).to_string(),
            ),
            ("/srun_portal_success", html.into()),
            (
                "/v1/auth/device/get",
                json!({"code":0,"data":[]}).to_string(),
            ),
        ]
    }
    fn offline_steps() -> Vec<(&'static str, String)> {
        vec![
            ("/index_6.html", String::new()),
            (
                "/index_6.html",
                r#"ip: "10.0.0.2" <meta name="_xsrf" content="fixture-xsrf">"#.into(),
            ),
            (
                "/cgi-bin/rad_user_info",
                json!({"error":"not_online_error"}).to_string(),
            ),
        ]
    }
    #[test]
    fn switching_verifies_logout_and_final_identity_and_never_rolls_back() {
        for outcome in [
            "success",
            "logout-failed",
            "password-error",
            "wrong-identity",
        ] {
            let mut responses = online_steps(); // Switch preflight: A.
            responses.extend(online_steps()); // Logout preflight: A.
            responses.push(("/cgi-bin/srun_portal", json!({"error":"ok"}).to_string()));
            if outcome == "logout-failed" {
                responses.extend(online_steps()); // A is still online: do not authenticate B.
            } else {
                responses.extend(offline_steps()); // Logout confirmed.
                responses.extend(offline_steps()); // Login preflight.
                responses.push((
                    "/cgi-bin/get_challenge",
                    json!({"error":"ok","challenge":"fictional-challenge"}).to_string(),
                ));
                responses.push((
                    "/cgi-bin/srun_portal",
                    if outcome == "password-error" {
                        json!({"ecode":"E2901","error":"authentication_failed","error_msg":"E2901: (Third party 1)bind_user2: ldap_bind error"})
                    } else {
                        json!({"error":"ok"})
                    }
                    .to_string(),
                ));
                if outcome != "password-error" {
                    responses.extend(online_steps().into_iter().map(|(path, text)| {
                        (
                            path,
                            if outcome == "success" {
                                text.replace("fixture-student", "fixture-other")
                            } else {
                                text
                            },
                        )
                    }));
                }
            }
            let (mut portal, server) =
                fixture_for_users(responses, "fixture-other", "fixture-student");
            let account = Account {
                username: "fixture-other".into(),
                password: "fictional-other-password".into(),
            };
            let result = runtime().block_on(portal.switch_account(&account));
            if outcome == "success" {
                assert_eq!(portal.password_status, Some(true));
                assert_eq!(result.unwrap().username, "fixture-other");
            } else {
                assert!(result.is_err(), "{outcome} must not report success");
                if outcome == "password-error" {
                    assert_eq!(portal.password_status, Some(false));
                    assert_eq!(
                        result.err().unwrap(),
                        "数智人大密码错误，请在设置中修改该账号的密码后重试"
                    );
                } else {
                    assert_eq!(portal.password_status, None);
                }
            }
            server.join().unwrap();
        }
    }
    #[test]
    fn switching_to_the_online_identity_does_not_disconnect() {
        let (mut portal, server) = fixture(online_steps());
        portal.password_status = Some(false);
        let account = Account {
            username: "fixture-student".into(),
            password: "fictional-password".into(),
        };
        assert!(
            runtime()
                .block_on(portal.switch_account(&account))
                .unwrap()
                .authenticated
        );
        assert_eq!(
            portal.password_status, None,
            "Observing an online session does not verify the saved password"
        );
        server.join().unwrap();
    }
    #[test]
    fn logout_confirms_offline_even_with_a_non_json_operation_response() {
        for offline in [true, false] {
            let mut responses = online_steps();
            responses.push((
                "/cgi-bin/srun_portal",
                "<html>operation finished</html>".into(),
            ));
            if offline {
                responses.extend(vec![
                    ("/index_6.html", String::new()),
                    ("/index_6.html", r#"ip: "10.0.0.2""#.into()),
                    (
                        "/cgi-bin/rad_user_info",
                        json!({"error":"user not login"}).to_string(),
                    ),
                ]);
            } else {
                responses.extend(online_steps());
            }
            let (mut portal, worker) = fixture(responses);
            let result = runtime().block_on(portal.logout());
            if offline {
                let snapshot = result.unwrap();
                assert!(!snapshot.authenticated);
                assert_eq!(snapshot.status, "offline");
            } else {
                assert!(
                    result.is_err(),
                    "A malformed response must not count as a successful logout while still online"
                );
            }
            worker.join().unwrap();
        }
    }
}
