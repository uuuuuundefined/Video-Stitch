import type { EncoderStatus, Performance, Summary } from "../lib/contracts";
import { bytes, duration } from "../lib/format";

// 不可读取的指标明确显示不可用，绝不使用模拟值补齐。
export function PerformanceBar({
  value,
  environment,
  summary,
}: {
  value: Performance | null;
  environment: EncoderStatus | null;
  summary?: Summary;
}) {
  return (
    <footer className="term-statusbar">
      <span className="term-session">STITCH</span>
      <span title={environment?.detail}>
        {environment?.label ?? "等待桌面处理环境"}
      </span>
      <div className="vs-metrics" aria-label="性能占用">
        <span title="应用及可归属子进程 CPU，按整机总算力归一化">
          软件CPU:
          <b>
            {value?.cpuPercent == null
              ? "暂不可用"
              : `${value.cpuPercent.toFixed(1)}%`}
          </b>
        </span>
        <span title={value?.memoryScope}>
          软件内存:<b>{bytes(value?.processBytes)}</b>
        </span>
        <span>
          系统内存:
          <b>
            {value
              ? `${bytes(value.systemUsed)}/${bytes(value.systemTotal)}`
              : "暂不可用"}
          </b>
        </span>
        <span title={value?.gpuScope}>
          GPU:
          <b>
            {value?.gpuPercent == null
              ? "暂不可用"
              : `${value.gpuPercent.toFixed(1)}%`}
          </b>
        </span>
        <span>
          速度:
          <b>{value?.speed == null ? "—" : `${value.speed.toFixed(2)}×`}</b>
        </span>
        <span title="整批预计剩余时间，会随实际处理速度变化">
          剩余:
          <b>
            {summary?.paused
              ? "已暂停"
              : !summary?.active
                ? "—"
                : value?.etaSeconds == null
                  ? "估算中"
                  : `约 ${duration(value.etaSeconds)}`}
          </b>
        </span>
        <span title="输出卷可用空间">
          磁盘: <b>{bytes(value?.diskFree)}</b>
        </span>
      </div>
    </footer>
  );
}
