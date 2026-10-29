use crate::error::{Error, Result};
use crate::model::{IndexDep, IndexEntry, PublishBody};
use std::collections::BTreeMap;

pub fn shard_for_name(name_lower: &str) -> String {
    let bytes = name_lower.as_bytes();
    if bytes.is_empty() {
        return "1".to_string();
    }
    if name_lower.len() == 1 {
        return format!("1/{}", name_lower);
    }
    if name_lower.len() == 2 {
        return format!("2/{}", name_lower);
    }
    if name_lower.len() == 3 {
        return format!("3/{}/{}", &name_lower[0..1], name_lower);
    }
    format!("{}/{}/{}", &name_lower[0..2], &name_lower[2..4], name_lower)
}

pub fn native_config_json(public_base_url: &str) -> String {
    let dl = format!("{}/blobs/{{sha256-checksum}}.crate", public_base_url.trim_end_matches('/'));
    let api = public_base_url.to_string();
    let obj = serde_json::json!({
        "dl": dl,
        "api": api,
        "auth-required": false,
    });
    serde_json::to_string(&obj).unwrap()
}

pub fn mirror_config_json(public_base_url: &str) -> String {
    let dl = format!("{}/blobs/{{sha256-checksum}}.crate", public_base_url.trim_end_matches('/'));
    let obj = serde_json::json!({
        "dl": dl,
    });
    serde_json::to_string(&obj).unwrap()
}

pub fn publish_to_index(body: &PublishBody) -> Result<IndexEntry> {
    let mut deps_out_fixed: Vec<IndexDep> = Vec::new();
    for d in &body.deps {
        let mut dep_name = d.name.clone().unwrap_or_default();
        let mut dep_package = d.package.clone();
        if let Some(ref explicit) = d.explicit_name_in_toml {
            dep_name = explicit.clone();
            if dep_package.is_none() {
                dep_package = d.name.clone();
            }
        }

        let req = d.version_req.clone();

        let registry_out = if let Some(ref reg) = d.registry {
            if reg == "https://github.com/rust-lang/crates.io-index" {
                Some("https://github.com/rust-lang/crates.io-index".to_string())
            } else if reg.is_empty() {
                None
            } else {
                Some(reg.clone())
            }
        } else {
            None
        };

        deps_out_fixed.push(IndexDep {
            name: dep_name,
            req,
            features: d.features.clone(),
            optional: d.optional,
            default_features: d.default_features,
            target: d.target.clone(),
            kind: d.kind.clone(),
            registry: registry_out,
            package: dep_package,
        });
    }

    let mut has_v2 = false;
    for v in body.features.values() {
        for val in v {
            if val.contains("dep:") || val.contains('?') {
                has_v2 = true;
                break;
            }
        }
        if has_v2 {
            break;
        }
    }
    let features_out: BTreeMap<String, Vec<String>> = body.features.clone();

    let entry = IndexEntry {
        name: body.name.clone(),
        vers: body.vers.clone(),
        deps: deps_out_fixed,
        cksum: body.checksum.clone(),
        features: features_out,
        yanked: body.yanked,
        links: body.links.clone(),
        rust_version: body.rust_version.clone(),
        v: if has_v2 { Some(2) } else { Some(1) },
        extra: BTreeMap::new(),
    };
    Ok(entry)
}

pub fn serialize_index_line(entry: &IndexEntry) -> Result<String> {
    let mut obj = serde_json::Map::new();
    obj.insert("name".to_string(), serde_json::Value::String(entry.name.clone()));
    obj.insert("vers".to_string(), serde_json::Value::String(entry.vers.clone()));
    let mut deps_arr = Vec::new();
    for dep in &entry.deps {
        let mut d = serde_json::Map::new();
        d.insert("name".to_string(), serde_json::Value::String(dep.name.clone()));
        d.insert("req".to_string(), serde_json::Value::String(dep.req.clone()));
        d.insert("features".to_string(), serde_json::to_value(&dep.features).unwrap());
        d.insert("optional".to_string(), serde_json::Value::Bool(dep.optional));
        d.insert("default_features".to_string(), serde_json::Value::Bool(dep.default_features));
        if let Some(ref t) = dep.target {
            d.insert("target".to_string(), serde_json::Value::String(t.clone()));
        }
        if let Some(ref k) = dep.kind {
            d.insert("kind".to_string(), serde_json::Value::String(k.clone()));
        }
        if dep.registry.is_none() {
            d.insert("registry".to_string(), serde_json::Value::Null);
        } else if let Some(ref r) = dep.registry {
            d.insert("registry".to_string(), serde_json::Value::String(r.clone()));
        }
        if let Some(ref p) = dep.package {
            d.insert("package".to_string(), serde_json::Value::String(p.clone()));
        }
        deps_arr.push(serde_json::Value::Object(d));
    }
    obj.insert("deps".to_string(), serde_json::Value::Array(deps_arr));
    obj.insert("cksum".to_string(), serde_json::Value::String(entry.cksum.clone()));
    obj.insert("features".to_string(), serde_json::to_value(&entry.features).unwrap());
    obj.insert("yanked".to_string(), serde_json::Value::Bool(entry.yanked));
    if let Some(ref links) = entry.links {
        obj.insert("links".to_string(), serde_json::Value::String(links.clone()));
    }
    if let Some(ref rv) = entry.rust_version {
        obj.insert("rust_version".to_string(), serde_json::Value::String(rv.clone()));
    }
    if let Some(v) = entry.v {
        obj.insert("v".to_string(), serde_json::Value::Number(serde_json::Number::from(v)));
    }
    for (k, v) in &entry.extra {
        obj.insert(k.clone(), v.clone());
    }
    let s = serde_json::to_string(&obj).map_err(|e| Error::Operation(e.to_string()))?;
    Ok(s)
}
