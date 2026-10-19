mod execution;
mod restore;

use crate::{
    contracts::*,
    engine::{self, Binaries, Encoder},
    error::{AppError, Result},
    files,
    metrics::{self, Sampler},
    plan::{self, Plan},
    probe::{self, Media},
    process,
    recovery::{self, Record, RecordAsset, RecordTask},
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

pub const PAGE_SIZE: usize = 80;

#[derive(Clone)]
struct Source {
    view: Asset,
    media: Option<Media>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 万项异常组合也不能消失，分页响应始终保持固定上限。
    #[test]
    fn ten_thousand_combinations_keep_order_and_bounded_pages() {
        let service = Service::new(None);
        let source = |role: &str, index: usize| Source {
            view: Asset {
                id: format!("{role}{index}"),
                name: format!("{role}{index}.mp4"),
                path: format!("/fixture/{role}{index}.mp4"),
                bytes: 1.0,
                seconds: None,
                width: None,
                height: None,
                fps: None,
                issue: Some("不支持的测试输入".into()),
            },
            media: None,
        };
        {
            let mut state = service.lock();
            state.a = (0..100).map(|i| source("A", i)).collect();
            state.b = (0..100).map(|i| source("B", i)).collect();
            Service::rebuild(&mut state).unwrap();
        }
        let snapshot = service.snapshot(0);
        assert_eq!(snapshot.summary.total, 10000);
        assert_eq!(snapshot.summary.blocked, 10000);
        assert_eq!(snapshot.tasks.len(), PAGE_SIZE);
        let second = service.snapshot(1);
        assert_eq!(second.tasks[20].source_a, "A0.mp4");
        assert_eq!(second.tasks[20].source_b, "B1.mp4");
        let last = service.snapshot(usize::MAX);
        assert_eq!(last.page, 124);
        assert_eq!(last.tasks.len(), 80);
        assert_eq!(last.tasks.last().unwrap().source_a, "A99.mp4");
        assert_eq!(last.tasks.last().unwrap().source_b, "B99.mp4");
    }
}

#[derive(Clone)]
struct Task {
    view: TaskView,
    a: usize,
    b: usize,
    plan: Option<Plan>,
    record: RecordTask,
}

struct Batch {
    id: String,
    request_id: String,
    directory: PathBuf,
}

struct Inner {
    a: Vec<Source>,
    b: Vec<Source>,
    tasks: Vec<Task>,
    settings: Settings,
    directory: Option<PathBuf>,
    batch: Option<Batch>,
    revision: u64,
    active: bool,
    paused: bool,
    worker: bool,
    notice: Option<String>,
    environment: EncoderStatus,
    encoders: Vec<Encoder>,
    cancellations: HashMap<String, Arc<AtomicBool>>,
    speed_samples: VecDeque<(Instant, f64)>,
    work_done: f64,
    started: Option<Instant>,
    completed_seconds: f64,
}

pub struct Service {
    inner: Mutex<Inner>,
    pub binaries: Option<Binaries>,
    importing: AtomicBool,
    stopping: AtomicBool,
    sampler: Mutex<Sampler>,
}

impl Service {
    /// 建立仅存于内存的应用服务，不创建设置库或运行日志。
    pub fn new(binaries: Option<Binaries>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                a: vec![],
                b: vec![],
                tasks: vec![],
                settings: Settings::default(),
                directory: None,
                batch: None,
                revision: 0,
                active: false,
                paused: false,
                worker: false,
                notice: None,
                environment: EncoderStatus {
                    detecting: true,
                    available: false,
                    accelerated: false,
                    label: "正在检测处理环境…".into(),
                    detail: "检测只在本次运行内存中保留。".into(),
                },
                encoders: vec![],
                cancellations: HashMap::new(),
                speed_samples: VecDeque::new(),
                work_done: 0.0,
                started: None,
                completed_seconds: 0.0,
            }),
            binaries,
            importing: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            sampler: Mutex::new(Sampler::default()),
        })
    }

    /// 每次锁定仅保护内存；不在编码 await 期间持有互斥锁。
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// 异步检测硬件路径，导入不等待检测完成。
    pub async fn initialize(&self) {
        let (status, encoders) = match &self.binaries {
            Some(binaries) => engine::detect(binaries).await,
            None => (
                EncoderStatus {
                    detecting: false,
                    available: false,
                    accelerated: false,
                    label: "媒体组件不可用".into(),
                    detail: "请安装包含 FFmpeg / ffprobe 的完整应用。".into(),
                },
                vec![],
            ),
        };
        let mut state = self.lock();
        state.environment = status;
        state.encoders = encoders;
        state.revision += 1;
    }

    /// 在批次冻结期间阻止输入、参数及目录变化。
    fn editable(&self, state: &Inner) -> Result<()> {
        if state.active || state.paused || state.worker || self.importing.load(Ordering::Relaxed) {
            return Err(AppError::new(
                "BATCH_LOCKED",
                "当前批次尚未结束，请完成或取消后再修改。",
            ));
        }
        if state.batch.is_some() && state.tasks.iter().any(|t| !t.view.state.terminal()) {
            return Err(AppError::new(
                "RECOVERY_REQUIRED",
                "还有未完成任务，请先重试、恢复或取消剩余任务。",
            ));
        }
        Ok(())
    }

    /// 将所有组合保留在队列，按 B 外层、A 内层生成稳定顺序。
    fn rebuild(state: &mut Inner) -> Result<()> {
        let count = state
            .a
            .len()
            .checked_mul(state.b.len())
            .ok_or_else(|| AppError::new("RESOURCE_LIMIT", "组合数量超出可处理范围。"))?;
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        if system.available_memory() > 0
            && (count as u128) * 2048 > u128::from(system.available_memory() / 4)
        {
            return Err(AppError::new(
                "RESOURCE_LIMIT",
                "当前可用内存不足以安全建立此批次，请减少输入视频数量。",
            ));
        }
        let mut tasks = Vec::with_capacity(count);
        let mut names = HashMap::<String, usize>::new();
        for (bi, b) in state.b.iter().enumerate() {
            for (ai, a) in state.a.iter().enumerate() {
                let id = Uuid::new_v4().to_string();
                let attempt = Uuid::new_v4().to_string();
                let stem = |s: &str| {
                    files::safe_stem(
                        Path::new(s)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .as_ref(),
                    )
                };
                let base = format!("{}__{}", stem(&a.view.name), stem(&b.view.name));
                let sequence = names
                    .entry(base.nfc().collect::<String>().to_lowercase())
                    .or_default();
                *sequence += 1;
                let name = if *sequence == 1 {
                    format!("{base}.mp4")
                } else {
                    format!("{base} ({sequence}).mp4")
                };
                let result = match (&a.media, &b.media) {
                    (Some(a), Some(b)) => plan::build(a, b, &state.settings),
                    _ => Err(AppError::new(
                        "PROBE_FAILED",
                        a.view
                            .issue
                            .as_ref()
                            .or(b.view.issue.as_ref())
                            .cloned()
                            .unwrap_or_else(|| "输入尚未就绪。".into()),
                    )),
                };
                let (plan, issue) = match result {
                    Ok(p) => (Some(p), None),
                    Err(e) => (None, Some(e.message)),
                };
                let status = if issue.is_some() {
                    TaskState::Blocked
                } else {
                    TaskState::Pending
                };
                let temporary = recovery::temporary_name(&id, &attempt)?;
                tasks.push(Task {
                    view: TaskView {
                        id: id.clone(),
                        name: name.clone(),
                        source_a: a.view.name.clone(),
                        source_b: b.view.name.clone(),
                        seconds: a.view.seconds.zip(b.view.seconds).map(|(x, y)| x + y),
                        canvas: plan.as_ref().map(|p| p.canvas.clone()),
                        state: status,
                        progress: 0.0,
                        issue,
                        warnings: plan.as_ref().map_or_else(Vec::new, |p| p.warnings.clone()),
                        result: None,
                    },
                    a: ai,
                    b: bi,
                    plan,
                    record: RecordTask {
                        id,
                        a: ai,
                        b: bi,
                        name,
                        attempt,
                        temporary,
                        state: status,
                        partial_identity: None,
                        committed: None,
                    },
                });
            }
        }
        state.tasks = tasks;
        state.batch = None;
        state.paused = false;
        state.notice = None;
        state.revision += 1;
        Ok(())
    }

    /// 导入原生选择器或已授权拖入路径，不复制视频；逐文件错误保留在列表。
    pub async fn import(&self, paths: Vec<PathBuf>, group: &str) -> Result<()> {
        if !matches!(group, "a" | "b") {
            return Err(AppError::new("INVALID_REQUEST", "视频组无效。"));
        }
        {
            let state = self.lock();
            self.editable(&state)?;
        }
        if self.importing.swap(true, Ordering::SeqCst) {
            return Err(AppError::new("BUSY", "正在读取视频，请稍候。"));
        }
        let mut additions = Vec::new();
        let mut skipped_directory = false;
        let mut identities = HashSet::new();
        {
            let state = self.lock();
            let sources = if group == "a" { &state.a } else { &state.b };
            for s in sources {
                identities.insert(
                    s.media
                        .as_ref()
                        .map_or_else(|| s.view.path.clone(), |m| m.fingerprint.identity.clone()),
                );
            }
        }
        for path in paths {
            if path.is_dir() {
                skipped_directory = true;
                continue;
            }
            let path = fs::canonicalize(&path).unwrap_or(path);
            let identity = files::fingerprint(&path)
                .map_or_else(|_| path.to_string_lossy().into_owned(), |f| f.identity);
            if !identities.insert(identity) {
                continue;
            }
            let id = Uuid::new_v4().to_string();
            let result = match &self.binaries {
                Some(b) => probe::probe(&b.ffprobe, &path, id.clone()).await,
                None => Err(AppError::new(
                    "ENGINE_UNAVAILABLE",
                    "缺少 FFprobe，请检查安装。",
                )),
            };
            additions.push(match result {
                Ok(media) => Source {
                    view: media.view.clone(),
                    media: Some(media),
                },
                Err(error) => Source {
                    view: Asset {
                        id,
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        path: path.to_string_lossy().into_owned(),
                        bytes: fs::metadata(&path).map_or(0.0, |m| m.len() as f64),
                        seconds: None,
                        width: None,
                        height: None,
                        fps: None,
                        issue: Some(error.message),
                    },
                    media: None,
                },
            });
        }
        let mut state = self.lock();
        if group == "a" {
            state.a.extend(additions);
        } else {
            state.b.extend(additions);
        }
        let result = Self::rebuild(&mut state);
        if skipped_directory {
            state.notice = Some("请导入视频文件，不支持文件夹。".into());
            state.revision += 1;
        }
        self.importing.store(false, Ordering::SeqCst);
        result
    }

    /// 移除或清空仅影响引用，从不删除源视频。
    pub fn remove(&self, group: &str, id: Option<&str>) -> Result<()> {
        let mut state = self.lock();
        self.editable(&state)?;
        let sources = match group {
            "a" => &mut state.a,
            "b" => &mut state.b,
            _ => return Err(AppError::new("INVALID_REQUEST", "视频组无效。")),
        };
        if let Some(id) = id {
            sources.retain(|a| a.view.id != id);
        } else {
            sources.clear();
        }
        Self::rebuild(&mut state)
    }

    /// 设置与计划在后端一起更新，界面不独立计算媒体参数。
    pub fn configure(&self, settings: Settings) -> Result<()> {
        let mut state = self.lock();
        self.editable(&state)?;
        state.settings = settings;
        Self::rebuild(&mut state)
    }

    /// 用户选择的目录就是最终目录，不额外建立子目录。
    pub fn set_directory(&self, directory: PathBuf) -> Result<()> {
        let mut state = self.lock();
        self.editable(&state)?;
        let directory = fs::canonicalize(directory)?;
        crate::runtime::validate_output(&directory)?;
        state.directory = Some(directory);
        state.revision += 1;
        Ok(())
    }

    /// 仅返回当前页任务，避免每次进度刷新传输整个组合列表。
    pub fn snapshot(&self, page: usize) -> Snapshot {
        let s = self.lock();
        let total = s.tasks.len();
        let count = |status| s.tasks.iter().filter(|t| t.view.state == status).count();
        let total_seconds = s.tasks.iter().filter_map(|t| t.view.seconds).sum::<f64>();
        let completed = s
            .tasks
            .iter()
            .map(|t| t.view.seconds.unwrap_or(0.0) * t.view.progress / 100.0)
            .sum::<f64>();
        let path = s
            .batch
            .as_ref()
            .map(|b| b.directory.to_string_lossy().into_owned())
            .or_else(|| {
                s.directory
                    .as_ref()
                    .map(|d| d.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "桌面 / 视频拼接_2026-10-17_153045".into());
        let page = page.min(total.saturating_sub(1) / PAGE_SIZE);
        Snapshot {
            revision: s.revision as f64,
            assets_a: s.a.iter().map(|a| a.view.clone()).collect(),
            assets_b: s.b.iter().map(|a| a.view.clone()).collect(),
            settings: s.settings.clone(),
            summary: Summary {
                total,
                runnable: s
                    .tasks
                    .iter()
                    .filter(|t| {
                        matches!(
                            t.view.state,
                            TaskState::Pending | TaskState::Interrupted | TaskState::Ready
                        )
                    })
                    .count(),
                blocked: count(TaskState::Blocked),
                succeeded: count(TaskState::Succeeded),
                failed: count(TaskState::Failed),
                canceled: count(TaskState::Canceled),
                progress: if total_seconds > 0.0 {
                    (completed / total_seconds * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                },
                active: s.active,
                paused: s.paused,
                locked: s.active
                    || s.paused
                    || s.worker
                    || self.importing.load(Ordering::Relaxed)
                    || s.batch.is_some() && s.tasks.iter().any(|t| !t.view.state.terminal()),
                output_directory: path,
                recovery_path: s
                    .batch
                    .as_ref()
                    .filter(|_| s.tasks.iter().any(|t| !t.view.state.terminal()))
                    .and_then(|b| recovery::record_path(&b.directory, &b.id).ok())
                    .map(|p| p.to_string_lossy().into_owned()),
                notice: s.notice.clone(),
            },
            tasks: s
                .tasks
                .iter()
                .skip(page * PAGE_SIZE)
                .take(PAGE_SIZE)
                .map(|t| t.view.clone())
                .collect(),
            page,
        }
    }

    /// 当前环境状态来自真实试编码，不从界面状态推断。
    pub fn environment(&self) -> EncoderStatus {
        self.lock().environment.clone()
    }

    /// 生成最小恢复快照；媒体元数据、进度和错误不落盘。
    fn record(s: &Inner) -> Result<Record> {
        let batch = s
            .batch
            .as_ref()
            .ok_or_else(|| AppError::new("INVALID_REQUEST", "当前没有导出批次。"))?;
        let asset = |a: &Source| RecordAsset {
            id: a.view.id.clone(),
            path: PathBuf::from(&a.view.path),
            fingerprint: a.media.as_ref().map(|m| m.fingerprint.clone()),
        };
        Ok(Record {
            schema_version: 1,
            batch_id: batch.id.clone(),
            settings: s.settings.clone(),
            assets_a: s.a.iter().map(asset).collect(),
            assets_b: s.b.iter().map(asset).collect(),
            tasks: s
                .tasks
                .iter()
                .map(|t| {
                    let mut r = t.record.clone();
                    r.state = t.view.state;
                    r
                })
                .collect(),
        })
    }

    /// 关键阶段同步保存，不写逐帧进度。
    fn persist(s: &Inner) -> Result<()> {
        let batch = s
            .batch
            .as_ref()
            .ok_or_else(|| AppError::new("INVALID_REQUEST", "没有导出批次。"))?;
        if s.tasks.iter().all(|t| t.view.state.terminal()) {
            let path = recovery::record_path(&batch.directory, &batch.id)?;
            if path.exists() {
                fs::remove_file(path)?;
                files::sync_directory(&batch.directory)?;
            }
            Ok(())
        } else {
            recovery::save(&batch.directory, &Self::record(s)?)
        }
    }

    /// 预检输入身份、空间、输出冲突和编码能力；相同请求不重复建立批次。
    pub fn start(self: &Arc<Self>, request: String, allow_blocked: bool) -> Result<()> {
        let mut s = self.lock();
        if s.batch.as_ref().is_some_and(|b| b.request_id == request) {
            return Ok(());
        }
        self.editable(&s)?;
        if !s.environment.available {
            return Err(AppError::new("ENGINE_UNAVAILABLE", &s.environment.detail));
        }
        if s.tasks.is_empty() {
            return Err(AppError::new("EMPTY_BATCH", "请先添加视频组 A 和 B。"));
        }
        for source in s.a.iter().chain(&s.b) {
            if let Some(media) = &source.media {
                if files::fingerprint(&media.path).ok().as_ref() != Some(&media.fingerprint) {
                    return Err(AppError::new(
                        "INPUT_CHANGED",
                        format!(
                            "源视频已变化或丢失：{}。请重新定位后导出。",
                            source.view.name
                        ),
                    ));
                }
            }
        }
        if !allow_blocked && s.tasks.iter().any(|t| t.view.state == TaskState::Blocked) {
            return Err(AppError::new(
                "PARTIAL_BATCH",
                "存在受阻组合，请明确选择是否只导出正常组合。",
            ));
        }
        if !s.tasks.iter().any(|t| t.plan.is_some()) {
            return Err(AppError::new(
                "EMPTY_BATCH",
                "没有可导出的组合，请检查输入。",
            ));
        }
        let directory = files::create_output(s.directory.as_deref())?;
        let estimated = s
            .tasks
            .iter()
            .filter_map(|t| t.plan.as_ref())
            .map(|p| {
                p.seconds
                    * f64::from(p.canvas.width)
                    * f64::from(p.canvas.height)
                    * p.canvas.fps.value()
                    * 0.10
                    / 8.0
            })
            .sum::<f64>();
        if metrics::available_space(&directory)
            .is_none_or(|bytes| (bytes as f64) < estimated + 256.0 * 1024.0 * 1024.0)
        {
            return Err(AppError::new(
                "INSUFFICIENT_SPACE",
                "无法确认输出卷空间足够。请释放空间或选择其他目录。",
            ));
        }
        let mut taken: HashSet<String> = fs::read_dir(&directory)?
            .filter_map(|e| e.ok())
            .map(|e| {
                e.file_name()
                    .to_string_lossy()
                    .nfc()
                    .collect::<String>()
                    .to_lowercase()
            })
            .collect();
        for task in &mut s.tasks {
            let stem = Path::new(&task.view.name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let mut name = task.view.name.clone();
            let mut n = 2;
            while !taken.insert(name.nfc().collect::<String>().to_lowercase()) {
                name = format!("{stem} ({n}).mp4");
                n += 1;
            }
            if directory.join(&name).as_os_str().len() > 240 && cfg!(windows) {
                return Err(AppError::new(
                    "INVALID_PATH",
                    "保存路径过长，请选择更短的输出目录。",
                ));
            }
            task.view.name = name.clone();
            task.record.name = name;
        }
        s.batch = Some(Batch {
            id: Uuid::new_v4().to_string(),
            request_id: request,
            directory: directory.clone(),
        });
        if let Err(e) = Self::persist(&s) {
            s.batch = None;
            return Err(e);
        }
        let estimated_label = if estimated < 1024.0_f64.powi(3) {
            format!("{:.1} MiB", estimated / 1024.0_f64.powi(2))
        } else {
            format!("{:.1} GiB", estimated / 1024.0_f64.powi(3))
        };
        s.notice = Some(format!(
            "预计空间约 {estimated_label}（实际大小随内容变化）；输出至 {}。",
            directory.display()
        ));
        s.active = true;
        s.paused = false;
        s.started = Some(Instant::now());
        s.work_done = 0.0;
        s.completed_seconds = 0.0;
        s.speed_samples.clear();
        s.revision += 1;
        drop(s);
        self.spawn_worker();
        Ok(())
    }

    /// 调度保持单个在途任务，优先稳定吞吐及有界内存，不同时展开全部组合。
    fn spawn_worker(self: &Arc<Self>) {
        {
            let mut s = self.lock();
            if s.worker {
                return;
            }
            s.worker = true;
        }
        let service = Arc::clone(self);
        tokio::spawn(async move {
            service.run_queue().await;
        });
    }

    /// 暂停只停止后续调度，当前视频继续到提交。
    pub fn pause(&self) -> Result<()> {
        let mut s = self.lock();
        if !s.active {
            return Err(AppError::new("INVALID_REQUEST", "当前没有运行队列。"));
        }
        s.paused = true;
        s.revision += 1;
        Self::persist(&s)
    }

    /// 继续暂停或恢复后的待处理任务，不重跑完成项。
    pub fn resume(self: &Arc<Self>) -> Result<()> {
        {
            let mut s = self.lock();
            if s.batch.is_none() {
                return Err(AppError::new("INVALID_REQUEST", "没有可继续的批次。"));
            }
            if s.worker {
                return Err(AppError::new("BUSY", "请等待当前视频完成。"));
            }
            s.paused = false;
            s.active = true;
            s.speed_samples.clear();
            s.revision += 1;
        }
        self.spawn_worker();
        Ok(())
    }

    /// 先持久化取消意图，再终止对应处理进程；成功视频始终保留。
    pub fn cancel(&self, id: Option<&str>) -> Result<()> {
        let mut s = self.lock();
        if s.batch.is_none() {
            return Err(AppError::new("INVALID_REQUEST", "当前没有导出批次。"));
        }
        let directory = s.batch.as_ref().unwrap().directory.clone();
        for task in &mut s.tasks {
            if task.view.state.terminal() || id.is_some_and(|id| task.view.id != id) {
                continue;
            }
            task.view.state = TaskState::Canceling;
        }
        Self::persist(&s)?;
        for index in 0..s.tasks.len() {
            if s.tasks[index].view.state != TaskState::Canceling {
                continue;
            }
            if let Some(cancel) = s.cancellations.get(&s.tasks[index].view.id) {
                cancel.store(true, Ordering::Relaxed);
            } else {
                recovery::clean_partial(&directory, &s.tasks[index].record)?;
                s.tasks[index].view.state = TaskState::Canceled;
                s.tasks[index].view.progress = 0.0;
            }
        }
        Self::persist(&s)?;
        if s.tasks.iter().all(|t| t.view.state.terminal()) {
            s.paused = false;
        }
        s.revision += 1;
        Ok(())
    }

    /// 只重试失败和已修复的阻断项，取消项不能恢复。
    pub fn retry(self: &Arc<Self>, id: Option<&str>) -> Result<()> {
        {
            let mut s = self.lock();
            if s.worker {
                return Err(AppError::new("BUSY", "请等待当前任务完成。"));
            }
            for task in &mut s.tasks {
                if matches!(
                    task.view.state,
                    TaskState::Failed | TaskState::Blocked | TaskState::Interrupted
                ) && task.plan.is_some()
                    && id.is_none_or(|id| task.view.id == id)
                {
                    task.view.state = TaskState::Pending;
                    task.view.progress = 0.0;
                    task.view.issue = None;
                }
            }
            Self::persist(&s)?;
            s.paused = false;
            s.active = true;
            s.revision += 1;
        }
        self.spawn_worker();
        Ok(())
    }

    /// 只打开当前批次中可证明归属且仍存在的成果或目录。
    pub fn output_path(&self, id: Option<&str>) -> Result<PathBuf> {
        let s = self.lock();
        let batch = s
            .batch
            .as_ref()
            .ok_or_else(|| AppError::new("INVALID_REQUEST", "尚未建立导出目录。"))?;
        let path = if let Some(id) = id {
            let task = s
                .tasks
                .iter()
                .find(|t| t.view.id == id && t.view.state == TaskState::Succeeded)
                .ok_or_else(|| AppError::new("INVALID_REQUEST", "此任务尚未完成。"))?;
            files::owned_child(&batch.directory, &task.view.name)?
        } else {
            batch.directory.clone()
        };
        if !path.exists() {
            return Err(AppError::new(
                "INPUT_MISSING",
                "结果或目录已不存在，请检查文件是否被移动。",
            ));
        }
        Ok(path)
    }

    /// 有界速度窗口按实际输出时间增量计算，回退会清空窗口，暂停不倒计时。
    pub fn performance(&self) -> Performance {
        let (directory, speed, eta) = {
            let s = self.lock();
            let directory = s
                .batch
                .as_ref()
                .map(|b| b.directory.clone())
                .or_else(|| s.directory.clone())
                .or_else(dirs::desktop_dir)
                .unwrap_or_else(std::env::temp_dir);
            let speed = if s.active && !s.paused {
                s.speed_samples
                    .front()
                    .zip(s.speed_samples.back())
                    .and_then(|((t0, v0), (_, v1))| {
                        let seconds = t0.elapsed().as_secs_f64();
                        (seconds >= 1.0 && v1 > v0).then_some((v1 - v0) / seconds)
                    })
            } else {
                None
            };
            let remaining = s
                .tasks
                .iter()
                .filter(|t| !t.view.state.terminal())
                .map(|t| t.view.seconds.unwrap_or(0.0) * (1.0 - t.view.progress / 100.0))
                .sum::<f64>();
            let eta = speed
                .filter(|speed| *speed > 0.0)
                .map(|speed| remaining / speed);
            (directory, speed, eta)
        };
        self.sampler
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .sample(&directory, speed, eta)
    }

    /// 退出停止受控进程并保留中断状态，不自动取消，也不删除输出目录。
    pub async fn shutdown(&self) -> Result<()> {
        self.stopping.store(true, Ordering::SeqCst);
        {
            let mut s = self.lock();
            s.active = false;
            s.paused = true;
            for cancel in s.cancellations.values() {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        for _ in 0..400 {
            if !self.lock().worker {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let mut s = self.lock();
        if s.worker {
            return Err(AppError::new(
                "BUSY",
                "视频处理进程尚未停止，请稍后再退出。",
            ));
        }
        for task in &mut s.tasks {
            if matches!(
                task.view.state,
                TaskState::Pending
                    | TaskState::Ready
                    | TaskState::Running
                    | TaskState::Validating
                    | TaskState::Preflight
            ) {
                task.view.state = TaskState::Interrupted;
            }
        }
        let result = if s.batch.is_some() {
            Self::persist(&s)
        } else {
            Ok(())
        };
        self.stopping.store(false, Ordering::SeqCst);
        result
    }
}
