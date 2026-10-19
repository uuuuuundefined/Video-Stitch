use crate::{
    contracts::{Canvas, Ratio, Settings, Transition},
    error::{AppError, Result},
    probe::Media,
};

#[derive(Debug, Clone)]
pub struct Plan {
    pub canvas: Canvas,
    pub a_frames: u64,
    pub b_frames: u64,
    pub half_transition: u64,
    pub seconds: f64,
    pub copy: bool,
    pub warnings: Vec<String>,
}

/// 按 B 的显示尺寸生成偶数画布；固定比例保持严格有理数比例。
pub fn canvas(width: u32, height: u32, ratio: Ratio) -> (u32, u32) {
    if ratio == Ratio::FollowB {
        return (width.max(2).div_ceil(2) * 2, height.max(2).div_ceil(2) * 2);
    }
    let (p, q) = match ratio {
        Ratio::Portrait => (9, 16),
        Ratio::Landscape => (16, 9),
        _ => (1, 1),
    };
    let k =
        ((f64::from(width.min(height)) / (2.0 * f64::from(p.min(q)))).round() as u32).max(1) * 2;
    (p * k, q * k)
}

/// 计算偶数叠化帧数；每个输入至少留一帧非转场画面。
pub fn half_transition(a: u64, b: u64, fps: f64, transition: Transition) -> u64 {
    if transition == Transition::None {
        return 0;
    }
    ((0.5 * fps / 2.0).floor() as u64).min(a.min(b).saturating_sub(1))
}

/// 创建不改变原播放速度的帧级计划，复制路径采取保守兼容判定。
pub fn build(a: &Media, b: &Media, settings: &Settings) -> Result<Plan> {
    if let Some(issue) = a.view.issue.as_ref().or(b.view.issue.as_ref()) {
        return Err(AppError::new("UNSUPPORTED_STREAMS", issue));
    }
    let (width, height) = canvas(b.width, b.height, settings.ratio);
    let target = Canvas {
        width,
        height,
        fps: b.fps,
    };
    let a_frames = (a.seconds * b.fps.value()).round().max(1.0) as u64;
    let b_frames = (b.seconds * b.fps.value()).round().max(1.0) as u64;
    let half_transition = half_transition(a_frames, b_frames, b.fps.value(), settings.transition);
    let exact_audio = |m: &Media| {
        m.audio.as_ref().is_none_or(|s| {
            s.codec == "aac" && s.start.abs() < 0.0001 && (s.seconds - m.seconds).abs() < 0.0001
        })
    };
    let copy = settings.transition == Transition::None
        && a.video_index == 0
        && b.video_index == 0
        && a.signature == b.signature
        && a.codec == "h264"
        && b.codec == "h264"
        && a.pixel_format == "yuv420p"
        && b.pixel_format == "yuv420p"
        && a.cfr
        && b.cfr
        && a.fps.num == b.fps.num
        && a.fps.den == b.fps.den
        && a.width == width
        && a.height == height
        && b.width == width
        && b.height == height
        && a.rotation == 0
        && b.rotation == 0
        && (a.sar - 1.0).abs() < 1e-6
        && (b.sar - 1.0).abs() < 1e-6
        && a.start.abs() < 1e-6
        && b.start.abs() < 1e-6
        && exact_audio(a)
        && exact_audio(b)
        && !a.path.to_string_lossy().contains(['\r', '\n'])
        && !b.path.to_string_lossy().contains(['\r', '\n']);
    let mut warnings = Vec::new();
    if !copy {
        warnings.push("需适配画面或兼容参数，将重新编码。".into());
    }
    if a.width * height != a.height * width || b.width * height != b.height * width {
        warnings.push("等比裁边铺满，可能裁掉边缘人物或字幕。".into());
    }
    if !a.cfr || !b.cfr || (a.fps.value() - b.fps.value()).abs() > 0.001 {
        warnings.push("帧率按视频 B 重采样，保持播放速度。".into());
    }
    if settings.transition == Transition::BoundaryFade {
        if half_transition == 0 {
            warnings.push("片段过短，此组合采用直接拼接。".into());
        } else if half_transition < (0.5 * b.fps.value() / 2.0).floor() as u64 {
            warnings.push(format!(
                "片段较短，实际叠化 {:.3} 秒。",
                2.0 * half_transition as f64 / b.fps.value()
            ));
        }
    }
    Ok(Plan {
        canvas: target,
        a_frames,
        b_frames,
        half_transition,
        seconds: (a_frames + b_frames) as f64 / b.fps.value(),
        copy,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canvas_follows_display_short_edge() {
        assert_eq!(canvas(3840, 2160, Ratio::Portrait), (2160, 3840));
        assert_eq!(canvas(3840, 2160, Ratio::Square), (2160, 2160));
        let (w, h) = canvas(1919, 1079, Ratio::Landscape);
        assert_eq!(w * 9, h * 16);
        assert_eq!(w % 2, 0);
        assert_eq!(h % 2, 0);
    }
    #[test]
    fn transition_compensation_preserves_frame_sum() {
        for fps in [
            24.0,
            25.0,
            30000.0 / 1001.0,
            30.0,
            50.0,
            60000.0 / 1001.0,
            60.0,
        ] {
            let (a, b) = (137, 219);
            let h = half_transition(a, b, fps, Transition::BoundaryFade);
            assert_eq!((a + h) + (b + h) - 2 * h, a + b);
            assert!(h < a.min(b));
        }
        assert_eq!(half_transition(1, 90, 30.0, Transition::BoundaryFade), 0);
        assert_eq!(half_transition(3, 90, 60.0, Transition::BoundaryFade), 2);
    }
}
