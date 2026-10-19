#[cfg(test)]
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Ratio {
    FollowB,
    Portrait,
    Landscape,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Transition {
    None,
    BoundaryFade,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub ratio: Ratio,
    pub transition: Transition,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ratio: Ratio::FollowB,
            transition: Transition::None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub struct Rational {
    pub num: u32,
    pub den: u32,
}

impl Rational {
    /// 使用有理数计算帧率，不把 29.97 强行当成 30。
    pub fn value(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub path: String,
    pub bytes: f64,
    pub seconds: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<Rational>,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Pending,
    Preflight,
    Ready,
    Running,
    Validating,
    Committing,
    Succeeded,
    Blocked,
    Failed,
    Canceling,
    Canceled,
    Interrupted,
}

impl TaskState {
    /// 只有提交成功或明确取消的任务允许结束恢复记录生命周期。
    pub fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Canceled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub fps: Rational,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OutputInfo {
    pub bytes: f64,
    pub seconds: f64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    pub id: String,
    pub name: String,
    pub source_a: String,
    pub source_b: String,
    pub seconds: Option<f64>,
    pub canvas: Option<Canvas>,
    pub state: TaskState,
    pub progress: f64,
    pub issue: Option<String>,
    pub warnings: Vec<String>,
    pub result: Option<OutputInfo>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub total: usize,
    pub runnable: usize,
    pub blocked: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub canceled: usize,
    pub progress: f64,
    pub active: bool,
    pub paused: bool,
    pub locked: bool,
    pub output_directory: String,
    pub recovery_path: Option<String>,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub revision: f64,
    pub assets_a: Vec<Asset>,
    pub assets_b: Vec<Asset>,
    pub settings: Settings,
    pub summary: Summary,
    pub tasks: Vec<TaskView>,
    pub page: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EncoderStatus {
    pub detecting: bool,
    pub available: bool,
    pub accelerated: bool,
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Performance {
    pub cpu_percent: Option<f64>,
    pub process_bytes: Option<f64>,
    pub memory_scope: String,
    pub system_used: Option<f64>,
    pub system_total: Option<f64>,
    pub gpu_percent: Option<f64>,
    pub gpu_scope: String,
    pub speed: Option<f64>,
    pub eta_seconds: Option<f64>,
    pub disk_free: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 从 Rust 唯一契约源生成前端类型，避免手写类型漂移。
    #[test]
    fn export_contracts() {
        let declarations = [
            AppError::decl(),
            Ratio::decl(),
            Transition::decl(),
            Settings::decl(),
            Rational::decl(),
            Asset::decl(),
            TaskState::decl(),
            Canvas::decl(),
            OutputInfo::decl(),
            TaskView::decl(),
            Summary::decl(),
            Snapshot::decl(),
            EncoderStatus::decl(),
            Performance::decl(),
        ];
        let source = format!(
            "// Generated from Rust contracts. Run npm run bindings.\n{}\n",
            declarations
                .iter()
                .map(|s| format!("export {s}"))
                .collect::<Vec<_>>()
                .join("\n\n")
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/contracts.ts");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
}
