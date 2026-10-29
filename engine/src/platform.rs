//! Windows-only primitives. The shared engine never imports Win32 types.
pub mod filesystem;
pub mod ipc;
use anyhow::{Result, ensure};
use std::{
    ffi::c_void,
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, Cryptography::*, *},
    Storage::FileSystem::*,
    System::{Registry::*, Threading::*},
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn replace_file(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let f: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let t: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    ensure!(
        unsafe {
            MoveFileExW(
                f.as_ptr(),
                t.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } != 0,
        "Cannot commit cache file: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}
pub fn protect(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    let ok = unsafe {
        if decrypt {
            CryptUnprotectData(
                &input,
                null_mut(),
                null(),
                null_mut(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                null(),
                null(),
                null_mut(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    ensure!(
        ok != 0,
        "Windows credential protection failed: {}",
        std::io::Error::last_os_error()
    );
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(result)
}
pub fn user_sid() -> Result<String> {
    unsafe {
        let mut token = null_mut();
        ensure!(
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) != 0,
            "Cannot open current user token"
        );
        let mut size = 0;
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut size);
        // usize alignment is sufficient for TOKEN_USER and SID structures.
        let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        let ok = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        );
        CloseHandle(token);
        ensure!(ok != 0, "Cannot read current user token");
        let user = &*(buffer.as_ptr().cast::<TOKEN_USER>());
        let mut sid = null_mut();
        ensure!(
            ConvertSidToStringSidW(user.User.Sid, &mut sid) != 0,
            "Cannot encode current user SID"
        );
        let mut len = 0;
        while *sid.add(len) != 0 {
            len += 1;
        }
        let out = String::from_utf16_lossy(std::slice::from_raw_parts(sid, len));
        LocalFree(sid.cast());
        Ok(out)
    }
}
pub fn security_descriptor() -> Result<Vec<u8>> {
    let sddl = wide(&format!(
        "O:{0}G:{0}D:P(A;;FA;;;{0})(A;;FA;;;SY)",
        user_sid()?
    ));
    unsafe {
        let mut descriptor: null_mut_type = null_mut();
        let mut size = 0;
        ensure!(
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                &mut size
            ) != 0,
            "Cannot secure local endpoint"
        );
        let out = std::slice::from_raw_parts(descriptor.cast::<u8>(), size as usize).to_vec();
        LocalFree(descriptor);
        Ok(out)
    }
}
#[allow(non_camel_case_types)]
type null_mut_type = *mut c_void;
pub fn startup(enabled: bool) -> Result<()> {
    unsafe {
        let mut key = null_mut();
        let path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        ensure!(
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                null(),
                0,
                KEY_SET_VALUE,
                null(),
                &mut key,
                null_mut()
            ) == 0,
            "Cannot open startup preferences"
        );
        let name = wide("StorageBridge");
        let code = if enabled {
            let exe = std::env::current_exe()?;
            let command = wide(&format!("\"{}\" --daemon", exe.display()));
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(key, name.as_ptr())
        };
        RegCloseKey(key);
        ensure!(
            code == 0 || (!enabled && code == ERROR_FILE_NOT_FOUND),
            "Cannot change startup preference (Windows {code})"
        );
        Ok(())
    }
}
