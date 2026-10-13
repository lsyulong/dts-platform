use super::*;
use crate::SqliteProductResourceRepo;
use dts_backend::{CreateResourceParams, ProductResourceRepo, ResourceWrite};
use serde_json::json;

pub struct Fixture {
    pub pool: SqlitePool,
    pub repo: SqliteOrganizationMembersRepo,
}
impl Fixture {
    pub async fn new() -> Self {
        Self::with_legacy(false).await
    }
    pub async fn with_legacy(legacy: bool) -> Self {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .unwrap();
        for migration in [
            include_str!("../../../../migrations/001_initial_schema.sql"),
            include_str!("../../../../migrations/002_product_resources.sql"),
            include_str!("../../../../migrations/004_product_external_refs.sql"),
        ] {
            sqlx::raw_sql(migration).execute(&pool).await.unwrap();
        }
        sqlx::raw_sql("INSERT INTO tenant(id,name,slug) VALUES('tenant','Tenant','tenant'),('other','Other','other');
            INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('admin','tenant','admin','admin@test','hash'),('admin2','tenant','admin2','admin2@test','hash'),
            ('reader','tenant','reader','reader@test','hash'),('writer','tenant','writer','writer@test','hash'),('one','tenant','one','one@test','hash'),('two','tenant','two','two@test','hash'),('foreign','other','foreign','foreign@test','hash');
            INSERT INTO role(id,tenant_id,name) VALUES('manager-role','tenant','manager'),('reader-role','tenant','reader'),('writer-role','tenant','writer'),('member-role','tenant','member');
            INSERT INTO user_role(user_id,role_id) VALUES('admin','manager-role'),('admin2','manager-role'),('reader','reader-role'),('writer','writer-role'),('one','member-role');
            INSERT INTO role_permission(role_id,permission) VALUES('manager-role','system:write'),('manager-role','user:manage'),('manager-role','system:read'),('reader-role','system:read'),('writer-role','system:write');")
            .execute(&pool).await.unwrap();
        let f = Self {
            repo: SqliteOrganizationMembersRepo::new(pool.clone()),
            pool,
        };
        for (id, parent, tenant) in [
            ("a", None, "tenant"),
            ("b", None, "tenant"),
            ("child", Some("a"), "tenant"),
            ("foreign-org", None, "other"),
        ] {
            SqliteProductResourceRepo::new(f.pool.clone())
                .create(&CreateResourceParams {
                    id: id.into(),
                    tenant_id: tenant.into(),
                    kind: "organization".into(),
                    actor_id: if tenant == "tenant" {
                        "admin"
                    } else {
                        "foreign"
                    }
                    .into(),
                    fields: ResourceWrite {
                        code: id.into(),
                        name: id.into(),
                        description: None,
                        parent_id: parent.map(str::to_owned),
                        status: "draft".into(),
                        properties: json!({"sort_order":0}),
                        references: vec![],
                    },
                })
                .await
                .unwrap();
        }
        if legacy {
            sqlx::query("UPDATE product_resource SET properties=?1 WHERE id='a'")
                .bind(
                    json!({"member_ids":["one"],"manager_user_id":"one","sort_order":0})
                        .to_string(),
                )
                .execute(&f.pool)
                .await
                .unwrap();
        }
        sqlx::raw_sql(include_str!(
            "../../../../migrations/020_organization_members.sql"
        ))
        .execute(&f.pool)
        .await
        .unwrap();
        f
    }
    pub fn scope(&self, actor: &str) -> OrganizationScope {
        OrganizationScope {
            tenant_id: "tenant".into(),
            actor_id: actor.into(),
        }
    }
    pub fn transfer(
        &self,
        user: &str,
        version: i64,
        target: &str,
    ) -> TransferOrganizationMemberParams {
        TransferOrganizationMemberParams {
            scope: self.scope("admin"),
            user_id: user.into(),
            expected_membership_version: version,
            target_organization_id: target.into(),
            expected_target_version: 1,
            reason: "岗位调动".into(),
            idempotency_key: uuid::Uuid::new_v4().to_string(),
            manager_handover: None,
        }
    }
    pub fn remove(&self, user: &str, version: i64) -> RemoveOrganizationMemberParams {
        RemoveOrganizationMemberParams {
            scope: self.scope("admin"),
            user_id: user.into(),
            expected_membership_version: version,
            reason: "归属清理".into(),
            idempotency_key: uuid::Uuid::new_v4().to_string(),
            manager_handover: None,
        }
    }
    pub fn manager(
        &self,
        organization: &str,
        version: i64,
        user: Option<&str>,
    ) -> SetOrganizationManagerParams {
        SetOrganizationManagerParams {
            scope: self.scope("admin"),
            organization_id: organization.into(),
            expected_organization_version: version,
            manager_user_id: user.map(str::to_owned),
            reason: "负责人任免".into(),
            idempotency_key: uuid::Uuid::new_v4().to_string(),
        }
    }
    pub async fn details(&self, id: &str) -> OrganizationDetails {
        self.repo
            .organization(&ReadOrganizationParams {
                scope: self.scope("reader"),
                organization_id: id.into(),
            })
            .await
            .unwrap()
    }
    pub async fn membership(&self, user: &str) -> UserOrganizationMembership {
        self.repo
            .membership(&ReadMembershipParams {
                scope: self.scope("reader"),
                user_id: user.into(),
            })
            .await
            .unwrap()
    }
    pub async fn history(&self) -> OrganizationHistoryPage {
        self.repo
            .history(&OrganizationHistoryFilter {
                scope: self.scope("reader"),
                organization_id: None,
                user_id: None,
                limit: 100,
                offset: 0,
            })
            .await
            .unwrap()
    }
}
