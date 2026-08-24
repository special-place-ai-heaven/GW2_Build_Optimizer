use serde_json::Value;
use sqlx::PgPool;

const EMBEDDED: &str = include_str!("../../../data/feedback_taxonomy.json");

#[derive(Debug, Clone)]
pub struct Taxonomy {
    pub version: i32,
    pub body: Value,
}

impl Taxonomy {
    pub fn embedded() -> Self {
        let body: Value = serde_json::from_str(EMBEDDED).expect("embedded taxonomy is valid JSON");
        let version = body["taxonomy_version"].as_i64().expect("taxonomy_version") as i32;
        Self { version, body }
    }

    /// True when the category exists and every path element is a choice of
    /// one of that category's steps. Unknown ids are not an error for the
    /// caller — they mark the report `unvalidated`.
    pub fn validate(&self, category: &str, path: &[String]) -> bool {
        let Some(cat) = self.body["categories"]
            .as_array()
            .and_then(|cs| cs.iter().find(|c| c["id"] == category))
        else {
            return false;
        };
        let steps = self.body["steps"].as_object();
        let step_ids: Vec<&str> = cat["steps"]
            .as_array()
            .map(|s| s.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        path.iter().all(|p| {
            step_ids.iter().any(|sid| {
                steps
                    .and_then(|s| s.get(*sid))
                    .and_then(|st| st["choices"].as_array())
                    .map(|ch| ch.iter().any(|c| c == p))
                    .unwrap_or(false)
            })
        })
    }
}

pub async fn seed_if_empty(pool: &PgPool) -> sqlx::Result<()> {
    let t = Taxonomy::embedded();
    sqlx::query(
        "insert into taxonomy (version, body) values ($1, $2) on conflict (version) do nothing",
    )
    .bind(t.version)
    .bind(&t.body)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_current(pool: &PgPool) -> sqlx::Result<Taxonomy> {
    let row: (i32, Value) =
        sqlx::query_as("select version, body from taxonomy order by version desc limit 1")
            .fetch_one(pool)
            .await?;
    Ok(Taxonomy {
        version: row.0,
        body: row.1,
    })
}
