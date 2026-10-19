#[cfg(any(windows, test))]
use crate::error::AppError;
use crate::error::Result;
use std::path::Path;

#[cfg(any(windows, test))]
use std::fs;
#[cfg(windows)]
use std::path::PathBuf;

#[cfg(any(windows, test))]
const OWNER_FILE: &str = ".video-stitch-owner";
#[cfg(any(windows, test))]
const OWNER_VALUE: &[u8] = b"video-stitch-runtime-v2";

/// Windows 运行数据使用新的专属命名空间，不递归清理旧版可能混有导出数据的目录。
#[cfg(windows)]
pub fn root() -> Result<PathBuf> {
    let local = dirs::data_local_dir()
        .ok_or_else(|| AppError::new("CLEANUP_FAILED", "无法定位软件私有运行目录。"))?;
    Ok(fs::canonicalize(local)?
        .join("com.videostitch.desktop")
        .join("Runtime-v2"))
}

/// WebView 只能使用已声明归属的运行目录。
#[cfg(windows)]
pub fn prepare() -> Result<PathBuf> {
    let root = root()?;
    remove_owned(&root)?;
    fs::create_dir_all(root.parent().unwrap())?;
    fs::create_dir(&root)?;
    use std::io::Write;
    let mut marker = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(OWNER_FILE))?;
    marker.write_all(OWNER_VALUE)?;
    marker.sync_all()?;
    Ok(root.join("WebView"))
}

/// 删除前验证固定归属标记；拒绝符号链接及未知目录，不遍历导出目录。
#[cfg(any(windows, test))]
fn remove_owned(root: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let marker = root.join(OWNER_FILE);
    let owned = metadata.is_dir()
        && !metadata.file_type().is_symlink()
        && fs::symlink_metadata(&marker).is_ok_and(|m| {
            m.is_file() && !m.file_type().is_symlink() && m.len() == OWNER_VALUE.len() as u64
        })
        && fs::read(&marker).is_ok_and(|bytes| bytes == OWNER_VALUE);
    if !owned {
        return Err(AppError::new(
            "CLEANUP_FAILED",
            format!("运行目录归属无法确认，已保留全部文件：{}。", root.display()),
        ));
    }
    fs::remove_dir_all(root)?;
    Ok(())
}

/// 等待 WebView 释放文件；失败时保留目录并明确提示。
#[cfg(windows)]
pub fn cleanup() -> Result<()> {
    let root = root()?;
    for _ in 0..30 {
        match remove_owned(&root) {
            Ok(()) => return Ok(()),
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
    Err(AppError::new(
        "CLEANUP_FAILED",
        format!(
            "私有运行数据未能清理：{}。请关闭残留界面进程后重试；导出目录不受影响。",
            root.display()
        ),
    ))
}

/// 已规范化的 Windows 路径按组件忽略大小写比较，防止输出进入自动清理区域。
#[cfg(any(windows, test))]
fn outside_runtime(directory: &Path, root: &Path) -> Result<()> {
    let components = |path: &Path| {
        path.components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect::<Vec<_>>()
    };
    if components(directory).starts_with(&components(root)) {
        return Err(AppError::new(
            "OUTPUT_NOT_WRITABLE",
            "不能将软件私有运行目录用作导出或恢复目录，请选择其他位置。",
        ));
    }
    Ok(())
}

/// 导出、恢复和默认目录入口共同排除运行数据目录；其他平台保持既有行为。
pub fn validate_output(directory: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let root = root()?;
        let root = if root.exists() {
            fs::canonicalize(root)?
        } else {
            root
        };
        outside_runtime(directory, &root)?;
    }
    #[cfg(not(windows))]
    let _ = directory;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧版目录及其视频、恢复记录均不符合新目录归属，必须原样保留。
    #[test]
    fn unknown_runtime_keeps_export_data() {
        let directory = tempfile::tempdir().unwrap();
        let video = directory.path().join("成果.mp4");
        let recovery = directory.path().join("unfinished-tasks.test.json");
        fs::write(&video, b"video").unwrap();
        fs::write(&recovery, b"record").unwrap();
        assert!(remove_owned(directory.path()).is_err());
        assert_eq!(fs::read(video).unwrap(), b"video");
        assert_eq!(fs::read(recovery).unwrap(), b"record");
    }

    /// 只删除带正确归属标记的新运行目录，同级导出目录保持不变。
    #[test]
    fn owned_runtime_cleanup_preserves_sibling_exports() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = directory.path().join("Runtime-v2");
        fs::create_dir(&runtime).unwrap();
        fs::write(runtime.join(OWNER_FILE), OWNER_VALUE).unwrap();
        fs::write(runtime.join("cache"), b"runtime").unwrap();
        let output = directory.path().join("export.mp4");
        fs::write(&output, b"video").unwrap();
        remove_owned(&runtime).unwrap();
        assert!(!runtime.exists());
        assert_eq!(fs::read(output).unwrap(), b"video");
    }

    /// 路径检查按组件匹配，拒绝子目录但不误伤名称相近的同级目录。
    #[test]
    fn runtime_output_boundary_is_component_based() {
        let root = Path::new("/private/Runtime-v2");
        assert!(outside_runtime(root, root).is_err());
        assert!(outside_runtime(Path::new("/PRIVATE/runtime-V2/exports"), root).is_err());
        assert!(outside_runtime(Path::new("/private/Runtime-v2-exports"), root).is_ok());
    }

    /// 伪装为运行目录的符号链接不能触发目标目录删除。
    #[cfg(unix)]
    #[test]
    fn cleanup_refuses_a_symlink_to_user_data() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("exports");
        fs::create_dir(&output).unwrap();
        fs::write(output.join(OWNER_FILE), OWNER_VALUE).unwrap();
        fs::write(output.join("成果.mp4"), b"video").unwrap();
        let link = directory.path().join("Runtime-v2");
        std::os::unix::fs::symlink(&output, &link).unwrap();
        assert!(remove_owned(&link).is_err());
        assert_eq!(fs::read(output.join("成果.mp4")).unwrap(), b"video");
    }
}
