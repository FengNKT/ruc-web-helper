use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    },
};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};
static STORE_LOCK: Mutex<()> = Mutex::new(());
const HEADER: &[u8] = b"RUC-DPAPI\x01";
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Account {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct SavedAccount {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub password_error: bool,
    #[serde(flatten)]
    pub account: Account,
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct AccountBook {
    version: u32,
    next_id: u64,
    pub selected_id: Option<String>,
    #[serde(default)]
    pub automatic_id: Option<String>,
    pub accounts: Vec<SavedAccount>,
}

impl Default for AccountBook {
    fn default() -> Self {
        Self {
            version: 2,
            next_id: 1,
            selected_id: None,
            automatic_id: None,
            accounts: Vec::new(),
        }
    }
}

impl AccountBook {
    pub fn selected(&self) -> Option<&SavedAccount> {
        self.accounts
            .iter()
            .find(|a| Some(&a.id) == self.selected_id.as_ref())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub id: String,
    pub name: String,
    pub username: String,
    pub password_error: bool,
}

impl From<&SavedAccount> for AccountSummary {
    fn from(saved: &SavedAccount) -> Self {
        Self {
            id: saved.id.clone(),
            name: saved.name.clone(),
            username: saved.account.username.clone(),
            password_error: saved.password_error,
        }
    }
}

fn protect(input: &[u8], decrypt: bool) -> Result<Zeroizing<Vec<u8>>, String> {
    let blob = CRYPT_INTEGER_BLOB {
        cbData: input.len().try_into().map_err(|_| "账号文件过大")?,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // DPAPI owns the output allocation. User scope, with interaction disabled for unattended startup.
    let success = unsafe {
        if decrypt {
            CryptUnprotectData(
                &blob,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &blob,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if success == 0 {
        return Err("无法处理加密账号文件，请使用保存时的 Windows 用户，或重新保存账号".into());
    }
    let bytes = unsafe { std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize) };
    let result = Zeroizing::new(bytes.to_vec());
    bytes.zeroize();
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(result)
}
pub fn load(path: &Path) -> Result<Option<Account>, String> {
    Ok(load_book(path)?
        .selected()
        .map(|saved| saved.account.clone()))
}

pub fn load_book(path: &Path) -> Result<AccountBook, String> {
    let _lock = STORE_LOCK.lock().map_err(|_| "凭据存储正忙")?;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AccountBook::default())
        }
        Err(_) => return Err("无法读取用户数据目录中的账号文件".into()),
    };
    let encrypted = bytes
        .strip_prefix(HEADER)
        .ok_or("账号文件损坏，请重新保存账号")?;
    let secret = protect(encrypted, true)?;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Book(AccountBook),
        Legacy(Account),
    }
    let stored: Stored =
        serde_json::from_slice(&secret).map_err(|_| "账号文件损坏，请重新保存账号")?;
    let mut book = match stored {
        Stored::Book(book) => book,
        Stored::Legacy(account) => AccountBook {
            selected_id: Some("account-1".into()),
            automatic_id: Some("account-1".into()),
            next_id: 2,
            accounts: vec![SavedAccount {
                id: "account-1".into(),
                name: account.username.clone(),
                password_error: false,
                account,
            }],
            ..AccountBook::default()
        },
    };
    // Preserve the old login choice when upgrading existing encrypted files.
    if book.automatic_id.is_none() {
        book.automatic_id = book
            .selected_id
            .clone()
            .or_else(|| book.accounts.first().map(|a| a.id.clone()));
    }
    if book.version != 2
        || book.next_id == 0
        || book.selected_id.is_some() && book.selected().is_none()
        || book
            .automatic_id
            .as_ref()
            .is_some_and(|id| !book.accounts.iter().any(|a| &a.id == id))
        || book.accounts.iter().enumerate().any(|(i, a)| {
            a.id.is_empty()
                || a.account.username.is_empty()
                || a.account.password.is_empty()
                || book.accounts[..i]
                    .iter()
                    .any(|b| b.id == a.id || b.account.username == a.account.username)
        })
    {
        return Err("账号文件损坏或版本不受支持，请重新配置账号".into());
    }
    Ok(book)
}
pub fn save(path: &Path, account: &Account) -> Result<(), String> {
    save_value(path, account)
}
fn save_value(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let _lock = STORE_LOCK.lock().map_err(|_| "凭据存储正忙")?;
    let secret = Zeroizing::new(serde_json::to_vec(value).map_err(|_| "无法保存账号")?);
    let encrypted = protect(&secret, false)?;
    let mut bytes = HEADER.to_vec();
    bytes.extend_from_slice(&encrypted);
    let temporary = path.with_extension("enc.tmp");
    std::fs::write(&temporary, bytes)
        .map_err(|_| "无法写入用户数据目录，请检查当前 Windows 用户的权限")?;
    std::fs::rename(&temporary, path).map_err(|_| "无法更新加密账号文件".into())
}

pub fn upsert(
    path: &Path,
    id: Option<String>,
    name: String,
    username: String,
    password: String,
) -> Result<(), String> {
    // Service serializes all mutations with the portal lock; readers use STORE_LOCK.
    let mut book = load_book(path)?;
    let username = username.trim().to_string();
    let name = name.trim().to_string();
    let password = Zeroizing::new(password);
    if username.is_empty()
        || username.len() > 128
        || username.chars().any(char::is_control)
        || name.chars().count() > 24
        || name.chars().any(char::is_control)
        || password.len() > 512
    {
        return Err("请填写有效的学工号、备注和密码".into());
    }
    if book
        .accounts
        .iter()
        .any(|a| a.account.username == username && Some(&a.id) != id.as_ref())
    {
        return Err("该学工号已保存，请编辑现有账号".into());
    }
    let name = if name.is_empty() {
        username.clone()
    } else {
        name
    };
    if let Some(id) = id {
        let saved = book
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or("账号不存在，请刷新后重试")?;
        if password.is_empty() && saved.account.username != username {
            return Err("学工号已更改，请输入该账号的密码".into());
        }
        saved.name = name;
        saved.account.username = username;
        if !password.is_empty() {
            saved.account.password.zeroize();
            saved.account.password = password.to_string();
            saved.password_error = false;
        }
    } else {
        if password.is_empty() {
            return Err("请填写密码后保存".into());
        }
        let id = format!("account-{}", book.next_id);
        book.next_id = book.next_id.checked_add(1).ok_or("账号数量超出限制")?;
        if book.accounts.is_empty() {
            book.selected_id = Some(id.clone());
            book.automatic_id = Some(id.clone());
        }
        book.accounts.push(SavedAccount {
            id,
            name,
            password_error: false,
            account: Account {
                username,
                password: password.to_string(),
            },
        });
    }
    save_value(path, &book)
}

pub fn record_password_status(path: &Path, account: &Account, valid: bool) -> Result<(), String> {
    let mut book = load_book(path)?;
    let Some(saved) = book.accounts.iter_mut().find(|saved| {
        saved.account.username == account.username && saved.account.password == account.password
    }) else {
        return Ok(());
    };
    if saved.password_error == !valid {
        return Ok(());
    }
    saved.password_error = !valid;
    save_value(path, &book)
}

pub fn load_automatic(path: &Path) -> Result<Option<Account>, String> {
    let book = load_book(path)?;
    Ok(book
        .accounts
        .iter()
        .find(|a| Some(&a.id) == book.automatic_id.as_ref())
        .map(|a| a.account.clone()))
}

pub fn select_automatic(path: &Path, id: &str) -> Result<(), String> {
    let mut book = load_book(path)?;
    if !book.accounts.iter().any(|a| a.id == id) {
        return Err("账号不存在，请刷新后重试".into());
    }
    book.automatic_id = Some(id.to_string());
    save_value(path, &book)
}

pub fn select(path: &Path, id: &str) -> Result<Account, String> {
    let mut book = load_book(path)?;
    let account = book
        .accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or("账号不存在，请刷新后重试")?
        .account
        .clone();
    book.selected_id = Some(id.to_string());
    save_value(path, &book)?;
    Ok(account)
}

// Observing an online identity only updates local selection, never authenticates.
// An unknown identity must not inherit another account's credentials.
pub fn align_online(path: &Path, username: &str) -> Result<bool, String> {
    if username.is_empty() {
        return Ok(false);
    }
    // Credential read errors are already exposed by Preferences. They must not
    // prevent observing a connection made outside this application.
    let Ok(mut book) = load_book(path) else {
        return Ok(false);
    };
    let Some(saved) = book
        .accounts
        .iter()
        .find(|a| a.account.username == username)
    else {
        return Ok(false);
    };
    if book.selected_id.as_ref() == Some(&saved.id) {
        return Ok(false);
    }
    book.selected_id = Some(saved.id.clone());
    save_value(path, &book)?;
    Ok(true)
}

pub fn remove(path: &Path, id: &str) -> Result<bool, String> {
    let mut book = load_book(path)?;
    if !book.accounts.iter().any(|a| a.id == id) {
        return Err("账号不存在，请刷新后重试".into());
    }
    book.accounts.retain(|a| a.id != id);
    let removed_selected = book.selected_id.as_deref() == Some(id);
    if removed_selected {
        book.selected_id = None;
    }
    if book.automatic_id.as_deref() == Some(id) {
        book.automatic_id = book.accounts.first().map(|a| a.id.clone());
    }
    save_value(path, &book)?;
    Ok(removed_selected)
}

pub fn reset_with_backup(path: &Path) -> Result<(), String> {
    if path.exists() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "无法生成备份文件名")?
            .as_nanos();
        let backup = path.with_extension(format!("enc.recovery-{stamp}"));
        std::fs::copy(path, backup).map_err(|_| "无法备份原账号文件，未重置凭据")?;
    }
    save_value(path, &AccountBook::default())
}
#[cfg(test)]
pub fn delete(path: &Path) -> Result<(), String> {
    let _lock = STORE_LOCK.lock().map_err(|_| "凭据存储正忙")?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("无法删除用户数据目录中的账号文件".into()),
    }
}
#[cfg(test)]
pub fn update(path: &Path, username: String, password: String) -> Result<(), String> {
    let account = Account {
        username: username.trim().to_string(),
        password,
    };
    if account.username.is_empty()
        || account.username.len() > 128
        || account.username.chars().any(char::is_control)
        || account.password.len() > 512
    {
        return Err("请填写有效的学工号和密码".into());
    }
    if account.password.is_empty() {
        let previous = load(path)?.ok_or("请填写密码后保存")?;
        if previous.username != account.username || previous.password.is_empty() {
            return Err("学工号已更改，请输入该账号的密码".into());
        }
        return Ok(());
    }
    save(path, &account)
}
pub fn migrate_legacy(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    // Read the legacy service ID only when migrating the pre-portable credential store.
    let entry = keyring::Entry::new("cn.ruc.campusassistant", "campus-account")
        .map_err(|_| "无法迁移旧版账号")?;
    match entry.get_password() {
        Ok(secret) => {
            let secret = Zeroizing::new(secret);
            let account: Account =
                serde_json::from_str(&secret).map_err(|_| "旧版账号无法读取，请重新保存")?;
            save(path, &account)?;
            let restored = load(path)?.ok_or("迁移账号失败")?;
            if restored.username != account.username || restored.password != account.password {
                return Err("迁移账号校验失败".into());
            }
            entry
                .delete_credential()
                .map_err(|_| "账号已迁移，但无法清理旧版凭据")?;
            Ok(())
        }
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("无法迁移旧版账号，请重新保存账号".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn password_error_belongs_to_saved_password_and_survives_observation_and_renaming() {
        let root =
            std::env::temp_dir().join(format!("ruc-password-status-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("account.enc");
        upsert(
            &path,
            None,
            "".into(),
            "student".into(),
            "old-secret".into(),
        )
        .unwrap();
        let id = load_book(&path).unwrap().accounts[0].id.clone();
        let old = load(&path).unwrap().unwrap();
        record_password_status(&path, &old, false).unwrap();
        assert!(load_book(&path).unwrap().accounts[0].password_error);
        align_online(&path, "student").unwrap();
        upsert(
            &path,
            Some(id.clone()),
            "新备注".into(),
            "student".into(),
            "".into(),
        )
        .unwrap();
        assert!(load_book(&path).unwrap().accounts[0].password_error);
        record_password_status(&path, &old, true).unwrap();
        assert!(!load_book(&path).unwrap().accounts[0].password_error);
        record_password_status(&path, &old, false).unwrap();
        upsert(
            &path,
            Some(id),
            "".into(),
            "student".into(),
            "new-secret".into(),
        )
        .unwrap();
        assert!(!load_book(&path).unwrap().accounts[0].password_error);
        record_password_status(&path, &old, false).unwrap();
        assert!(
            !load_book(&path).unwrap().accounts[0].password_error,
            "A result for an old password cannot invalidate its replacement"
        );
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn automatic_account_is_persistent_and_independent_of_online_identity() {
        let root =
            std::env::temp_dir().join(format!("ruc-auto-account-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("account.enc");
        assert!(load_automatic(&path).unwrap().is_none());
        upsert(
            &path,
            None,
            "".into(),
            "first".into(),
            "first-secret".into(),
        )
        .unwrap();
        upsert(
            &path,
            None,
            "".into(),
            "second".into(),
            "second-secret".into(),
        )
        .unwrap();
        let book = load_book(&path).unwrap();
        let first = book.accounts[0].id.clone();
        let second = book.accounts[1].id.clone();
        assert_eq!(load_automatic(&path).unwrap().unwrap().username, "first");
        // Simulate an old multi-account file without the new field.
        let mut old = serde_json::to_value(&book).unwrap();
        old.as_object_mut().unwrap().remove("automatic_id");
        save_value(&path, &old).unwrap();
        align_online(&path, "second").unwrap();
        assert_eq!(load_automatic(&path).unwrap().unwrap().username, "first");
        select_automatic(&path, &second).unwrap();
        assert_eq!(load(&path).unwrap().unwrap().username, "second");
        select(&path, &first).unwrap();
        align_online(&path, "first").unwrap();
        assert_eq!(
            load_automatic(&path).unwrap().unwrap().password,
            "second-secret"
        );
        let before = std::fs::read(&path).unwrap();
        assert!(select_automatic(&path, "missing").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        remove(&path, &second).unwrap();
        assert_eq!(load_automatic(&path).unwrap().unwrap().username, "first");
        remove(&path, &first).unwrap();
        assert!(load_automatic(&path).unwrap().is_none());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn observing_online_accounts_aligns_known_identity_without_recording_unknown_credentials() {
        let root =
            std::env::temp_dir().join(format!("ruc-align-online-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("account.enc");
        assert!(!align_online(&path, "external-user").unwrap());
        assert!(!path.exists());
        upsert(
            &path,
            None,
            "主账号".into(),
            "saved-a".into(),
            "fictional-a-password".into(),
        )
        .unwrap();
        upsert(
            &path,
            None,
            "备用账号".into(),
            "saved-b".into(),
            "fictional-b-password".into(),
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(!align_online(&path, "external-user").unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(align_online(&path, "saved-b").unwrap());
        let selected = load(&path).unwrap().unwrap();
        assert_eq!(selected.username, "saved-b");
        assert_eq!(selected.password, "fictional-b-password");
        let aligned = std::fs::read(&path).unwrap();
        assert!(!align_online(&path, "saved-b").unwrap());
        assert!(!align_online(&path, "").unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), aligned);
        std::fs::write(&path, b"corrupt-original").unwrap();
        assert!(!align_online(&path, "saved-a").unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"corrupt-original");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn multi_account_migration_isolation_selection_and_recovery() {
        let root =
            std::env::temp_dir().join(format!("ruc-multi-account-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("account.enc");
        save(
            &path,
            &Account {
                username: "legacy-user".into(),
                password: "legacy-secret".into(),
            },
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        let book = load_book(&path).unwrap();
        assert_eq!(book.selected().unwrap().account.username, "legacy-user");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "Reading legacy data must not modify it"
        );
        let legacy_id = book.selected_id.clone().unwrap();
        upsert(
            &path,
            None,
            "备用".into(),
            "other-user".into(),
            "other-secret".into(),
        )
        .unwrap();
        assert_eq!(
            load(&path).unwrap().unwrap().username,
            "legacy-user",
            "Adding must not select or authenticate"
        );
        let book = load_book(&path).unwrap();
        assert_eq!(book.accounts.len(), 2);
        let other_id = book.accounts[1].id.clone();
        assert!(upsert(
            &path,
            None,
            "重复".into(),
            "other-user".into(),
            "secret".into()
        )
        .is_err());
        assert!(upsert(
            &path,
            Some(other_id.clone()),
            "".into(),
            "renamed-user".into(),
            "".into()
        )
        .is_err());
        upsert(
            &path,
            Some(other_id.clone()),
            "新备注".into(),
            "other-user".into(),
            "".into(),
        )
        .unwrap();
        assert_eq!(select(&path, &other_id).unwrap().password, "other-secret");
        assert!(!remove(&path, &legacy_id).unwrap());
        assert_eq!(load(&path).unwrap().unwrap().username, "other-user");
        let public = serde_json::to_string(&AccountSummary::from(
            load_book(&path).unwrap().selected().unwrap(),
        ))
        .unwrap();
        assert!(!public.contains("\"password\"") && !public.contains("secret"));
        assert!(remove(&path, &other_id).unwrap());
        assert!(load(&path).unwrap().is_none());
        let id_after_delete = load_book(&path).unwrap().next_id;
        upsert(&path, None, "".into(), "fresh-user".into(), "secret".into()).unwrap();
        assert_eq!(
            load_book(&path).unwrap().selected_id.as_deref(),
            Some(format!("account-{id_after_delete}").as_str())
        );
        std::fs::write(&path, b"corrupt-original").unwrap();
        assert!(load_book(&path).is_err());
        reset_with_backup(&path).unwrap();
        assert!(load_book(&path).unwrap().accounts.is_empty());
        let backup = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| e.file_name().to_string_lossy().contains("recovery-"))
            .unwrap();
        assert_eq!(std::fs::read(backup.path()).unwrap(), b"corrupt-original");
        std::fs::remove_file(backup.path()).unwrap();
        delete(&path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn password_update_retains_blank_and_requires_password_for_new_account() {
        let root = std::env::temp_dir().join(format!(
            "ruc-credentials-update-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("account.enc");
        delete(&path).unwrap();
        assert!(update(&path, "student-a".into(), "".into()).is_err());
        update(&path, "student-a".into(), "fictional-old-password".into()).unwrap();
        let original = std::fs::read(&path).unwrap();
        update(&path, " student-a ".into(), "".into()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(update(&path, "student-b".into(), "".into()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        update(&path, "student-a".into(), "fictional-new-password".into()).unwrap();
        assert_eq!(
            load(&path).unwrap().unwrap().password,
            "fictional-new-password"
        );
        update(&path, "student-b".into(), "fictional-other-password".into()).unwrap();
        assert_eq!(load(&path).unwrap().unwrap().username, "student-b");
        delete(&path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn encrypted_file_roundtrip_replace_corruption_and_delete() {
        let root =
            std::env::temp_dir().join(format!("ruc-credentials-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("account.enc");
        delete(&path).unwrap();
        assert!(load(&path).unwrap().is_none());
        let mut account = Account {
            username: "test-student".into(),
            password: "fake-password-密码".into(),
        };
        save(&path, &account).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes
            .windows(account.password.len())
            .any(|w| w == account.password.as_bytes()));
        assert_eq!(load(&path).unwrap().unwrap().password, account.password);
        account.password = "replacement-test-password".into();
        save(&path, &account).unwrap();
        assert_eq!(load(&path).unwrap().unwrap().password, account.password);
        let mut bytes = std::fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        std::fs::write(&path, bytes).unwrap();
        assert!(load(&path).is_err());
        delete(&path).unwrap();
        assert!(load(&path).unwrap().is_none());
        std::fs::remove_dir(&root).unwrap();
    }
}
