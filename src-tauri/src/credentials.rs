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
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Account {
    pub username: String,
    pub password: String,
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
    let _lock = STORE_LOCK.lock().map_err(|_| "凭据存储正忙")?;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("无法读取程序目录中的账号文件".into()),
    };
    let encrypted = bytes
        .strip_prefix(HEADER)
        .ok_or("账号文件损坏，请重新保存账号")?;
    let secret = protect(encrypted, true)?;
    serde_json::from_slice(&secret)
        .map(Some)
        .map_err(|_| "账号文件损坏，请重新保存账号".into())
}
pub fn save(path: &Path, account: &Account) -> Result<(), String> {
    let _lock = STORE_LOCK.lock().map_err(|_| "凭据存储正忙")?;
    let secret = Zeroizing::new(serde_json::to_vec(account).map_err(|_| "无法保存账号")?);
    let encrypted = protect(&secret, false)?;
    let mut bytes = HEADER.to_vec();
    bytes.extend_from_slice(&encrypted);
    let temporary = path.with_extension("enc.tmp");
    std::fs::write(&temporary, bytes).map_err(|_| "无法写入程序目录，请将软件放在可写文件夹")?;
    std::fs::rename(&temporary, path).map_err(|_| "无法更新加密账号文件".into())
}
pub fn delete(path: &Path) -> Result<(), String> {
    let _lock = STORE_LOCK.lock().map_err(|_| "凭据存储正忙")?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("无法删除程序目录中的账号文件".into()),
    }
}
pub fn update(path: &Path, username: String, password: String) -> Result<(), String> {
    let account = Account { username: username.trim().to_string(), password };
    if account.username.is_empty() || account.username.len() > 128
        || account.username.chars().any(char::is_control) || account.password.len() > 512 {
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
    fn password_update_retains_blank_and_requires_password_for_new_account() {
        let root = std::env::temp_dir().join(format!("ruc-credentials-update-test-{}", std::process::id()));
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
        assert_eq!(load(&path).unwrap().unwrap().password, "fictional-new-password");
        update(&path, "student-b".into(), "fictional-other-password".into()).unwrap();
        assert_eq!(load(&path).unwrap().unwrap().username, "student-b");
        delete(&path).unwrap();std::fs::remove_dir(root).unwrap();
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
