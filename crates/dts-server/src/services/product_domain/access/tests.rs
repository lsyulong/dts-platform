#![allow(clippy::unwrap_used)]

use super::*;

fn portal() -> Value {
    let module = catalog::modules().unwrap().first().unwrap();
    json!({"authorized_role_ids":["role-1"],"layout":{"columns":12,"widgets":[
        {"id":"widget-1","type":"shortcut","title":"快捷入口","module_id":module.id,"width":6,"height":2}
    ]},"theme":{"title":"门户","primary_color":"#0D9488","background_color":"#F4F7FA"}})
}

fn template() -> Value {
    json!({"channel":"in_app","subject":"Hello {{ name }}","body":"Count={{count}}, enabled={{enabled}}","variables":["name","count","enabled"]})
}

#[test]
fn portal_uses_real_modules_strict_layout_and_safe_colors() {
    assert!(validate("portal", &portal()).is_ok());
    let mut unknown = portal();
    unknown["layout"]["widgets"][0]["module_id"] = json!("imaginary");
    assert!(validate("portal", &unknown).is_err());
    let mut extra = portal();
    extra["layout"]["widgets"][0]["html"] = json!("<script>alert(1)</script>");
    assert!(validate("portal", &extra).is_err());
    let mut color = portal();
    color["theme"]["primary_color"] = json!("url(javascript:alert(1))");
    assert!(validate("portal", &color).is_err());
    let mut dimension = portal();
    dimension["layout"]["widgets"][0]["width"] = json!(6.5);
    assert!(validate("portal", &dimension).is_err());
    let mut no_roles = portal();
    no_roles["authorized_role_ids"] = json!([]);
    assert!(validate("portal", &no_roles).is_err());
}

#[test]
fn portal_duplicate_components_and_fake_preview_data_are_not_accepted() {
    let mut definition = portal();
    let duplicate = definition["layout"]["widgets"][0].clone();
    definition["layout"]["widgets"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(validate("portal", &definition).is_err());
    let result = action("portal", &portal(), "preview", &json!({}))
        .unwrap()
        .unwrap();
    assert_eq!(result["data_loaded"], false);
    assert!(
        action("portal", &portal(), "publish", &json!({}))
            .unwrap()
            .is_none()
    );
}

#[test]
fn menu_validates_real_routes_and_registered_permissions() {
    let path = &catalog::modules().unwrap().first().unwrap().path;
    assert!(
        validate(
            "system_menu",
            &json!({"path":path,"permission":PERMISSIONS[0],"visible":true})
        )
        .is_ok()
    );
    for path in [
        "https://evil.example",
        "//evil.example",
        "/does-not-exist",
        "/system/users?admin=true",
    ] {
        assert!(validate("system_menu", &json!({"path":path})).is_err());
    }
    assert!(
        validate(
            "system_menu",
            &json!({"path":path,"permission":"system:administrator"})
        )
        .is_err()
    );
    assert!(validate("system_menu", &json!({"path":path,"visible":"false"})).is_err());
}

#[test]
fn organizational_members_and_menu_templates_require_unique_ids() {
    assert!(
        validate(
            "organization",
            &json!({"manager_user_id":"user-1","member_ids":["user-1","user-2"],"sort_order":0})
        )
        .is_ok()
    );
    assert!(validate("organization", &json!({"member_ids":["user-1","user-1"]})).is_err());
    assert!(validate("organization", &json!({"sort_order":-1})).is_err());
    assert!(validate("menu_template", &json!({"menu_ids":[],"validity_days":1})).is_err());
    assert!(
        validate(
            "menu_template",
            &json!({"menu_ids":["menu-1"],"validity_days":1.5})
        )
        .is_err()
    );
}

#[test]
fn announcement_audience_is_typed_and_produces_real_references() {
    let properties = json!({"content":"Notice","audience":{"type":"roles","role_ids":["role-1"]},
        "publish_at":"2026-10-08T08:00:00+08:00","expires_at":"2026-10-09T08:00:00+08:00"});
    assert!(validate("announcement", &properties).is_ok());
    let refs = audience_references(&properties).unwrap();
    assert_eq!(refs[0].target_kind, "role");
    assert_eq!(refs[0].field, "audience.role_ids");
    for audience in [
        json!({"type":"tenant","role_ids":["role-1"]}),
        json!({"type":"users","user_ids":[]}),
        json!({"type":"everyone"}),
    ] {
        assert!(
            validate(
                "announcement",
                &json!({"content":"Notice","audience":audience})
            )
            .is_err()
        );
    }
    let mut reversed = properties;
    reversed["expires_at"] = json!("2026-10-07T08:00:00+08:00");
    assert!(validate("announcement", &reversed).is_err());
}

#[test]
fn template_preview_is_real_single_pass_text_rendering_with_zero_and_false() {
    let result = render_template(
        &template(),
        &json!({"name":"{{count}} <script>","count":0,"enabled":false}),
    )
    .unwrap();
    assert_eq!(result.subject, "Hello {{count}} <script>");
    assert_eq!(result.body, "Count=0, enabled=false");
    let preview = action(
        "message_template",
        &template(),
        "preview",
        &json!({"parameters":{"name":"MI","count":0,"enabled":false}}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(preview["delivered"], false);
    assert_eq!(preview["format"], "plain_text");
}

#[test]
fn templates_reject_undeclared_missing_sensitive_and_executable_tokens() {
    for body in [
        "{{undeclared}}",
        "{{enabled",
        "}} {{enabled}}",
        "{{ name.to_uppercase() }}",
    ] {
        let mut definition = template();
        definition["body"] = json!(body);
        assert!(validate("message_template", &definition).is_err());
    }
    assert!(render_template(&template(), &json!({"name":"MI","enabled":true})).is_err());
    assert!(
        render_template(
            &template(),
            &json!({"name":"MI","count":1,"enabled":true,"extra":"x"})
        )
        .is_err()
    );
    assert!(validate("message_template",&json!({"channel":"in_app","subject":"Secret","body":"{{apiToken}}","variables":["apiToken"]})).is_err());
    assert!(
        render_template(
            &template(),
            &json!({"name":"MI","count":[1],"enabled":true})
        )
        .is_err()
    );
}

#[test]
fn rendered_subject_limits_and_literal_templates_are_enforced() {
    assert!(
        render_template(
            &template(),
            &json!({"name":"line\nbreak","count":1,"enabled":true})
        )
        .is_err()
    );
    assert!(
        render_template(
            &template(),
            &json!({"name":"x".repeat(256),"count":1,"enabled":true})
        )
        .is_err()
    );
    let result = action(
        "message_template",
        &json!({"channel":"in_app","subject":"Hi","body":"Literal text"}),
        "preview",
        &json!({}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(result["body"], "Literal text");
}

#[test]
fn message_validation_does_not_claim_external_or_in_app_delivery() {
    let properties = json!({"channel":"email","recipient_ids":["user-1"],"content":"Hello"});
    assert!(validate("system_message", &properties).is_ok());
    assert!(action("system_message", &properties, "send", &json!({})).is_err());
    assert!(
        validate(
            "system_message",
            &json!({"channel":"in_app","recipient_ids":["user-1","user-1"],"content":"Hello"})
        )
        .is_err()
    );
    assert!(validate("system_message",&json!({"channel":"in_app","recipient_ids":["user-1"],"content":"Hello","parameters":{"api_secret":"x"}})).is_err());
}

#[test]
fn alert_policy_requires_bounded_integer_windows_thresholds_and_unique_recipients() {
    let valid = json!({
        "event_type":"task_failure","enabled":true,"window_seconds":3600,
        "threshold":1,"message_template_id":null,"recipient_ids":["user-1"],
        "cooldown_seconds":600,"evaluation_interval_seconds":300
    });
    assert!(validate("alert_policy", &valid).is_ok());
    for (field, value) in [
        ("window_seconds", json!(59)),
        ("window_seconds", json!(60.5)),
        ("threshold", json!(0)),
        ("cooldown_seconds", json!(-1)),
        ("evaluation_interval_seconds", json!(59)),
        ("evaluation_interval_seconds", json!(3601)),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        assert!(validate("alert_policy", &invalid).is_err(), "{field}");
    }
    let mut duplicate = valid;
    duplicate["recipient_ids"] = json!(["user-1", "user-1"]);
    assert!(validate("alert_policy", &duplicate).is_err());
}
