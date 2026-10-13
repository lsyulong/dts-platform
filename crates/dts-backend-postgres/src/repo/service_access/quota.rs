use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::service_access::ReserveServiceCallParams};
use sqlx::PgConnection;

use super::storage::error;

pub(super) fn retry_after_secs(now: DateTime<Utc>) -> u32 {
    60 - now.timestamp().rem_euclid(60) as u32
}

pub(super) async fn reserve(
    conn: &mut PgConnection,
    p: &ReserveServiceCallParams,
    minute: DateTime<Utc>,
    rate: u32,
) -> Result<bool, BackendError> {
    let api_count:Option<i64>=sqlx::query_scalar("SELECT reserved_count FROM service_api_minute_quota WHERE tenant_id=$1 AND api_id=$2 AND minute_bucket=$3 FOR UPDATE")
        .bind(&p.tenant_id).bind(&p.api_id).bind(minute).fetch_optional(&mut *conn).await.map_err(error)?;
    let credential_count: Option<i64> = sqlx::query_scalar(
        "SELECT reserved_count FROM service_credential_minute_quota
        WHERE tenant_id=$1 AND credential_id=$2 AND api_id=$3 AND minute_bucket=$4 FOR UPDATE",
    )
    .bind(&p.tenant_id)
    .bind(&p.credential_id)
    .bind(&p.api_id)
    .bind(minute)
    .fetch_optional(&mut *conn)
    .await
    .map_err(error)?;
    if api_count.unwrap_or(0) >= i64::from(rate) || credential_count.unwrap_or(0) >= i64::from(rate)
    {
        return Ok(false);
    }
    sqlx::query("SAVEPOINT service_quota_reservation")
        .execute(&mut *conn)
        .await
        .map_err(error)?;
    let api=sqlx::query("INSERT INTO service_api_minute_quota(tenant_id,api_id,minute_bucket,reserved_count) VALUES($1,$2,$3,1)
        ON CONFLICT(tenant_id,api_id,minute_bucket) DO UPDATE SET reserved_count=service_api_minute_quota.reserved_count+1
        WHERE service_api_minute_quota.reserved_count<$4")
        .bind(&p.tenant_id).bind(&p.api_id).bind(minute).bind(i64::from(rate)).execute(&mut *conn).await.map_err(error)?.rows_affected();
    let credential = if api == 1 {
        sqlx::query("INSERT INTO service_credential_minute_quota(tenant_id,credential_id,api_id,minute_bucket,reserved_count) VALUES($1,$2,$3,$4,1)
            ON CONFLICT(tenant_id,credential_id,api_id,minute_bucket) DO UPDATE SET reserved_count=service_credential_minute_quota.reserved_count+1
            WHERE service_credential_minute_quota.reserved_count<$5")
            .bind(&p.tenant_id).bind(&p.credential_id).bind(&p.api_id).bind(minute).bind(i64::from(rate)).execute(&mut *conn).await.map_err(error)?.rows_affected()
    } else {
        0
    };
    if api != 1 || credential != 1 {
        sqlx::query("ROLLBACK TO SAVEPOINT service_quota_reservation")
            .execute(&mut *conn)
            .await
            .map_err(error)?;
    }
    sqlx::query("RELEASE SAVEPOINT service_quota_reservation")
        .execute(conn)
        .await
        .map_err(error)?;
    Ok(api == 1 && credential == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_uses_the_reserved_database_minute_and_rounds_up() {
        for (time, delay) in [
            ("2027-01-02T03:04:00Z", 60),
            ("2027-01-02T03:04:00.000001Z", 60),
            ("2027-01-02T03:04:01.999999Z", 59),
            ("2027-01-02T03:04:59.999999Z", 1),
        ] {
            let now = DateTime::parse_from_rfc3339(time)
                .expect("DB time")
                .with_timezone(&Utc);
            assert_eq!(retry_after_secs(now), delay);
        }
    }
}
