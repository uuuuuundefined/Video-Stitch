use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fingerprint {
    pub bytes: u64,
    pub modified_ns: String,
    pub identity: String,
    pub digest: String,
}

/// 读取头尾采样和文件身份，识别源变化及提交前后的同一个文件。
pub fn fingerprint(path: &Path) -> Result<Fingerprint> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AppError::new(
            "INPUT_MISSING",
            "请导入视频文件，不支持文件夹。",
        ));
    }
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut block = vec![0; 65536];
    let n = file.read(&mut block)?;
    hash.update(&block[..n]);
    if metadata.len() > 65536 {
        file.seek(SeekFrom::End(-65536))?;
        let n = file.read(&mut block)?;
        hash.update(&block[..n]);
    }
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}", metadata.dev(), metadata.ino())
    };
    #[cfg(windows)]
    let identity = {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut information = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        if unsafe {
            GetFileInformationByHandle(file.as_raw_handle().cast(), information.as_mut_ptr())
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let info = unsafe { information.assume_init() };
        format!(
            "{}:{}:{}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        )
    };
    #[cfg(not(any(unix, windows)))]
    let identity = fs::canonicalize(path)?.to_string_lossy().into_owned();
    Ok(Fingerprint {
        bytes: metadata.len(),
        modified_ns: metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AppError::new("INVALID_TIMESTAMPS", "文件修改时间无效。"))?
            .as_nanos()
            .to_string(),
        identity,
        digest: format!("{:x}", hash.finalize()),
    })
}

/// 生成跨平台安全名称，保留中文并限制单个结果名称长度。
pub fn safe_stem(value: &str) -> String {
    let mut text: String = value
        .nfc()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    text = text.trim_matches([' ', '.']).to_string();
    while text.len() > 80 {
        text.pop();
    }
    let upper = text.to_uppercase();
    if text.is_empty()
        || [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&upper.split('.').next().unwrap_or(""))
    {
        text.insert(0, '_');
    }
    text
}

/// 只允许记录中受限的单层文件名，禁止跨目录和符号链接清理。
pub fn owned_child(directory: &Path, name: &str) -> Result<PathBuf> {
    if name.contains(['/', '\\', ':'])
        || name.len() > 240
        || !matches!(
            Path::new(name).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
    {
        return Err(AppError::new(
            "INVALID_RECORD",
            "恢复记录包含不安全的文件路径。",
        ));
    }
    let path = directory.join(name);
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(AppError::new(
            "OUTPUT_CONFLICT",
            "输出文件是符号链接，已停止操作。",
        ));
    }
    Ok(path)
}

/// 使用原子的不覆盖提交；硬链接与删除只改变目录项，不复制视频数据。
pub fn commit_no_replace(source: &Path, target: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        unsafe extern "C" {
            fn renamex_np(
                from: *const std::ffi::c_char,
                to: *const std::ffi::c_char,
                flags: u32,
            ) -> i32;
        }
        let from = CString::new(source.as_os_str().as_bytes())
            .map_err(|_| AppError::new("INVALID_PATH", "路径包含无效字符。"))?;
        let to = CString::new(target.as_os_str().as_bytes())
            .map_err(|_| AppError::new("INVALID_PATH", "路径包含无效字符。"))?;
        // RENAME_EXCL guarantees that a concurrently created user file is never replaced.
        if unsafe { renamex_np(from.as_ptr(), to.as_ptr(), 0x4) } != 0 {
            return Err(AppError::new(
                "OUTPUT_CONFLICT",
                format!("无法安全提交结果：{}", std::io::Error::last_os_error()),
            ));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
        let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x8) } == 0 {
            return Err(AppError::new(
                "OUTPUT_CONFLICT",
                format!("无法安全提交结果：{}", std::io::Error::last_os_error()),
            ));
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        fs::hard_link(source, target)?;
        fs::remove_file(source)?;
    }
    sync_directory(
        target
            .parent()
            .ok_or_else(|| AppError::new("INVALID_PATH", "输出目录无效。"))?,
    )?;
    Ok(())
}

/// 同步目录项，减少提交成功后断电导致记录和文件不一致的窗口。
pub fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// 默认批次目录在真正导出时创建，同秒冲突也不覆盖。
pub fn create_output(custom: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = custom {
        let directory = fs::canonicalize(path)?;
        crate::runtime::validate_output(&directory)?;
        return Ok(directory);
    }
    let desktop = dirs::desktop_dir()
        .ok_or_else(|| AppError::new("OUTPUT_NOT_WRITABLE", "无法定位桌面，请选择保存目录。"))?;
    crate::runtime::validate_output(&fs::canonicalize(&desktop)?)?;
    let stem = format!(
        "视频拼接_{}",
        chrono::Local::now().format("%Y-%m-%d_%H%M%S")
    );
    for i in 0..10000 {
        let path = desktop.join(if i == 0 {
            stem.clone()
        } else {
            format!("{stem} ({})", i + 1)
        });
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(AppError::new("OUTPUT_CONFLICT", "无法创建唯一导出目录。"))
}

/// 预留当前任务的唯一输出实例，拒绝已有文件和链接。
pub fn reserve_partial(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| AppError::new("OUTPUT_NOT_WRITABLE", format!("无法创建进行中视频：{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_ownership_are_portable() {
        assert_eq!(safe_stem("CON"), "_CON");
        assert_eq!(safe_stem(" 城市:漫游. "), "城市_漫游");
        for value in ["../a", "a/b", "a\\b", "C:foo", "..", "/tmp/a"] {
            assert!(owned_child(Path::new("/tmp"), value).is_err());
        }
        assert!(safe_stem(&"中文".repeat(100)).len() <= 80);
    }
    #[test]
    fn commit_never_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        fs::write(&a, b"new").unwrap();
        fs::write(&b, b"user").unwrap();
        assert!(commit_no_replace(&a, &b).is_err());
        assert_eq!(fs::read(&b).unwrap(), b"user");
        fs::remove_file(&b).unwrap();
        commit_no_replace(&a, &b).unwrap();
        assert!(!a.exists());
        assert_eq!(fs::read(&b).unwrap(), b"new");
    }
}
