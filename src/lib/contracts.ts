// Generated from Rust contracts. Run npm run bindings.
export type AppError = { code: string, message: string, };

export type Ratio = "follow-b" | "portrait" | "landscape" | "square";

export type Transition = "none" | "boundary-fade";

export type Settings = { ratio: Ratio, transition: Transition, };

export type Rational = { num: number, den: number, };

export type Asset = { id: string, name: string, path: string, bytes: number, seconds: number | null, width: number | null, height: number | null, fps: Rational | null, issue: string | null, };

export type TaskState = "pending" | "preflight" | "ready" | "running" | "validating" | "committing" | "succeeded" | "blocked" | "failed" | "canceling" | "canceled" | "interrupted";

export type Canvas = { width: number, height: number, fps: Rational, };

export type OutputInfo = { bytes: number, seconds: number, width: number, height: number, };

export type TaskView = { id: string, name: string, sourceA: string, sourceB: string, seconds: number | null, canvas: Canvas | null, state: TaskState, progress: number, issue: string | null, warnings: Array<string>, result: OutputInfo | null, };

export type Summary = { total: number, runnable: number, blocked: number, succeeded: number, failed: number, canceled: number, progress: number, active: boolean, paused: boolean, locked: boolean, outputDirectory: string, recoveryPath: string | null, notice: string | null, };

export type Snapshot = { revision: number, assetsA: Array<Asset>, assetsB: Array<Asset>, settings: Settings, summary: Summary, tasks: Array<TaskView>, page: number, };

export type EncoderStatus = { detecting: boolean, available: boolean, accelerated: boolean, label: string, detail: string, };

export type Performance = { cpuPercent: number | null, processBytes: number | null, memoryScope: string, systemUsed: number | null, systemTotal: number | null, gpuPercent: number | null, gpuScope: string, speed: number | null, etaSeconds: number | null, diskFree: number | null, };
