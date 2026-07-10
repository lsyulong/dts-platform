use dts_engine::{EngineError, RunStatus, SyncProgress, TaskSubmission};

/// 通过 ape-dts 库执行同步任务。
///
/// 当前为骨架实现：发送一条"启动"进度消息后立即返回。
/// TODO: 替换为真实的 `dt-task::run()` 调用。
///
/// ```ignore
/// // 未来的实现：
/// let config = build_ape_config(&task);
/// dt_task::run(config, |progress| {
///     let _ = progress_tx.send(convert_progress(progress));
/// }).await?;
/// ```
pub(crate) async fn run_sync(
    task: TaskSubmission,
    run_id: &str,
    progress_tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
) -> Result<(), EngineError> {
    // 发送启动进度
    let _ = progress_tx.send(SyncProgress {
        run_id: run_id.into(),
        task_id: task.task_id.clone(),
        status: RunStatus::Running,
        rows_synced: 0,
        bytes_synced: 0,
        total_rows: None,
        current_table: task
            .config
            .table_mappings
            .first()
            .map(|m| m.source_table.clone()),
        percent: 0.0,
        message: Some("Starting sync with ape-dts engine...".into()),
    });

    // TODO: 调用真实的 ape-dts 库
    // dt_task::run(config, progress_callback).await?;

    // 发送完成进度（骨架实现直接标记完成）
    let _ = progress_tx.send(SyncProgress {
        run_id: run_id.into(),
        task_id: task.task_id,
        status: RunStatus::Completed,
        rows_synced: 0,
        bytes_synced: 0,
        total_rows: None,
        current_table: None,
        percent: 100.0,
        message: Some("Sync completed (skeleton — no actual data transfer)".into()),
    });

    Ok(())
}
