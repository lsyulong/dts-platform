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


use std::process::Stdio;
use std::time::Duration;

use chrono::Local;
use dts_engine::{EngineError, RunStatus, SyncProgress, TaskSubmission};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;
use tracing::{error, info, warn};

/// ape-dts 二进制文件路径（可通过环境变量覆盖）
const DEFAULT_APE_DTS_BIN: &str = "../ape-dts/target/release/dt-main";

/// 通过子进程调用 ape-dts 二进制执行同步任务。
///
/// 流程：
/// 1. 将 TaskSubmission 序列化为 ape-dts 的 TOML 配置文件
/// 2. 写入临时文件
/// 3. 启动 `dt-main <config_path>` 子进程
/// 4. 逐行读取 stdout 解析日志，提取进度信息
/// 5. 等待进程结束或收到取消信号，报告最终状态
///
/// 当 `cancel_rx` 收到信号时，kill 子进程并清理临时文件。
pub(crate) async fn run_sync(
    task: TaskSubmission,
    run_id: &str,
    progress_tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
    cancel_rx: oneshot::Receiver<()>,
) -> Result<(), EngineError> {
    let task_id = task.task_id.clone();
    let total_tables = task.config.table_mappings.len();
    let source_table = task
        .config
        .table_mappings
        .first()
        .map(|m| m.source_table.clone())
        .unwrap_or_default();

    // 1. 发送启动进度
    if let Err(_e) = progress_tx.send(SyncProgress {
        run_id: run_id.into(),
        task_id: task_id.clone(),
        status: RunStatus::Running,
        rows_synced: 0,
        bytes_synced: 0,
        total_rows: None,
        current_table: Some(source_table.clone()),
        percent: 0.0,
        message: Some(format!(
            "Starting ape-dts: {} -> {}, {} table(s)",
            task.source.db_type, task.target.db_type, total_tables,
        )),
    }) {
        warn!(run_id = %run_id, "progress receiver disconnected at startup");
        return Err(EngineError::Internal("progress receiver disconnected".into()));
    }

    // 2. 构建并写入配置文件
    let config = ApeDtsTomlConfig::from_submission(&task, run_id);
    let config_path = write_temp_config(&config, run_id)?;

    info!(
        run_id = %run_id,
        task_id = %task_id,
        config_path = %config_path,
        "启动 ape-dts 子进程"
    );

    // 3. 查找 ape-dts 二进制
    let bin_path = std::env::var("APE_DTS_BIN").unwrap_or_else(|_| DEFAULT_APE_DTS_BIN.to_string());

    // 4. 启动子进程
    let mut child = Command::new(&bin_path)
        .arg(&config_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            EngineError::Internal(format!("无法启动 ape-dts 子进程 (path={bin_path}): {e}"))
        })?;

    // 5. 读取 stdout 解析进度
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| EngineError::Internal("ape-dts: stdout pipe not available".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| EngineError::Internal("ape-dts: stderr pipe not available".into()))?;

    let rid = run_id.to_string();
    let tid = task_id.clone();
    let tx = progress_tx.clone();

    // 后台读取 stdout
    let stdout_handle = tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut rows_synced: u64 = 0;
        let mut current_table: Option<String> = None;

        while let Ok(Some(line)) = lines.next_line().await {
            // 解析 ape-dts 日志，提取进度信息
            if let Some(progress) = parse_progress_line(&line, &rid, &tid) {
                rows_synced = progress.rows_synced;
                current_table = progress.current_table.clone();
                let _ = tx.send(progress);
            }
        }
        (rows_synced, current_table)
    });

    // 后台读取 stderr
    let rid2 = run_id.to_string();
    let tid2 = task_id.clone();
    let tx2 = progress_tx.clone();
    let stderr_handle = tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.contains("ERROR") || line.contains("error") {
                error!(run_id = %rid2, "ape-dts: {line}");
                let _ = tx2.send(SyncProgress {
                    run_id: rid2.clone(),
                    task_id: tid2.clone(),
                    status: RunStatus::Running,
                    rows_synced: 0,
                    bytes_synced: 0,
                    total_rows: None,
                    current_table: None,
                    percent: 0.0,
                    message: Some(format!("ape-dts error: {line}")),
                });
            }
        }
    });

    // 6. 等待进程结束或收到取消信号
    let max_duration = Duration::from_secs(3600); // 1 hour max

    let exit_status = tokio::select! {
        // Check cancel signal first (biased ordering)
        _ = cancel_rx => {
            info!(run_id = %run_id, "收到取消信号，终止 ape-dts 子进程");
            child.kill().await.ok();
            let _ = tokio::time::timeout(Duration::from_secs(2), stdout_handle).await;
            let _ = tokio::time::timeout(Duration::from_secs(2), stderr_handle).await;
            if let Err(e) = std::fs::remove_file(&config_path) {
                warn!(run_id = %run_id, path = %config_path, error = %e, "failed to clean up temp config");
            }
            let _ = progress_tx.send(SyncProgress {
                run_id: run_id.into(),
                task_id,
                status: RunStatus::Cancelled,
                rows_synced: 0,
                bytes_synced: 0,
                total_rows: None,
                current_table: None,
                percent: 0.0,
                message: Some("任务已取消".into()),
            });
            return Ok(());
        }
        exit = tokio::time::timeout(max_duration, child.wait()) => {
            exit
        }
    };

    let (rows, last_table) = match stdout_handle.await {
        Ok(result) => result,
        Err(e) => {
            error!(run_id = %run_id, error = %e, "stdout reader panicked");
            (0, None)
        },
    };
    // 等待 stderr 排空后再清理（最多等待 2 秒）
    let _ = tokio::time::timeout(Duration::from_secs(2), stderr_handle).await;

    // 清理临时文件
    if let Err(e) = std::fs::remove_file(&config_path) {
        warn!(run_id = %run_id, path = %config_path, error = %e, "Failed to clean up temp config file");
    }

    match exit_status {
        Ok(Ok(status)) if status.success() => {
            info!(
                run_id = %run_id,
                task_id = %task_id,
                rows_synced = rows,
                "ape-dts 同步完成"
            );
            let _ = progress_tx.send(SyncProgress {
                run_id: run_id.into(),
                task_id,
                status: RunStatus::Completed,
                rows_synced: rows,
                bytes_synced: 0,
                total_rows: None,
                current_table: last_table,
                percent: 100.0,
                message: Some("ape-dts sync completed".into()),
            });
        },
        Ok(Ok(status)) => {
            let code = status.code().unwrap_or(-1);
            error!(run_id = %run_id, exit_code = code, "ape-dts 异常退出");
            let _ = progress_tx.send(SyncProgress {
                run_id: run_id.into(),
                task_id,
                status: RunStatus::Failed,
                rows_synced: rows,
                bytes_synced: 0,
                total_rows: None,
                current_table: last_table,
                percent: 0.0,
                message: Some(format!("ape-dts exited with code {code}")),
            });
        },
        Ok(Err(e)) => {
            error!(run_id = %run_id, error = %e, "等待 ape-dts 进程失败");
            let _ = progress_tx.send(SyncProgress {
                run_id: run_id.into(),
                task_id,
                status: RunStatus::Failed,
                rows_synced: 0,
                bytes_synced: 0,
                total_rows: None,
                current_table: None,
                percent: 0.0,
                message: Some(format!("ape-dts process error: {e}")),
            });
        },
        Err(_) => {
            error!(run_id = %run_id, "ape-dts 任务超时");
            let _ = progress_tx.send(SyncProgress {
                run_id: run_id.into(),
                task_id,
                status: RunStatus::Failed,
                rows_synced: 0,
                bytes_synced: 0,
                total_rows: None,
                current_table: None,
                percent: 0.0,
                message: Some("ape-dts task timed out (1 hour)".into()),
            });
        },
    }

    Ok(())
}

/// 解析 ape-dts 日志行，提取进度信息
///
/// 支持的日志模式：
/// - "extracted N rows from TABLE"           → 提取行数
/// - "written N rows to TABLE"               → 写入行数
/// - "progress: N% (current/total)"          → 百分比 + 总数
/// - "snapshot completed for TABLE:"         → 单表完成
/// - "total extracted: N rows, M bytes"      → 汇总统计
/// - "estimated total: N rows"               → 预估总行数
fn parse_progress_line(line: &str, run_id: &str, task_id: &str) -> Option<SyncProgress> {
    let mut rows_synced: u64 = 0;
    let mut bytes_synced: u64 = 0;
    let mut current_table: Option<String> = None;
    let mut total_rows: Option<u64> = None;
    let mut message: Option<String> = None;
    let mut percent: f64 = 0.0;

    // 模式 1: "extracted N rows from TABLE"
    if let Some(rest) = line.split("extracted ").nth(1) {
        if let Some(num_str) = rest.split_whitespace().next() {
            if let Ok(n) = num_str.replace([',', '_'], "").parse::<u64>() {
                rows_synced = n;
                if let Some(table_part) = rest.split("from ").nth(1) {
                    current_table = table_part.split_whitespace().next().map(String::from);
                }
                message = Some(format!("Extracting: {line}"));
            }
        }
    }

    // 模式 2: "written N rows to TABLE"
    if let Some(rest) = line.split("written ").nth(1) {
        if let Some(num_str) = rest.split_whitespace().next() {
            if let Ok(n) = num_str.replace([',', '_'], "").parse::<u64>() {
                rows_synced = n;
                if let Some(table_part) = rest.split("to ").nth(1) {
                    current_table = table_part.split_whitespace().next().map(String::from);
                }
                message = Some(format!("Writing: {line}"));
            }
        }
    }

    // 模式 3: "progress: N% (current/total)"
    if let Some(rest) = line.split("progress:").nth(1) {
        if let Some(pct_str) = rest.trim().split('%').next() {
            if let Ok(p) = pct_str.parse::<f64>() {
                percent = p;
            }
        }
        // 提取 "(current/total)" 中的 total
        if let Some(paren) = rest.split('(').nth(1) {
            if let Some(frac) = paren.split(')').next() {
                if let Some(total_str) = frac.split('/').nth(1) {
                    if let Ok(t) = total_str.trim().replace(',', "").parse::<u64>() {
                        total_rows = Some(t);
                    }
                }
            }
        }
        message = Some(line.to_string());
    }

    // 模式 4: "snapshot completed for TABLE:" or "table TABLE completed"
    if let Some(table) = line.split("completed for ").nth(1).or_else(|| {
        line.split("table ")
            .nth(1)
            .and_then(|r| r.split(" completed").next())
    }) {
        current_table = table
            .split(&[':', ','][..])
            .next()
            .map(|s| s.trim().to_string());
        message = Some(format!(
            "Table completed: {}",
            current_table.as_deref().unwrap_or("?")
        ));
    }

    // 模式 5: "total extracted: N rows, M bytes" or "total synced: N rows"
    if line.contains("total extracted") || line.contains("total synced") {
        for part in line.split(',').map(|s| s.trim()) {
            if let Some(num_str) = part.split_whitespace().next() {
                if let Ok(n) = num_str.replace(',', "").parse::<u64>() {
                    if part.contains("row") {
                        rows_synced = rows_synced.max(n);
                    } else if part.contains("byte") {
                        bytes_synced = bytes_synced.max(n);
                    }
                }
            }
        }
        message = Some(format!("Summary: {line}"));
    }

    // 模式 6: "estimated total: N rows" or "total rows: N"
    if let Some(rest) = line.split("total").nth(1) {
        if rest.contains("row") {
            for part in rest.split_whitespace() {
                if let Ok(n) = part.replace([',', ':'], "").parse::<u64>() {
                    total_rows = Some(n);
                    break;
                }
            }
        }
    }

    // 模式 7: "processing table TABLE (N/M)" or "[N/M]"
    if let Some(table) = line.split("processing table ").nth(1) {
        current_table = table.split_whitespace().next().map(String::from);
        message = Some(format!(
            "Processing: {}",
            current_table.as_deref().unwrap_or("?")
        ));
    }

    // 模式 8: 纯数字行 "N rows" 或 "N / M"
    if rows_synced == 0 {
        for part in line.split(&['/', '|'][..]) {
            let trimmed = part.trim();
            if let Ok(n) = trimmed.replace(',', "").parse::<u64>() {
                if n > 0 && line.contains("row") {
                    rows_synced = n;
                    message = Some(line.to_string());
                    break;
                }
            }
        }
    }

    // 总是发送（即使是0行），让前端知道任务还在运行
    Some(SyncProgress {
        run_id: run_id.to_string(),
        task_id: task_id.to_string(),
        status: RunStatus::Running,
        rows_synced,
        bytes_synced: 0,
        total_rows,
        current_table,
        percent,
        message: message.or_else(|| Some(line.to_string())),
    })
}

/// ape-dts 期望的 TOML 配置结构（简化版，仅全量同步）
#[derive(Debug, Serialize)]
struct ApeDtsTomlConfig {
    #[serde(rename = "global")]
    global: GlobalSection,
    extractor_basic: ExtractorBasicSection,
    extractor: ExtractorSection,
    sinker_basic: SinkerBasicSection,
    sinker: SinkerSection,
    pipeline: PipelineSection,
    parallelizer: ParallelizerSection,
    runtime: RuntimeSection,
    filter: FilterSection,
}

#[derive(Debug, Serialize)]
struct GlobalSection {
    task_id: String,
    print_interval_secs: u32,
}

#[derive(Debug, Serialize)]
struct ExtractorBasicSection {
    db_type: String,
    max_connections: u32,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExtractorSection {
    extract_type: String,
    db_type: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    tables: Vec<String>,
    batch_size: usize,
    app_name: String,
}

#[derive(Debug, Serialize)]
struct SinkerBasicSection {
    db_type: String,
    max_connections: u32,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
}

#[derive(Debug, Serialize)]
struct SinkerSection {
    sink_type: String,
    db_type: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    disable_foreign_key_checks: bool,
    batch_size: usize,
    app_name: String,
}

#[derive(Debug, Serialize)]
struct PipelineSection {
    batch_size: usize,
}

#[derive(Debug, Serialize)]
struct ParallelizerSection {
    task_type: String,
}

#[derive(Debug, Serialize)]
struct RuntimeSection {
    log_level: String,
}

#[derive(Debug, Serialize)]
struct FilterSection {
    tables: Vec<String>,
}

impl ApeDtsTomlConfig {
    fn from_submission(task: &TaskSubmission, run_id: &str) -> Self {
        let db_type = task.source.db_type.to_lowercase();
        let target_type = task.target.db_type.to_lowercase();
        let source_url = build_url(&task.source);
        let target_url = build_url(&task.target);
        let username = if task.source.username.is_empty() {
            None
        } else {
            Some(task.source.username.clone())
        };
        let password = if task.source.password.is_empty() {
            None
        } else {
            Some(task.source.password.clone())
        };
        let target_username = if task.target.username.is_empty() {
            None
        } else {
            Some(task.target.username.clone())
        };
        let target_password = if task.target.password.is_empty() {
            None
        } else {
            Some(task.target.password.clone())
        };

        let tables: Vec<String> = task
            .config
            .table_mappings
            .iter()
            .map(|m| m.source_table.clone())
            .collect();

        Self {
            global: GlobalSection {
                task_id: run_id.to_string(),
                print_interval_secs: 30,
            },
            extractor_basic: ExtractorBasicSection {
                db_type: db_type.clone(),
                max_connections: 10,
                url: source_url.clone(),
                username: username.clone(),
                password: password.clone(),
            },
            extractor: ExtractorSection {
                extract_type: "Snapshot".to_string(),
                db_type,
                url: source_url,
                username,
                password,
                tables: tables.clone(),
                batch_size: task.config.batch_size,
                app_name: "dts-platform".to_string(),
            },
            sinker_basic: SinkerBasicSection {
                db_type: target_type.clone(),
                max_connections: 10,
                url: target_url.clone(),
                username: target_username.clone(),
                password: target_password.clone(),
            },
            sinker: SinkerSection {
                sink_type: "Write".to_string(),
                db_type: target_type,
                url: target_url,
                username: target_username,
                password: target_password,
                disable_foreign_key_checks: true,
                batch_size: task.config.batch_size,
                app_name: "dts-platform".to_string(),
            },
            pipeline: PipelineSection {
                batch_size: task.config.batch_size,
            },
            parallelizer: ParallelizerSection {
                task_type: "Snapshot_Direct_Write".to_string(),
            },
            runtime: RuntimeSection {
                log_level: "info".to_string(),
            },
            filter: FilterSection { tables },
        }
    }
}

fn build_url(conn: &dts_engine::ConnectionConfig) -> String {
    match conn.db_type.to_lowercase().as_str() {
        "mysql" => format!(
            "mysql://{}:{}@{}:{}/{}",
            conn.username, conn.password, conn.host, conn.port, conn.database
        ),
        "postgresql" | "pg" | "postgres" => format!(
            "postgresql://{}:{}@{}:{}/{}",
            conn.username, conn.password, conn.host, conn.port, conn.database
        ),
        _ => format!(
            "{}://{}:{}@{}:{}/{}",
            conn.db_type, conn.username, conn.password, conn.host, conn.port, conn.database
        ),
    }
}

fn write_temp_config(config: &ApeDtsTomlConfig, run_id: &str) -> Result<String, EngineError> {
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let filename = format!("dts_task_{run_id}_{timestamp}.toml");
    let path = std::env::temp_dir().join(&filename);

    let toml_str = toml::to_string_pretty(config)
        .map_err(|e| EngineError::Internal(format!("序列化 config 失败: {e}")))?;

    std::fs::write(&path, toml_str)
        .map_err(|e| EngineError::Internal(format!("写入 config 文件失败: {e}")))?;

    Ok(path.to_string_lossy().to_string())
}
