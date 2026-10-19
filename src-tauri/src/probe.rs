use crate::{
    contracts::{Asset, Rational},
    error::{AppError, Result},
    files::{fingerprint, Fingerprint},
    process,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Audio {
    pub index: u32,
    pub start: f64,
    pub seconds: f64,
    pub sample_rate: u32,
    pub codec: String,
}

#[derive(Debug, Clone)]
pub struct Media {
    pub video_index: u32,
    pub view: Asset,
    pub path: PathBuf,
    pub fingerprint: Fingerprint,
    pub seconds: f64,
    pub start: f64,
    pub width: u32,
    pub height: u32,
    pub coded_width: u32,
    pub coded_height: u32,
    pub rotation: i32,
    pub sar: f64,
    pub fps: Rational,
    pub cfr: bool,
    pub codec: String,
    pub pixel_format: String,
    pub audio: Option<Audio>,
    pub signature: String,
}

/// 读取 ffprobe 的数字字段，兼容字符串和 JSON 数值。
fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|n| n.is_finite())
}

/// 解析并约分合法正帧率，异常输入不会得到默认 30 FPS。
pub fn rational(value: &str) -> Option<Rational> {
    let (a, b) = value.split_once('/').or_else(|| value.split_once(':'))?;
    let mut n: u32 = a.parse().ok()?;
    let mut d: u32 = b.parse().ok()?;
    if n == 0 || d == 0 {
        return None;
    }
    let (mut x, mut y) = (n, d);
    while y != 0 {
        (x, y) = (y, x % y);
    }
    n /= x;
    d /= x;
    Some(Rational { num: n, den: d })
}

/// 仅接受可信流时长；容器时长不替代有效视频时间区间。
fn duration(stream: &Value) -> Option<f64> {
    let duration = number(&stream["duration_ts"])
        .zip(stream["time_base"].as_str().and_then(rational))
        .map(|(n, t)| n * t.value())
        .or_else(|| number(&stream["duration"]));
    duration.filter(|v| *v > 0.0)
}

/// 探测实际媒体流，元数据仅驻留内存，不读取视频到前端。
pub async fn probe(binary: &Path, path: &Path, id: String) -> Result<Media> {
    let path = std::fs::canonicalize(path)
        .map_err(|_| AppError::new("INPUT_MISSING", "源视频不存在或无法读取，请重新定位。"))?;
    let before = fingerprint(&path)?;
    let args = vec![
        "-v".into(),
        "error".into(),
        "-show_streams".into(),
        "-show_format".into(),
        "-show_data".into(),
        "-of".into(),
        "json".into(),
        path.to_string_lossy().into_owned(),
    ];
    let data: Value = serde_json::from_slice(&process::capture(binary, &args, 30).await?)?;
    if before != fingerprint(&path)? {
        return Err(AppError::new(
            "INPUT_CHANGED",
            "读取期间源视频发生变化，请重新导入。",
        ));
    }
    let streams = data["streams"]
        .as_array()
        .ok_or_else(|| AppError::new("PROBE_FAILED", "未读取到有效媒体流。"))?;
    let videos: Vec<_> = streams
        .iter()
        .filter(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .collect();
    let video = videos
        .first()
        .ok_or_else(|| AppError::new("PROBE_FAILED", "文件中没有有效视频画面。"))?;
    let audios: Vec<_> = streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .collect();
    let seconds = duration(video).ok_or_else(|| {
        AppError::new(
            "INVALID_TIMESTAMPS",
            "无法确定有效视频时长，首版暂不支持此文件。",
        )
    })?;
    let fps = video["avg_frame_rate"]
        .as_str()
        .and_then(rational)
        .filter(|r| (1.0..=240.0).contains(&r.value()))
        .ok_or_else(|| {
            AppError::new("INVALID_TIMESTAMPS", "无法确定可靠帧率，暂不能拼接此视频。")
        })?;
    let coded_width = video["width"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 0)
        .ok_or_else(|| AppError::new("PROBE_FAILED", "视频宽度无效。"))?;
    let coded_height = video["height"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 0)
        .ok_or_else(|| AppError::new("PROBE_FAILED", "视频高度无效。"))?;
    let sar = video["sample_aspect_ratio"]
        .as_str()
        .and_then(rational)
        .map_or(1.0, Rational::value);
    let rotation = video["side_data_list"]
        .as_array()
        .and_then(|a| a.iter().find_map(|v| number(&v["rotation"])))
        .or_else(|| number(&video["tags"]["rotate"]))
        .unwrap_or(0.0)
        .round() as i32;
    let display_width = (f64::from(coded_width) * sar).round() as u32;
    let (width, height) = if rotation.rem_euclid(180) == 90 {
        (coded_height, display_width)
    } else {
        (display_width, coded_height)
    };
    let start = number(&video["start_time"]).unwrap_or(0.0);
    let audio = if let Some(a) = audios.first() {
        Some(Audio {
            index: a["index"].as_u64().unwrap_or(0) as u32,
            start: number(&a["start_time"]).unwrap_or(start) - start,
            seconds: duration(a).ok_or_else(|| {
                AppError::new(
                    "INVALID_TIMESTAMPS",
                    "无法判断声音时长，请使用时间戳完整的源视频。",
                )
            })?,
            sample_rate: number(&a["sample_rate"])
                .filter(|v| *v > 0.0)
                .ok_or_else(|| AppError::new("PROBE_FAILED", "音频采样率无效。"))?
                as u32,
            codec: a["codec_name"].as_str().unwrap_or("").into(),
        })
    } else {
        None
    };
    let pixel_format = video["pix_fmt"].as_str().unwrap_or("").to_string();
    let cfr = video["r_frame_rate"]
        .as_str()
        .and_then(rational)
        .is_some_and(|r| r.num == fps.num && r.den == fps.den)
        && number(&video["nb_frames"])
            .is_some_and(|n| (n / fps.value() - seconds).abs() <= 0.1 / fps.value());
    let codec = video["codec_name"].as_str().unwrap_or("").to_string();
    let mut issues = Vec::new();
    if videos.len() != 1 {
        issues.push("首版仅支持单视频流文件");
    }
    if audios.len() > 1 {
        issues.push("首版不支持多音轨");
    }
    if streams.iter().any(|s| s["codec_type"] == "subtitle") {
        issues.push("首版不支持独立字幕流");
    }
    if matches!(
        video["color_transfer"].as_str(),
        Some("smpte2084" | "arib-std-b67")
    ) || video["side_data_list"].as_array().is_some_and(|v| {
        v.iter().any(|s| {
            s["side_data_type"]
                .as_str()
                .is_some_and(|s| s.contains("DOVI") || s.contains("Mastering display"))
        })
    }) {
        issues.push("首版不支持 HDR，请先转换为 SDR");
    }
    if pixel_format.contains("yuva")
        || pixel_format.contains("rgba")
        || pixel_format.contains("argb")
        || pixel_format.contains("gbrap")
    {
        issues.push("首版不支持透明画面");
    }
    if !matches!(codec.as_str(), "h264" | "hevc") {
        issues.push("首版仅支持 H.264 / HEVC 视频");
    }
    if !data["format"]["format_name"]
        .as_str()
        .is_some_and(|s| s.contains("mov") || s.contains("mp4"))
    {
        issues.push("首版重点支持 MP4 / MOV 封装");
    }
    if rotation.rem_euclid(90) != 0 {
        issues.push("暂不支持非直角旋转的视频");
    }
    if let Some(a) = &audio {
        let tolerance = (2.0 / fps.value()).max(2048.0 / f64::from(a.sample_rate));
        if a.start + a.seconds > seconds + tolerance || a.start < -tolerance {
            issues.push("音轨超出画面区间，无法完整保留原声，请先修复源文件");
        }
    }
    let signature = streams
        .iter()
        .filter(|s| s["codec_type"] == "video" || s["codec_type"] == "audio")
        .map(|s| {
            [
                "codec_type",
                "codec_name",
                "codec_tag_string",
                "profile",
                "level",
                "width",
                "height",
                "pix_fmt",
                "sample_aspect_ratio",
                "time_base",
                "extradata",
                "sample_fmt",
                "sample_rate",
                "channels",
                "channel_layout",
            ]
            .iter()
            .map(|k| format!("{k}={}", s[k]))
            .collect::<Vec<_>>()
            .join(";")
        })
        .collect::<Vec<_>>()
        .join("|");
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Ok(Media {
        video_index: video["index"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| AppError::new("PROBE_FAILED", "视频流索引无效。"))?,
        view: Asset {
            id,
            name,
            path: path.to_string_lossy().into_owned(),
            bytes: before.bytes as f64,
            seconds: Some(seconds),
            width: Some(width),
            height: Some(height),
            fps: Some(fps),
            issue: (!issues.is_empty()).then(|| issues.join("；")),
        },
        path,
        fingerprint: before,
        seconds,
        start,
        width,
        height,
        coded_width,
        coded_height,
        rotation,
        sar,
        fps,
        cfr,
        codec,
        pixel_format,
        audio,
        signature,
    })
}
