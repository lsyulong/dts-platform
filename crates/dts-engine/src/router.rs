// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{EngineError, EngineService, RunStatus, SyncProgress, TaskSubmission};

/// Selects an engine adapter for each submission and remembers the owner of an
/// active run so cancellation reaches the same adapter.
pub struct EngineRouter {
    engines: Vec<Arc<dyn EngineService>>,
    active: Arc<Mutex<HashMap<String, Arc<dyn EngineService>>>>,
}

impl EngineRouter {
    pub fn new(engines: Vec<Arc<dyn EngineService>>) -> Result<Self, EngineError> {
        if engines.is_empty() {
            return Err(EngineError::InvalidConfig(
                "至少需要注册一个同步引擎".into(),
            ));
        }
        Ok(Self {
            engines,
            active: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn owner(&self, task: &TaskSubmission) -> Result<Arc<dyn EngineService>, EngineError> {
        self.engines
            .iter()
            .find(|engine| engine.supports(task))
            .cloned()
            .ok_or_else(|| {
                EngineError::InvalidConfig(format!(
                    "没有可执行 {} 到 {} 的同步引擎",
                    task.source.db_type, task.target.db_type
                ))
            })
    }
}

#[async_trait::async_trait]
impl EngineService for EngineRouter {
    fn supports(&self, task: &TaskSubmission) -> bool {
        self.engines.iter().any(|engine| engine.supports(task))
    }

    async fn submit(
        &self,
        task: TaskSubmission,
        progress_tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
    ) -> Result<String, EngineError> {
        let engine = self.owner(&task)?;
        let run_id = task.run_id.clone();
        let (forward_tx, mut forward_rx) = tokio::sync::mpsc::unbounded_channel();
        {
            let mut active = self
                .active
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if active.insert(run_id.clone(), engine.clone()).is_some() {
                return Err(EngineError::InvalidConfig("PipelineRun ID 已在执行".into()));
            }
        }
        if let Err(error) = engine.submit(task, forward_tx).await {
            self.active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&run_id);
            return Err(error);
        }
        let active = self.active.clone();
        let forwarded_run = run_id.clone();
        tokio::spawn(async move {
            while let Some(progress) = forward_rx.recv().await {
                let terminal = progress.status != RunStatus::Running;
                if progress_tx.send(progress).is_err() || terminal {
                    break;
                }
            }
            active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&forwarded_run);
        });
        Ok(run_id)
    }

    async fn cancel(&self, run_id: &str) -> Result<(), EngineError> {
        let engine = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(run_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(run_id.into()))?;
        engine.cancel(run_id).await
    }
}
