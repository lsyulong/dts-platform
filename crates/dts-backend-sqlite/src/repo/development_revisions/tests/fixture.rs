use super::*;
pub struct Fixture {
    pub pool: SqlitePool,
    pub repo: SqliteDevelopmentRevisionRepo,
}
impl Fixture {
    pub async fn new() -> Self {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .unwrap();
        for sql in [
            include_str!("../../../../migrations/001_initial_schema.sql"),
            include_str!("../../../../migrations/002_product_resources.sql"),
            include_str!("../../../../migrations/004_product_external_refs.sql"),
            include_str!("../../../../migrations/023_development_revisions.sql"),
        ] {
            sqlx::raw_sql(sql).execute(&pool).await.unwrap();
        }
        sqlx::raw_sql("INSERT INTO tenant(id,name,slug) VALUES('tenant','Tenant','tenant'),('other','Other','other');
            INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('admin','tenant','admin','admin@test','hash'),('admin2','tenant','admin2','admin2@test','hash'),('reader','tenant','reader','reader@test','hash');
            INSERT INTO role(id,tenant_id,name) VALUES('writers','tenant','writers'),('readers','tenant','readers');
            INSERT INTO user_role(user_id,role_id) VALUES('admin','writers'),('admin2','writers'),('reader','readers');
            INSERT INTO role_permission(role_id,permission) VALUES('writers','development:read'),('writers','development:write'),('readers','development:read');")
            .execute(&pool).await.unwrap();
        let f = Self {
            repo: SqliteDevelopmentRevisionRepo::new(pool.clone()),
            pool,
        };
        for (id, kind, properties) in [
            (
                "document",
                "sql_document",
                json!({"sql":"SELECT 1 AS value","parameters":[]}),
            ),
            (
                "workflow",
                "development_workflow",
                json!({"definition":{"nodes":[{"id":"query","name":"Query","sql":"SELECT 1","depends_on":[]}]},"cron_expression":"0 0 * * * *","timezone":"UTC"}),
            ),
        ] {
            SqliteProductResourceRepo::new(f.pool.clone())
                .create(&CreateResourceParams {
                    id: id.into(),
                    tenant_id: "tenant".into(),
                    kind: kind.into(),
                    actor_id: "admin".into(),
                    fields: ResourceWrite {
                        code: id.into(),
                        name: id.into(),
                        description: None,
                        parent_id: None,
                        status: "draft".into(),
                        properties,
                        references: vec![],
                    },
                })
                .await
                .unwrap();
        }
        f
    }
    pub fn scope(&self, id: &str) -> DevelopmentRevisionScope {
        DevelopmentRevisionScope {
            tenant_id: "tenant".into(),
            actor_id: "admin".into(),
            kind: if id == "workflow" {
                "development_workflow"
            } else {
                "sql_document"
            }
            .into(),
            resource_id: id.into(),
        }
    }
    pub async fn save(&self, id: &str, version: i64) -> SaveDevelopmentRevisionParams {
        let current = self.repo.current(&self.scope(id)).await.unwrap();
        SaveDevelopmentRevisionParams {
            scope: self.scope(id),
            expected_version: version,
            reason: "记录当前定义".into(),
            idempotency_key: uuid::Uuid::new_v4().to_string(),
            validation: Some(
                ValidatedDevelopmentDefinition::from_validated_snapshot(&current).unwrap(),
            ),
        }
    }
    pub async fn detail(&self, id: &str, revision: &str) -> DevelopmentRevision {
        self.repo
            .detail(&ReadDevelopmentRevisionParams {
                scope: self.scope(id),
                revision_id: revision.into(),
            })
            .await
            .unwrap()
    }
    pub async fn restore(
        &self,
        id: &str,
        version: i64,
        revision: &str,
    ) -> RestoreDevelopmentRevisionParams {
        let detail = self.detail(id, revision).await;
        RestoreDevelopmentRevisionParams {
            scope: self.scope(id),
            revision_id: revision.into(),
            expected_version: version,
            reason: "恢复经过确认的历史定义".into(),
            idempotency_key: uuid::Uuid::new_v4().to_string(),
            validation: ValidatedDevelopmentDefinition::from_validated_snapshot(&detail.snapshot)
                .unwrap(),
        }
    }
    pub async fn edit(&self, id: &str, version: i64, properties: serde_json::Value) {
        let mut current = self.repo.current(&self.scope(id)).await.unwrap();
        current.properties = properties;
        current.references = vec![];
        SqliteProductResourceRepo::new(self.pool.clone())
            .update(
                "tenant",
                &self.scope(id).kind,
                id,
                &UpdateResourceParams {
                    expected_version: version,
                    actor_id: "admin".into(),
                    fields: fields(&current),
                },
            )
            .await
            .unwrap();
    }
    pub async fn status(&self, id: &str, status: &str) {
        sqlx::query("UPDATE product_resource SET status=?1 WHERE tenant_id='tenant' AND id=?2")
            .bind(status)
            .bind(id)
            .execute(&self.pool)
            .await
            .unwrap();
    }
}
pub fn fields(r: &dts_backend::ProductResource) -> ResourceWrite {
    ResourceWrite {
        code: r.code.clone(),
        name: r.name.clone(),
        description: r.description.clone(),
        parent_id: r.parent_id.clone(),
        status: r.status.clone(),
        properties: r.properties.clone(),
        references: r.references.clone(),
    }
}
