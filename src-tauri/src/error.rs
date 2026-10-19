use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct AppError {
    pub code: String,
    pub message: String,
}

pub type Result<T> = std::result::Result<T, AppError>;

impl AppError {
    /// 将可操作错误限制在当前界面，不产生错误日志。
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::new("IO_ERROR", format!("文件操作失败：{error}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(_: serde_json::Error) -> Self {
        Self::new("INVALID_RECORD", "任务记录格式无效或版本不受支持。")
    }
}
