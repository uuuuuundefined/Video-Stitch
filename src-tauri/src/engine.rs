use crate::{
    contracts::{EncoderStatus, OutputInfo},
    error::{AppError, Result},
    plan::Plan,
    probe::{self, Media},
    process,
};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};

#[derive(Debug, Clone)]
pub struct Binaries {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Encoder {
    pub name: String,
    pub hardware: bool,
    pub elapsed: f64,
}

/// 正式包只从应用旁查找组件；开发模式才允许本机 PATH。
pub fn locate() -> Result<Binaries> {
    let executable = std::env::current_exe()?;
    let parent = executable
        .parent()
        .ok_or_else(|| AppError::new("ENGINE_UNAVAILABLE", "安装目录无效。"))?;
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let bundled = Binaries {
        ffmpeg: parent.join(format!("ffmpeg{suffix}")),
        ffprobe: parent.join(format!("ffprobe{suffix}")),
    };
    if bundled.ffmpeg.is_file() && bundled.ffprobe.is_file() {
        return Ok(bundled);
    }
    #[cfg(debug_assertions)]
    {
        let find = |name: &str| -> Option<PathBuf> {
            if let Some(path) = std::env::var_os(format!("VIDEO_STITCH_{}", name.to_uppercase())) {
                if Path::new(&path).is_file() {
                    return Some(path.into());
                }
            }
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|p| p.join(format!("{name}{suffix}")))
                .find(|p| p.is_file())
        };
        if let (Some(ffmpeg), Some(ffprobe)) = (find("ffmpeg"), find("ffprobe")) {
            return Ok(Binaries { ffmpeg, ffprobe });
        }
    }
    Err(AppError::new(
        "ENGINE_UNAVAILABLE",
        "缺少内置 FFmpeg / ffprobe，请重新安装完整软件。",
    ))
}

/// 各编码器采用独立质量参数，正式画质基线仍需代表性真实素材校准。
fn encoder_args(name: &str) -> Vec<String> {
    let args = match name {
        "h264_videotoolbox" => vec!["-c:v", name, "-q:v", "65", "-allow_sw", "0"],
        "h264_nvenc" => vec![
            "-c:v", name, "-preset", "p5", "-rc", "vbr", "-cq", "21", "-b:v", "0",
        ],
        "h264_qsv" => vec!["-c:v", name, "-global_quality", "21", "-preset", "medium"],
        "h264_amf" => vec![
            "-c:v", name, "-quality", "quality", "-rc", "cqp", "-qp_i", "20", "-qp_p", "22",
        ],
        _ => vec!["-c:v", "libx264", "-preset", "fast", "-crf", "20"],
    };
    args.into_iter().map(str::to_string).collect()
}

/// 对候选执行真实试编码，输出至空设备而不产生检测文件。
pub async fn detect(binaries: &Binaries) -> (EncoderStatus, Vec<Encoder>) {
    let hardware: &[&str] = if cfg!(target_os = "macos") {
        &["h264_videotoolbox"]
    } else if cfg!(windows) {
        &["h264_nvenc", "h264_qsv", "h264_amf"]
    } else {
        &[]
    };
    let mut encoders = Vec::new();
    for name in hardware.iter().copied().chain(std::iter::once("libx264")) {
        let mut args: Vec<String> = [
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1280x720:rate=30:duration=0.4",
            "-vf",
            "format=yuv420p",
            "-threads",
            "2",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        args.extend(encoder_args(name));
        args.extend(["-f".into(), "null".into(), "-".into()]);
        let begin = Instant::now();
        if process::capture(&binaries.ffmpeg, &args, 12).await.is_ok() {
            encoders.push(Encoder {
                name: name.into(),
                hardware: name != "libx264",
                elapsed: begin.elapsed().as_secs_f64(),
            });
        }
    }
    encoders.sort_by(|a, b| a.elapsed.total_cmp(&b.elapsed));
    let accelerated = encoders.first().is_some_and(|e| e.hardware);
    let status = EncoderStatus {
        detecting: false,
        available: !encoders.is_empty(),
        accelerated,
        label: if encoders.is_empty() {
            "媒体组件不可用"
        } else if accelerated {
            "自动加速 · 已就绪"
        } else {
            "兼容处理 · 已就绪"
        }
        .into(),
        detail: if encoders.is_empty() {
            "未找到能够实际工作的 H.264 编码器，请检查安装。".into()
        } else {
            format!(
                "候选已通过短样本试编码：{}。实际输入仍会检查；当前质量参数尚需真实素材基线验收。",
                encoders
                    .iter()
                    .map(|e| e.name.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            )
        },
    };
    (status, encoders)
}

/// 规范化显示方向之后的像素比例、画布与时间基，保持原视频播放速度。
fn normalized(index: usize, media: &Media, frames: u64, plan: &Plan) -> String {
    let c = &plan.canvas;
    format!("[{index}:{}]setpts=PTS-STARTPTS,scale=w='max(2,trunc(iw*sar/2)*2)':h=ih,setsar=1,scale={}:{}:force_original_aspect_ratio=increase:force_divisible_by=2:out_color_matrix=bt709:out_range=tv,crop={}:{},setsar=1,fps={}/{},tpad=stop_mode=clone:stop_duration=1,trim=end_frame={frames},settb=AVTB,setpts=PTS-STARTPTS,format=yuv420p",media.video_index,c.width,c.height,c.width,c.height,c.fps.num,c.fps.den)
}

/// 原音映射到各段画面区间，超出部分裁掉、缺少部分补静音，不执行交叉淡化。
fn audio(index: usize, media: &Media, seconds: f64) -> String {
    match &media.audio {
        Some(a) => {
            let skip = (-a.start).max(0.0);
            let delay = (a.start.max(0.0) * 48000.0).round() as u64;
            format!("[{index}:{}]atrim=start={skip:.9},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,adelay={delay}S:all=1,apad,atrim=duration={seconds:.9},asetpts=PTS-STARTPTS[a{index}]",a.index)
        }
        None => format!(
            "anullsrc=r=48000:cl=stereo,atrim=duration={seconds:.9},asetpts=PTS-STARTPTS[a{index}]"
        ),
    }
}

/// 使用边界帧延展抵消 xfade 重叠；最终截掉的只可能是补偿帧。
pub fn filter_graph(a: &Media, b: &Media, plan: &Plan) -> String {
    let fps = plan.canvas.fps.value();
    let mut filters = Vec::new();
    if plan.half_transition > 0 {
        let h = plan.half_transition;
        filters.push(format!(
            "{},tpad=stop_mode=clone:stop={h}[v0]",
            normalized(0, a, plan.a_frames, plan)
        ));
        filters.push(format!(
            "{},tpad=start_mode=clone:start={h}[v1]",
            normalized(1, b, plan.b_frames, plan)
        ));
        filters.push(format!("[v0][v1]xfade=transition=fade:duration={:.9}:offset={:.9},trim=end_frame={},setpts=PTS-STARTPTS,format=yuv420p[v]",2.0*h as f64/fps,(plan.a_frames-h) as f64/fps,plan.a_frames+plan.b_frames));
    } else {
        filters.push(format!("{}[v0]", normalized(0, a, plan.a_frames, plan)));
        filters.push(format!("{}[v1]", normalized(1, b, plan.b_frames, plan)));
        filters.push("[v0][v1]concat=n=2:v=1:a=0[v]".into());
    }
    if a.audio.is_some() || b.audio.is_some() {
        filters.push(audio(0, a, plan.a_frames as f64 / fps));
        filters.push(audio(1, b, plan.b_frames as f64 / fps));
        filters.push("[a0][a1]concat=n=2:v=0:a=1[au]".into());
    }
    filters.join(";")
}

/// 构建无辅助文件的 FFmpeg 命令；兼容复制使用 stdin 中的拼接清单。
pub fn arguments(
    a: &Media,
    b: &Media,
    plan: &Plan,
    encoder: &str,
    output: &Path,
    copy: bool,
) -> (Vec<String>, Option<String>) {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-nostats",
        "-progress",
        "pipe:1",
        "-stats_period",
        "0.25",
        "-y",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let stdin = if copy {
        let quote = |p: &Path| {
            p.to_string_lossy()
                .replace('\\', "/")
                .replace('\'', "'\\''")
        };
        let list = format!("ffconcat version 1.0\nfile 'file:{}'\nduration {:.9}\nfile 'file:{}'\nduration {:.9}\n",quote(&a.path),a.seconds,quote(&b.path),b.seconds);
        args.extend(
            [
                "-protocol_whitelist",
                "file,pipe",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                "pipe:0",
                "-map",
                "0:v:0",
                "-map",
                "0:a:0?",
                "-c",
                "copy",
            ]
            .into_iter()
            .map(str::to_string),
        );
        Some(list)
    } else {
        args.extend([
            "-i".into(),
            a.path.to_string_lossy().into_owned(),
            "-i".into(),
            b.path.to_string_lossy().into_owned(),
            "-filter_complex_threads".into(),
            "2".into(),
            "-filter_complex".into(),
            filter_graph(a, b, plan),
            "-map".into(),
            "[v]".into(),
        ]);
        if a.audio.is_some() || b.audio.is_some() {
            args.extend(
                [
                    "-map", "[au]", "-c:a", "aac", "-b:a", "192k", "-ar", "48000",
                ]
                .into_iter()
                .map(str::to_string),
            );
        }
        args.extend(encoder_args(encoder));
        args.extend(
            ["-pix_fmt", "yuv420p", "-r"]
                .into_iter()
                .map(str::to_string),
        );
        args.push(format!("{}/{}", plan.canvas.fps.num, plan.canvas.fps.den));
        args.extend(
            [
                "-color_primaries",
                "bt709",
                "-color_trc",
                "bt709",
                "-colorspace",
                "bt709",
            ]
            .into_iter()
            .map(str::to_string),
        );
        None
    };
    args.extend(
        [
            "-map_metadata",
            "-1",
            "-map_chapters",
            "-1",
            "-metadata:s:v:0",
            "rotate=0",
            "-f",
            "mp4",
        ]
        .into_iter()
        .map(str::to_string),
    );
    args.push(output.to_string_lossy().into_owned());
    (args, stdin)
}

/// 成功编码之后检查输出结构、参数、时长及首尾可解码性。
pub async fn validate(
    binaries: &Binaries,
    output: &Path,
    plan: &Plan,
    has_audio: bool,
) -> Result<OutputInfo> {
    let media = probe::probe(&binaries.ffprobe, output, "validation".into())
        .await
        .map_err(|_| {
            AppError::new(
                "OUTPUT_INVALID",
                "输出媒体信息校验失败，尚未提交为完成文件。",
            )
        })?;
    let tolerance = (2.0 / plan.canvas.fps.value()).max(2048.0 / 48000.0);
    if media.codec != "h264"
        || media.pixel_format != "yuv420p"
        || media.width != plan.canvas.width
        || media.height != plan.canvas.height
        || (media.fps.value() - plan.canvas.fps.value()).abs() > 0.001
        || (media.seconds - plan.seconds).abs() > tolerance
        || media.audio.is_some() != has_audio
        || media.audio.as_ref().is_some_and(|a| {
            a.codec != "aac" || (a.start + a.seconds - plan.seconds).abs() > tolerance
        })
    {
        return Err(AppError::new(
            "OUTPUT_INVALID",
            "输出尺寸、时长、帧率或音轨未通过检查，请重试。",
        ));
    }
    for seek in [0.0, (plan.seconds - 0.5).max(0.0)] {
        let args = vec![
            "-hide_banner".into(),
            "-v".into(),
            "error".into(),
            "-xerror".into(),
            "-ss".into(),
            format!("{seek:.6}"),
            "-i".into(),
            output.to_string_lossy().into_owned(),
            "-frames:v".into(),
            "1".into(),
            "-f".into(),
            "null".into(),
            "-".into(),
        ];
        process::capture(&binaries.ffmpeg, &args, 30)
            .await
            .map_err(|_| AppError::new("OUTPUT_INVALID", "输出视频无法正常解码，尚未提交。"))?;
    }
    Ok(OutputInfo {
        bytes: media.fingerprint.bytes as f64,
        seconds: media.seconds,
        width: media.width,
        height: media.height,
    })
}

/// 为媒体集成测试提供同一条正式处理路径。
pub async fn render_software(
    binaries: &Binaries,
    a: &Media,
    b: &Media,
    plan: &Plan,
    output: &Path,
) -> Result<OutputInfo> {
    let (args, input) = arguments(a, b, plan, "libx264", output, false);
    process::render(
        &binaries.ffmpeg,
        &args,
        input,
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .await?;
    validate(
        binaries,
        output,
        plan,
        a.audio.is_some() || b.audio.is_some(),
    )
    .await
}
