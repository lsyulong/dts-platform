use super::*;
use crate::SqliteProductResourceRepo;
use dts_backend::{
    CreateResourceParams, ProductResource, ProductResourceRepo, ResourceReference, ResourceWrite,
};
use serde_json::json;
use std::sync::Arc;

pub struct Fixture {
    pub pool: SqlitePool,
    pub repo: SqliteNavigationRepo,
    pub registry: Arc<NavigationRegistry>,
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
        for migration in [
            include_str!("../../../../migrations/001_initial_schema.sql"),
            include_str!("../../../../migrations/002_product_resources.sql"),
            include_str!("../../../../migrations/004_product_external_refs.sql"),
            include_str!("../../../../migrations/019_tenant_navigation.sql"),
        ] {
            sqlx::raw_sql(migration).execute(&pool).await.unwrap();
        }
        sqlx::raw_sql("INSERT INTO tenant(id,name,slug) VALUES('tenant','Tenant','tenant'),('other','Other','other');
            INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('manager','tenant','manager','manager@test','hash'),
            ('reader','tenant','reader','reader@test','hash'),('writer','tenant','writer','writer@test','hash'),('other-user','other','other','other@test','hash');
            INSERT INTO role(id,tenant_id,name) VALUES('manage','tenant','manager'),('read','tenant','reader'),('write','tenant','writer');
            INSERT INTO user_role(user_id,role_id) VALUES('manager','manage'),('reader','read'),('writer','write');
            INSERT INTO role_permission(role_id,permission) VALUES('manage','system:write'),('manage','user:manage'),('manage','system:read'),
            ('manage','development:read'),('read','development:read'),('write','system:write');").execute(&pool).await.unwrap();
        let registry = Arc::new(
            NavigationRegistry::new(vec![
                RegisteredNavigationRoute {
                    id: "sql".into(),
                    path: "/development/sql".into(),
                    name: "SQL".into(),
                    required_permissions: vec!["development:read".into()],
                    sort_order: 10,
                },
                RegisteredNavigationRoute {
                    id: "menus".into(),
                    path: "/system/menus".into(),
                    name: "Menus".into(),
                    required_permissions: vec!["system:read".into()],
                    sort_order: 90,
                },
                RegisteredNavigationRoute {
                    id: "templates".into(),
                    path: "/system/menu-templates".into(),
                    name: "Templates".into(),
                    required_permissions: vec!["system:read".into()],
                    sort_order: 91,
                },
                RegisteredNavigationRoute {
                    id: "users".into(),
                    path: "/system/users".into(),
                    name: "Users".into(),
                    required_permissions: vec!["user:manage".into()],
                    sort_order: 92,
                },
                RegisteredNavigationRoute {
                    id: "profile".into(),
                    path: "/account/profile".into(),
                    name: "Profile".into(),
                    required_permissions: vec![],
                    sort_order: 99,
                },
            ])
            .unwrap(),
        );
        let f = Self {
            repo: SqliteNavigationRepo::new(pool.clone()),
            pool,
            registry,
        };
        f.menu("sql-menu", None, "/development/sql").await;
        f.template("template", &["sql-menu"], None).await;
        f
    }
    pub fn scope(&self, actor: &str) -> NavigationScope {
        NavigationScope {
            tenant_id: "tenant".into(),
            actor_id: actor.into(),
        }
    }
    pub fn read(&self, actor: &str) -> ReadNavigationParams {
        ReadNavigationParams {
            scope: self.scope(actor),
            registry: self.registry.clone(),
        }
    }
    pub fn assign(&self, version: i64, key: &str) -> AssignNavigationParams {
        AssignNavigationParams {
            scope: self.scope("manager"),
            template_id: "template".into(),
            expected_template_version: 1,
            expected_assignment_version: version,
            idempotency_key: key.into(),
            expires_at: None,
            registry: self.registry.clone(),
        }
    }
    pub async fn menu(&self, id: &str, parent: Option<&str>, path: &str) -> ProductResource {
        SqliteProductResourceRepo::new(self.pool.clone())
            .create(&CreateResourceParams {
                id: id.into(),
                tenant_id: "tenant".into(),
                kind: "system_menu".into(),
                actor_id: "manager".into(),
                fields: ResourceWrite {
                    code: id.into(),
                    name: id.into(),
                    description: None,
                    parent_id: parent.map(str::to_owned),
                    status: "draft".into(),
                    properties: json!({"path":path,"visible":true,"sort_order":10}),
                    references: vec![],
                },
            })
            .await
            .unwrap()
    }
    pub async fn template(&self, id: &str, ids: &[&str], days: Option<u32>) -> ProductResource {
        SqliteProductResourceRepo::new(self.pool.clone())
            .create(&CreateResourceParams {
                id: id.into(),
                tenant_id: "tenant".into(),
                kind: "menu_template".into(),
                actor_id: "manager".into(),
                fields: ResourceWrite {
                    code: id.into(),
                    name: id.into(),
                    description: None,
                    parent_id: None,
                    status: "draft".into(),
                    properties: json!({"menu_ids":ids,"validity_days":days}),
                    references: ids
                        .iter()
                        .map(|id| ResourceReference {
                            field: "menu_ids".into(),
                            target_kind: "system_menu".into(),
                            target_id: (*id).into(),
                        })
                        .collect(),
                },
            })
            .await
            .unwrap()
    }
}
