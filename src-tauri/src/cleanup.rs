use crate::error::{AppError, Result};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

/// 固定私有偏好位置；清理入口不接受目录、文件名或业务输出路径。
fn preferences_path() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join("Library/Preferences/com.videostitch.desktop.plist"))
        .ok_or_else(|| AppError::new("CLEANUP_FAILED", "无法定位软件私有偏好目录。"))
}

/// 子进程等待主进程结束，避免 NSOpenPanel 在退出时补写最近目录。
pub fn schedule() -> Result<()> {
    Command::new(std::env::current_exe()?)
        .arg("--cleanup-private-after")
        .arg(std::process::id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| {
            AppError::new(
                "CLEANUP_FAILED",
                "无法启动退出清理，请重试退出，或使用随包提供的私有数据清理脚本。",
            )
        })?;
    Ok(())
}

/// 只删除当前软件的偏好域和空偏好文件，输出目录从不参与清理。
fn remove_preferences() -> Result<()> {
    let path = preferences_path()?;
    let _ = Command::new("/usr/bin/defaults")
        .args(["delete", "com.videostitch.desktop"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(AppError::new(
            "CLEANUP_FAILED",
            format!(
                "请运行随包清理脚本，或删除软件私有偏好文件：{}。导出目录不受影响。",
                path.display()
            ),
        )),
    }
}

/// 同一二进制的短生命周期模式；没有 Tauri 窗口、业务写入或常驻循环。
pub fn run_if_requested() -> bool {
    if std::env::args().nth(1).as_deref() != Some("--cleanup-private-after") {
        return false;
    }
    let Some(parent) = std::env::args()
        .nth(2)
        .and_then(|s| s.parse::<i32>().ok())
        .filter(|pid| *pid > 1)
    else {
        return true;
    };
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let mut exited = false;
    // 最多等待 20 秒；进程存在时绝不清理它仍在使用的原生偏好。
    for _ in 0..200 {
        if unsafe { kill(parent, 0) } != 0 {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !exited {
        return true;
    }
    // 给系统收尾写入一个有界窗口，然后重复核对固定文件，不建立后台服务。
    std::thread::sleep(Duration::from_millis(300));
    let result = (|| {
        for _ in 0..3 {
            remove_preferences()?;
            std::thread::sleep(Duration::from_millis(300));
        }
        if preferences_path()?.exists() {
            return Err(AppError::new(
                "CLEANUP_FAILED",
                "系统仍在写入软件偏好，请运行随包清理脚本。",
            ));
        }
        Ok(())
    })();
    if let Err(error) = result {
        rfd::MessageDialog::new()
            .set_title("私有运行数据未能清理")
            .set_description(error.message)
            .show();
    }
    true
}
