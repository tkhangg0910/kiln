//! Kilno's requests (the update check, and live usage when it is on), through WinHTTP: the system proxy (WPAD /
//! PAC / per-user settings) and the Windows certificate store, as .NET's HttpClient had, and no
//! TLS stack in the exe.

use std::io;
use std::time::Duration;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts, INTERNET_DEFAULT_HTTPS_PORT,
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_DISABLE_REDIRECTS, WINHTTP_FLAG_SECURE, WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

/// A WinHTTP handle, closed on drop.
struct Handle(*mut core::ffi::c_void);
impl Handle {
    fn new(h: *mut core::ffi::c_void) -> io::Result<Self> {
        if h.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(h))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle WinHTTP gave us, closed once (children are dropped before parents).
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

/// `https://host/path` → (host, /path).
pub fn split_https(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    (!host.is_empty() && !host.contains([':', '@'])).then_some((host, path))
}

/// GET an https URL: (status code, body). Each phase (resolve, connect, send, receive) times
/// out after `timeout`. Blocks; call it off the UI thread.
pub fn get(url: &str, user_agent: &str, timeout: Duration) -> io::Result<(u32, Vec<u8>)> {
    get_with(url, user_agent, "", timeout)
}

/// As [`get`], with extra request headers (CRLF-separated). A request with headers does not
/// follow redirects, so what they carry (a sign-in token) reaches the named host only.
pub fn get_with(url: &str, user_agent: &str, headers: &str, timeout: Duration) -> io::Result<(u32, Vec<u8>)> {
    let (host, path) = split_https(url).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not https"))?;
    let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    // SAFETY: valid wide strings; null proxy names with automatic proxy.
    let session = Handle::new(unsafe {
        WinHttpOpen(&HSTRING::from(user_agent), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0)
    })?;
    // SAFETY: a live session handle.
    unsafe { WinHttpSetTimeouts(session.0, ms, ms, ms, ms) }.map_err(io::Error::other)?;
    // SAFETY: a live session handle and a valid host string.
    let connect =
        Handle::new(unsafe { WinHttpConnect(session.0, &HSTRING::from(host), INTERNET_DEFAULT_HTTPS_PORT, 0) })?;
    // SAFETY: a live connection; null version/referrer/accept types mean the defaults.
    let request = Handle::new(unsafe {
        WinHttpOpenRequest(
            connect.0,
            &HSTRING::from("GET"),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    })?;
    let headers: Vec<u16> = headers.encode_utf16().collect();
    if !headers.is_empty() {
        let off = WINHTTP_DISABLE_REDIRECTS.to_ne_bytes();
        // SAFETY: a live request; the option takes one u32, passed as its bytes.
        unsafe { WinHttpSetOption(Some(request.0), WINHTTP_OPTION_DISABLE_FEATURE, Some(&off)) }.map_err(io::Error::other)?;
    }
    let headers = (!headers.is_empty()).then_some(&headers[..]);
    // SAFETY: a live request; the headers slice (if any) outlives the call; no body.
    unsafe { WinHttpSendRequest(request.0, headers, None, 0, 0, 0) }.map_err(io::Error::other)?;
    // SAFETY: as above; the reserved pointer must be null.
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }.map_err(io::Error::other)?;

    let mut status = 0u32;
    let mut len = std::mem::size_of::<u32>() as u32;
    // SAFETY: the buffer is one u32, its size passed in `len`.
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut len,
            std::ptr::null_mut(),
        )
    }
    .map_err(io::Error::other)?;

    let mut body = Vec::new();
    loop {
        let mut avail = 0u32;
        // SAFETY: a live request; writes one u32.
        unsafe { WinHttpQueryDataAvailable(request.0, &mut avail) }.map_err(io::Error::other)?;
        if avail == 0 {
            break;
        }
        // GitHub's answer is a few KB; anything huge is not what we asked for.
        if body.len() > 4 * 1024 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "response too large"));
        }
        let start = body.len();
        body.resize(start + avail as usize, 0);
        let mut read = 0u32;
        // SAFETY: the buffer tail holds `avail` bytes.
        unsafe { WinHttpReadData(request.0, body[start..].as_mut_ptr().cast(), avail, &mut read) }
            .map_err(io::Error::other)?;
        body.truncate(start + read as usize);
        if read == 0 {
            break;
        }
    }
    drop(request);
    drop(connect);
    drop(session);
    Ok((status, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_https_urls() {
        assert_eq!(
            split_https("https://api.github.com/repos/a/b/releases/latest"),
            Some(("api.github.com", "/repos/a/b/releases/latest"))
        );
        assert_eq!(split_https("https://example.com"), Some(("example.com", "/")));
        assert_eq!(split_https("http://example.com/"), None);
        assert_eq!(split_https("https:///x"), None);
        assert_eq!(split_https("https://u@h/x"), None);
    }
}
