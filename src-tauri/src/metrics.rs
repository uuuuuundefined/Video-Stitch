use crate::contracts::Performance;
use std::{collections::HashSet, path::Path, time::Instant};
use sysinfo::{Disks, Pid, ProcessRefreshKind, ProcessesToUpdate, System};
mod platform;

pub struct Sampler {
    system: System,
    last: Option<Instant>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            system: System::new(),
            last: None,
        }
    }
}

impl Sampler {
    /// 聚合应用可识别进程树，按平台私有内存口径采样。
    pub fn sample(
        &mut self,
        directory: &Path,
        speed: Option<f64>,
        eta: Option<f64>,
    ) -> Performance {
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let root = Pid::from_u32(std::process::id());
        let mut owned = HashSet::from([root]);
        loop {
            let before = owned.len();
            for (pid, process) in self.system.processes() {
                if process.parent().is_some_and(|p| owned.contains(&p)) {
                    owned.insert(*pid);
                }
            }
            if owned.len() == before {
                break;
            }
        }
        let ready = self.last.is_some_and(|t| t.elapsed().as_millis() >= 200);
        let cpu = owned
            .iter()
            .filter_map(|id| self.system.process(*id))
            .map(|p| f64::from(p.cpu_usage()))
            .sum::<f64>()
            / (self
                .system
                .cpus()
                .len()
                .max(std::thread::available_parallelism().map_or(1, usize::from))
                as f64);
        let memory: Option<Vec<u64>> = owned
            .iter()
            .filter(|id| self.system.process(**id).is_some())
            .map(|id| platform::process_memory(id.as_u32()))
            .collect();
        self.last = Some(Instant::now());
        let disks = Disks::new_with_refreshed_list();
        let disk = disks
            .list()
            .iter()
            .filter(|d| directory.starts_with(d.mount_point()))
            .max_by_key(|d| d.mount_point().as_os_str().len())
            .map(|d| d.available_space() as f64);
        let process_known = self.system.process(root).is_some();
        let memory_known = self.system.total_memory() > 0;
        let process_bytes = if process_known {
            memory.map(|v| v.iter().map(|x| *x as f64).sum())
        } else {
            None
        };
        let scope = if cfg!(target_os = "macos") {
            "物理内存计费 footprint"
        } else {
            "私有提交内存"
        };
        Performance { cpu_percent:(ready&&process_known).then_some(cpu.clamp(0.0,100.0)),process_bytes,memory_scope:format!("应用及可识别子进程的{scope}合计。系统独立托管、无法归属的 WebView 进程不计入；不代表所有系统界面开销。"),system_used:memory_known.then_some(self.system.used_memory() as f64),system_total:memory_known.then_some(self.system.total_memory() as f64),gpu_percent:None,gpu_scope:"当前平台暂未提供可靠的 GPU 编码引擎占用采样。".into(),speed,eta_seconds:eta,disk_free:disk }
    }
}

/// 读取输出卷空间，不创建空间探测文件。
pub fn available_space(directory: &Path) -> Option<u64> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| directory.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}
