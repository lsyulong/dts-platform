use async_trait::async_trait;
use dts_backend::{BackendError, repo::organization_members::*};
use sqlx::{Sqlite, SqlitePool, Transaction};
mod authority;
mod listing;
mod manager;
mod mutations;
mod self_membership;
mod storage;
#[cfg(test)]
mod tests;

pub struct SqliteOrganizationMembersRepo {
    pool: SqlitePool,
}
impl SqliteOrganizationMembersRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    async fn write(
        &self,
        scope: &OrganizationScope,
    ) -> Result<Transaction<'_, Sqlite>, BackendError> {
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage::error)?;
        authority::check(&mut tx, scope, true, true).await?;
        Ok(tx)
    }
    async fn read(
        &self,
        scope: &OrganizationScope,
    ) -> Result<Transaction<'_, Sqlite>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        authority::check(&mut tx, scope, false, false).await?;
        Ok(tx)
    }
}
#[async_trait]
impl OrganizationMembersRepo for SqliteOrganizationMembersRepo {
    async fn self_organization(
        &self,
        tenant_id: &str,
        actor_id: &str,
    ) -> Result<Option<String>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        let result = self_membership::current(&mut tx, tenant_id, actor_id).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn organization(
        &self,
        p: &ReadOrganizationParams,
    ) -> Result<OrganizationDetails, BackendError> {
        validation::text(&p.organization_id, 128)?;
        let mut tx = self.read(&p.scope).await?;
        let result = listing::details(&mut tx, p).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn membership(
        &self,
        p: &ReadMembershipParams,
    ) -> Result<UserOrganizationMembership, BackendError> {
        validation::text(&p.user_id, 128)?;
        let mut tx = self.read(&p.scope).await?;
        let user = storage::user(&mut tx, &p.scope.tenant_id, &p.user_id, false).await?;
        let membership = storage::membership(&mut tx, &p.scope.tenant_id, &p.user_id).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(UserOrganizationMembership { user, membership })
    }
    async fn list_members(
        &self,
        p: &OrganizationMemberFilter,
    ) -> Result<OrganizationMemberPage, BackendError> {
        validation::text(&p.organization_id, 128)?;
        validation::page(p.limit, p.offset)?;
        let search = validation::search(p.q.as_deref())?;
        let mut tx = self.read(&p.scope).await?;
        storage::organization(&mut tx, &p.scope.tenant_id, &p.organization_id).await?;
        let result = listing::members(&mut tx, p, search.as_deref()).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn transfer(
        &self,
        p: &TransferOrganizationMemberParams,
    ) -> Result<OrganizationDecisionResult, BackendError> {
        let semantics = validation::transfer(p)?;
        let mut tx = self.write(&p.scope).await?;
        let result = if let Some(result) =
            storage::replay(&mut tx, &p.scope, &p.idempotency_key, &semantics).await?
        {
            result
        } else {
            mutations::transfer(&mut tx, p, &semantics).await?
        };
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn remove(
        &self,
        p: &RemoveOrganizationMemberParams,
    ) -> Result<OrganizationDecisionResult, BackendError> {
        let semantics = validation::remove(p)?;
        let mut tx = self.write(&p.scope).await?;
        let result = if let Some(result) =
            storage::replay(&mut tx, &p.scope, &p.idempotency_key, &semantics).await?
        {
            result
        } else {
            mutations::remove(&mut tx, p, &semantics).await?
        };
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn set_manager(
        &self,
        p: &SetOrganizationManagerParams,
    ) -> Result<OrganizationDecisionResult, BackendError> {
        let semantics = validation::manager(p)?;
        let mut tx = self.write(&p.scope).await?;
        let result = if let Some(result) =
            storage::replay(&mut tx, &p.scope, &p.idempotency_key, &semantics).await?
        {
            result
        } else {
            manager::assign(&mut tx, p, &semantics).await?
        };
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn history(
        &self,
        p: &OrganizationHistoryFilter,
    ) -> Result<OrganizationHistoryPage, BackendError> {
        validation::page(p.limit, p.offset)?;
        if let Some(id) = &p.organization_id {
            validation::text(id, 128)?;
        }
        if let Some(id) = &p.user_id {
            validation::text(id, 128)?;
        }
        let mut tx = self.read(&p.scope).await?;
        let result = listing::history(&mut tx, p).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
}
