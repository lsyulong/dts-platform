use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::service_access::*};
use sqlx::PgConnection;

use super::{
    application, authority,
    storage::{self, error, invalid},
    subscription,
};

pub(super) struct CallContext {
    pub application: Option<ServiceApplication>,
    pub subscription: Option<ServiceSubscription>,
    pub rate_limit: Option<u32>,
    pub minute: DateTime<Utc>,
    pub now: DateTime<Utc>,
    pub reason: Option<&'static str>,
}

fn business_resource<T>(result: Result<T, BackendError>) -> Result<Option<T>, BackendError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(
            BackendError::Conflict(_)
            | BackendError::ConstraintViolation(_)
            | BackendError::NotFound(_),
        ) => Ok(None),
        Err(error) => Err(error),
    }
}

fn minute(now: DateTime<Utc>) -> Result<DateTime<Utc>, BackendError> {
    DateTime::<Utc>::from_timestamp(now.timestamp().div_euclid(60) * 60, 0)
        .ok_or_else(|| invalid("实际调用分钟窗口无效"))
}

fn unavailable_application(now: DateTime<Utc>) -> Result<CallContext, BackendError> {
    Ok(CallContext {
        application: None,
        subscription: None,
        rate_limit: None,
        minute: minute(now)?,
        now,
        reason: Some("application_inactive"),
    })
}

pub(super) fn origin_allowed(origins: &[String], origin: &Option<String>) -> bool {
    match origin {
        Some(value) => origins.contains(value),
        None => origins.is_empty(),
    }
}

pub(super) async fn check(
    conn: &mut PgConnection,
    p: &ReserveServiceCallParams,
    credential: &ServiceCredentialRecord,
) -> Result<CallContext, BackendError> {
    let owner = authority::get(conn, &p.tenant_id, &credential.summary.owner_id, true).await?;
    let app = business_resource(
        application::required(conn, &p.tenant_id, &credential.summary.application_id, true).await,
    )?;
    let now = storage::now(conn).await?;
    let Some(app) = app else {
        return unavailable_application(now);
    };
    let mut reason = if credential.summary.version != p.credential_version
        || credential.summary.revoked_at.is_some()
        || credential.summary.expires_at <= now
    {
        Some("credential_unavailable")
    } else if !owner.active {
        Some("owner_inactive")
    } else if app.resource.status != "active"
        || app.owner_id != credential.summary.owner_id
        || app.credential_expires_at <= now
    {
        Some("application_inactive")
    } else if !origin_allowed(&app.allowed_origins, &p.origin) {
        Some("origin_denied")
    } else {
        None
    };
    let sub_id:Option<String>=sqlx::query_scalar("SELECT subscription_id FROM service_subscription WHERE tenant_id=$1 AND application_id=$2 AND api_id=$3 AND status='approved'")
        .bind(&p.tenant_id).bind(&app.resource.id).bind(&p.api_id).fetch_optional(&mut *conn).await.map_err(error)?;
    let sub = if let Some(id) = sub_id {
        business_resource(subscription::load(conn, &p.tenant_id, &id, true).await)?.flatten()
    } else {
        None
    };
    if reason.is_none()
        && sub
            .as_ref()
            .is_none_or(|s| s.expires_at <= now || s.owner_id != app.owner_id)
    {
        reason = Some("subscription_unavailable");
    }
    let api = if let Some(sub) = &sub {
        let api = business_resource(
            subscription::api(conn, &p.tenant_id, &p.api_id, sub.api_version, true).await,
        )?;
        if api.is_none() && reason.is_none() {
            reason = Some("api_unavailable");
        }
        api
    } else {
        None
    };
    let rate = api
        .as_ref()
        .and_then(|api| api.properties["rate_limit"].as_u64())
        .filter(|v| (1..=100_000).contains(v))
        .map(|v| v as u32);
    if reason.is_none() && rate.is_none() {
        reason = Some("api_unavailable");
    }
    Ok(CallContext {
        application: Some(app),
        subscription: sub,
        rate_limit: rate,
        minute: minute(now)?,
        now,
        reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_business_resources_allow_denial_but_database_errors_propagate() {
        for error in [
            BackendError::Conflict("version".into()),
            BackendError::ConstraintViolation("proof".into()),
            BackendError::NotFound("source".into()),
        ] {
            assert!(
                business_resource::<String>(Err(error))
                    .expect("business denial")
                    .is_none()
            );
        }
        for error in [
            BackendError::Connection("offline".into()),
            BackendError::Query("database failure".into()),
            BackendError::Internal("decode failure".into()),
        ] {
            assert!(business_resource::<String>(Err(error)).is_err());
        }
        assert_eq!(
            business_resource(Ok("actual identity")).expect("resource"),
            Some("actual identity")
        );
    }

    #[test]
    fn invalid_application_has_denial_context_without_any_fabricated_permit() {
        let now = DateTime::parse_from_rfc3339("2027-03-04T05:06:59Z")
            .expect("time")
            .with_timezone(&Utc);
        let context = unavailable_application(now).expect("denial");
        assert_eq!(context.reason, Some("application_inactive"));
        assert!(context.application.is_none());
        assert!(context.subscription.is_none());
        assert!(context.rate_limit.is_none());
        assert_eq!(context.now, now);
        assert_eq!(context.minute.timestamp(), now.timestamp() - 59);
    }

    #[test]
    fn only_exact_configured_origin_or_explicit_server_to_server_is_allowed() {
        let whitelist = vec!["https://data.example.com".into()];
        assert!(origin_allowed(
            &whitelist,
            &Some("https://data.example.com".into())
        ));
        assert!(!origin_allowed(&whitelist, &None));
        assert!(!origin_allowed(
            &whitelist,
            &Some("https://data.example.com.evil".into())
        ));
        assert!(origin_allowed(&[], &None));
        assert!(!origin_allowed(
            &[],
            &Some("https://data.example.com".into())
        ));
    }
}
