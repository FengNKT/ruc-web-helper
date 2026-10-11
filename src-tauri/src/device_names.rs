use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};

#[derive(Default, Serialize, Deserialize)]
struct AccountDevices {
    #[serde(default)]
    devices: BTreeMap<String, Value>,
    #[serde(default)]
    months: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct Registry {
    version: u32,
    accounts: BTreeMap<String, AccountDevices>,
}
impl Default for Registry {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: BTreeMap::new(),
        }
    }
}
pub struct DeviceNames {
    path: PathBuf,
    lock: Mutex<()>,
}
pub fn normalize_mac(mac: &str) -> Result<String, String> {
    let key = mac.replace([':', '-'], "").to_ascii_lowercase();
    if key.len() != 12 || !key.bytes().all(|c| c.is_ascii_hexdigit()) || key == "000000000000" {
        return Err("设备缺少有效 MAC 地址，无法命名".into());
    }
    Ok(key)
}
fn account_key(username: &str) -> String {
    format!("{:x}", Sha1::digest(username.as_bytes()))
}
fn display_name(mac: &str, device: &Value) -> String {
    for field in ["customName", "schoolName"] {
        if let Some(name) = device[field].as_str().filter(|s| !s.is_empty()) {
            return name.into();
        }
    }
    format!(
        "{} · {}",
        device["os"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("设备"),
        mac[8..].to_uppercase()
    )
}
impl DeviceNames {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }
    fn read(&self) -> Result<Registry, String> {
        match std::fs::read(&self.path) {
            Ok(bytes) => {
                let data: Registry = serde_json::from_slice(&bytes)
                    .map_err(|_| "设备档案损坏，请检查 devices.json")?;
                if data.version != 1 {
                    return Err("设备档案版本不受支持".into());
                }
                Ok(data)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Import once. The old file remains a backup and is never written again.
                let legacy = self.path.with_file_name("device-names.json");
                let mut data = Registry::default();
                if legacy != self.path && legacy.exists() {
                    let bytes = std::fs::read(legacy).map_err(|_| "无法读取旧设备名称")?;
                    let names: Value = serde_json::from_slice(&bytes)
                        .map_err(|_| "旧设备名称文件损坏，迁移已停止")?;
                    let accounts = names["accounts"].as_object().ok_or("旧设备名称格式无效")?;
                    for (account, items) in accounts {
                        let entry = data.accounts.entry(account.clone()).or_default();
                        for (mac, name) in items.as_object().ok_or("旧设备名称格式无效")? {
                            let key = normalize_mac(mac)?;
                            let alias = name.as_str().ok_or("旧设备名称格式无效")?;
                            entry.devices.insert(key, json!({"customName":alias}));
                        }
                    }
                    self.write(&data)?;
                }
                Ok(data)
            }
            Err(_) => Err("无法读取设备档案".into()),
        }
    }
    fn write(&self, data: &Registry) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(data).map_err(|_| "无法编码设备档案")?;
        let temporary = self.path.with_extension("tmp");
        std::fs::write(&temporary, bytes).map_err(|_| "无法写入设备档案，请将软件放在可写目录")?;
        std::fs::rename(temporary, &self.path).map_err(|_| "无法更新设备档案".into())
    }
    pub fn get(&self, username: &str) -> Result<BTreeMap<String, String>, String> {
        let _guard = self.lock.lock().map_err(|_| "设备档案正忙")?;
        let data = self.read()?;
        Ok(data
            .accounts
            .get(&account_key(username))
            .map(|a| {
                a.devices
                    .iter()
                    .filter_map(|(mac, d)| {
                        d["customName"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .map(|name| (mac.clone(), name.into()))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
    pub fn set(
        &self,
        username: &str,
        mac: &str,
        name: &str,
    ) -> Result<BTreeMap<String, String>, String> {
        if username.is_empty() {
            return Err("请先登录校园网再命名设备".into());
        }
        let mac = normalize_mac(mac)?;
        let name = name.trim();
        if name.chars().count() > 40 || name.chars().any(char::is_control) {
            return Err("设备名称最多 40 个字符，不能包含控制字符".into());
        }
        {
            let _guard = self.lock.lock().map_err(|_| "设备档案正忙")?;
            let mut data = self.read()?;
            let d = data
                .accounts
                .entry(account_key(username))
                .or_default()
                .devices
                .entry(mac)
                .or_insert_with(|| json!({}));
            d["customName"] = json!(name);
            self.write(&data)?;
        }
        self.get(username)
    }
    pub fn set_ignored(&self, username: &str, mac: &str, ignored: bool) -> Result<(), String> {
        let mac = normalize_mac(mac)?;
        let _guard = self.lock.lock().map_err(|_| "设备档案正忙")?;
        let mut data = self.read()?;
        let device = data
            .accounts
            .get_mut(&account_key(username))
            .and_then(|account| account.devices.get_mut(&mac))
            .ok_or("未找到设备，请刷新后重试")?;
        device["ignored"] = json!(ignored);
        self.write(&data)
    }
    pub fn merge(
        &self,
        username: &str,
        live: &[Value],
        records: &[Value],
        month: Option<String>,
        fresh: bool,
    ) -> Result<(Vec<Value>, Vec<String>), String> {
        if username.is_empty() {
            return Ok((vec![], vec![]));
        }
        let _guard = self.lock.lock().map_err(|_| "设备档案正忙")?;
        let mut data = self.read()?;
        let before = serde_json::to_vec(&data).unwrap();
        let account = data.accounts.entry(account_key(username)).or_default();
        for item in live {
            let Some(mac) = item["mac"].as_str().and_then(|m| normalize_mac(m).ok()) else {
                continue;
            };
            let d = account.devices.entry(mac).or_insert_with(|| json!({}));
            for (source, target) in [("name", "schoolName"), ("os", "os")] {
                if let Some(value) = item[source].as_str().filter(|s| !s.is_empty()) {
                    d[target] = json!(value);
                }
            }
        }
        for item in records {
            let Some(mac) = item["user_mac"]
                .as_str()
                .and_then(|m| normalize_mac(m).ok())
            else {
                continue;
            };
            let d = account.devices.entry(mac).or_insert_with(|| json!({}));
            if let Some(start) = item["add_time"].as_i64().filter(|n| *n > 0) {
                d["firstRecord"] = json!(d["firstRecord"].as_i64().unwrap_or(start).min(start));
            }
            if let Some(end) = item["drop_time"].as_i64().filter(|n| *n > 0) {
                d["lastRecord"] = json!(d["lastRecord"].as_i64().unwrap_or(end).max(end));
            }
        }
        if let Some(month) = month {
            if !account.months.contains(&month) {
                account.months.push(month);
                account.months.sort();
            }
        }
        let mut result = vec![];
        for (mac, d) in &account.devices {
            let current = live.iter().find(|l| {
                l["mac"]
                    .as_str()
                    .and_then(|m| normalize_mac(m).ok())
                    .as_ref()
                    == Some(mac)
            });
            let mut row = current
                .cloned()
                .unwrap_or_else(|| json!({"mac":mac,"current":false}));
            let status = if !fresh {
                "unknown"
            } else if let Some(l) = current {
                match &l["online"] {
                    Value::Bool(true) => "online",
                    Value::Bool(false) => "offline",
                    Value::Number(n) if n.as_i64() == Some(1) => "online",
                    Value::Number(n) if n.as_i64() == Some(0) => "offline",
                    Value::String(s) if s == "1" => "online",
                    Value::String(s) if s == "0" => "offline",
                    _ => "unknown",
                }
            } else {
                "historical"
            };
            row["deviceStatus"] = json!(status);
            row["online"] = json!(status == "online");
            row["name"] = json!(display_name(mac, d));
            row["customName"] = d["customName"].clone();
            row["ignored"] = json!(d["ignored"].as_bool().unwrap_or(false));
            row["os"] = d["os"].clone();
            row["lastRecord"] = d["lastRecord"].clone();
            result.push(row);
        }
        result.sort_by(|a, b| {
            b["online"]
                .as_bool()
                .cmp(&a["online"].as_bool())
                .then_with(|| b["lastRecord"].as_i64().cmp(&a["lastRecord"].as_i64()))
                .then_with(|| a["mac"].as_str().cmp(&b["mac"].as_str()))
        });
        let months = account.months.clone();
        if serde_json::to_vec(&data).unwrap() != before {
            self.write(&data)?;
        }
        Ok((result, months))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_migrates_merges_preserves_and_isolates() {
        let dir = std::env::temp_dir().join(format!("ruc-registry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let legacy = dir.join("device-names.json");
        std::fs::write(
            &legacy,
            json!({"accounts":{account_key("a"):{"aabbccddeeff":"旧名称"}}}).to_string(),
        )
        .unwrap();
        let store = DeviceNames::new(dir.join("devices.json"));
        assert_eq!(store.get("a").unwrap()["aabbccddeeff"], "旧名称");
        let live = vec![
            json!({"mac":"AA:BB:CC:DD:EE:FF","online":true,"name":"学校名称","id":5}),
            json!({"mac":"112233445566","online":false}),
        ];
        let rows = vec![
            json!({"user_mac":"abcdef123456","add_time":100,"drop_time":200}),
            json!({"user_mac":"invalid"}),
        ];
        let (devices, months) = store
            .merge("a", &live, &rows, Some("2026-10".into()), true)
            .unwrap();
        assert_eq!(devices.len(), 3);
        assert_eq!(months, vec!["2026-10"]);
        assert_eq!(devices[0]["name"], "旧名称");
        assert_eq!(devices[0]["deviceStatus"], "online");
        assert!(devices.iter().any(|d| d["deviceStatus"] == "offline"));
        let history = devices
            .iter()
            .find(|d| d["deviceStatus"] == "historical")
            .unwrap();
        assert!(history["id"].is_null());
        assert_eq!(history["lastRecord"], 200);
        assert_eq!(store.merge("a", &[], &[], None, false).unwrap().0.len(), 3);
        assert!(store
            .merge("a", &[], &[], None, false)
            .unwrap()
            .0
            .iter()
            .all(|d| d["deviceStatus"] == "unknown"));
        assert!(store.merge("b", &[], &[], None, true).unwrap().0.is_empty());
        store.set("a", "AB-CD-EF-12-34-56", "历史电脑").unwrap();
        assert_eq!(store.get("a").unwrap()["abcdef123456"], "历史电脑");
        store.set_ignored("a", "AB-CD-EF-12-34-56", true).unwrap();
        let reopened = DeviceNames::new(dir.join("devices.json"));
        let (updated, _) = reopened.merge("a", &live, &rows, None, true).unwrap();
        assert_eq!(updated.len(), 3);
        let hidden = updated.iter().find(|d| d["mac"] == "abcdef123456").unwrap();
        assert_eq!(hidden["ignored"], true);
        assert_eq!(hidden["name"], "历史电脑");
        assert_eq!(hidden["lastRecord"], 200);
        assert!(reopened.set_ignored("b", "abcdef123456", true).is_err());
        assert!(reopened.set_ignored("a", "invalid", true).is_err());
        reopened.set_ignored("a", "abcdef123456", false).unwrap();
        assert!(reopened
            .merge("a", &[], &[], None, true)
            .unwrap()
            .0
            .iter()
            .all(|d| d["ignored"] == false));
        assert_eq!(reopened.get("a").unwrap()["abcdef123456"], "历史电脑");
        store.set("a", "abcdef123456", "").unwrap();
        assert!(!store.get("a").unwrap().contains_key("abcdef123456"));
        assert!(store.set("a", "invalid", "名称").is_err());
        assert!(store.set("a", "abcdef123456", &"a".repeat(41)).is_err());
        assert!(legacy.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
