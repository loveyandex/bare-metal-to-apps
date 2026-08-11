use crate::models::DbEngine;
use rand::distributions::Alphanumeric;
use rand::Rng;

pub struct GeneratedVar {
    pub key: String,
    pub value: String,
    pub is_secret: bool,
}

pub fn generate_password() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect()
}

/// Standard output env vars a freshly-provisioned database service publishes
/// for other services to link against via `${{db-slug.KEY}}`.
pub fn output_vars(engine: DbEngine, container_name: &str, password: &str) -> Vec<GeneratedVar> {
    let host = container_name.to_string();
    let port = engine.internal_port();
    match engine {
        DbEngine::Postgres => {
            let db = "railway".to_string();
            let user = "postgres".to_string();
            let url = format!("postgresql://{user}:{password}@{host}:{port}/{db}");
            vec![
                gv("POSTGRES_HOST", &host, false),
                gv("POSTGRES_PORT", &port.to_string(), false),
                gv("POSTGRES_DB", &db, false),
                gv("POSTGRES_USER", &user, false),
                gv("POSTGRES_PASSWORD", password, true),
                gv("DATABASE_URL", &url, true),
            ]
        }
        DbEngine::Redis => {
            let url = format!("redis://:{password}@{host}:{port}");
            vec![
                gv("REDIS_HOST", &host, false),
                gv("REDIS_PORT", &port.to_string(), false),
                gv("REDIS_PASSWORD", password, true),
                gv("REDIS_URL", &url, true),
            ]
        }
        DbEngine::Mysql => {
            let db = "railway".to_string();
            let user = "root".to_string();
            let url = format!("mysql://{user}:{password}@{host}:{port}/{db}");
            vec![
                gv("MYSQL_HOST", &host, false),
                gv("MYSQL_PORT", &port.to_string(), false),
                gv("MYSQL_DATABASE", &db, false),
                gv("MYSQL_ROOT_PASSWORD", password, true),
                gv("DATABASE_URL", &url, true),
            ]
        }
        DbEngine::Mongodb => {
            let user = "root".to_string();
            let url = format!("mongodb://{user}:{password}@{host}:{port}");
            vec![
                gv("MONGO_HOST", &host, false),
                gv("MONGO_PORT", &port.to_string(), false),
                gv("MONGO_INITDB_ROOT_USERNAME", &user, false),
                gv("MONGO_INITDB_ROOT_PASSWORD", password, true),
                gv("DATABASE_URL", &url, true),
            ]
        }
    }
}

/// Env vars actually passed to the database's own container process (image
/// entrypoints expect their own var names, which don't always match the
/// "output" names published for linking).
pub fn container_env(engine: DbEngine, password: &str) -> Vec<(String, String)> {
    match engine {
        DbEngine::Postgres => vec![
            ("POSTGRES_PASSWORD".into(), password.into()),
            ("POSTGRES_DB".into(), "railway".into()),
        ],
        DbEngine::Redis => vec![],
        DbEngine::Mysql => vec![
            ("MYSQL_ROOT_PASSWORD".into(), password.into()),
            ("MYSQL_DATABASE".into(), "railway".into()),
        ],
        DbEngine::Mongodb => vec![
            ("MONGO_INITDB_ROOT_USERNAME".into(), "root".into()),
            ("MONGO_INITDB_ROOT_PASSWORD".into(), password.into()),
        ],
    }
}

/// Redis needs `--requirepass` since it has no env-var password knob.
pub fn container_command(engine: DbEngine, password: &str) -> Option<Vec<String>> {
    match engine {
        DbEngine::Redis => Some(vec![
            "redis-server".to_string(),
            "--requirepass".to_string(),
            password.to_string(),
        ]),
        _ => None,
    }
}

fn gv(key: &str, value: &str, is_secret: bool) -> GeneratedVar {
    GeneratedVar {
        key: key.to_string(),
        value: value.to_string(),
        is_secret,
    }
}
