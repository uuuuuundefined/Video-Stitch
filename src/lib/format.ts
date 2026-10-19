// 仅用于展示，业务媒体参数始终由后端提供。
export function duration(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds)) return "—";
  const value = Math.max(0, Math.round(seconds));
  const hours = Math.floor(value / 3600);
  return `${hours ? `${hours}:` : ""}${String(Math.floor(value / 60) % 60).padStart(2, "0")}:${String(value % 60).padStart(2, "0")}`;
}

// 使用二进制单位，避免把字节数冒充十进制 MB。
export function bytes(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "暂不可用";
  if (value < 1024) return `${value} B`;
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), 4);
  return `${(value / 1024 ** index).toFixed(1)} ${["B", "KiB", "MiB", "GiB", "TiB"][index]}`;
}

// 终端进度格直接映射后端实际进度，不使用计时器推进。
export function progressBar(progress: number, blocks = 10): string {
  const count = Math.max(
    0,
    Math.min(blocks, Math.floor((progress / 100) * blocks)),
  );
  return `[${"█".repeat(count)}${"·".repeat(blocks - count)}]`;
}
