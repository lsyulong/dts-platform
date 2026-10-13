use super::common;
use crate::models::error::AppError;
use serde_json::{Map, Value};
use std::collections::HashSet;

pub struct RenderedMessage {
    pub subject: String,
    pub body: String,
}

pub(super) fn channel(properties: &Value) -> Result<&str, AppError> {
    let channel = common::text(properties, "channel", 32)?;
    if !matches!(
        channel,
        "in_app" | "email" | "sms" | "wechat_work" | "dingtalk" | "webhook"
    ) {
        return Err(common::bad("消息渠道不受支持"));
    }
    Ok(channel)
}

pub(super) fn parameters(value: &Value) -> Result<&Map<String, Value>, AppError> {
    let parameters = common::object(value, "parameters")?;
    if parameters.len() > 100 {
        return Err(common::bad("消息模板参数最多包含 100 项"));
    }
    for (name, value) in parameters {
        variable_name(name)?;
        match value {
            Value::String(text) if text.len() <= 8192 && !text.contains('\0') => {},
            Value::Bool(_) => {},
            Value::Number(number) if number.as_f64().is_some_and(f64::is_finite) => {},
            _ => {
                return Err(common::bad(
                    "消息参数只支持有长度限制的文本、有限数字和布尔值",
                ));
            },
        }
    }
    Ok(parameters)
}

pub(super) fn validate(properties: &Value) -> Result<(), AppError> {
    common::object(properties, "properties")?;
    channel(properties)?;
    let subject = common::text(properties, "subject", 256)?;
    if subject.chars().any(char::is_control) {
        return Err(common::bad("标题不能包含控制字符或换行"));
    }
    let body = common::text(properties, "body", 65_536)?;
    if body.len() > 65_536 {
        return Err(common::bad("消息模板正文不能超过 64KiB"));
    }
    let declared = common::string_list(properties, "variables", false, 0, 100, false)?;
    for name in &declared {
        variable_name(name)?;
    }
    let declared: HashSet<_> = declared.into_iter().collect();
    let used: HashSet<_> = tokens(subject)?.into_iter().chain(tokens(body)?).collect();
    if declared != used {
        return Err(common::bad(
            "variables 必须与标题和正文中实际使用的 {{变量名}} 完全一致",
        ));
    }
    Ok(())
}

fn variable_name(name: &str) -> Result<(), AppError> {
    common::identifier(name, "模板变量名")?;
    if name.len() > 64 {
        return Err(common::bad("模板变量名最多 64 字符"));
    }
    let normalized: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if [
        "password",
        "secret",
        "token",
        "apikey",
        "privatekey",
        "authorization",
        "cookie",
        "credential",
    ]
    .iter()
    .any(|sensitive| normalized.contains(sensitive))
    {
        return Err(common::bad("敏感参数不能作为普通消息模板变量保存或渲染"));
    }
    Ok(())
}

fn tokens(mut text: &str) -> Result<Vec<&str>, AppError> {
    let mut names = Vec::new();
    while let Some(start) = text.find("{{") {
        if text[..start].contains("}}") {
            return Err(common::bad("模板存在未匹配的结束标记"));
        }
        let rest = &text[start + 2..];
        let end = rest
            .find("}}")
            .ok_or_else(|| common::bad("模板变量缺少结束标记"))?;
        let name = rest[..end].trim();
        variable_name(name)?;
        names.push(name);
        text = &rest[end + 2..];
    }
    if text.contains("}}") {
        return Err(common::bad("模板存在未匹配的结束标记"));
    }
    Ok(names)
}

pub fn render_template(properties: &Value, values: &Value) -> Result<RenderedMessage, AppError> {
    validate(properties)?;
    let values = parameters(values)?;
    let names = common::string_list(properties, "variables", false, 0, 100, false)?;
    if names.len() != values.len() || names.iter().any(|name| !values.contains_key(*name)) {
        return Err(common::bad("模板参数缺失或包含未声明参数"));
    }
    let render = |mut text: &str, max: usize| -> Result<String, AppError> {
        let mut result = String::new();
        while let Some(start) = text.find("{{") {
            result.push_str(&text[..start]);
            let rest = &text[start + 2..];
            let end = rest
                .find("}}")
                .ok_or_else(|| common::bad("模板变量缺少结束标记"))?;
            let name = rest[..end].trim();
            let value = values
                .get(name)
                .ok_or_else(|| common::bad("缺少模板参数"))?;
            match value {
                Value::String(value) => result.push_str(value),
                _ => result.push_str(&value.to_string()),
            }
            if result.len() > max {
                return Err(common::bad("渲染后的消息超过长度限制"));
            }
            text = &rest[end + 2..];
        }
        result.push_str(text);
        if result.len() > max {
            return Err(common::bad("渲染后的消息超过长度限制"));
        }
        Ok(result)
    };
    let subject = render(common::text(properties, "subject", 256)?, 1024)?;
    if subject.chars().count() > 256 || subject.chars().any(char::is_control) {
        return Err(common::bad("渲染后的标题超过 256 字符或包含控制字符"));
    }
    let body = render(common::text(properties, "body", 65_536)?, 65_536)?;
    if subject.trim().is_empty() || body.trim().is_empty() {
        return Err(common::bad("渲染后的消息不能为空"));
    }
    Ok(RenderedMessage { subject, body })
}
