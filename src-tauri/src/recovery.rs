use crate::{
    contracts::{Settings, TaskState},
    error::{AppError, Result},
    files::{self, Fingerprint},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordAsset {
    pub id: String,
    pub path: PathBuf,
    pub fingerprint: Option<Fingerprint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordTask {
    pub id: String,
    pub a: usize,
    pub b: usize,
    pub name: String,
    pub attempt: String,
    pub temporary: String,
    pub state: TaskState,
    pub partial_identity: Option<String>,
    pub committed: Option<Fingerprint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub schema_version: u32,
    pub batch_id: String,
    pub settings: Settings,
    pub assets_a: Vec<RecordAsset>,
    pub assets_b: Vec<RecordAsset>,
    pub tasks: Vec<RecordTask>,
}

/// UUID 和固定模板共同限制可清理的进行中文件名。
pub fn temporary_name(id: &str, attempt: &str) -> Result<String> {
    Uuid::parse_str(id).map_err(|_| AppError::new("INVALID_RECORD", "任务标识无效。"))?;
    Uuid::parse_str(attempt).map_err(|_| AppError::new("INVALID_RECORD", "执行标识无效。"))?;
    Ok(format!(".{id}.{attempt}.partial.mp4"))
}

/// 只写恢复必要信息，同目录原子替换，不记录进度、错误或检测缓存。
pub fn save(directory: &Path, record: &Record) -> Result<()> {
    let path = record_path(directory, &record.batch_id)?;
    let next = directory.join(format!("unfinished-tasks.{}.next.json", record.batch_id));
    if next.exists() {
        return Err(AppError::new(
            "RECOVERY_REQUIRED",
            "存在未提交的恢复记录，请先选择该记录进行恢复。",
        ));
    }
    let bytes = serde_json::to_vec(record)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&next)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    #[cfg(not(windows))]
    fs::rename(&next, &path)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
        let from: Vec<u16> = next.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x1 | 0x8) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    files::sync_directory(directory)?;
    Ok(())
}

/// 恢复文件名包含批次身份，不覆盖同目录其他批次。
pub fn record_path(directory: &Path, batch: &str) -> Result<PathBuf> {
    Uuid::parse_str(batch).map_err(|_| AppError::new("INVALID_RECORD", "批次标识无效。"))?;
    files::owned_child(directory, &format!("unfinished-tasks.{batch}.json"))
}

/// 拒绝无界 JSON、索引越界、重复任务和任意删除路径。
pub fn load(path: &Path) -> Result<(PathBuf, Record)> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(AppError::new(
            "INVALID_RECORD",
            "请选择实际恢复记录，不支持符号链接。",
        ));
    }
    let metadata = fs::metadata(path)?;
    if metadata.len() > 64 * 1024 * 1024 {
        return Err(AppError::new(
            "INVALID_RECORD",
            "恢复记录超过 64MB，已停止读取。",
        ));
    }
    let record: Record = serde_json::from_reader(std::fs::File::open(path)?)?;
    if record.schema_version != 1 {
        return Err(AppError::new(
            "INVALID_RECORD",
            "恢复记录版本不受支持，请使用匹配版本的软件。",
        ));
    }
    let directory = fs::canonicalize(
        path.parent()
            .ok_or_else(|| AppError::new("INVALID_RECORD", "恢复目录无效。"))?,
    )?;
    let expected = record_path(&directory, &record.batch_id)?;
    let next = format!("unfinished-tasks.{}.next.json", record.batch_id);
    if path.file_name() != expected.file_name()
        && path.file_name() != Some(std::ffi::OsStr::new(&next))
    {
        return Err(AppError::new(
            "INVALID_RECORD",
            "恢复记录名称与批次不匹配。",
        ));
    }
    if record.assets_a.len().checked_mul(record.assets_b.len()) != Some(record.tasks.len()) {
        return Err(AppError::new(
            "INVALID_RECORD",
            "恢复记录的组合数量不一致。",
        ));
    }
    let mut ids = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    let mut pairs = std::collections::HashSet::new();
    for task in &record.tasks {
        if task.a >= record.assets_a.len()
            || task.b >= record.assets_b.len()
            || !ids.insert(&task.id)
            || !pairs.insert((task.a, task.b))
            || !names.insert(task.name.to_lowercase())
            || task.temporary != temporary_name(&task.id, &task.attempt)?
            || !task.name.ends_with(".mp4")
        {
            return Err(AppError::new(
                "INVALID_RECORD",
                "恢复记录中存在冲突或无效任务。",
            ));
        }
        files::owned_child(&directory, &task.name)?;
        files::owned_child(&directory, &task.temporary)?;
    }
    // 先安全提升经过验证的原子更新记录；整个过程不接触导出视频。
    let next_path = directory.join(&next);
    if path.file_name() == Some(std::ffi::OsStr::new(&next)) {
        if expected.exists() {
            fs::remove_file(&expected)?;
        }
        fs::rename(path, &expected)?;
    } else if next_path.exists() {
        return Err(AppError::new(
            "RECOVERY_REQUIRED",
            format!("存在更新的恢复记录，请选择 {next}。"),
        ));
    }
    Ok((directory, record))
}

/// 只清理记录身份与实际文件身份都吻合的单个残片。
pub fn clean_partial(directory: &Path, task: &RecordTask) -> Result<()> {
    if task.temporary != temporary_name(&task.id, &task.attempt)? {
        return Err(AppError::new("INVALID_RECORD", "进行中文件归属无效。"));
    }
    let path = files::owned_child(directory, &task.temporary)?;
    if !path.exists() {
        return Ok(());
    }
    let actual = files::fingerprint(&path)?;
    if task.partial_identity.as_ref() != Some(&actual.identity) {
        return Err(AppError::new(
            "OUTPUT_CONFLICT",
            "进行中文件身份发生变化，未删除文件，请手动检查。",
        ));
    }
    fs::remove_file(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_requires_matching_identity() {
        let dir = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4().to_string();
        let attempt = Uuid::new_v4().to_string();
        let temporary = temporary_name(&id, &attempt).unwrap();
        let path = dir.path().join(&temporary);
        fs::write(&path, b"user").unwrap();
        let mut task = RecordTask {
            id,
            a: 0,
            b: 0,
            name: "a.mp4".into(),
            attempt,
            temporary,
            state: TaskState::Interrupted,
            partial_identity: None,
            committed: None,
        };
        assert!(clean_partial(dir.path(), &task).is_err());
        assert!(path.exists());
        task.partial_identity = Some(files::fingerprint(&path).unwrap().identity);
        clean_partial(dir.path(), &task).unwrap();
        assert!(!path.exists());
    }
}
