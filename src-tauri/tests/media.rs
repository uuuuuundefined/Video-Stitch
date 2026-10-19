use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use video_stitch::{
    contracts::{Ratio, Settings, TaskState, Transition},
    engine::{self, Binaries},
    plan, probe, process,
    queue::Service,
};

/// 在临时测试目录生成运动画面和可选测试音，不成为应用业务文件。
async fn fixture(b: &Binaries, path: &Path, size: &str, fps: &str, seconds: &str, audio: bool) {
    let mut args = vec![
        "-hide_banner".into(),
        "-v".into(),
        "error".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("testsrc2=size={size}:rate={fps}:duration={seconds}"),
    ];
    if audio {
        args.extend([
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!("sine=frequency=440:sample_rate=48000:duration={seconds}"),
        ]);
    }
    args.extend(
        [
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
        ]
        .into_iter()
        .map(str::to_string),
    );
    if audio {
        args.extend(["-c:a", "aac"].into_iter().map(str::to_string));
    }
    args.push(path.to_string_lossy().into_owned());
    process::capture(&b.ffmpeg, &args, 60).await.unwrap();
}

/// 对实际输出完整解码并读取真实帧数，不只检查编码进程退出码。
async fn frame_count(b: &Binaries, path: &Path) -> u64 {
    let args = vec![
        "-v".into(),
        "error".into(),
        "-count_frames".into(),
        "-select_streams".into(),
        "v:0".into(),
        "-show_entries".into(),
        "stream=nb_read_frames".into(),
        "-of".into(),
        "default=nw=1:nk=1".into(),
        path.to_string_lossy().into_owned(),
    ];
    let bytes = process::capture(&b.ffprobe, &args, 60).await.unwrap();
    let decode = vec![
        "-v".into(),
        "error".into(),
        "-xerror".into(),
        "-i".into(),
        path.to_string_lossy().into_owned(),
        "-f".into(),
        "null".into(),
        "-".into(),
    ];
    process::capture(&b.ffmpeg, &decode, 60).await.unwrap();
    String::from_utf8(bytes).unwrap().trim().parse().unwrap()
}

#[tokio::test]
#[ignore = "requires real FFmpeg"]
async fn fixed_duration_fade_is_600_frames_and_ten_seconds() {
    let b = engine::locate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("素材 A.mp4");
    let p = dir.path().join("正片 B.mp4");
    fixture(&b, &a, "960x540", "60", "5", true).await;
    fixture(&b, &p, "540x960", "60", "5", true).await;
    let a = probe::probe(&b.ffprobe, &a, "a".into()).await.unwrap();
    let p = probe::probe(&b.ffprobe, &p, "b".into()).await.unwrap();
    let plan = plan::build(
        &a,
        &p,
        &Settings {
            ratio: Ratio::FollowB,
            transition: Transition::BoundaryFade,
        },
    )
    .unwrap();
    assert_eq!(plan.half_transition, 15);
    let out = dir.path().join("output.mp4");
    let info = engine::render_software(&b, &a, &p, &plan, &out)
        .await
        .unwrap();
    assert_eq!(frame_count(&b, &out).await, 600);
    assert!((info.seconds - 10.0).abs() < 0.00001);
    assert_eq!((info.width, info.height), (540, 960));
}

#[tokio::test]
#[ignore = "requires real FFmpeg"]
async fn mixed_rates_silent_audio_short_clips_and_copy_are_valid() {
    let b = engine::locate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    for (i, fps) in ["24", "25", "30000/1001", "30", "50", "60000/1001", "60"]
        .into_iter()
        .enumerate()
    {
        let ap = dir.path().join(format!("a{i}.mp4"));
        let bp = dir.path().join(format!("b{i}.mp4"));
        fixture(&b, &ap, "128x72", "60", "0.8", false).await;
        fixture(&b, &bp, "72x128", fps, "0.7", true).await;
        let a = probe::probe(&b.ffprobe, &ap, "a".into()).await.unwrap();
        let p = probe::probe(&b.ffprobe, &bp, "b".into()).await.unwrap();
        for transition in [Transition::None, Transition::BoundaryFade] {
            let plan = plan::build(
                &a,
                &p,
                &Settings {
                    ratio: Ratio::Square,
                    transition,
                },
            )
            .unwrap();
            let out = dir.path().join(format!("out{i}-{transition:?}.mp4"));
            let info = engine::render_software(&b, &a, &p, &plan, &out)
                .await
                .unwrap();
            assert!((info.seconds - plan.seconds).abs() <= 2.0 / p.fps.value());
            assert_eq!(frame_count(&b, &out).await, plan.a_frames + plan.b_frames);
        }
    }
    let ap = dir.path().join("单帧.mp4");
    let bp = dir.path().join("另一个.mp4");
    fixture(&b, &ap, "128x72", "30", "0.033333", false).await;
    fixture(&b, &bp, "128x72", "30", "1", false).await;
    let a = probe::probe(&b.ffprobe, &ap, "a".into()).await.unwrap();
    let p = probe::probe(&b.ffprobe, &bp, "b".into()).await.unwrap();
    let short = plan::build(
        &a,
        &p,
        &Settings {
            ratio: Ratio::FollowB,
            transition: Transition::BoundaryFade,
        },
    )
    .unwrap();
    assert_eq!(short.half_transition, 0);
    let out = dir.path().join("short.mp4");
    engine::render_software(&b, &a, &p, &short, &out)
        .await
        .unwrap();
    assert_eq!(frame_count(&b, &out).await, 31);
    let copy = plan::build(&p, &p, &Settings::default()).unwrap();
    assert!(copy.copy);
    let output = dir.path().join("copy.mp4");
    let (args, input) = engine::arguments(&p, &p, &copy, "copy", &output, true);
    process::render(
        &b.ffmpeg,
        &args,
        input,
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .await
    .unwrap();
    engine::validate(&b, &output, &copy, false).await.unwrap();
    assert_eq!(frame_count(&b, &output).await, 60);
}

/// 超长音轨不阻断输入，接缝后应播放 B 的静音而不是 A 的尾音。
#[tokio::test]
#[ignore = "requires real FFmpeg"]
async fn audio_outside_video_interval_is_trimmed_without_blocking() {
    use sha2::{Digest, Sha256};

    let b = engine::locate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("long-audio.mp4");
    let args: Vec<String> = [
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=128x72:rate=30:duration=1",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000:duration=2",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
    ]
    .into_iter()
    .map(str::to_string)
    .chain([source.to_string_lossy().into_owned()])
    .collect();
    process::capture(&b.ffmpeg, &args, 60).await.unwrap();
    let bp = dir.path().join("silent-b.mp4");
    fixture(&b, &bp, "128x72", "30", "0.5", false).await;
    let p = probe::probe(&b.ffprobe, &bp, "b".into()).await.unwrap();
    let mut inputs = vec![source];
    // 可选真实素材复用相同正式处理和验收路径，不在仓库保存用户路径。
    if let Some(path) = std::env::var_os("VIDEO_STITCH_QA_SOURCE") {
        inputs.push(PathBuf::from(path));
    }
    for (index, source) in inputs.iter().enumerate() {
        let a = probe::probe(&b.ffprobe, source, "a".into()).await.unwrap();
        assert!(a.view.issue.is_none(), "{:?}", a.view.issue);
        let audio = a.audio.as_ref().unwrap();
        assert!(audio.start + audio.seconds > a.seconds);
        for transition in [Transition::None, Transition::BoundaryFade] {
            let plan = plan::build(
                &a,
                &p,
                &Settings {
                    ratio: Ratio::FollowB,
                    transition,
                },
            )
            .unwrap();
            assert!(!plan.copy);
            let out = dir
                .path()
                .join(format!("aligned-{index}-{transition:?}.mp4"));
            engine::render_software(&b, &a, &p, &plan, &out)
                .await
                .unwrap();
            assert_eq!(frame_count(&b, &out).await, plan.a_frames + plan.b_frames);
            let seek = plan.a_frames as f64 / p.fps.value() + 0.15;
            let args: Vec<String> = ["-v", "error", "-i"]
                .into_iter()
                .map(str::to_string)
                .chain([
                    out.to_string_lossy().into_owned(),
                    "-ss".into(),
                    format!("{seek:.9}"),
                ])
                .chain(
                    [
                        "-t",
                        "0.2",
                        "-map",
                        "0:a:0",
                        "-c:a",
                        "pcm_s16le",
                        "-f",
                        "framehash",
                        "-",
                    ]
                    .into_iter()
                    .map(str::to_string),
                )
                .collect();
            let hashes = process::capture(&b.ffmpeg, &args, 60).await.unwrap();
            let hashes = String::from_utf8(hashes).unwrap();
            let mut packets = 0;
            for line in hashes
                .lines()
                .filter(|line| !line.starts_with('#') && !line.is_empty())
            {
                let fields: Vec<_> = line.split(',').map(str::trim).collect();
                let bytes: usize = fields[4].parse().unwrap();
                assert_eq!(
                    fields[5],
                    format!("{:x}", Sha256::digest(vec![0; bytes])),
                    "A 的尾音进入了 B：{line}"
                );
                packets += 1;
            }
            assert!(packets > 0, "B 的静音区间没有解码出音频");
        }
    }
}

/// 等待真实队列完成；超时会给出当前任务状态以便定位失败。
async fn wait(service: &Service) {
    for _ in 0..600 {
        let snapshot = service.snapshot(0);
        if !snapshot.summary.active && !snapshot.summary.paused {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("queue timed out: {:?}", service.snapshot(0));
}

#[tokio::test]
#[ignore = "requires real FFmpeg"]
async fn cartesian_queue_exports_four_real_files_and_cleans_record() {
    let b = engine::locate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("导出");
    std::fs::create_dir(&output).unwrap();
    let a = dir.path().join("素材.mp4");
    let c = dir.path().join("正片.mp4");
    fixture(&b, &a, "128x72", "30", "1", true).await;
    fixture(&b, &c, "72x128", "25", "1", false).await;
    let service = Service::new(Some(b.clone()));
    service.initialize().await;
    service
        .import(vec![a.clone(), c.clone(), a.clone()], "a")
        .await
        .unwrap();
    service.import(vec![a, c], "b").await.unwrap();
    service.set_directory(output.clone()).unwrap();
    service
        .configure(Settings {
            ratio: Ratio::Square,
            transition: Transition::BoundaryFade,
        })
        .unwrap();
    assert_eq!(service.snapshot(0).summary.total, 4);
    service.start("request".into(), false).unwrap();
    service.start("request".into(), false).unwrap();
    wait(&service).await;
    let snapshot = service.snapshot(0);
    assert_eq!(snapshot.summary.succeeded, 4, "{snapshot:?}");
    let files: Vec<PathBuf> = std::fs::read_dir(&output)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(files.len(), 4);
    for path in files {
        assert!(path.extension().is_some_and(|x| x == "mp4"));
        frame_count(&b, &path).await;
    }
    assert!(snapshot
        .tasks
        .iter()
        .all(|t| t.state == TaskState::Succeeded));
}

#[tokio::test]
#[ignore = "requires real FFmpeg"]
async fn pause_cancel_shutdown_and_manual_recovery_preserve_intent() {
    let binaries = engine::locate().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let a = directory.path().join("a.mp4");
    let b = directory.path().join("b.mp4");
    fixture(&binaries, &a, "160x90", "30", "1", true).await;
    fixture(&binaries, &b, "90x160", "30", "1", false).await;
    let output = directory.path().join("output");
    std::fs::create_dir(&output).unwrap();
    let service = Service::new(Some(binaries.clone()));
    service.initialize().await;
    service
        .import(vec![a.clone(), b.clone()], "a")
        .await
        .unwrap();
    service.import(vec![a, b], "b").await.unwrap();
    service.set_directory(output.clone()).unwrap();
    service.start("original".into(), false).unwrap();
    service.pause().unwrap();
    for _ in 0..100 {
        if !service.snapshot(0).summary.active {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let snapshot = service.snapshot(0);
    assert!(snapshot.summary.paused);
    assert_eq!(snapshot.summary.succeeded, 0);
    let cancel_id = snapshot.tasks[0].id.clone();
    service.cancel(Some(&cancel_id)).unwrap();
    service.shutdown().await.unwrap();
    let record = PathBuf::from(service.snapshot(0).summary.recovery_path.unwrap());
    let json = std::fs::read_to_string(&record).unwrap();
    assert!(!json.contains("progress"));
    assert!(!json.contains("stderr"));
    assert!(!json.contains("warnings"));
    let resumed = Service::new(Some(binaries.clone()));
    resumed.initialize().await;
    resumed.recover(&record).await.unwrap();
    let snapshot = resumed.snapshot(0);
    assert_eq!(snapshot.summary.canceled, 1);
    assert_eq!(snapshot.summary.runnable, 3);
    assert!(snapshot
        .tasks
        .iter()
        .find(|t| t.id == cancel_id)
        .is_some_and(|t| t.state == TaskState::Canceled));
    resumed.resume().unwrap();
    wait(&resumed).await;
    let snapshot = resumed.snapshot(0);
    assert_eq!(snapshot.summary.succeeded, 3, "{snapshot:?}");
    assert_eq!(snapshot.summary.canceled, 1);
    assert!(!record.exists());
    assert_eq!(std::fs::read_dir(output).unwrap().count(), 3);
}

#[tokio::test]
#[ignore = "requires real FFmpeg"]
async fn changed_source_is_rejected_before_queue_creation() {
    let binaries = engine::locate().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.mp4");
    fixture(&binaries, &path, "128x72", "30", "1", false).await;
    let service = Service::new(Some(binaries));
    service.initialize().await;
    service.import(vec![path.clone()], "a").await.unwrap();
    service.import(vec![path.clone()], "b").await.unwrap();
    std::fs::write(path, b"changed input").unwrap();
    let error = service.start("changed".into(), false).unwrap_err();
    assert_eq!(error.code, "INPUT_CHANGED");
    assert!(!service.snapshot(0).summary.active);
}

#[tokio::test]
#[ignore = "requires real FFmpeg"]
async fn crash_after_rename_is_recognized_without_duplicate_export() {
    use video_stitch::{files, recovery};
    let binaries = engine::locate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.mp4");
    fixture(&binaries, &source, "128x72", "30", "1", false).await;
    let output = dir.path().join("output");
    std::fs::create_dir(&output).unwrap();
    let service = Service::new(Some(binaries.clone()));
    service.initialize().await;
    service.import(vec![source.clone()], "a").await.unwrap();
    service.import(vec![source.clone()], "b").await.unwrap();
    service.set_directory(output.clone()).unwrap();
    service.start("before-crash".into(), false).unwrap();
    service.pause().unwrap();
    for _ in 0..100 {
        if !service.snapshot(0).summary.active {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    service.shutdown().await.unwrap();
    let record_path = PathBuf::from(service.snapshot(0).summary.recovery_path.unwrap());
    let (_, mut record) = recovery::load(&record_path).unwrap();
    let media = probe::probe(&binaries.ffprobe, &source, "input".into())
        .await
        .unwrap();
    let plan = plan::build(&media, &media, &Settings::default()).unwrap();
    let task = &mut record.tasks[0];
    let temporary = output.join(&task.temporary);
    engine::render_software(&binaries, &media, &media, &plan, &temporary)
        .await
        .unwrap();
    let fingerprint = files::fingerprint(&temporary).unwrap();
    task.partial_identity = Some(fingerprint.identity.clone());
    task.committed = Some(fingerprint);
    task.state = TaskState::Committing;
    let target = output.join(&task.name);
    recovery::save(&output, &record).unwrap();
    files::commit_no_replace(&temporary, &target).unwrap();
    // 模拟正式改名成功、succeeded 记录更新前崩溃的磁盘状态。
    let resumed = Service::new(Some(binaries));
    resumed.initialize().await;
    resumed.recover(&record_path).await.unwrap();
    let snapshot = resumed.snapshot(0);
    assert_eq!(snapshot.summary.succeeded, 1, "{snapshot:?}");
    assert_eq!(snapshot.summary.runnable, 0);
    assert!(!record_path.exists());
    assert_eq!(std::fs::read_dir(output).unwrap().count(), 1);
    assert!(target.exists());
}

#[tokio::test]
#[ignore = "requires real FFmpeg"]
async fn active_process_cancel_removes_only_its_partial() {
    let binaries = engine::locate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.mp4");
    fixture(&binaries, &source, "640x360", "60", "12", true).await;
    let output = dir.path().join("output");
    std::fs::create_dir(&output).unwrap();
    let kept = output.join("用户文件.txt");
    std::fs::write(&kept, b"keep").unwrap();
    let service = Service::new(Some(binaries));
    service.initialize().await;
    service.import(vec![source.clone()], "a").await.unwrap();
    service.import(vec![source], "b").await.unwrap();
    service.set_directory(output.clone()).unwrap();
    service
        .configure(Settings {
            ratio: Ratio::Square,
            transition: Transition::BoundaryFade,
        })
        .unwrap();
    service.start("cancel-active".into(), false).unwrap();
    let mut saw_running = false;
    for _ in 0..1000 {
        if service
            .snapshot(0)
            .tasks
            .iter()
            .any(|t| t.state == TaskState::Running)
        {
            saw_running = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(saw_running);
    service.pause().unwrap();
    service.cancel(None).unwrap();
    wait(&service).await;
    let snapshot = service.snapshot(0);
    assert_eq!(snapshot.summary.canceled, 1, "{snapshot:?}");
    assert_eq!(snapshot.summary.succeeded, 0);
    assert_eq!(std::fs::read(&kept).unwrap(), b"keep");
    assert_eq!(std::fs::read_dir(output).unwrap().count(), 1);
    assert!(!snapshot.summary.locked);
}
