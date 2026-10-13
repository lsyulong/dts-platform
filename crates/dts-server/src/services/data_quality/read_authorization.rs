use super::*;
use crate::services::data_access::{AccessOutcome, AuthorizedQuery, QueryAuthorizationService};

impl QualityService {
    pub fn with_query_authorization(mut self, guard: Arc<QueryAuthorizationService>) -> Self {
        self.read_guard = Some(guard);
        self
    }

    pub(super) async fn authorize_results<'a>(
        &self,
        actor: &AuthIdentity,
        jobs: impl Iterator<Item = &'a ProductJob>,
    ) -> Result<Vec<AuthorizedQuery>, AppError> {
        let guard = self
            .read_guard
            .as_ref()
            .ok_or_else(|| AppError::Forbidden("质量结果未配置实际数据授权出口".into()))?;
        let mut tokens = Vec::new();
        for job in jobs {
            if !job
                .nodes
                .iter()
                .any(|node| node.status == ProductJobNodeStatus::Succeeded && node.result.is_some())
            {
                continue;
            }
            let compiled = prepare_claimed(job)?;
            for node in &job.nodes {
                if node.status != ProductJobNodeStatus::Succeeded || node.result.is_none() {
                    continue;
                }
                let sql = compiled
                    .iter()
                    .find(|(id, _)| id == &node.id)
                    .map(|(_, sql)| sql.as_str())
                    .ok_or_else(|| AppError::Internal("质量结果无法对应冻结查询".into()))?;
                let token = guard
                    .authorize_quality_result(actor, job, &node.id, sql)
                    .await?;
                guard.reauthorize(actor, &token).await?;
                tokens.push(token);
            }
        }
        Ok(tokens)
    }

    pub(super) async fn finish_result_read(
        &self,
        actor: &AuthIdentity,
        tokens: &[AuthorizedQuery],
    ) -> Result<(), AppError> {
        let guard = self
            .read_guard
            .as_ref()
            .ok_or_else(|| AppError::Forbidden("质量结果未配置授权出口".into()))?;
        for token in tokens {
            if let Err(error) = guard.reauthorize(actor, token).await {
                guard
                    .record_access(actor, token, AccessOutcome::Denied)
                    .await?;
                return Err(error);
            }
            guard
                .record_access(actor, token, AccessOutcome::Succeeded)
                .await?;
        }
        Ok(())
    }
}
