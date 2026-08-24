use axum::body::Body;
use axum::http::{Request, StatusCode};
use gw2bo_feedback::app::{router, AppState};
use gw2bo_feedback::config::Config;
use http_body_util::BodyExt;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

fn test_config() -> Arc<Config> {
    Arc::new(Config {
        database_url: String::new(), // pool is injected by sqlx::test
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        admin_token: "test-admin-token".into(),
        ip_salt: "test-salt".into(),
        min_addon_version: "1.6.0".into(),
    })
}

async fn state(pool: PgPool) -> AppState {
    AppState::new(pool, test_config()).await
}

fn healthz_req() -> Request<Body> {
    Request::builder()
        .uri("/healthz")
        .body(Body::empty())
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn healthz_returns_ok(pool: PgPool) {
    let res = router(state(pool).await)
        .oneshot(healthz_req())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], b"ok");
}

#[sqlx::test(migrations = "./migrations")]
async fn healthz_is_503_when_the_database_is_gone(pool: PgPool) {
    let st = state(pool.clone()).await;
    pool.close().await;
    let res = router(st).oneshot(healthz_req()).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "healthz must probe the pool, not just answer from the router"
    );
    let v = json_body(res).await;
    assert_eq!(v["error"], "db_unavailable");
}

#[sqlx::test(migrations = "./migrations")]
async fn schema_has_reports_and_taxonomy(pool: PgPool) {
    let cols: Vec<(String,)> = sqlx::query_as(
        "select column_name::text from information_schema.columns where table_name = 'reports' order by ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let names: Vec<&str> = cols.iter().map(|c| c.0.as_str()).collect();
    for required in [
        "short_id",
        "report_id",
        "client_id",
        "category",
        "path",
        "body",
        "status",
        "payload",
        "ip_hash",
        "unvalidated",
        "closing_note",
    ] {
        assert!(names.contains(&required), "missing column {required}");
    }
    let (n,): (i64,) = sqlx::query_as("select count(*) from taxonomy")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

async fn json_body(res: axum::response::Response) -> serde_json::Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn taxonomy_is_seeded_and_served(pool: PgPool) {
    let app = router(state(pool.clone()).await);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/v1/taxonomy")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = json_body(res).await;
    assert_eq!(v["taxonomy_version"], 1);
    assert!(v["categories"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "praise"));
    let (n,): (i64,) = sqlx::query_as("select count(*) from taxonomy")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "seeded exactly once");
}

#[test]
fn taxonomy_validate_knows_its_ids() {
    use gw2bo_feedback::taxonomy::Taxonomy;
    let t = Taxonomy::embedded();
    assert!(t.validate("bug", &["optimize".into(), "wrong".into()]));
    assert!(t.validate("praise", &["everything".into()]));
    assert!(!t.validate("bug", &["optimize".into(), "nope".into()]));
    assert!(!t.validate("teleport", &[]));
}

fn report_json(report_id: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "report_id": report_id,
        "client_id": "11111111-1111-4111-8111-111111111111",
        "category": "bug",
        "path": ["optimize", "wrong"],
        "title": "Optimize picks Trident on land",
        "body": "Expected a land weapon, got Trident on a Ranger in WvW roam.",
        "contact": null,
        "account": null,
        "context": { "addon_version": "1.6.0", "game_build": 174122, "locale": "en",
                     "mode": "WvW", "scale": "Roam", "role": "Damage",
                     "profession": "Ranger", "elite": "Untamed", "llm_provider": "gemini" },
        "build_snapshot": null
    })
}

fn post_report(body: serde_json::Value, ip: &str, addon_version: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/reports")
        .header("content-type", "application/json")
        .header("x-forwarded-for", ip)
        .header("x-addon-version", addon_version)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn post_report_creates_row_and_returns_short_id(pool: PgPool) {
    let app = router(state(pool.clone()).await);
    let res = app
        .oneshot(post_report(
            report_json("22222222-2222-4222-8222-222222222222"),
            "203.0.113.5",
            "1.6.0",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let v = json_body(res).await;
    assert_eq!(v["status"], "received");
    assert_eq!(v["id"].as_str().unwrap().len(), 8);
    let (cat, unval, ip_hash): (String, bool, String) =
        sqlx::query_as("select category, unvalidated, ip_hash from reports")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cat, "bug");
    assert!(!unval);
    assert_ne!(ip_hash, "203.0.113.5", "raw ip must never be stored");
}

#[sqlx::test(migrations = "./migrations")]
async fn post_same_report_id_twice_is_one_row_same_short_id(pool: PgPool) {
    let st = state(pool.clone()).await;
    let a = json_body(
        router(st.clone())
            .oneshot(post_report(
                report_json("33333333-3333-4333-8333-333333333333"),
                "203.0.113.5",
                "1.6.0",
            ))
            .await
            .unwrap(),
    )
    .await;
    let b = json_body(
        router(st)
            .oneshot(post_report(
                report_json("33333333-3333-4333-8333-333333333333"),
                "203.0.113.5",
                "1.6.0",
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(a["id"], b["id"]);
    let (n,): (i64,) = sqlx::query_as("select count(*) from reports")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn post_unknown_category_is_stored_unvalidated(pool: PgPool) {
    let mut body = report_json("44444444-4444-4444-8444-444444444444");
    body["category"] = "teleport".into();
    let res = router(state(pool.clone()).await)
        .oneshot(post_report(body, "203.0.113.5", "1.6.0"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let (unval,): (bool,) = sqlx::query_as("select unvalidated from reports")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(unval);
}

#[sqlx::test(migrations = "./migrations")]
async fn post_rejects_too_long_body_and_bad_shape(pool: PgPool) {
    let st = state(pool).await;
    let mut long = report_json("55555555-5555-4555-8555-555555555555");
    long["body"] = "x".repeat(4001).into();
    let res = router(st.clone())
        .oneshot(post_report(long, "203.0.113.5", "1.6.0"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let huge = serde_json::json!({ "pad": "y".repeat(20_000) });
    let res = router(st.clone())
        .oneshot(post_report(huge, "203.0.113.5", "1.6.0"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let res = router(st)
        .oneshot(post_report(
            report_json("66666666-6666-4666-8666-666666666666"),
            "203.0.113.5",
            "1.5.3",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UPGRADE_REQUIRED);
}

#[sqlx::test(migrations = "./migrations")]
async fn eleventh_post_in_a_minute_from_one_ip_is_429(pool: PgPool) {
    let st = state(pool).await;
    for i in 0..10 {
        let id = format!("77777777-7777-4777-8777-7777777777{:02}", i);
        let res = router(st.clone())
            .oneshot(post_report(report_json(&id), "198.51.100.9", "1.6.0"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED, "request {i}");
    }
    let res = router(st)
        .oneshot(post_report(
            report_json("77777777-7777-4777-8777-777777777799"),
            "198.51.100.9",
            "1.6.0",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry: u64 = res.headers()["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=60).contains(&retry));
}

const CLIENT: &str = "11111111-1111-4111-8111-111111111111"; // matches report_json()

fn status_req(ids: &str, client_id: &str, ip: &str) -> Request<Body> {
    Request::builder()
        .uri(format!(
            "/v1/reports/status?ids={ids}&client_id={client_id}"
        ))
        .header("x-forwarded-for", ip)
        .body(Body::empty())
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn status_returns_only_requested_ids_owned_by_client(pool: PgPool) {
    let st = state(pool.clone()).await;
    let a = json_body(
        router(st.clone())
            .oneshot(post_report(
                report_json("88888888-8888-4888-8888-888888888881"),
                "203.0.113.5",
                "1.6.0",
            ))
            .await
            .unwrap(),
    )
    .await;
    let _b = json_body(
        router(st.clone())
            .oneshot(post_report(
                report_json("88888888-8888-4888-8888-888888888882"),
                "203.0.113.5",
                "1.6.0",
            ))
            .await
            .unwrap(),
    )
    .await;
    let id = a["id"].as_str().unwrap();

    let res = router(st.clone())
        .oneshot(status_req(&format!("{id},NOPE1234"), CLIENT, "203.0.113.5"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = json_body(res).await;
    let arr = v.as_array().unwrap();
    assert_eq!(
        arr.len(),
        1,
        "only the requested, existing id — never the sibling row"
    );
    assert_eq!(arr[0]["id"], a["id"]);
    assert_eq!(arr[0]["status"], "received");
    assert!(arr[0]["reply"].is_null());

    // Same id, different client: nothing. Ownership is short_id AND client_id.
    let other = json_body(
        router(st.clone())
            .oneshot(status_req(
                id,
                "22222222-2222-4222-8222-222222222222",
                "203.0.113.5",
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(other.as_array().unwrap().len(), 0);

    // Missing client_id is a 400, not an open door.
    let res = router(st.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/v1/reports/status?ids={id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Rate-limited per ip like POST: a fresh ip gets 10 GETs, then 429.
    for i in 0..10 {
        assert_eq!(
            router(st.clone())
                .oneshot(status_req(id, CLIENT, "203.0.113.77"))
                .await
                .unwrap()
                .status(),
            StatusCode::OK,
            "get {i}"
        );
    }
    assert_eq!(
        router(st)
            .oneshot(status_req(id, CLIENT, "203.0.113.77"))
            .await
            .unwrap()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[test]
fn limiter_sliding_window() {
    use gw2bo_feedback::ratelimit::RateLimiter;
    use std::time::Duration;
    let l = RateLimiter::new();
    for _ in 0..3 {
        assert!(l.check("k", 3, Duration::from_secs(60)).is_ok());
    }
    let err = l.check("k", 3, Duration::from_secs(60)).unwrap_err();
    assert!(err >= 1);
    assert!(l.check("other", 3, Duration::from_secs(60)).is_ok());
}

#[test]
fn ids_are_shaped_right() {
    use gw2bo_feedback::ids::{ip_hash, short_id};
    let s = short_id();
    assert_eq!(s.len(), 8);
    assert!(s
        .chars()
        .all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c)));
    let d = chrono::NaiveDate::from_ymd_opt(2026, 8, 24).unwrap();
    assert_eq!(ip_hash("1.2.3.4", "salt", d), ip_hash("1.2.3.4", "salt", d));
    assert_ne!(
        ip_hash("1.2.3.4", "salt", d),
        ip_hash("1.2.3.4", "salt", d.succ_opt().unwrap())
    );
}

fn admin(method: &str, uri: &str, body: Option<serde_json::Value>, token: &str) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    if body.is_some() {
        b = b.header("content-type", "application/json");
    }
    b.body(
        body.map(|v| Body::from(v.to_string()))
            .unwrap_or(Body::empty()),
    )
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_requires_token(pool: PgPool) {
    let res = router(state(pool).await)
        .oneshot(admin("GET", "/v1/admin/reports", None, "wrong"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_list_marks_read_and_reply_marks_answered(pool: PgPool) {
    let st = state(pool).await;
    let created = json_body(
        router(st.clone())
            .oneshot(post_report(
                report_json("99999999-9999-4999-8999-999999999991"),
                "203.0.113.5",
                "1.6.0",
            ))
            .await
            .unwrap(),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let list = json_body(
        router(st.clone())
            .oneshot(admin(
                "GET",
                "/v1/admin/reports?status=received",
                None,
                "test-admin-token",
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let st_after = json_body(
        router(st.clone())
            .oneshot(status_req(&id, CLIENT, "203.0.113.5"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        st_after[0]["status"], "read",
        "listing through admin marks it read"
    );

    let res = router(st.clone())
        .oneshot(admin(
            "POST",
            &format!("/v1/admin/reports/{id}/reply"),
            Some(serde_json::json!({ "reply": "Fixed in 1.6.1, thanks!", "status": "answered" })),
            "test-admin-token",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let final_status = json_body(
        router(st)
            .oneshot(status_req(&id, CLIENT, "203.0.113.5"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(final_status[0]["status"], "answered");
    assert_eq!(final_status[0]["reply"], "Fixed in 1.6.1, thanks!");
    assert!(final_status[0]["replied_at"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_get_one_returns_full_row_and_marks_read(pool: PgPool) {
    let st = state(pool).await;
    let created = json_body(
        router(st.clone())
            .oneshot(post_report(
                report_json("99999999-9999-4999-8999-999999999992"),
                "203.0.113.5",
                "1.6.0",
            ))
            .await
            .unwrap(),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let res = router(st.clone())
        .oneshot(admin(
            "GET",
            &format!("/v1/admin/reports/{id}"),
            None,
            "test-admin-token",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let row = json_body(res).await;
    assert_eq!(row["id"], id);
    assert_eq!(row["category"], "bug");
    assert_eq!(row["title"], "Optimize picks Trident on land");
    assert_eq!(row["unvalidated"], false);
    assert_eq!(
        row["status"], "read",
        "the response must show the status the fetch just wrote, not the one it replaced"
    );

    let st_after = json_body(
        router(st.clone())
            .oneshot(status_req(&id, CLIENT, "203.0.113.5"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        st_after[0]["status"], "read",
        "fetching a single row through admin marks it read too"
    );

    let res = router(st)
        .oneshot(admin(
            "GET",
            "/v1/admin/reports/NOPE1234",
            None,
            "test-admin-token",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_set_status_updates_status_and_preserves_closing_note_when_omitted(pool: PgPool) {
    let st = state(pool).await;
    let created = json_body(
        router(st.clone())
            .oneshot(post_report(
                report_json("99999999-9999-4999-8999-999999999993"),
                "203.0.113.5",
                "1.6.0",
            ))
            .await
            .unwrap(),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let res = router(st.clone())
        .oneshot(admin(
            "POST",
            &format!("/v1/admin/reports/{id}/status"),
            Some(serde_json::json!({ "status": "closed", "closing_note": "duplicate of #42" })),
            "test-admin-token",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = json_body(res).await;
    assert_eq!(body["status"], "closed");
    assert_eq!(body["closing_note"], "duplicate of #42");

    // Second call omits closing_note; the `coalesce($3, closing_note)` in the
    // query must preserve the note already on the row rather than nulling it.
    let res = router(st.clone())
        .oneshot(admin(
            "POST",
            &format!("/v1/admin/reports/{id}/status"),
            Some(serde_json::json!({ "status": "read" })),
            "test-admin-token",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = json_body(res).await;
    assert_eq!(body["status"], "read");
    assert_eq!(
        body["closing_note"], "duplicate of #42",
        "coalesce keeps the prior note when the field is omitted"
    );

    let res = router(st.clone())
        .oneshot(admin(
            "POST",
            &format!("/v1/admin/reports/{id}/status"),
            Some(serde_json::json!({ "status": "bogus" })),
            "test-admin-token",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let res = router(st)
        .oneshot(admin(
            "POST",
            "/v1/admin/reports/NOPE1234/status",
            Some(serde_json::json!({ "status": "closed" })),
            "test-admin-token",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_can_replace_taxonomy_and_it_is_served(pool: PgPool) {
    let st = state(pool).await;
    let mut t = gw2bo_feedback::taxonomy::Taxonomy::embedded().body;
    t["taxonomy_version"] = 2.into();
    t["categories"].as_array_mut().unwrap().push(serde_json::json!({
        "id": "translation", "type": "report", "label": "cat.translation", "icon": "globe", "color": "blue", "steps": ["describe"]
    }));
    let res = router(st.clone())
        .oneshot(admin(
            "PUT",
            "/v1/admin/taxonomy",
            Some(t),
            "test-admin-token",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let served = json_body(
        router(st)
            .oneshot(
                Request::builder()
                    .uri("/v1/taxonomy")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(served["taxonomy_version"], 2);
    assert!(served["categories"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "translation"));
}

#[test]
fn client_ip_takes_the_rightmost_forwarded_entry() {
    use axum::http::HeaderMap;
    use gw2bo_feedback::ids::ip_hash;
    use gw2bo_feedback::reports::client_ip;
    let mut h = HeaderMap::new();
    h.insert("x-forwarded-for", "1.1.1.1, 203.0.113.5".parse().unwrap());
    assert_eq!(client_ip(&h, None), "203.0.113.5");
    let d = chrono::NaiveDate::from_ymd_opt(2026, 8, 24).unwrap();
    assert_eq!(
        ip_hash(&client_ip(&h, None), "salt", d),
        ip_hash("203.0.113.5", "salt", d),
        "the proxy-appended entry is what gets hashed"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn spoofed_forwarded_prefix_shares_one_rate_bucket(pool: PgPool) {
    let st = state(pool).await;
    // Every request forges a different leftmost entry; only the rightmost one —
    // the entry the proxy appended — may decide the bucket.
    for i in 0..10 {
        let id = format!("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbb{:03}", i);
        let xff = format!("10.0.0.{i}, 198.51.100.42");
        let res = router(st.clone())
            .oneshot(post_report(report_json(&id), &xff, "1.6.0"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED, "request {i}");
    }
    let res = router(st)
        .oneshot(post_report(
            report_json("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbb999"),
            "10.0.0.99, 198.51.100.42",
            "1.6.0",
        ))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "a fresh left entry must not buy a fresh bucket"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn extractor_rejections_carry_the_error_envelope(pool: PgPool) {
    let st = state(pool).await;

    // Valid JSON, wrong shape: axum would answer 422 in plain text on its own.
    let mut missing = report_json("cccccccc-cccc-4ccc-8ccc-cccccccccc01");
    missing.as_object_mut().unwrap().remove("category");
    let res = router(st.clone())
        .oneshot(post_report(missing, "203.0.113.5", "1.6.0"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let v = json_body(res).await;
    assert_eq!(v["error"], "bad_request");
    assert!(
        !v["reason"].as_str().unwrap().is_empty(),
        "the reason must say what was wrong"
    );

    // No content-type: axum would answer 415 in plain text on its own.
    let res = router(st.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/reports")
                .header("x-forwarded-for", "203.0.113.6")
                .header("x-addon-version", "1.6.0")
                .body(Body::from(
                    report_json("cccccccc-cccc-4ccc-8ccc-cccccccccc02").to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let v = json_body(res).await;
    assert_eq!(v["error"], "bad_request");
    assert!(!v["reason"].as_str().unwrap().is_empty());

    // Missing client_id on the status GET: a Query rejection, same envelope.
    let res = router(st)
        .oneshot(
            Request::builder()
                .uri("/v1/reports/status?ids=A3F9K2QD")
                .header("x-forwarded-for", "203.0.113.7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let v = json_body(res).await;
    assert_eq!(v["error"], "bad_request");
    assert!(!v["reason"].as_str().unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_list_without_a_status_filter_returns_newest_first_and_marks_read(pool: PgPool) {
    let st = state(pool.clone()).await;
    let mut ids = Vec::new();
    for i in 0..2 {
        let created = json_body(
            router(st.clone())
                .oneshot(post_report(
                    report_json(&format!("dddddddd-dddd-4ddd-8ddd-ddddddddd{:03}", i)),
                    "203.0.113.5",
                    "1.6.0",
                ))
                .await
                .unwrap(),
        )
        .await;
        ids.push(created["id"].as_str().unwrap().to_string());
    }
    // Both rows default received_at to now(); backdate the first so "newest first"
    // is a deterministic assertion rather than a race between two round-trips.
    sqlx::query("update reports set received_at = now() - interval '1 hour' where short_id = $1")
        .bind(&ids[0])
        .execute(&pool)
        .await
        .unwrap();

    let list = json_body(
        router(st.clone())
            .oneshot(admin("GET", "/v1/admin/reports", None, "test-admin-token"))
            .await
            .unwrap(),
    )
    .await;
    let arr = list.as_array().unwrap();
    assert_eq!(arr.len(), 2, "no filter means every row");
    assert_eq!(arr[0]["id"], ids[1], "newest first");
    assert_eq!(arr[1]["id"], ids[0]);

    for id in &ids {
        let seen = json_body(
            router(st.clone())
                .oneshot(status_req(id, CLIENT, "203.0.113.5"))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(seen[0]["status"], "read", "listing marks every row read");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_accepts_any_case_of_the_bearer_scheme(pool: PgPool) {
    let res = router(state(pool).await)
        .oneshot(
            Request::builder()
                .uri("/v1/admin/reports")
                .header("authorization", "bearer test-admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "RFC 7235 auth schemes are case-insensitive"
    );
}

#[test]
fn limiter_with_a_zero_limit_rejects_instead_of_panicking() {
    use gw2bo_feedback::ratelimit::RateLimiter;
    use std::time::Duration;
    let l = RateLimiter::new();
    // An empty window trivially satisfies `len >= 0`; indexing it would panic
    // inside the global Mutex and poison every later request.
    assert_eq!(l.check("k", 0, Duration::from_secs(60)), Err(60));
    assert_eq!(l.check("k", 0, Duration::from_secs(0)), Err(1));
    assert!(l.check("k", 1, Duration::from_secs(60)).is_ok());
}
