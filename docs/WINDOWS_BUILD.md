# Windows 构建与验证

目标为 Windows 10 22H2+ / Windows 11 x64。已在 GitHub Actions Windows runner 完成原生构建、安装和卸载测试；具体版本与通过范围见 [验证记录](VALIDATION.md)。目标设备矩阵与完整界面操作仍待验收。

## 构建环境

- Windows x64 Node.js、Rust stable MSVC 工具链及 Visual Studio C++ Build Tools / Windows SDK，版本须满足项目 `Cargo.toml` 的 Rust 要求。
- MSYS2 的 **MINGW64** 环境：`make`、`tar`、`xz`、`gcc`、`nasm`、`pkg-config`，其中 GCC 和 pkg-config 使用 `mingw-w64-x86_64-gcc`、`mingw-w64-x86_64-pkgconf`。构建配方检查 `MSYSTEM=MINGW64` 和编译器目标，不使用 MSYS Cygwin 运行库。
- 在 MINGW64 中可调用 Windows 版 `node`。媒体构建使用 MinGW 生成独立 `.exe`，应用本身仍使用 Rust MSVC；两者通过进程通信，不混合链接 ABI。

## 媒体组件

先在项目目录安装前端依赖并获取校验和固定的 FFmpeg 8.1.2、x264 `b35605a` 源码：

```sh
npm ci
npm run media:sources
```

随后在 Windows MSYS2 MINGW64 的项目目录运行：

```sh
npm run media:build:windows
```

该入口调用 `scripts/build-ffmpeg-windows.sh`，先核对源码 SHA-256，再构建静态 FFmpeg/ffprobe 和 x264，暂存文件为：

```text
src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe
src-tauri/binaries/ffprobe-x86_64-pc-windows-msvc.exe
```

当前配方是 **libx264 软件编码基础版**，不含 NVENC/QSV/AMF 构建依赖，不代表硬件编码功能已完成发布。应用会检测实际存在且能工作的编码器，基础版应显示兼容处理；增加硬件组件时须另外固定头文件/源码、许可及质量/设备测试。

`media:prepare` 要求在匹配宿主机运行两个组件读取版本，检查目标架构、SHA-256 及 PE 导入表，只允许系统 DLL；拒绝 `libwinpthread-1.dll` 等外部运行库。两个组件都通过后才更新暂存区和配套 manifest。脚本同时打包对应源码、许可证、构建配置和配方；不会把 Mac 二进制改名使用，也不会在业务运行时下载组件。

## 原生检查和安装包

回到 Windows PowerShell 或配置好 MSVC 的终端，在项目目录运行：

```powershell
npm run test:media-components
npm run test:core
npm run test:media
npm run rust -- clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run desktop:build -- --bundles nsis
```

Windows 平台配置限定 NSIS，避免生成未接入本项目卸载 hook 的 MSI。WebView2 使用离线安装器；构建时需要获取微软安装组件，安装包内包含该组件；缺少 WebView2 的干净离线设备仍须单独验收。离线安装器会明显增加包体积，不能沿用 Mac 31 MiB 的包体积说明。

发布检查验证暂存 manifest 与构建目标相同、两份 manifest 一致，以及组件架构、哈希、DLL 依赖和源码许可资料。当前 Mac 清单不能通过 Windows 目标检查。安装包位于 `src-tauri/target/release/bundle/nsis/`；签名与干净安装验证另行完成。

## 清理与导出边界

新的固定私有目录为 `%LOCALAPPDATA%\com.videostitch.desktop\Runtime-v2`，内部含 `.video-stitch-owner` 常量归属标记和 `WebView` 数据目录。归属标记不是业务设置或任务索引。

- 启动和退出仅清理带正确标记且非符号链接的这一目录；未知目录保留并提示。删除内容成功后才移除归属标记，文件被占用时仍可安全重试。
- 目录选择、默认输出路径和恢复入口共同拒绝把这一区域用于导出，检查规范化路径和大小写，防止别名绕过。
- NSIS hook 禁用 Tauri 默认的整目录应用数据删除，仅删除正确标记的新目录，拒绝根目录和内部的 reparse point；上级目录只允许非递归删除空目录。文件占用或链接导致清理中止时，保留归属标记和剩余文件，交互卸载显示说明，静默卸载不会等待弹窗。
- 旧版 `%LOCALAPPDATA%\com.videostitch.desktop\WebView` 不再自动递归删除，以保留以前可能混入的导出数据。旧目录的私有数据回收尚需单独核实，不按文件后缀扫描或推断归属。
- 所有其他导出目录及其中的正式视频、进行中视频、恢复记录保持原有保留规则。

Windows 在完成 WebView 和事件注册后才显示窗口；空队列关闭由后端处理，有活动任务时保留前端确认流程。正常退出先停止任务并持久化，再由同一可执行文件的短时清理模式等待主进程真正结束，以释放 WebView2 文件占用。入口只接受父进程标识，不接受业务目录；最多等待主进程 20 秒、最多尝试清理 100 次，每次失败间隔 100 毫秒（不含文件操作耗时），并获取现有应用实例锁，避免清理新实例正在使用的目录。成功后进程立即结束，不创建常驻服务或计划任务。

## CI 与剩余验收

仓库包含 `.github/workflows/windows-build.yml`。它在 Windows runner 上构建固定源码、运行原生核心/媒体测试、检查完整桌面代码并产出 NSIS 安装包，Actions 提交已锁定。支持手动触发、验证分支自动运行及被 `.github/workflows/release.yml` 复用。主分支更新版本或发布工作流后，发布流程会调用它，全部平台检查通过后将安装包发布到 Releases。用户已授权合并及发布，实际结果见验证记录，不将配置存在当作构建通过。

`scripts/smoke-windows-installer.ps1` 只允许在一次性 GitHub Actions runner 执行。它将安装器静默安装到含中文和空格的临时目录，运行已安装的 FFmpeg/ffprobe，检查主窗口位于 Windows 工作区内，首次出现窗口即发送正常关闭请求，等待运行目录清理，并确认短时清理进程结束。卸载覆盖未知目录、目录链接、文件被占用后重试，核对旧目录及导出视频/恢复记录保持不变。该检查不代表缺少 WebView2 的离线设备或完整界面操作验收。

Windows 条件编译的磁盘查询回归测试必须实际运行，验证普通路径和 `canonicalize` 扩展路径都能查询空间。真实媒体队列测试还需覆盖刷盘后原子提交、恢复记录替换和取消清理；不能只检查 FFmpeg 退出码。

安装/卸载应在缺少 WebView2 的离线设备，以及已有共享 WebView2 的设备上分别验证。另需覆盖中文/空格/网络路径、原生文件选择器、休眠或拔盘、强制结束主进程的子进程归属和清理文件被占用等情形。系统原生选择器历史和强制结束进程等场景仍不能标为已完成验收。正常退出及卸载的自动检查结果以验证记录中的 CI 运行证据为准。
