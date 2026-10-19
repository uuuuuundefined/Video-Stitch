use crate::{
    contracts::*,
    engine,
    error::{AppError, Result},
    queue::Service,
};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, Manager, State};

struct DesktopState {
    service: Arc<Service>,
    grants: Mutex<HashSet<PathBuf>>,
    exiting: AtomicBool,
}

/// 使用原生文件选择器授权输入路径。
#[tauri::command]
async fn select_assets(state: State<'_, DesktopState>, group: String) -> Result<()> {
    let files = rfd::AsyncFileDialog::new()
        .set_title("添加视频")
        .add_filter("视频", &["mp4", "mov", "m4v"])
        .pick_files()
        .await;
    clear_native_preferences()?;
    if let Some(files) = files {
        state
            .service
            .import(
                files.into_iter().map(|f| f.path().to_path_buf()).collect(),
                &group,
            )
            .await?;
    }
    Ok(())
}

/// 只接受原生拖入事件已经授权的路径，不开放通用路径执行。
#[tauri::command]
async fn import_dropped(
    state: State<'_, DesktopState>,
    paths: Vec<PathBuf>,
    group: String,
) -> Result<()> {
    {
        let mut grants = state
            .grants
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !paths.iter().all(|p| grants.contains(p)) {
            return Err(AppError::new(
                "UNAUTHORIZED_PATH",
                "请通过文件选择器或拖入操作添加视频。",
            ));
        }
        for path in &paths {
            grants.remove(path);
        }
    }
    state.service.import(paths, &group).await
}

/// 移除当前输入集合中的引用。
#[tauri::command]
fn remove_assets(state: State<'_, DesktopState>, group: String, id: Option<String>) -> Result<()> {
    state.service.remove(&group, id.as_deref())
}

/// 参数变更必须同步生成后端媒体计划。
#[tauri::command]
fn configure(state: State<'_, DesktopState>, settings: Settings) -> Result<()> {
    state.service.configure(settings)
}

/// 分页读取内存状态。
#[tauri::command]
fn get_snapshot(state: State<'_, DesktopState>, page: usize) -> Snapshot {
    state.service.snapshot(page)
}

/// 通过原生选择器指定最终输出目录。
#[tauri::command]
async fn select_directory(state: State<'_, DesktopState>) -> Result<()> {
    let folder = rfd::AsyncFileDialog::new()
        .set_title("选择视频保存目录")
        .pick_folder()
        .await;
    clear_native_preferences()?;
    if let Some(folder) = folder {
        state.service.set_directory(folder.path().to_path_buf())?;
    }
    Ok(())
}

/// 请求标识让双击或重复 IPC 不创建重复批次。
#[tauri::command]
async fn start_batch(
    state: State<'_, DesktopState>,
    request_id: String,
    allow_blocked: bool,
) -> Result<()> {
    state.service.start(request_id, allow_blocked)
}

/// 完成当前项后暂停调度。
#[tauri::command]
fn pause_batch(state: State<'_, DesktopState>) -> Result<()> {
    state.service.pause()
}

/// 继续暂停队列。
#[tauri::command]
async fn resume_batch(state: State<'_, DesktopState>) -> Result<()> {
    state.service.resume()
}

/// 取消指定项或所有剩余项。
#[tauri::command]
fn cancel_tasks(state: State<'_, DesktopState>, id: Option<String>) -> Result<()> {
    state.service.cancel(id.as_deref())
}

/// 手动重试失败项，成功和取消项排除在外。
#[tauri::command]
async fn retry_tasks(state: State<'_, DesktopState>, id: Option<String>) -> Result<()> {
    state.service.retry(id.as_deref())
}

/// 用户选择恢复记录后重新验证源文件与输出身份。
#[tauri::command]
async fn select_recovery_record(state: State<'_, DesktopState>) -> Result<()> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("选择导出目录中的任务恢复记录")
        .add_filter("任务恢复记录", &["json"])
        .pick_file()
        .await;
    clear_native_preferences()?;
    if let Some(file) = file {
        state.service.recover(file.path()).await?;
    }
    Ok(())
}

/// 修复源文件路径必须经原生选择器重新授权。
#[tauri::command]
async fn relink_asset(state: State<'_, DesktopState>, id: String) -> Result<()> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("重新定位源视频并重新检查")
        .add_filter("视频", &["mp4", "mov", "m4v"])
        .pick_file()
        .await;
    clear_native_preferences()?;
    if let Some(file) = file {
        state.service.relink(&id, file.path()).await?;
    }
    Ok(())
}

/// 获取真实环境能力及检测说明。
#[tauri::command]
fn get_environment_status(state: State<'_, DesktopState>) -> EncoderStatus {
    state.service.environment()
}

/// 性能采样保持在内存中。
#[tauri::command]
fn get_performance_snapshot(state: State<'_, DesktopState>) -> Performance {
    state.service.performance()
}

/// 用系统默认程序打开已完成的当前批次成果。
#[tauri::command]
fn open_output(state: State<'_, DesktopState>, id: Option<String>) -> Result<()> {
    let path = state.service.output_path(id.as_deref())?;
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("/usr/bin/open");
        c.arg("--");
        c
    } else {
        std::process::Command::new("explorer.exe")
    };
    command
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

/// 受控退出先停止编码并持久化，再关闭 WebView。
#[tauri::command]
async fn exit_app(app: tauri::AppHandle, state: State<'_, DesktopState>) -> Result<()> {
    state.service.shutdown().await?;
    #[cfg(target_os = "macos")]
    crate::cleanup::schedule()?;
    #[cfg(windows)]
    crate::runtime::schedule_cleanup()?;
    state.exiting.store(true, Ordering::SeqCst);
    app.exit(0);
    Ok(())
}

/// 本地单窗口入口；无日志插件、远端页面、预览缓存或设置持久化。
pub fn run() {
    // 对当前可执行文件加进程生命周期锁，不新增持久化锁文件。
    let instance = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::File::open(p).ok());
    let Some(_instance) = instance.filter(|file| file.try_lock().is_ok()) else {
        rfd::MessageDialog::new()
            .set_title("Video Stitch")
            .set_description("当前应用已在运行、正在退出清理，或无法获取运行锁。请稍后重试。")
            .show();
        return;
    };
    if let Err(error) = clear_native_preferences() {
        rfd::MessageDialog::new()
            .set_title("运行数据需要处理")
            .set_description(error.message)
            .show();
        return;
    }
    #[cfg(windows)]
    let runtime_directory = match crate::runtime::prepare() {
        Ok(directory) => directory,
        Err(error) => {
            rfd::MessageDialog::new()
                .set_title("运行数据需要处理")
                .set_description(error.message)
                .show();
            return;
        }
    };
    let service = Service::new(engine::locate().ok());
    let app = tauri::Builder::default()
        .manage(DesktopState {
            service: service.clone(),
            grants: Mutex::new(HashSet::new()),
            exiting: AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            select_assets,
            import_dropped,
            remove_assets,
            configure,
            get_snapshot,
            select_directory,
            start_batch,
            pause_batch,
            resume_batch,
            cancel_tasks,
            retry_tasks,
            select_recovery_record,
            relink_asset,
            get_environment_status,
            get_performance_snapshot,
            open_output,
            exit_app
        ])
        .setup(move |app| {
            #[allow(unused_mut)]
            let mut builder = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Video Stitch · 视频拼接工作台")
            .inner_size(1200.0, 860.0)
            .min_inner_size(760.0, 640.0)
            .incognito(true);
            #[cfg(windows)]
            {
                // 预留系统边框与标题栏，小屏幕或高缩放时底栏也位于可用工作区内。
                if let Ok(Some(monitor)) = app.app_handle().primary_monitor() {
                    let work = monitor.work_area();
                    let scale = monitor.scale_factor();
                    let width = (work.size.width as f64 / scale - 32.0).clamp(280.0, 1200.0);
                    let height = (work.size.height as f64 / scale - 64.0).clamp(200.0, 860.0);
                    builder = builder
                        .min_inner_size(width.min(760.0), height.min(640.0))
                        .inner_size(width, height)
                        .position(
                            work.position.x as f64 / scale + 8.0,
                            work.position.y as f64 / scale + 8.0,
                        );
                }
                builder = builder.data_directory(runtime_directory);
            }
            let _window = builder.build()?;
            tauri::async_runtime::spawn(async move {
                service.initialize().await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            let state = window.state::<DesktopState>();
            match event {
                tauri::WindowEvent::CloseRequested { api, .. }
                    if !state.exiting.load(Ordering::Relaxed) =>
                {
                    api.prevent_close();
                    let _ = window.emit("close-requested", ());
                }
                tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                    let mut grants = state
                        .grants
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    grants.clear();
                    grants.extend(paths.iter().cloned());
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!());
    match app {
        Ok(app) => {
            let on_event = |app: &tauri::AppHandle, event: tauri::RunEvent| {
                if matches!(event, tauri::RunEvent::Exit) {
                    if let Err(error) = clear_native_preferences() {
                        rfd::MessageDialog::new()
                            .set_title("运行数据未能清理")
                            .set_description(error.message)
                            .show();
                    }
                }
                if let tauri::RunEvent::ExitRequested { ref api, .. } = event {
                    let state = app.state::<DesktopState>();
                    if !state.exiting.load(Ordering::Relaxed) {
                        api.prevent_exit();
                        let _ = app.emit("close-requested", ());
                    }
                }
            };
            app.run(on_event);
        }
        Err(error) => {
            let _ = rfd::MessageDialog::new()
                .set_title("Video Stitch 启动失败")
                .set_description(format!("请检查安装是否完整。\n{error}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
    }
}

/// 清除 NSOpenPanel 自动记住的路径和窗口偏好，不建立原生最近目录索引。
fn clear_native_preferences() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSString, NSUserDefaults};
        let defaults = NSUserDefaults::standardUserDefaults();
        defaults.removePersistentDomainForName(&NSString::from_str("com.videostitch.desktop"));
        if !defaults.synchronize() {
            return Err(AppError::new(
                "CLEANUP_FAILED",
                "系统文件选择器偏好未能清理，请关闭应用后运行随包提供的私有数据清理脚本。",
            ));
        }
    }
    Ok(())
}
