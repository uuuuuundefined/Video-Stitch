use super::*;

impl Service {
    /// 推进处理、校验、提交阶段；失败保持恢复记录，不影响已完成结果。
    pub(super) async fn run_queue(self: Arc<Self>) {
        loop {
            let next = {
                let mut s = self.lock();
                if self.stopping.load(Ordering::Relaxed) || s.paused || !s.active {
                    None
                } else {
                    s.tasks
                        .iter()
                        .position(|t| {
                            matches!(
                                t.view.state,
                                TaskState::Pending | TaskState::Ready | TaskState::Interrupted
                            )
                        })
                        .map(|i| {
                            let cancel = Arc::new(AtomicBool::new(false));
                            let id = s.tasks[i].view.id.clone();
                            s.cancellations.insert(id, cancel.clone());
                            (i, cancel)
                        })
                }
            };
            let Some((index, cancel)) = next else { break };
            let result = self.execute(index, cancel).await;
            let mut s = self.lock();
            let id = s.tasks[index].view.id.clone();
            s.cancellations.remove(&id);
            if let Err(error) = result {
                let stopping = self.stopping.load(Ordering::Relaxed);
                let task = &mut s.tasks[index];
                task.view.state = if task.view.state == TaskState::Succeeded {
                    TaskState::Succeeded
                } else if task.view.state == TaskState::Canceling {
                    TaskState::Canceled
                } else if stopping {
                    TaskState::Interrupted
                } else if task.view.state == TaskState::Committing {
                    TaskState::Committing
                } else {
                    TaskState::Failed
                };
                if task.view.state != TaskState::Succeeded {
                    task.view.progress = 0.0;
                }
                task.view.issue = Some(error.message);
                let commit_uncertain = task.view.state == TaskState::Committing;
                let was_canceled = task.view.state == TaskState::Canceled;
                if commit_uncertain {
                    s.paused = true;
                    s.notice =
                        Some("提交阶段需要核对，请停止并退出后选择恢复记录，避免重复生成。".into());
                }
                // 取消不能因清理失败谎称成功；残片归属错误保留为 canceling。
                if was_canceled {
                    if let Some(directory) = s.batch.as_ref().map(|b| b.directory.clone()) {
                        if let Err(error) =
                            recovery::clean_partial(&directory, &s.tasks[index].record)
                        {
                            s.tasks[index].view.state = TaskState::Canceling;
                            s.tasks[index].view.issue = Some(error.message);
                        }
                    }
                }
                if matches!(
                    error.code.as_str(),
                    "INSUFFICIENT_SPACE" | "OUTPUT_NOT_WRITABLE" | "RECOVERY_REQUIRED"
                ) {
                    s.paused = true;
                    s.notice = Some("保存目录或恢复记录需要处理，队列已停止调度。".into());
                }
            }
            if let Err(error) = Self::persist(&s) {
                s.paused = true;
                s.notice = Some(format!(
                    "恢复状态未能保存：{}。请保留当前窗口并检查输出目录。",
                    error.message
                ));
            }
            s.revision += 1;
        }
        let mut s = self.lock();
        s.worker = false;
        s.active = false;
        if s.tasks.iter().all(|task| task.view.state.terminal()) {
            s.paused = false;
        }
        if s.environment.available {
            s.environment.label = if s.environment.accelerated {
                "自动加速 · 已就绪"
            } else {
                "兼容处理 · 已就绪"
            }
            .into();
        }
        s.revision += 1;
    }

    /// 在每个关键阶段落盘后执行外部操作，保证中断恢复的归属可判定。
    async fn execute(&self, index: usize, cancel: Arc<AtomicBool>) -> Result<()> {
        let binaries = self
            .binaries
            .as_ref()
            .ok_or_else(|| AppError::new("ENGINE_UNAVAILABLE", "媒体组件不可用。"))?;
        let (a, b, plan, directory, mut candidates) = {
            let mut s = self.lock();
            let task = &s.tasks[index];
            let a = s.a[task.a]
                .media
                .clone()
                .ok_or_else(|| AppError::new("PROBE_FAILED", "视频 A 需要重新定位。"))?;
            let b = s.b[task.b]
                .media
                .clone()
                .ok_or_else(|| AppError::new("PROBE_FAILED", "视频 B 需要重新定位。"))?;
            let p = task
                .plan
                .clone()
                .ok_or_else(|| AppError::new("INVALID_PLAN", "处理计划无效。"))?;
            let directory = s
                .batch
                .as_ref()
                .ok_or_else(|| AppError::new("INVALID_PLAN", "没有导出目录。"))?
                .directory
                .clone();
            s.tasks[index].view.state = TaskState::Preflight;
            s.tasks[index].view.issue = None;
            s.revision += 1;
            Self::persist(&s)?;
            (a, b, p, directory, s.encoders.clone())
        };
        for media in [&a, &b] {
            if files::fingerprint(&media.path)? != media.fingerprint {
                return Err(AppError::new(
                    "INPUT_CHANGED",
                    format!("源视频已变更：{}。请重新定位。", media.view.name),
                ));
            }
        }
        if metrics::available_space(&directory).is_none_or(|n| n < 64 * 1024 * 1024) {
            return Err(AppError::new(
                "INSUFFICIENT_SPACE",
                "输出卷剩余空间不足或磁盘已断开。",
            ));
        }
        let old = self.lock().tasks[index].record.clone();
        recovery::clean_partial(&directory, &old)?;
        if plan.copy {
            candidates.insert(
                0,
                Encoder {
                    name: "copy".into(),
                    hardware: false,
                    elapsed: 0.0,
                },
            );
        }
        let mut last_error = AppError::new("ENGINE_UNAVAILABLE", "没有已验证的编码器。");
        for encoder in candidates {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::new("CANCELED", "已停止导出。"));
            }
            let (partial, attempt) = {
                let mut s = self.lock();
                let task = &mut s.tasks[index];
                task.record.attempt = Uuid::new_v4().to_string();
                task.record.temporary =
                    recovery::temporary_name(&task.view.id, &task.record.attempt)?;
                let partial = files::owned_child(&directory, &task.record.temporary)?;
                files::reserve_partial(&partial)?.sync_all()?;
                task.record.partial_identity = Some(files::fingerprint(&partial)?.identity);
                task.view.state = TaskState::Running;
                task.view.progress = 0.0;
                let attempt = task.record.attempt.clone();
                s.environment.accelerated = encoder.hardware;
                s.environment.label = if encoder.name == "copy" {
                    "直接拼接 · 正在处理"
                } else if encoder.hardware {
                    "自动加速 · 正在处理"
                } else {
                    "兼容处理 · 正在处理"
                }
                .into();
                s.revision += 1;
                Self::persist(&s)?;
                (partial, attempt)
            };
            let (args, input) = engine::arguments(
                &a,
                &b,
                &plan,
                &encoder.name,
                &partial,
                encoder.name == "copy",
            );
            let mut previous = 0.0;
            let mut disk_check = Instant::now();
            let result =
                process::render(&binaries.ffmpeg, &args, input, cancel.clone(), |seconds| {
                    let mut s = self.lock();
                    if s.tasks[index].record.attempt != attempt {
                        return;
                    }
                    let seconds = seconds.min(plan.seconds);
                    let delta = (seconds - previous).max(0.0);
                    previous = seconds;
                    s.work_done += delta;
                    s.tasks[index].view.progress = (seconds / plan.seconds * 99.0).clamp(0.0, 99.0);
                    let work = s.work_done;
                    s.speed_samples.push_back((Instant::now(), work));
                    while s
                        .speed_samples
                        .front()
                        .is_some_and(|(t, _)| t.elapsed() > Duration::from_secs(6))
                    {
                        s.speed_samples.pop_front();
                    }
                    s.revision += 1;
                    if disk_check.elapsed() > Duration::from_secs(2) {
                        if metrics::available_space(&directory).is_none_or(|n| n < 16 * 1024 * 1024)
                        {
                            cancel.store(true, Ordering::Relaxed);
                            s.notice = Some("输出卷空间不足或断开，当前任务已停止。".into());
                            s.paused = true;
                        }
                        disk_check = Instant::now();
                    }
                })
                .await;
            match result {
                Ok(()) => {
                    {
                        let mut s = self.lock();
                        if cancel.load(Ordering::Relaxed)
                            || s.tasks[index].view.state == TaskState::Canceling
                        {
                            return Err(AppError::new("CANCELED", "已停止导出。"));
                        }
                        s.tasks[index].view.state = TaskState::Validating;
                        s.revision += 1;
                        Self::persist(&s)?;
                    }
                    let info = engine::validate(
                        binaries,
                        &partial,
                        &plan,
                        a.audio.is_some() || b.audio.is_some(),
                    )
                    .await;
                    if let Err(error) = info {
                        last_error = error;
                    } else {
                        if cancel.load(Ordering::Relaxed) {
                            return Err(AppError::new("CANCELED", "导出已停止。"));
                        }
                        let info = info?;
                        fs::File::open(&partial)?.sync_all()?;
                        let identity = files::fingerprint(&partial)?;
                        let mut s = self.lock();
                        if cancel.load(Ordering::Relaxed)
                            || s.tasks[index].view.state == TaskState::Canceling
                        {
                            return Err(AppError::new("CANCELED", "已停止导出。"));
                        }
                        s.tasks[index].view.state = TaskState::Committing;
                        s.tasks[index].record.committed = Some(identity);
                        s.revision += 1;
                        Self::persist(&s)?;
                        let target = files::owned_child(&directory, &s.tasks[index].view.name)?;
                        files::commit_no_replace(&partial, &target)?;
                        s.tasks[index].view.result = Some(info);
                        s.tasks[index].view.state = TaskState::Succeeded;
                        s.tasks[index].view.progress = 100.0;
                        s.completed_seconds += plan.seconds;
                        s.revision += 1;
                        Self::persist(&s)?;
                        return Ok(());
                    }
                }
                Err(error) => last_error = error,
            }
            if !matches!(
                last_error.code.as_str(),
                "HARDWARE_PATH_FAILED" | "PROCESS_FAILED" | "OUTPUT_INVALID"
            ) {
                return Err(last_error);
            }
            if !encoder.hardware && encoder.name != "copy" {
                return Err(last_error);
            }
            let mut s = self.lock();
            recovery::clean_partial(&directory, &s.tasks[index].record)?;
            s.work_done = (s.work_done - previous).max(0.0);
            s.speed_samples.clear();
            s.tasks[index].view.warnings.push(format!(
                "{}；已回退到下一条已验证路径。",
                last_error.message
            ));
            s.notice = Some("当前处理路径未能完成输入，正在采用兼容路径重新处理。".into());
        }
        Err(last_error)
    }
}
