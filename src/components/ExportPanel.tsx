import type { EncoderStatus, Ratio, Settings, Summary } from "../lib/contracts";
import { progressBar } from "../lib/format";

interface Props {
  summary?: Summary;
  settings: Settings;
  busy: boolean;
  environment: EncoderStatus | null;
  enabled: boolean;
  onSettings: (settings: Settings) => void;
  onDirectory: () => void;
  onStart: () => void;
  onPause: () => void;
  onCancel: () => void;
  onResume: () => void;
  onRetry: () => void;
  onOpen: () => void;
}

// 单页导出控件只选择产品参数，不暴露编码器和硬件开关。
export function ExportPanel({
  summary: s,
  settings,
  busy,
  environment,
  enabled,
  onSettings,
  onDirectory,
  onStart,
  onPause,
  onCancel,
  onResume,
  onRetry,
  onOpen,
}: Props) {
  const locked = !enabled || busy || s?.locked;
  const started = !!(
    s?.recoveryPath ||
    s?.succeeded ||
    s?.canceled ||
    s?.failed
  );
  const primary = s?.active
    ? "正在导出…"
    : s?.paused && s.runnable
      ? "继续导出"
      : s?.failed
        ? `重试失败 · ${s.failed} 个`
        : started && !s?.runnable
          ? "打开文件夹"
          : `开始导出 · ${s?.runnable ?? 0} 个视频`;
  const action =
    s?.paused && s.runnable
      ? onResume
      : s?.failed
        ? onRetry
        : started && !s?.runnable
          ? onOpen
          : onStart;
  return (
    <section className="term-export" aria-label="拼接设置与导出">
      <div className="term-section-label">
        <span>03 / EXPORT</span>
      </div>
      <div className="vs-settings">
        <fieldset>
          <legend>转场 / transition</legend>
          <div className="vs-segment">
            <button
              disabled={locked}
              aria-pressed={settings.transition === "none"}
              onClick={() => onSettings({ ...settings, transition: "none" })}
            >
              不选择转场
            </button>
            <button
              disabled={locked}
              aria-pressed={settings.transition === "boundary-fade"}
              onClick={() =>
                onSettings({ ...settings, transition: "boundary-fade" })
              }
            >
              叠化
            </button>
          </div>
        </fieldset>
        <label className="term-ratio-label">
          画面比例 / ratio
          <select
            disabled={locked}
            value={settings.ratio}
            onChange={(event) =>
              onSettings({ ...settings, ratio: event.target.value as Ratio })
            }
          >
            <option value="follow-b">跟随视频 B</option>
            <option value="portrait">9:16 · 竖屏</option>
            <option value="landscape">16:9 · 横屏</option>
            <option value="square">1:1 · 方形</option>
          </select>
        </label>
        <p className="term-setting-note">
          {settings.transition === "boundary-fade"
            ? "叠化约 0.5 秒 · 总时长不变 · "
            : ""}
          等比裁边铺满 · 分辨率、帧率跟随 B
        </p>
      </div>
      <div className="term-command">
        <div className="term-destination">
          <span className="term-muted">output →</span>
          <button
            className="vs-path"
            disabled={locked}
            onClick={onDirectory}
            title={s?.outputDirectory}
          >
            <span>
              {s?.outputDirectory ?? "桌面 / 视频拼接_2026-10-17_153045"}
            </span>
            <span className="term-change">[ 更改 ]</span>
          </button>
        </div>
        <div className="vs-main-actions">
          {s?.recoveryPath ? (
            <button className="vs-secondary" disabled={busy} onClick={onCancel}>
              取消剩余
            </button>
          ) : null}
          {s?.active ? (
            <button
              className="vs-secondary"
              disabled={busy || s.paused}
              onClick={onPause}
            >
              {s.paused ? "等待当前视频完成" : "暂停队列"}
            </button>
          ) : null}
          <button
            className="vs-primary"
            disabled={
              !enabled ||
              busy ||
              s?.active ||
              !environment?.available ||
              !s?.total ||
              (!started && !s?.runnable)
            }
            onClick={action}
          >
            <span>{busy ? "正在处理操作…" : primary}</span>
            <span aria-hidden="true">↵</span>
          </button>
        </div>
      </div>
      {started ? (
        <div className="vs-overall">
          <div className="term-overall-head">
            <span>
              已完成 {s?.succeeded} / {s?.total}
              {s?.failed ? ` · ${s.failed} 个失败` : ""}
              {s?.blocked ? ` · ${s.blocked} 个受阻` : ""}
              {s?.canceled ? ` · ${s.canceled} 个已取消` : ""}
            </span>
            <span>
              {s?.active
                ? "正在处理并检查结果"
                : s?.paused
                  ? "当前项结束后，后续任务等待继续"
                  : "完成结果已保留"}
            </span>
          </div>
          <div
            className="vs-total-progress"
            role="progressbar"
            aria-label="整体导出进度"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.floor(s?.progress ?? 0)}
          >
            <span aria-hidden="true">{progressBar(s?.progress ?? 0, 20)}</span>{" "}
            {Math.floor(s?.progress ?? 0)}%
          </div>
        </div>
      ) : null}
    </section>
  );
}
