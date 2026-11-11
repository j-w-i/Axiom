use crate::app::App;
use crate::settings::{known, KNOWN};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::settings::{DidChange, GetParams, GetResult, SetParams};
use axiom_protocol::{ErrorKind, RpcError};
use axiom_rpc::RequestCtx;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Stored value if any, else the known default, else absent.
fn effective(stored: &BTreeMap<String, Value>, key: &str) -> Option<Value> {
    stored
        .get(key)
        .cloned()
        .or_else(|| known(key).map(|k| (k.default)()))
}

pub async fn get(app: Arc<App>, p: GetParams) -> Result<GetResult, RpcError> {
    let stored = app.store_call(|s| s.settings_all()).await?;
    let mut values = BTreeMap::new();
    if p.keys.is_empty() {
        for k in KNOWN {
            values.insert(k.key.to_string(), (k.default)());
        }
        values.extend(stored);
    } else {
        for key in &p.keys {
            if let Some(v) = effective(&stored, key) {
                values.insert(key.clone(), v);
            }
        }
    }
    Ok(GetResult { values })
}

pub async fn set(app: Arc<App>, p: SetParams, ctx: RequestCtx) -> Result<Empty, RpcError> {
    for (key, value) in &p.values {
        if let Some(k) = known(key) {
            if !value.is_null() && !(k.valid)(value) {
                return Err(RpcError::new(
                    ErrorKind::InvalidParams,
                    format!("Invalid value for `{key}`."),
                )
                .with_details(serde_json::json!({ "key": key })));
            }
        }
    }
    let changed: Vec<String> = p.values.keys().cloned().collect();
    let batch: BTreeMap<String, Option<Value>> = p
        .values
        .into_iter()
        .map(|(k, v)| (k, if v.is_null() { None } else { Some(v) }))
        .collect();
    let stored = app
        .store_call(move |s| {
            s.settings_set(&batch)?;
            s.settings_all()
        })
        .await?;
    let values: BTreeMap<String, Value> = changed
        .into_iter()
        .map(|k| {
            let v = effective(&stored, &k).unwrap_or(Value::Null);
            (k, v)
        })
        .collect();
    let notifier = ctx.notifier.clone();
    ctx.after_response(async move {
        let _ = notifier.notify::<DidChange>(GetResult { values }).await;
    });
    Ok(Empty {})
}
