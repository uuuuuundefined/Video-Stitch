# Video Stitch

本地、离线的视频全组合拼接工具，视频组 A 在前、视频组 B 在后。面向 macOS 13+ Apple Silicon 与 Windows 10 22H2+ x64。

安装包见 [Releases](https://github.com/uuuuuundefined/Video-Stitch/releases/latest)：Windows 下载 `*-setup.exe`，Apple Silicon Mac 下载 `.dmg`；同页提供 `SHA256SUMS.txt`。安装包尚未正式签名或公证，具体功能及验证限制见 [版本说明](docs/RELEASE_NOTES.md)。

- [PRD](docs/PRD.md)：已确认的单页面产品规则及验收要求。
- [关键决策](docs/DECISIONS.md)：本轮最终决定、已撤回的旧建议及授权边界。
- [AI 开发执行说明](docs/AI_DEVELOPMENT.md)：模块、IPC、时间线、恢复、性能和安装卸载契约。
- [产品原型](prototype/index.html)：已确认的黑白像素终端界面，可直接用浏览器离线打开；使用说明及状态检查见 [原型说明](prototype/README.md)。

正式应用已实现首个可运行版本：React/TypeScript/Vite 单页面接入 Tauri/Rust 媒体引擎，包含真实探测、全组合导出、定长叠化、暂停/取消/重试和手动恢复。原型保持独立，不进入正式包。当前验证范围与尚未完成的生产验收见 [验证记录](docs/VALIDATION.md)。

业务无磁盘缓存、无日志、无预览；任务恢复记录和进行中视频只放输出目录。**卸载仅清理应用和私有运行数据，保留所有导出目录及其任务文件。**

需求优先级：最新明确决策 → PRD → AI 开发执行说明 → 原型。正式开发已由当前会话授权；不能将界面交互通过当作媒体功能通过。

## 本地开发

```sh
npm ci
npm run media:prepare -- --dev
npm run desktop
```

开发需 Node.js、Rust 和对应平台原生构建工具。`scripts/rust.mjs` 优先使用项目 `.tools` 下的 Rust，不改全局设置。开发模式允许本机 FFmpeg；发布包强制使用通过校验的自包含组件，不查系统 PATH。

macOS 固定版本构建：运行 `npm run media:sources`、`npm run media:build:mac`，再执行 `npm run desktop:build -- --bundles app`。源码 URL 和 SHA-256 已固定；构建只使用 Xcode Command Line Tools 自带编译器，不需要 Homebrew。脚本固定最低 macOS 13，包内包含匹配源码、许可证与 SHA-256。Windows 的媒体组件须在 Windows 构建并按校验和通过 `media:prepare` 提供；不能使用 Mac 二进制代替。

Windows 构建入口和前置条件见 [Windows 构建说明](docs/WINDOWS_BUILD.md)。已提供固定源码的软件编码基础组件配方、PE 架构/DLL 依赖校验及 NSIS 离线 WebView2 配置；已在 Windows runner 通过原生构建、媒体回归、启动立即关闭和安装/卸载数据保留验证；具体提交和限制见验证记录。硬件编码尚未验收。

## 检查

```sh
npm run typecheck
npm run format:check
npm run rust -- fmt --manifest-path src-tauri/Cargo.toml -- --check
npm run rust -- clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run test:core
npm run test:media
npm run test:media-components
```

`test:media` 明确使用待打包的组件，临时素材由测试目录生命周期清理。业务运行不执行这些生成测试素材的操作。

## 实现位置

- `src/components`：沿用原型的文件组、任务分页、导出、性能与确认弹窗。
- `src/hooks`、`src/lib`：内存界面状态、受限 IPC 和 Rust 生成的契约。
- `src-tauri/src/probe.rs`、`plan.rs`、`engine.rs`、`process.rs`：真实媒体探测、时间线、滤镜、编码与进程管理。
- `src-tauri/src/queue`、`queue.rs`、`recovery.rs`、`files.rs`：队列、恢复、一致性和不覆盖提交。
- `src-tauri/src/metrics`：真实 CPU、平台内存和磁盘采样；GPU 不能可靠采集时显示不可用。
- `scripts`：本地工具链入口、媒体构建/校验和限制归属的 Mac 卸载清理脚本。

Mac 将应用包移入废纸篓可移除内置组件。包外固定私有路径清理说明与脚本见 [验证记录](docs/VALIDATION.md)；所有导出目录始终保留。

## 发布

`.github/workflows/release.yml` 在主分支更新版本或发布工作流时自动运行，也可在 Actions 手动触发。发布前同步 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 和 `src-tauri/tauri.conf.json` 的版本，并更新 `docs/RELEASE_NOTES.md` 的版本标题及内容。Windows 原生构建/安装测试与 macOS Apple Silicon 构建/真实媒体测试全部通过后，自动创建对应 `vX.Y.Z` 标签和正式 Release，上传两份安装包和校验和；已发布的同版本不会被覆盖。上传中断可重新运行发布 job，恢复同一提交的草稿。
