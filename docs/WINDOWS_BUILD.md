# Windows 构建与验证

目标为 Windows 10 22H2+ / Windows 11 x64。当前已提供构建配方和发布校验，尚无 Windows 实机构建或运行证据。Mac 交叉编译检查不能代替下列 Windows 原生流程。

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

Windows 平台配置限定 NSIS，避免生成未接入本项目卸载 hook 的 MSI。WebView2 使用离线安装器；构建时需要获取微软安装组件，生成后的安装包可在缺少 WebView2 的离线系统安装。离线安装器会明显增加包体积，不能沿用 Mac 31 MiB 的包体积说明。

发布检查验证暂存 manifest 与构建目标相同、两份 manifest 一致，以及组件架构、哈希、DLL 依赖和源码许可资料。当前 Mac 清单不能通过 Windows 目标检查。安装包位于 `src-tauri/target/release/bundle/nsis/`；签名与干净安装验证另行完成。

## 清理与导出边界

新的固定私有目录为 `%LOCALAPPDATA%\com.videostitch.desktop\Runtime-v2`，内部含 `.video-stitch-owner` 常量归属标记和 `WebView` 数据目录。归属标记不是业务设置或任务索引。

- 启动和退出仅清理带正确标记且非符号链接的这一目录；未知目录保留并提示。
- 目录选择、默认输出路径和恢复入口共同拒绝把这一区域用于导出，检查规范化路径和大小写，防止别名绕过。
- NSIS hook 只删除正确标记的新目录，拒绝 reparse point；上级目录只允许非递归删除空目录。
- 旧版 `%LOCALAPPDATA%\com.videostitch.desktop\WebView` 不再自动递归删除，以保留以前可能混入的导出数据。旧目录的私有数据回收尚需单独核实，不按文件后缀扫描或推断归属。
- 所有其他导出目录及其中的正式视频、进行中视频、恢复记录保持原有保留规则。

## 尚需 Windows 环境的验收

仓库包含 `.github/workflows/windows-build.yml`。它在 Windows runner 上构建固定源码、运行原生核心/媒体测试、检查完整桌面代码并产出 NSIS 安装包，Actions 提交已锁定。除手动触发外，仅对 `codex/windows-compatibility` 分支的构建相关文件推送自动运行；主分支不会因此自动运行。用户已授权推送修复及执行 CI，实际结果见验证记录，不将配置存在当作构建通过。

Windows 条件编译的磁盘查询回归测试必须实际运行，验证普通路径和 `canonicalize` 扩展路径都能查询空间。真实媒体队列测试还需覆盖刷盘后原子提交、恢复记录替换和取消清理；不能只检查 FFmpeg 退出码。

安装/卸载应在缺少 WebView2 的离线设备，以及已有共享 WebView2 的设备上分别验证。另需覆盖中文/空格/网络路径、原生文件选择器、休眠或拔盘、强制结束主进程的子进程归属和清理文件被占用等情形。当前正常退出的删除重试、系统原生选择器历史和强制结束进程场景均不能标为已完成验收。
