use super::*;

impl QualityAlertsService {
    pub(super) async fn evaluation_view(
        &self,
        actor: &AuthIdentity,
        result: QualityEvaluation,
    ) -> Result<QualityEvaluationView, AppError> {
        Ok(QualityEvaluationView {
            observation: self.observation_view(actor, result.observation).await?,
            alert: result.alert,
            replayed: result.replayed,
        })
    }
    pub(super) async fn observation_view(
        &self,
        actor: &AuthIdentity,
        observation: QualityObservation,
    ) -> Result<QualityObservationView, AppError> {
        let tokens = self.authorize_observation(actor, &observation).await?;
        self.finish_read(actor, &tokens).await?;
        let mut summary = observation.summary;
        // JSON consumers must preserve exact u64 counts beyond JS safe integers.
        for key in [
            "completed_runs",
            "failed_runs",
            "incomplete_runs",
            "successful_checks",
            "empty_checks",
            "execution_error_checks",
            "sampled_checks",
            "checked_rows",
            "failed_rows",
        ] {
            if let Some(value) = summary[key]
                .as_u64()
                .filter(|value| *value > 9_007_199_254_740_991)
            {
                summary[key] = Value::String(value.to_string());
            }
        }
        Ok(QualityObservationView {
            id: observation.id,
            monitor_id: observation.monitor_id,
            monitor_version: observation.monitor_version,
            object_id: observation.object_id,
            actor_id: observation.actor_id,
            period_start: observation.period_start,
            period_end: observation.period_end,
            state: observation.state,
            reason: observation.reason,
            summary,
            created_at: observation.created_at,
            result_access: "allowed".into(),
            dependencies: observation
                .snapshot
                .resources
                .into_iter()
                .map(|proof| QualityDependencyView {
                    kind: proof.kind,
                    id: proof.id,
                    version: proof.version,
                })
                .collect(),
            run_ids: observation
                .snapshot
                .jobs
                .into_iter()
                .map(|job| job.id)
                .collect(),
        })
    }
    pub(super) async fn alert_view(
        &self,
        actor: &AuthIdentity,
        alert: QualityAlert,
    ) -> Result<QualityAlertView, AppError> {
        let observation = self
            .alerts
            .get_observation(&actor.tenant_id, &alert.last_observation_id)
            .await?
            .ok_or_else(|| AppError::Internal("告警缺少真实观察记录".into()))?;
        Ok(QualityAlertView {
            alert,
            observation: self.observation_view(actor, observation).await?,
        })
    }
}
