//! An isolated WinHTTP session. No portal cookies, account data or environment proxies.
use std::{
    ffi::c_void,
    io::Write,
    ptr::null,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows_sys::Win32::{Foundation::GetLastError, Networking::WinHttp::*};

struct Handle(*mut c_void);
impl Handle {
    fn new(value: *mut c_void) -> Result<Self, String> {
        if value.is_null() {
            Err(network_error())
        } else {
            Ok(Self(value))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn network_error() -> String {
    format!(
        "更新网络请求失败（Windows {}），请检查网络或系统代理后重试",
        unsafe { GetLastError() }
    )
}
fn checked(ok: i32) -> Result<(), String> {
    if ok == 0 {
        Err(network_error())
    } else {
        Ok(())
    }
}
unsafe fn option(handle: &Handle, name: u32, value: u32) -> Result<(), String> {
    checked(WinHttpSetOption(handle.0, name, &value as *const _ as _, 4))
}

pub fn allowed_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "更新地址无效")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
        || url.fragment().is_some()
        || !matches!(
            url.host_str(),
            Some(
                "api.github.com"
                    | "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
    {
        return Err("更新地址不在允许的 GitHub HTTPS 地址范围内".into());
    }
    Ok(url)
}

unsafe fn header(request: &Handle, name: u32) -> Option<String> {
    let mut bytes = 0;
    WinHttpQueryHeaders(
        request.0,
        name,
        null(),
        std::ptr::null_mut(),
        &mut bytes,
        std::ptr::null_mut(),
    );
    if bytes == 0 || bytes > 65536 {
        return None;
    }
    let mut buffer = vec![0u16; (bytes as usize + 1) / 2];
    if WinHttpQueryHeaders(
        request.0,
        name,
        null(),
        buffer.as_mut_ptr() as _,
        &mut bytes,
        std::ptr::null_mut(),
    ) == 0
    {
        return None;
    }
    Some(String::from_utf16_lossy(
        &buffer[..buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len())],
    ))
}

pub struct Response {
    pub status: u32,
    pub etag: Option<String>,
    pub retry_after: Option<String>,
    pub reset: Option<String>,
}

/// Each redirect is checked and receives the same per-user Windows proxy policy.
/// WinHTTP AUTOMATIC_PROXY includes manual proxy, PAC and WPAD (Windows 8.1+).
pub fn get(
    url: &str,
    direct: bool,
    etag: Option<&str>,
    limit: u64,
    cancel: &AtomicBool,
    output: &mut impl Write,
    mut progress: impl FnMut(u64),
) -> Result<Response, String> {
    let started = Instant::now();
    let agent = wide(concat!("RUC-Web-Helper/", env!("CARGO_PKG_VERSION")));
    let session = Handle::new(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            if direct {
                WINHTTP_ACCESS_TYPE_NO_PROXY
            } else {
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY
            },
            null(),
            null(),
            0,
        )
    })?;
    unsafe {
        checked(WinHttpSetTimeouts(session.0, 15000, 15000, 15000, 15000))?;
        option(
            &session,
            WINHTTP_OPTION_SECURE_PROTOCOLS,
            WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3,
        )?;
    }
    let mut url = allowed_url(url)?;
    for redirect in 0..=5 {
        if cancel.load(Ordering::Relaxed) {
            return Err("下载已取消".into());
        }
        let host = wide(url.host_str().ok_or("更新地址无效")?);
        let connection = Handle::new(unsafe { WinHttpConnect(session.0, host.as_ptr(), 443, 0) })?;
        let resource = wide(&format!(
            "{}{}",
            url.path(),
            url.query().map(|q| format!("?{q}")).unwrap_or_default()
        ));
        let method = wide("GET");
        let request = Handle::new(unsafe {
            WinHttpOpenRequest(
                connection.0,
                method.as_ptr(),
                resource.as_ptr(),
                null(),
                null(),
                null(),
                WINHTTP_FLAG_SECURE,
            )
        })?;
        let mut headers =
            "Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n"
                .to_owned();
        if redirect == 0 {
            if let Some(tag) = etag.filter(|s| !s.contains(['\r', '\n'])) {
                headers.push_str(&format!("If-None-Match: {tag}\r\n"));
            }
        }
        let headers = wide(&headers);
        unsafe {
            option(
                &request,
                WINHTTP_OPTION_REDIRECT_POLICY,
                WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
            )?;
            option(
                &request,
                WINHTTP_OPTION_DISABLE_FEATURE,
                WINHTTP_DISABLE_COOKIES,
            )?;
            option(
                &request,
                WINHTTP_OPTION_AUTOLOGON_POLICY,
                WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH,
            )?;
            checked(WinHttpSendRequest(
                request.0,
                headers.as_ptr(),
                (headers.len() - 1) as u32,
                null(),
                0,
                0,
                0,
            ))?;
            checked(WinHttpReceiveResponse(request.0, std::ptr::null_mut()))?;
        }
        let status = unsafe { header(&request, WINHTTP_QUERY_STATUS_CODE) }
            .and_then(|s| s.parse().ok())
            .ok_or("无法读取更新服务器响应")?;
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let location =
                unsafe { header(&request, WINHTTP_QUERY_LOCATION) }.ok_or("更新下载重定向无效")?;
            url = allowed_url(
                url.join(&location)
                    .map_err(|_| "更新下载重定向无效")?
                    .as_str(),
            )?;
            continue;
        }
        let response = Response {
            status,
            etag: unsafe { header(&request, WINHTTP_QUERY_ETAG) },
            retry_after: unsafe { custom_header(&request, "Retry-After") },
            reset: unsafe { custom_header(&request, "X-RateLimit-Reset") },
        };
        if status != 200 {
            return Ok(response);
        }
        if let Some(length) = unsafe { header(&request, WINHTTP_QUERY_CONTENT_LENGTH) }
            .and_then(|s| s.parse::<u64>().ok())
        {
            if length > limit {
                return Err("更新文件超出允许大小".into());
            }
        }
        let mut received = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("下载已取消".into());
            }
            if started.elapsed() > Duration::from_secs(1800) {
                return Err("更新下载超时，请重试".into());
            }
            let mut count = 0;
            unsafe {
                checked(WinHttpReadData(
                    request.0,
                    buffer.as_mut_ptr() as _,
                    buffer.len() as u32,
                    &mut count,
                ))?;
            }
            if count == 0 {
                break;
            }
            received += count as u64;
            if received > limit {
                return Err("更新文件超出允许大小".into());
            }
            output
                .write_all(&buffer[..count as usize])
                .map_err(|_| "无法保存更新文件，请检查目录权限和磁盘空间")?;
            progress(received);
        }
        return Ok(response);
    }
    Err("更新下载重定向次数过多".into())
}

unsafe fn custom_header(request: &Handle, name: &str) -> Option<String> {
    let name = wide(name);
    let mut bytes = 0;
    WinHttpQueryHeaders(
        request.0,
        WINHTTP_QUERY_CUSTOM,
        name.as_ptr(),
        std::ptr::null_mut(),
        &mut bytes,
        std::ptr::null_mut(),
    );
    if bytes == 0 || bytes > 65536 {
        return None;
    }
    let mut buffer = vec![0u16; (bytes as usize + 1) / 2];
    if WinHttpQueryHeaders(
        request.0,
        WINHTTP_QUERY_CUSTOM,
        name.as_ptr(),
        buffer.as_mut_ptr() as _,
        &mut bytes,
        std::ptr::null_mut(),
    ) == 0
    {
        return None;
    }
    Some(String::from_utf16_lossy(
        &buffer[..buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len())],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Read-only live GitHub check; run explicitly outside CI"]
    fn live_github_system_proxy() {
        let mut body = Vec::new();
        let response = get(
            "https://api.github.com/repos/FengNKT/ruc-web-helper/releases/latest",
            false,
            None,
            4 * 1024 * 1024,
            &AtomicBool::new(false),
            &mut body,
            |_| {},
        )
        .unwrap();
        assert_eq!(response.status, 200);
        let release: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(release["draft"], false);
        assert!(release["tag_name"].as_str().unwrap().starts_with('v'));
    }
    #[test]
    #[ignore = "Read-only live release redirect check; run explicitly outside CI"]
    fn live_github_asset_redirect() {
        let mut body = Vec::new();
        let response = get(
            "https://github.com/FengNKT/ruc-web-helper/releases/download/v1.0.0/SHA256SUMS.txt",
            false,
            None,
            65536,
            &AtomicBool::new(false),
            &mut body,
            |_| {},
        )
        .unwrap();
        assert_eq!(response.status, 200);
        assert!(String::from_utf8(body)
            .unwrap()
            .contains("ruc-web-helper_1.0.0_windows-x86_64.exe"));
    }
    #[test]
    fn restricts_download_destinations() {
        for url in [
            "http://github.com/a",
            "https://github.com.evil.test/a",
            "https://a@github.com/a",
            "https://github.com:444/a",
            "https://go.ruc.edu.cn",
            "file:///a",
        ] {
            assert!(allowed_url(url).is_err(), "{url}");
        }
        assert!(allowed_url("https://release-assets.githubusercontent.com/a?signature=x").is_ok());
    }
}
