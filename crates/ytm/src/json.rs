//! Caminhar no JSON do InnerTube sem `unwrap` a cada passo.

use serde_json::Value;

static NULL: Value = Value::Null;

/// `at(v, "a.b.0.c")`: o valor no caminho, ou `Null` se faltar qualquer passo.
/// Número no caminho indexa lista.
pub fn at<'a>(mut v: &'a Value, path: &str) -> &'a Value {
    for key in path.split('.') {
        v = match v {
            Value::Object(map) => map.get(key).unwrap_or(&NULL),
            Value::Array(list) => match key.parse::<usize>() {
                Ok(i) => list.get(i).unwrap_or(&NULL),
                Err(_) => &NULL,
            },
            _ => return &NULL,
        };
    }
    v
}

pub fn str_at<'a>(v: &'a Value, path: &str) -> Option<&'a str> {
    at(v, path).as_str().filter(|s| !s.is_empty())
}

pub fn list(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// O texto de um campo `{runs:[{text}]}` ou `{simpleText}`.
pub fn text(v: &Value) -> String {
    if let Some(s) = v.get("simpleText").and_then(Value::as_str) {
        return s.to_owned();
    }
    list(at(v, "runs"))
        .iter()
        .filter_map(|r| r.get("text").and_then(Value::as_str))
        .collect()
}

/// A primeira ocorrência de `key` até `depth` níveis abaixo de `v`.
pub fn find<'a>(v: &'a Value, key: &str, depth: u32) -> Option<&'a Value> {
    match v {
        Value::Object(map) => {
            if let Some(found) = map.get(key) {
                return Some(found);
            }
            if depth == 0 {
                return None;
            }
            map.values().find_map(|child| find(child, key, depth - 1))
        }
        Value::Array(items) if depth > 0 => items.iter().find_map(|child| find(child, key, depth - 1)),
        _ => None,
    }
}

/// O par (nome do renderer, conteúdo) de um objeto `{ fooRenderer: {...} }`.
pub fn renderer(v: &Value) -> Option<(&str, &Value)> {
    v.as_object()?
        .iter()
        .find(|(k, _)| k.ends_with("Renderer"))
        .map(|(k, v)| (k.as_str(), v))
}
