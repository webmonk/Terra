//! Serde-based parameter reflection for [`LayerKind`].
//!
//! Every kind's params struct is `Serialize + Deserialize`, so its JSON tree
//! doubles as a reflection surface: dot-paths (`"base.amplitude"`) address
//! individual parameters with no per-struct descriptor code. Used for
//! per-parameter history labels and coalescing, and as the generic fallback
//! for param bindings beyond the curated alias names.

use crate::layer::LayerKind;
use serde_json::Value;

/// Params payload of the externally tagged kind enum (`{"Fbm": {...}}`).
fn params_value(kind: &LayerKind) -> Option<Value> {
    match serde_json::to_value(kind).ok()? {
        Value::Object(map) => map.into_values().next(),
        _ => None,
    }
}

fn navigate<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path.split('.') {
        cur = cur.as_object()?.get(seg)?;
    }
    Some(cur)
}

/// Read one parameter by dot-path (relative to the params struct).
pub fn get_param(kind: &LayerKind, path: &str) -> Option<Value> {
    let params = params_value(kind)?;
    navigate(&params, path).cloned()
}

/// Read one numeric parameter as f32.
pub fn get_param_f32(kind: &LayerKind, path: &str) -> Option<f32> {
    get_param(kind, path)?.as_f64().map(|v| v as f32)
}

/// Write one numeric parameter by dot-path, preserving integer-ness of the
/// existing value. Returns false (kind unchanged) when the path is missing,
/// not numeric, or the patched params no longer deserialize.
pub fn set_param_f32(kind: &mut LayerKind, path: &str, value: f32) -> bool {
    let Ok(Value::Object(mut map)) = serde_json::to_value(&*kind) else {
        return false;
    };
    let Some((tag, params)) = map.iter_mut().next() else {
        return false;
    };
    let tag = tag.clone();
    let mut cur = &mut *params;
    for seg in path.split('.') {
        let Some(obj) = cur.as_object_mut() else {
            return false;
        };
        let Some(next) = obj.get_mut(seg) else {
            return false;
        };
        cur = next;
    }
    let Value::Number(existing) = &*cur else {
        return false;
    };
    let replacement = if existing.is_f64() {
        serde_json::Number::from_f64(value as f64).map(Value::Number)
    } else if existing.as_i64().is_some_and(|v| v < 0) || value < 0.0 {
        Some(Value::Number(serde_json::Number::from(
            value.round() as i64
        )))
    } else {
        Some(Value::Number(serde_json::Number::from(
            value.round().max(0.0) as u64,
        )))
    };
    let Some(replacement) = replacement else {
        return false;
    };
    *cur = replacement;
    let patched = Value::Object(serde_json::Map::from_iter([(tag, params.take())]));
    match serde_json::from_value::<LayerKind>(patched) {
        Ok(k) => {
            *kind = k;
            true
        }
        Err(_) => false,
    }
}

/// Dot-paths of leaf values that differ between two kinds of the *same*
/// variant. Returns empty when the variants differ (a real kind swap) or
/// nothing changed. Arrays and non-object leaves diff as a single path.
pub fn changed_paths(a: &LayerKind, b: &LayerKind) -> Vec<String> {
    if std::mem::discriminant(a) != std::mem::discriminant(b) {
        return Vec::new();
    }
    let (Some(pa), Some(pb)) = (params_value(a), params_value(b)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    diff_values("", &pa, &pb, &mut out);
    out
}

fn diff_values(prefix: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            for (key, va) in ma {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                match mb.get(key) {
                    Some(vb) => diff_values(&path, va, vb, out),
                    None => out.push(path),
                }
            }
            for key in mb.keys().filter(|k| !ma.contains_key(*k)) {
                out.push(if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                });
            }
        }
        _ => {
            if a != b {
                out.push(prefix.to_string());
            }
        }
    }
}

/// Artist-facing label for a dot-path: last segment, snake_case -> Title Case.
pub fn humanize_path(path: &str) -> String {
    let leaf = path.rsplit('.').next().unwrap_or(path);
    let mut out = String::with_capacity(leaf.len());
    for (i, word) in leaf.split('_').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(c) = chars.next() {
            out.extend(c.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::FlatParams;

    fn fbm() -> LayerKind {
        // A nested kind: Fbm exposes base.* sub-struct paths.
        LayerKind::Fbm(crate::layer::FbmParams::default())
    }

    #[test]
    fn get_and_set_nested_param() {
        let mut kind = fbm();
        let before = get_param_f32(&kind, "base.amplitude")
            .expect("fbm kind should expose base.amplitude");
        assert!(set_param_f32(&mut kind, "base.amplitude", before * 2.0));
        assert_eq!(get_param_f32(&kind, "base.amplitude"), Some(before * 2.0));
        assert!(!set_param_f32(&mut kind, "no.such.path", 1.0));
    }

    #[test]
    fn integer_params_stay_integers() {
        let mut kind = fbm();
        let before = get_param_f32(&kind, "base.octaves")
            .expect("fbm kind should expose base.octaves");
        assert!(set_param_f32(&mut kind, "base.octaves", before + 1.4));
        assert_eq!(
            get_param_f32(&kind, "base.octaves"),
            Some((before + 1.4).round())
        );
    }

    #[test]
    fn diff_reports_exact_paths() {
        let a = LayerKind::Flat(FlatParams { height: 1.0 });
        let b = LayerKind::Flat(FlatParams { height: 2.0 });
        assert_eq!(changed_paths(&a, &b), vec!["height".to_string()]);
        assert!(changed_paths(&a, &a.clone()).is_empty());
        // Different variants -> no per-param paths (real kind swap).
        assert!(changed_paths(&a, &fbm()).is_empty());
    }

    #[test]
    fn binding_falls_back_to_reflection_path() {
        use crate::layer::{BindingSource, ParamBinding};
        let mut kind = fbm();
        let before = get_param_f32(&kind, "base.frequency").unwrap();
        // "base.frequency" is not in the curated alias whitelist.
        let binding = ParamBinding::new("base.frequency", BindingSource::Constant(1.0));
        kind.apply_param_binding(&binding, 0.5);
        let after = get_param_f32(&kind, "base.frequency").unwrap();
        assert!((after - before * 0.5).abs() < 1e-6);
    }

    #[test]
    fn humanize() {
        assert_eq!(humanize_path("base.noise_frequency"), "Noise Frequency");
        assert_eq!(humanize_path("height"), "Height");
    }
}
