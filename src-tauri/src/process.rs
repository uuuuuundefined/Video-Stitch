use crate::error::{AppError, Result};
use std::{
    path::Path,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::{sleep, timeout, Instant},
};

/// 固定媒体程序通过参数数组执行，不经过 shell，也不继承报告文件环境变量。
pub fn command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .env_remove("FFREPORT")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command
}

/// 在内存上限和时间上限内运行探测、试编码及结果检查。
pub async fn capture(binary: &Path, args: &[String], seconds: u64) -> Result<Vec<u8>> {
    let mut child = command(binary).args(args).spawn().map_err(|_| {
        AppError::new(
            "ENGINE_UNAVAILABLE",
            "媒体组件无法运行，请检查安装是否完整。",
        )
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::new("PROCESS_FAILED", "无法读取媒体组件输出。"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::new("PROCESS_FAILED", "无法读取媒体组件状态。"))?;
    let work = async {
        let out = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stdout
                .take(16 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .await
                .map(|_| bytes)
        });
        let err = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stderr
                .take(65536)
                .read_to_end(&mut bytes)
                .await
                .map(|_| bytes)
        });
        let status = child.wait().await?;
        let out = out
            .await
            .map_err(|_| AppError::new("PROCESS_FAILED", "媒体探测线程中断。"))??;
        let _ = err.await;
        if !status.success() {
            return Err(AppError::new(
                "PROBE_FAILED",
                "无法读取或处理此媒体，请确认文件完整且未被加密。",
            ));
        }
        if out.len() > 16 * 1024 * 1024 {
            return Err(AppError::new(
                "PROBE_FAILED",
                "媒体结构过于复杂，已停止读取。",
            ));
        }
        Ok(out)
    };
    timeout(Duration::from_secs(seconds), work)
        .await
        .map_err(|_| AppError::new("PROCESS_TIMEOUT", "媒体组件响应超时，请检查文件与磁盘。"))?
}

/// 解析真实输出时间；取消时等待进程终止，stderr 只用于有界错误分类。
pub async fn render(
    binary: &Path,
    args: &[String],
    stdin: Option<String>,
    cancel: Arc<AtomicBool>,
    mut progress: impl FnMut(f64) + Send,
) -> Result<()> {
    let mut cmd = command(binary);
    cmd.args(args);
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd
        .spawn()
        .map_err(|_| AppError::new("ENGINE_UNAVAILABLE", "无法启动 FFmpeg，请检查安装。"))?;
    if let Some(text) = stdin {
        let mut pipe = child
            .stdin
            .take()
            .ok_or_else(|| AppError::new("PROCESS_FAILED", "无法创建拼接管道。"))?;
        tokio::spawn(async move {
            let _ = pipe.write_all(text.as_bytes()).await;
        });
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::new("PROCESS_FAILED", "无法读取导出进度。"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::new("PROCESS_FAILED", "无法读取导出状态。"))?;
    let error_reader = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buffer = [0; 4096];
        let mut tail = Vec::new();
        while let Ok(n) = reader.read(&mut buffer).await {
            if n == 0 {
                break;
            }
            tail.extend_from_slice(&buffer[..n]);
            if tail.len() > 8192 {
                tail.drain(..tail.len() - 8192);
            }
        }
        String::from_utf8_lossy(&tail).into_owned()
    });
    let mut lines = BufReader::new(stdout).lines();
    let mut last_output = Instant::now();
    loop {
        tokio::select! {
            line = lines.next_line() => match line? {
                Some(line) => {
                    if let Some(value) = line.strip_prefix("out_time_us=").and_then(|v| v.parse::<f64>().ok()) {
                        last_output = Instant::now(); progress((value / 1_000_000.0).max(0.0));
                    }
                },
                None => break,
            },
            _ = sleep(Duration::from_millis(100)) => {
                if cancel.load(Ordering::Relaxed) {
                    let _ = child.kill().await; let _ = child.wait().await; let _ = error_reader.await;
                    return Err(AppError::new("CANCELED", "导出已停止。"));
                }
                if last_output.elapsed() > Duration::from_secs(180) {
                    let _ = child.kill().await; let _ = child.wait().await; let _ = error_reader.await;
                    return Err(AppError::new("PROCESS_TIMEOUT", "长时间没有处理进展，请检查磁盘或输入文件后重试。"));
                }
            }
        }
    }
    let status = child.wait().await?;
    let error = error_reader.await.unwrap_or_default();
    if cancel.load(Ordering::Relaxed) {
        return Err(AppError::new("CANCELED", "导出已停止。"));
    }
    if !status.success() {
        let (code, message) = if error.contains("No space left") {
            ("INSUFFICIENT_SPACE", "保存磁盘空间不足，请释放空间后重试。")
        } else if error.contains("Permission denied") || error.contains("Read-only file system") {
            (
                "OUTPUT_NOT_WRITABLE",
                "保存目录无法写入，请检查权限与磁盘连接。",
            )
        } else if error.contains("No such file") || error.contains("Input/output error") {
            (
                "INPUT_MISSING",
                "输入或输出磁盘不可访问，请重新连接后重试。",
            )
        } else if error.contains("videotoolbox")
            || error.contains("nvenc")
            || error.contains("qsv")
            || error.contains("amf")
        {
            ("HARDWARE_PATH_FAILED", "当前硬件编码路径无法处理此输入。")
        } else {
            (
                "PROCESS_FAILED",
                "视频处理失败，请检查源视频是否完整，或重试兼容处理。",
            )
        };
        return Err(AppError::new(code, message));
    }
    Ok(())
}
