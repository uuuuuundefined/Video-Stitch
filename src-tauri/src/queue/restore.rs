use super::*;

impl Service {
    /// 手动恢复并重新探测；提交窗口中断时通过文件指纹认领已完成结果。
    pub async fn recover(&self, path: &Path) -> Result<()> {
        {
            let s = self.lock();
            self.editable(&s)?;
        }
        let (directory, record) = recovery::load(path)?;
        let binaries = self
            .binaries
            .as_ref()
            .ok_or_else(|| AppError::new("ENGINE_UNAVAILABLE", "缺少媒体组件。"))?;
        let mut a = Vec::new();
        let mut b = Vec::new();
        for (records, target) in [(&record.assets_a, &mut a), (&record.assets_b, &mut b)] {
            for asset in records {
                let result = probe::probe(&binaries.ffprobe, &asset.path, asset.id.clone()).await;
                target.push(match result {
                    Ok(mut m) => {
                        if asset.fingerprint.as_ref() != Some(&m.fingerprint) {
                            m.view.issue = Some("源文件已变更，请重新定位确认后重试。".into());
                        }
                        Source {
                            view: m.view.clone(),
                            media: Some(m),
                        }
                    }
                    Err(error) => Source {
                        view: Asset {
                            id: asset.id.clone(),
                            name: asset
                                .path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                            path: asset.path.to_string_lossy().into_owned(),
                            bytes: 0.0,
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
        }
        let mut tasks = Vec::with_capacity(record.tasks.len());
        for mut r in record.tasks {
            let source_a = &a[r.a];
            let source_b = &b[r.b];
            let plan = match (&source_a.media, &source_b.media) {
                (Some(a), Some(b)) => plan::build(a, b, &record.settings).ok(),
                _ => None,
            };
            let mut result = None;
            let mut issue = None;
            if matches!(r.state, TaskState::Succeeded | TaskState::Committing) {
                let target = files::owned_child(&directory, &r.name)?;
                if r.committed
                    .as_ref()
                    .is_some_and(|fp| files::fingerprint(&target).ok().as_ref() == Some(fp))
                {
                    r.state = TaskState::Succeeded;
                    if let Some(p) = &plan {
                        result = engine::validate(
                            binaries,
                            &target,
                            p,
                            source_a.media.as_ref().is_some_and(|m| m.audio.is_some())
                                || source_b.media.as_ref().is_some_and(|m| m.audio.is_some()),
                        )
                        .await
                        .ok();
                    }
                } else if r.state == TaskState::Succeeded {
                    r.state = TaskState::Blocked;
                    issue = Some("已完成结果已缺失或被修改，保留用户文件，不自动重做。".into());
                } else if target.exists() {
                    r.state = TaskState::Blocked;
                    issue = Some("正式文件身份不符，请检查输出冲突。".into());
                } else {
                    r.state = TaskState::Interrupted;
                }
            }
            if r.state == TaskState::Canceling {
                recovery::clean_partial(&directory, &r)?;
                r.state = TaskState::Canceled;
            } else if !r.state.terminal() {
                recovery::clean_partial(&directory, &r)?;
                if plan.is_none() {
                    r.state = TaskState::Blocked;
                    issue = issue
                        .or_else(|| source_a.view.issue.clone().or(source_b.view.issue.clone()));
                } else if !matches!(r.state, TaskState::Blocked | TaskState::Failed) {
                    r.state = TaskState::Interrupted;
                }
            }
            let view = TaskView {
                id: r.id.clone(),
                name: r.name.clone(),
                source_a: source_a.view.name.clone(),
                source_b: source_b.view.name.clone(),
                seconds: source_a
                    .view
                    .seconds
                    .zip(source_b.view.seconds)
                    .map(|(a, b)| a + b),
                canvas: plan.as_ref().map(|p| p.canvas.clone()),
                state: r.state,
                progress: if r.state == TaskState::Succeeded {
                    100.0
                } else {
                    0.0
                },
                issue,
                warnings: plan.as_ref().map_or_else(Vec::new, |p| p.warnings.clone()),
                result,
            };
            tasks.push(Task {
                view,
                a: r.a,
                b: r.b,
                plan,
                record: r,
            });
        }
        let mut s = self.lock();
        self.editable(&s)?;
        s.a = a;
        s.b = b;
        s.settings = record.settings;
        s.tasks = tasks;
        s.directory = Some(directory.clone());
        s.batch = Some(Batch {
            id: record.batch_id,
            request_id: Uuid::new_v4().to_string(),
            directory,
        });
        s.paused = s.tasks.iter().any(|t| !t.view.state.terminal());
        s.notice = Some("已重新检查源文件。中断项从头处理，已完成项保留，取消项不恢复。".into());
        s.revision += 1;
        Self::persist(&s)
    }

    /// 重新定位受影响源文件，重新生成相关未完成计划；不改动成功项。
    pub async fn relink(&self, id: &str, path: &Path) -> Result<()> {
        {
            let s = self.lock();
            if s.worker || s.active {
                return Err(AppError::new("BUSY", "请暂停并等待当前项完成后重新定位。"));
            }
        }
        let binaries = self
            .binaries
            .as_ref()
            .ok_or_else(|| AppError::new("ENGINE_UNAVAILABLE", "媒体组件不可用。"))?;
        let media = probe::probe(&binaries.ffprobe, path, id.to_string()).await?;
        let mut s = self.lock();
        let mut found = false;
        for source in &mut s.a {
            if source.view.id == id {
                source.view = media.view.clone();
                source.media = Some(media.clone());
                found = true;
            }
        }
        for source in &mut s.b {
            if source.view.id == id {
                source.view = media.view.clone();
                source.media = Some(media.clone());
                found = true;
            }
        }
        if !found {
            return Err(AppError::new("INVALID_REQUEST", "未找到对应源文件。"));
        }
        if s.batch.is_none() {
            return Self::rebuild(&mut s);
        }
        let a = s.a.clone();
        let b = s.b.clone();
        let settings = s.settings.clone();
        for task in &mut s.tasks {
            if task.view.state.terminal() {
                continue;
            }
            let result = match (&a[task.a].media, &b[task.b].media) {
                (Some(a), Some(b)) => plan::build(a, b, &settings),
                _ => Err(AppError::new("INPUT_MISSING", "请重新定位缺失源文件。")),
            };
            match result {
                Ok(plan) => {
                    task.view.seconds = Some(plan.seconds);
                    task.view.canvas = Some(plan.canvas.clone());
                    task.view.warnings = plan.warnings.clone();
                    task.view.issue = None;
                    task.view.state = TaskState::Interrupted;
                    task.plan = Some(plan);
                }
                Err(e) => {
                    task.view.issue = Some(e.message);
                    task.view.state = TaskState::Blocked;
                    task.plan = None;
                }
            }
        }
        s.revision += 1;
        Self::persist(&s)
    }
}
