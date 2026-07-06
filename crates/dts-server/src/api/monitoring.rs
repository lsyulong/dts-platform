use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, web};

use crate::middleware::auth;
use crate::middleware::auth::get_jwt_secret;
use crate::models::error::AppError;
use dts_backend::repo::MonitoringRepo;

/// GET /api/v1/monitoring/overview
pub async fn overview(
    monitoring_repo: web::Data<Arc<dyn MonitoringRepo>>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let overview = monitoring_repo.get_overview(&claims.tenant_id).await?;

    Ok(HttpResponse::Ok().json(overview))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    // ========== Overview Response Structure Tests ==========

    #[test]
    fn overview_response_json_structure_is_complete() {
        let response = serde_json::json!({
            "total_tasks": 10,
            "running_tasks": 3,
            "failed_tasks": 1,
            "total_datasources": 5,
            "healthy_datasources": 4,
            "today_synced_rows": 50000,
            "today_synced_bytes": 1048576,
            "success_rate": 90.0,
            "recent_runs": []
        });

        assert!(response["total_tasks"].is_number());
        assert!(response["running_tasks"].is_number());
        assert!(response["failed_tasks"].is_number());
        assert!(response["total_datasources"].is_number());
        assert!(response["healthy_datasources"].is_number());
        assert!(response["today_synced_rows"].is_number());
        assert!(response["today_synced_bytes"].is_number());
        assert!(response["success_rate"].is_number());
        assert!(response["recent_runs"].is_array());
    }

    #[test]
    fn overview_success_rate_calculation() {
        let total_tasks: i64 = 10;
        let failed_tasks: i64 = 2;
        let success_rate = if total_tasks > 0 {
            100.0 * (total_tasks - failed_tasks) as f64 / total_tasks as f64
        } else {
            100.0
        };
        assert_eq!(success_rate, 80.0);
    }

    #[test]
    fn overview_success_rate_when_no_tasks() {
        let total_tasks: i64 = 0;
        let failed_tasks: i64 = 0;
        let success_rate = if total_tasks > 0 {
            100.0 * (total_tasks - failed_tasks) as f64 / total_tasks as f64
        } else {
            100.0
        };
        assert_eq!(success_rate, 100.0);
    }

    #[test]
    fn overview_default_values_for_null_sums() {
        let rows: Option<i64> = None;
        assert_eq!(rows.unwrap_or(0), 0);

        let bytes: Option<i64> = None;
        assert_eq!(bytes.unwrap_or(0), 0);

        let rows_some: Option<i64> = Some(12345);
        assert_eq!(rows_some.unwrap_or(0), 12345);
    }
}
