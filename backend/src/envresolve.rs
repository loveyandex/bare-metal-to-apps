use crate::models::EnvVar;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Resolves `${{service-slug.KEY}}` references in env var values against the
/// other services in the same project. Unresolvable references are left as
/// the literal `${{...}}` text and reported so the caller can surface a
/// warning, but never block a deploy.
pub async fn resolve_env(
    pool: &PgPool,
    project_id: Uuid,
    service_id: Uuid,
) -> anyhow::Result<(HashMap<String, String>, Vec<String>)> {
    let rows: Vec<EnvVar> = sqlx::query_as(
        "select id, service_id, key, value, is_secret, is_generated from env_vars where service_id = $1",
    )
    .bind(service_id)
    .fetch_all(pool)
    .await?;

    let services: Vec<(Uuid, String)> =
        sqlx::query_as("select id, slug from services where project_id = $1")
            .bind(project_id)
            .fetch_all(pool)
            .await?;

    let mut by_slug: HashMap<String, HashMap<String, String>> = HashMap::new();
    for (svc_id, slug) in &services {
        let vars: Vec<(String, String)> =
            sqlx::query_as("select key, value from env_vars where service_id = $1")
                .bind(svc_id)
                .fetch_all(pool)
                .await?;
        by_slug.insert(slug.clone(), vars.into_iter().collect());
    }

    let mut resolved = HashMap::new();
    let mut warnings = Vec::new();
    for row in rows {
        let value = interpolate(&row.value, &by_slug, &mut warnings);
        resolved.insert(row.key, value);
    }
    Ok((resolved, warnings))
}

fn interpolate(
    input: &str,
    by_slug: &HashMap<String, HashMap<String, String>>,
    warnings: &mut Vec<String>,
) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("${{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 3..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let reference = after[..end].trim();
        if let Some((svc, key)) = reference.split_once('.') {
            match by_slug.get(svc.trim()).and_then(|m| m.get(key.trim())) {
                Some(v) => out.push_str(v),
                None => {
                    warnings.push(format!("unresolved reference ${{{{{reference}}}}}"));
                    out.push_str(&format!("${{{{{reference}}}}}"));
                }
            }
        } else {
            warnings.push(format!("malformed reference ${{{{{reference}}}}}"));
            out.push_str(&format!("${{{{{reference}}}}}"));
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}
