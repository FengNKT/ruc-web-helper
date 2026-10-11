use base64::{
    alphabet::Alphabet,
    engine::{general_purpose::GeneralPurpose, GeneralPurposeConfig},
    Engine,
};
use hmac::{Hmac, Mac};
use md5::Md5;
use serde::Serialize;
use sha1::{Digest, Sha1};
use zeroize::Zeroizing;

// Preserve JavaScript charCodeAt packing, including its UTF-16 behavior.
fn words(text: &str, length: bool) -> Vec<u32> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut result = vec![0; units.len().div_ceil(4)];
    for (i, unit) in units.iter().enumerate() {
        result[i / 4] |= u32::from(*unit) << ((i % 4) * 8);
    }
    if length {
        result.push(units.len() as u32);
    }
    result
}

fn encode(text: &str, token: &str) -> Vec<u8> {
    let mut v = words(text, true);
    let mut key = words(token, false);
    key.resize(key.len().max(4), 0);
    let n = v.len() - 1;
    let mut z = v[n];
    let mut sum = 0u32;
    for _ in 0..(6 + 52 / v.len()) {
        sum = sum.wrapping_add(0x9e3779b9);
        let e = (sum >> 2) & 3;
        for p in 0..=n {
            let y = v[if p < n { p + 1 } else { 0 }];
            let mut m = (z >> 5) ^ (y << 2);
            m = m.wrapping_add(((y >> 3) ^ (z << 4)) ^ (sum ^ y));
            m = m.wrapping_add(key[(p & 3) ^ e as usize] ^ z);
            v[p] = v[p].wrapping_add(m);
            z = v[p];
        }
    }
    v.iter().flat_map(|word| word.to_le_bytes()).collect()
}

pub fn login_parameters(
    username: &str,
    password: &str,
    ip: &str,
    acid: &str,
    token: &str,
) -> Vec<(String, String)> {
    let mut mac =
        Hmac::<Md5>::new_from_slice(token.as_bytes()).expect("HMAC accepts arbitrary keys");
    mac.update(password.as_bytes());
    let hmd5 = format!("{:x}", mac.finalize().into_bytes());
    // Borrow the password; avoid a second non-zeroized JSON Value allocation.
    #[derive(Serialize)]
    struct LoginInfo<'a> {
        username: &'a str,
        password: &'a str,
        ip: &'a str,
        acid: &'a str,
        enc_ver: &'a str,
    }
    let data = LoginInfo {
        username,
        password,
        ip,
        acid,
        enc_ver: "srun_bx1",
    };
    let plain = Zeroizing::new(serde_json::to_string(&data).expect("string serialization"));
    let alphabet =
        Alphabet::new("LVoJPiCN2R8G90yg+hmFHuacZ1OWMnrsSTXkYpUq/3dlbfKwv6xztjI7DeBE45QA")
            .expect("valid SRun alphabet");
    let info = format!(
        "{{SRBX1}}{}",
        GeneralPurpose::new(&alphabet, GeneralPurposeConfig::new()).encode(encode(&plain, token))
    );
    let checksum = format!(
        "{token}{username}{token}{hmd5}{token}{acid}{token}{ip}{token}200{token}1{token}{info}"
    );
    let checksum = format!("{:x}", Sha1::digest(checksum.as_bytes()));
    [
        ("action", "login".into()),
        ("username", username.into()),
        ("password", format!("{{MD5}}{hmd5}")),
        ("ac_id", acid.into()),
        ("ip", ip.into()),
        ("chksum", checksum),
        ("info", info),
        ("n", "200".into()),
        ("type", "1".into()),
        ("os", "Windows 10".into()),
        ("name", "Windows".into()),
        ("double_stack", "0".into()),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_school_javascript_vectors() {
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/protocol-vectors.json")).unwrap();
        for vector in vectors.as_array().unwrap() {
            let params = login_parameters(
                vector["username"].as_str().unwrap(),
                vector["password"].as_str().unwrap(),
                vector["ip"].as_str().unwrap(),
                vector["acid"].as_str().unwrap(),
                vector["token"].as_str().unwrap(),
            );
            for key in ["password", "info", "chksum"] {
                assert_eq!(
                    params.iter().find(|(name, _)| name == key).unwrap().1,
                    vector["expected"][key].as_str().unwrap(),
                    "field {key}"
                );
            }
        }
    }
}
