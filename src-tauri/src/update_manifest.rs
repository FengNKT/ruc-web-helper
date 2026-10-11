use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

pub const REPOSITORY: &str = "FengNKT/ruc-web-helper";
pub const MAX_EXE_SIZE: u64 = 200 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub project: String,
    pub version: String,
    pub tag: String,
    pub min_updater_version: String,
    #[serde(rename = "windowsX86_64")]
    pub windows_x86_64: Asset,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Asset {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}
pub fn release_url(version: &str, name: &str) -> String {
    format!("https://github.com/{REPOSITORY}/releases/download/v{version}/{name}")
}
pub fn stable(value: &str) -> Result<semver::Version, String> {
    let version = semver::Version::parse(value).map_err(|_| "版本号格式无效")?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err("仅支持正式版本".into());
    }
    Ok(version)
}
pub fn verify(raw: &[u8], signature: &[u8], expected: &str) -> Result<Manifest, String> {
    let key: [u8; 32] = STANDARD
        .decode(include_str!("../update-public-key.txt").trim())
        .map_err(|_| "更新公钥无效")?
        .try_into()
        .map_err(|_| "更新公钥无效")?;
    verify_with_key(raw, signature, expected, key)
}
fn verify_with_key(
    raw: &[u8],
    signature: &[u8],
    expected: &str,
    key: [u8; 32],
) -> Result<Manifest, String> {
    if raw.len() > 65536 || signature.len() > 256 {
        return Err("更新清单过大".into());
    }
    let sig = STANDARD
        .decode(
            std::str::from_utf8(signature)
                .map_err(|_| "更新签名无效")?
                .trim(),
        )
        .map_err(|_| "更新签名无效")?;
    let sig = Signature::from_slice(&sig).map_err(|_| "更新签名无效")?;
    VerifyingKey::from_bytes(&key)
        .map_err(|_| "更新公钥无效")?
        .verify_strict(raw, &sig)
        .map_err(|_| "更新签名校验失败，已停止更新")?;
    let manifest: Manifest = serde_json::from_slice(raw).map_err(|_| "更新清单格式无效")?;
    validate(&manifest, expected)?;
    Ok(manifest)
}
fn validate(manifest: &Manifest, expected: &str) -> Result<(), String> {
    let version = stable(&manifest.version)?;
    let current = stable(env!("CARGO_PKG_VERSION"))?;
    let asset = &manifest.windows_x86_64;
    if manifest.schema != 1
        || manifest.project != "ruc-web-helper"
        || manifest.version != expected
        || manifest.tag != format!("v{expected}")
        || version <= current
        || stable(&manifest.min_updater_version)? > current
        || asset.url
            != release_url(
                expected,
                &format!("ruc-web-helper_{expected}_windows-x86_64.exe"),
            )
        || asset.size < 1024
        || asset.size > MAX_EXE_SIZE
        || asset.sha256.len() != 64
        || !asset
            .sha256
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("更新清单与版本或程序不匹配，请从发布页面手动更新".into());
    }
    Ok(())
}
pub fn verify_file(path: &Path, asset: &Asset) -> Result<(), String> {
    let mut file = File::open(path).map_err(|_| "无法读取更新文件")?;
    if file.metadata().map_err(|_| "无法读取更新文件")?.len() != asset.size {
        return Err("更新文件大小不匹配，请重新下载".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut first = true;
    loop {
        let size = file.read(&mut buffer).map_err(|_| "无法校验更新文件")?;
        if size == 0 {
            break;
        }
        if first && !buffer[..size].starts_with(b"MZ") {
            return Err("更新文件不是 Windows 程序".into());
        }
        first = false;
        hash.update(&buffer[..size]);
    }
    if format!("{:x}", hash.finalize()) != asset.sha256 {
        return Err("更新文件校验失败，请重新下载".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    fn manifest() -> Manifest {
        Manifest {
            schema: 1,
            project: "ruc-web-helper".into(),
            version: "9.0.0".into(),
            tag: "v9.0.0".into(),
            min_updater_version: "1.1.0".into(),
            windows_x86_64: Asset {
                url: release_url("9.0.0", "ruc-web-helper_9.0.0_windows-x86_64.exe"),
                size: 1024,
                sha256: "a".repeat(64),
            },
        }
    }
    #[test]
    fn verifies_signature_and_rejects_tampering() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let raw = serde_json::to_vec(&manifest()).unwrap();
        assert!(serde_json::from_slice::<serde_json::Value>(&raw)
            .unwrap()
            .get("windowsX86_64")
            .is_some());
        let signature = STANDARD.encode(key.sign(&raw).to_bytes());
        assert!(verify_with_key(
            &raw,
            signature.as_bytes(),
            "9.0.0",
            key.verifying_key().to_bytes()
        )
        .is_ok());
        let mut altered = raw.clone();
        altered[10] ^= 1;
        assert!(verify_with_key(
            &altered,
            signature.as_bytes(),
            "9.0.0",
            key.verifying_key().to_bytes()
        )
        .is_err());
        assert!(verify_with_key(
            &raw,
            signature.as_bytes(),
            "8.0.0",
            key.verifying_key().to_bytes()
        )
        .is_err());
        assert!(verify_with_key(&raw, signature.as_bytes(), "9.0.0", [0; 32]).is_err());
    }
    #[test]
    fn rejects_untrusted_targets_downgrades_and_unsupported_updaters() {
        let mut m = manifest();
        m.windows_x86_64.url = "https://evil.test/a.exe".into();
        assert!(validate(&m, "9.0.0").is_err());
        let mut m = manifest();
        m.version = "1.0.0".into();
        assert!(validate(&m, "1.0.0").is_err());
        let mut m = manifest();
        m.min_updater_version = "99.0.0".into();
        assert!(validate(&m, "9.0.0").is_err());
        assert!(stable("1.10.0").unwrap() > stable("1.9.0").unwrap());
        assert!(stable("1.2.0-beta.1").is_err());
    }
    #[test]
    fn detects_truncated_and_corrupt_payloads() {
        let path = std::env::temp_dir().join(format!("ruc-update-hash-{}.exe", std::process::id()));
        let data = [b"MZ".as_slice(), &[4; 1022]].concat();
        std::fs::write(&path, &data).unwrap();
        let mut asset = manifest().windows_x86_64;
        asset.sha256 = format!("{:x}", Sha256::digest(&data));
        assert!(verify_file(&path, &asset).is_ok());
        asset.sha256 = "b".repeat(64);
        assert!(verify_file(&path, &asset).is_err());
        asset.size += 1;
        assert!(verify_file(&path, &asset).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
