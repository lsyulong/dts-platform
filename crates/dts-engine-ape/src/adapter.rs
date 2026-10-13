// Copyright 2026 DTS Platform Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::io::Write;
use std::process::Stdio;
use std::time::Duration;

use dts_engine::{EngineError, RunStatus, SyncProgress, TaskSubmission};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;
use tracing::{info, warn};

use crate::config::ApeDtsIniConfig;

pub(crate) fn validate_submission(task: &TaskSubmission) -> Result<(), EngineError> {
    ApeDtsIniConfig::from_submission(task, &task.run_id).map(|_| ())
}

pub(crate) async fn run_sync(
    task: TaskSubmission,
    run_id: &str,
    bin_path: &str,
    progress_tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
    cancel_rx: oneshot::Receiver<()>,
) -> Result<(), EngineError> {
    let mut logger_file = tempfile::Builder::new()
        .prefix("dts-monitor-")
        .suffix(".yaml")
        .tempfile()
        .map_err(|e| EngineError::Internal(format!("创建日志配置失败: {e}")))?;
    logger_file.write_all(b"appenders:\n  stdout:\n    kind: console\n    encoder:\n      pattern: '{m}{n}'\nloggers:\n  monitor_logger:\n    level: info\n    appenders: [stdout]\n    additive: false\nroot:\n  level: off\n  appenders: []\n")
        .map_err(|e| EngineError::Internal(format!("写入日志配置失败: {e}")))?;
    logger_file
        .flush()
        .map_err(|e| EngineError::Internal(format!("刷新日志配置失败: {e}")))?;
    let log_dir =
        tempfile::tempdir().map_err(|e| EngineError::Internal(format!("创建日志目录失败: {e}")))?;
    let config = ApeDtsIniConfig::from_submission(&task, run_id)?
        .with_runtime_paths(logger_file.path(), log_dir.path())?;
    let mut config_file = tempfile::Builder::new()
        .prefix("dts-pipeline-")
        .suffix(".ini")
        .tempfile()
        .map_err(|e| EngineError::Internal(format!("创建临时配置失败: {e}")))?;
    config_file
        .write_all(config.as_str().as_bytes())
        .map_err(|e| EngineError::Internal(format!("写入临时配置失败: {e}")))?;
    config_file
        .flush()
        .map_err(|e| EngineError::Internal(format!("刷新配置失败: {e}")))?;
    let mut child = Command::new(bin_path)
        .arg(config_file.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| EngineError::Internal(format!("无法启动 ape-dts (path={bin_path}): {e}")))?;
    info!(run_id, task_id = %task.task_id, "ape-dts process started");
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| EngineError::Internal("stdout pipe unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| EngineError::Internal("stderr pipe unavailable".into()))?;
    let rid = run_id.to_string();
    let tid = task.task_id.clone();
    let tx = progress_tx.clone();
    let mut stdout_handle = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        let mut stats = GlobalStats::default();
        while let Some(line) = lines.next_line().await? {
            if let Some(metric) = parse_global_metric(&line) {
                stats.observe(metric);
                if tx
                    .send(SyncProgress {
                        run_id: rid.clone(),
                        task_id: tid.clone(),
                        status: RunStatus::Running,
                        rows_synced: stats.rows,
                        bytes_synced: stats.bytes,
                        metrics_available: stats.available(),
                        total_rows: None,
                        current_table: None,
                        percent: 0.0,
                        message: Some(format!(
                            "已观察到全局写入计数：{} 行，{} 字节",
                            stats.rows, stats.bytes
                        )),
                    })
                    .is_err()
                {
                    break;
                }
            }
        }
        Ok::<_, std::io::Error>(stats)
    });
    // Drain stderr without broadcasting raw connection strings or credentials.
    let mut stderr_handle = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while lines.next_line().await?.is_some() {}
        Ok::<_, std::io::Error>(())
    });
    let (status, message) = tokio::select! {
        cancelled = cancel_rx => {
            child.kill().await.map_err(|e| EngineError::Internal(format!("终止子进程失败: {e}")))?;
            if cancelled.is_ok() {
                (RunStatus::Cancelled, Some("任务已取消".to_string()))
            } else {
                (RunStatus::Failed, Some("引擎取消通道意外断开".to_string()))
            }
        }
        _ = progress_tx.closed() => {
            child.kill().await.map_err(|e| EngineError::Internal(format!("终止子进程失败: {e}")))?;
            return Err(EngineError::Internal("progress receiver disconnected".into()));
        }
        exit = tokio::time::timeout(Duration::from_secs(3600), child.wait()) => {
            match exit {
                Ok(Ok(exit)) if exit.success() => (RunStatus::Completed, None),
                Ok(Ok(exit)) => (RunStatus::Failed, Some(format!("ape-dts exited with code {:?}", exit.code()))),
                Ok(Err(e)) => {
                    child.kill().await.map_err(|kill| EngineError::Internal(format!("终止子进程失败: {kill}")))?;
                    (RunStatus::Failed, Some(format!("ape-dts process wait failed: {e}")))
                }
                Err(_) => {
                    child.kill().await.map_err(|e| EngineError::Internal(format!("终止子进程失败: {e}")))?;
                    (RunStatus::Failed, Some("ape-dts task timed out (1 hour)".into()))
                }
            }
        }
    };
    let stats = match tokio::time::timeout(Duration::from_secs(2), &mut stdout_handle).await {
        Ok(Ok(Ok(stats))) => stats,
        _ => {
            stdout_handle.abort();
            warn!(run_id, "stdout reader failed or did not close");
            GlobalStats::default()
        },
    };
    if tokio::time::timeout(Duration::from_secs(2), &mut stderr_handle)
        .await
        .is_err()
    {
        stderr_handle.abort();
    }
    progress_tx
        .send(SyncProgress {
            run_id: run_id.into(),
            task_id: task.task_id,
            status: status.clone(),
            rows_synced: stats.rows,
            bytes_synced: stats.bytes,
            metrics_available: stats.available(),
            total_rows: None,
            current_table: None,
            percent: if status == RunStatus::Completed {
                100.0
            } else {
                0.0
            },
            message: message.or_else(|| {
                Some(if stats.available() {
                    "全局计数来自最后的日志采样，需通过数据对账确认最终结果".into()
                } else {
                    "执行已结束，未获得完整的全局计数；需进行数据对账".into()
                })
            }),
        })
        .map_err(|_| EngineError::Internal("progress receiver disconnected".into()))?;
    Ok(())
}
#[derive(Debug)]
enum GlobalMetric {
    Rows(u64),
    Bytes(u64),
}

#[derive(Default)]
struct GlobalStats {
    rows: u64,
    bytes: u64,
    has_rows: bool,
    has_bytes: bool,
}

impl GlobalStats {
    fn observe(&mut self, metric: GlobalMetric) {
        match metric {
            GlobalMetric::Rows(rows) => {
                self.rows = self.rows.max(rows);
                self.has_rows = true;
            },
            GlobalMetric::Bytes(bytes) => {
                self.bytes = self.bytes.max(bytes);
                self.has_bytes = true;
            },
        }
    }
    fn available(&self) -> bool {
        self.has_rows && self.has_bytes
    }
}

fn parse_global_metric(line: &str) -> Option<GlobalMetric> {
    // Matches ape-dts GroupMonitor's cumulative run-wide statistics only.
    let fields: Vec<&str> = line.split('|').map(str::trim).collect();
    let fields = fields
        .windows(4)
        .find(|fields| fields[0] == "pipeline" && fields[1] == "global")?;
    let value = fields[3].strip_prefix("latest=")?.parse::<u64>().ok()?;
    match fields[2] {
        "sinked_records" => Some(GlobalMetric::Rows(value)),
        "sinked_bytes" => Some(GlobalMetric::Bytes(value)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_accepts_only_actual_upstream_global_counters() {
        assert!(matches!(
            parse_global_metric("2026-10-08 | pipeline | global | sinked_records | latest=42"),
            Some(GlobalMetric::Rows(42))
        ));
        assert!(matches!(
            parse_global_metric("pipeline | global | sinked_bytes | latest=128"),
            Some(GlobalMetric::Bytes(128))
        ));
        for ignored in [
            "extracted 42 rows from mysql://user:secret@host/db",
            "pipeline | orders | sinked_records | latest=99",
            "pipeline | global | sinked_records | latest=42 secret",
            "ERROR postgres://user:secret@host/db",
        ] {
            assert!(parse_global_metric(ignored).is_none());
        }
    }

    #[test]
    fn cumulative_global_samples_are_not_summed_and_missing_samples_remain_unknown() {
        let mut stats = GlobalStats::default();
        assert!(!stats.available());
        stats.observe(GlobalMetric::Rows(42));
        assert!(!stats.available());
        stats.observe(GlobalMetric::Rows(84));
        stats.observe(GlobalMetric::Bytes(128));
        assert!(stats.available());
        assert_eq!(stats.rows, 84);
        assert_eq!(stats.bytes, 128);
    }
}
