use super::*;
use serde_json::{Value, json};

fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
pub fn text(value: &str, maximum: usize) -> Result<(), BackendError> {
    if value.trim().is_empty()
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(invalid("组织标识或原因为空、过长或包含控制字符"));
    }
    Ok(())
}
pub fn scope(scope: &OrganizationScope) -> Result<(), BackendError> {
    text(&scope.tenant_id, 128)?;
    text(&scope.actor_id, 128)
}
pub fn page(limit: u32, offset: u32) -> Result<(), BackendError> {
    if !(1..=100).contains(&limit) || offset > 1_000_000 {
        return Err(invalid("组织分页无效"));
    }
    Ok(())
}
fn key(key: &str) -> Result<(), BackendError> {
    if uuid::Uuid::parse_str(key).is_err() {
        return Err(invalid("组织操作需要稳定UUID幂等键"));
    }
    Ok(())
}
fn handover(value: &Option<ManagerHandover>) -> Result<(), BackendError> {
    if let Some(h) = value {
        if h.expected_organization_version <= 0 {
            return Err(invalid("继任组织版本无效"));
        }
        if let Some(id) = &h.successor_user_id {
            text(id, 128)?;
        }
    }
    Ok(())
}
pub fn transfer(p: &TransferOrganizationMemberParams) -> Result<Value, BackendError> {
    scope(&p.scope)?;
    text(&p.user_id, 128)?;
    text(&p.target_organization_id, 128)?;
    text(&p.reason, 8192)?;
    key(&p.idempotency_key)?;
    handover(&p.manager_handover)?;
    if p.expected_membership_version < 0 || p.expected_target_version <= 0 {
        return Err(invalid("成员或目标组织版本无效"));
    }
    Ok(
        json!({"action":"transfer","actor_id":p.scope.actor_id,"user_id":p.user_id,"expected_membership_version":p.expected_membership_version,
        "target_organization_id":p.target_organization_id,"expected_target_version":p.expected_target_version,"reason":p.reason,"manager_handover":p.manager_handover}),
    )
}
pub fn remove(p: &RemoveOrganizationMemberParams) -> Result<Value, BackendError> {
    scope(&p.scope)?;
    text(&p.user_id, 128)?;
    text(&p.reason, 8192)?;
    key(&p.idempotency_key)?;
    handover(&p.manager_handover)?;
    if p.expected_membership_version <= 0 {
        return Err(invalid("移除成员版本无效"));
    }
    Ok(
        json!({"action":"remove","actor_id":p.scope.actor_id,"user_id":p.user_id,"expected_membership_version":p.expected_membership_version,"reason":p.reason,"manager_handover":p.manager_handover}),
    )
}
pub fn manager(p: &SetOrganizationManagerParams) -> Result<Value, BackendError> {
    scope(&p.scope)?;
    text(&p.organization_id, 128)?;
    text(&p.reason, 8192)?;
    key(&p.idempotency_key)?;
    if let Some(id) = &p.manager_user_id {
        text(id, 128)?;
    }
    if p.expected_organization_version <= 0 {
        return Err(invalid("负责人组织版本无效"));
    }
    Ok(
        json!({"action":"set_manager","actor_id":p.scope.actor_id,"organization_id":p.organization_id,"expected_organization_version":p.expected_organization_version,"manager_user_id":p.manager_user_id,"reason":p.reason}),
    )
}
pub fn search(q: Option<&str>) -> Result<Option<String>, BackendError> {
    let Some(q) = q.map(str::trim).filter(|q| !q.is_empty()) else {
        return Ok(None);
    };
    text(q, 128)?;
    Ok(Some(format!(
        "%{}%",
        q.to_lowercase()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_and_literal_search() {
        assert!(text("\nreason", 8192).is_err());
        assert!(text(" ", 8192).is_err());
        assert_eq!(search(Some("a_%\\")).unwrap(), Some("%a\\_\\%\\\\%".into()));
        assert!(page(0, 0).is_err());
        assert!(page(101, 0).is_err());
    }
    #[test]
    fn semantic_contains_actor_cas_and_explicit_resignation() {
        let p = TransferOrganizationMemberParams {
            scope: OrganizationScope {
                tenant_id: "t".into(),
                actor_id: "actor".into(),
            },
            user_id: "u".into(),
            expected_membership_version: 2,
            target_organization_id: "o".into(),
            expected_target_version: 3,
            reason: "move".into(),
            idempotency_key: uuid::Uuid::new_v4().to_string(),
            manager_handover: Some(ManagerHandover {
                expected_organization_version: 5,
                successor_user_id: None,
            }),
        };
        let value = transfer(&p).unwrap();
        assert_eq!(value["actor_id"], "actor");
        assert_eq!(value["manager_handover"]["successor_user_id"], Value::Null);
        assert_eq!(value["expected_membership_version"], 2);
    }
}
