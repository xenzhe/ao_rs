//! Local secrets (the remembered password): encrypted with the Windows Data Protection API
//! (DPAPI, bound to the current Windows user; another user or machine cannot read it).
//! Never stored in plain text. On other systems nothing is stored.

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};

    fn run(data: &[u8], protect: bool) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: plain FFI with valid input/output blobs; the output buffer is
        // allocated by Windows and released with LocalFree below.
        let ok = unsafe {
            if protect {
                CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out)
            } else {
                CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out)
            }
        };
        if ok == 0 || out.pbData.is_null() {
            return None;
        }
        // SAFETY: Windows returned `cbData` valid bytes at `pbData`.
        let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
        unsafe { LocalFree(out.pbData as _) };
        Some(bytes)
    }

    pub fn protect(plain: &str) -> Option<Vec<u8>> {
        run(plain.as_bytes(), true)
    }

    pub fn unprotect(blob: &[u8]) -> Option<String> {
        run(blob, false).and_then(|b| String::from_utf8(b).ok())
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn protect(_: &str) -> Option<Vec<u8>> {
        None
    }
    pub fn unprotect(_: &[u8]) -> Option<String> {
        None
    }
}

/// Saves `secret` encrypted at `path` (nothing if encryption is not available).
pub fn save(path: &str, secret: &str) {
    match imp::protect(secret) {
        Some(blob) => {
            let _ = std::fs::write(path, blob);
        }
        None => {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Reads a secret saved with [`save`] (None if missing or not readable by this user).
pub fn load(path: &str) -> Option<String> {
    std::fs::read(path).ok().and_then(|b| imp::unprotect(&b))
}

pub fn forget(path: &str) {
    let _ = std::fs::remove_file(path);
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn roundtrip_and_not_plain() {
        let blob = super::imp::protect("secreta1").expect("dpapi");
        assert!(!blob.windows(8).any(|w| w == b"secreta1"), "not stored in plain text");
        assert_eq!(super::imp::unprotect(&blob).as_deref(), Some("secreta1"));
    }
}
