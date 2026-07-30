use super::{
    build_app,
    config::ServerConfig,
    state::{AppState, OrderRecord, OrderStatus},
    storage::PostgresStore,
};
use axum::{
    body::{to_bytes, Body},
    extract::ConnectInfo,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        Method, Request, StatusCode,
    },
    Router,
};
use postgres::{Client, NoTls};
use std::net::SocketAddr;
use time::OffsetDateTime;
use uuid::Uuid;
use serde_json::{json, Value};
use tower::ServiceExt;

fn base_config(database_url: Option<String>) -> ServerConfig {
    ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "http://127.0.0.1:8787".to_string(),
        issuer_id: "test-issuer".to_string(),
        issuer_key_b64: None,
        default_license_days: 30,
        payment_provider: "manual".to_string(),
        storage_mode: if database_url.is_some() {
            "postgres"
        } else {
            "memory"
        }
        .to_string(),
        database_url,
        provider_callback_secret: Some("test-callback-secret".to_string()),
        license_issue_secret: Some("test-issue-secret".to_string()),
        order_recovery_secret: Some("test-recovery-secret".to_string()),
        yookassa_shop_id: None,
        yookassa_secret_key: None,
        yookassa_api_base_url: "https://api.yookassa.ru".to_string(),
        global_concurrency_limit: 128,
        provider_concurrency_limit: 8,
        request_timeout_seconds: 30,
        order_create_limit_per_hour: 1000,
        order_access_limit_per_minute: 1000,
        provider_callback_limit_per_minute: 1000,
        order_recovery_limit_per_minute: 1000,
        trusted_proxies: crate::traffic_guard::TrustedProxyConfig::default(),
    }
}

async fn call(
    app: Router,
    method: Method,
    uri: String,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let request_body = match body {
        Some(value) => {
            builder = builder.header(CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .oneshot(builder.body(request_body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

async fn call_authorized(
    app: Router,
    method: Method,
    uri: String,
    body: Option<Value>,
    access_token: &str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {access_token}"));
    let request_body = match body {
        Some(value) => {
            builder = builder.header(CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .oneshot(builder.body(request_body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

async fn call_from_proxy(
    app: Router,
    peer: SocketAddr,
    forwarded_for: Option<&str>,
    body: Value,
) -> StatusCode {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/api/orders")
        .header(CONTENT_TYPE, "application/json")
        .extension(ConnectInfo(peer));
    if let Some(forwarded_for) = forwarded_for {
        builder = builder.header("x-forwarded-for", forwarded_for);
    }
    app.oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
        .status()
}

async fn create_paid_order(app: Router, machine_hash: &str) -> String {
    let (status, order) = call(
        app.clone(),
        Method::POST,
        "/api/orders".to_string(),
        Some(json!({ "plan": "doctor_pro", "amount_rub": 3900, "machine_hash": machine_hash })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let order_id = order["order_id"].as_str().unwrap().to_string();
    let event_id = format!("evt-{order_id}");
    let callback = json!({ "order_id": order_id, "provider_event_id": event_id, "provider_payment_id": "pay-1", "provider": "manual", "status": "succeeded", "amount_rub": 3900, "callback_secret": "test-callback-secret" });
    let (status, _) = call(
        app,
        Method::POST,
        "/api/provider/callback".to_string(),
        Some(callback),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    order_id
}

async fn assert_order_payment_activation_flow(
    app: Router,
    expected_backend: &str,
    expected_database_connected: bool,
) {
    let (status, health) = call(app.clone(), Method::GET, "/healthz".to_string(), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["storage_backend"], expected_backend);
    assert_eq!(health["database_connected"], expected_database_connected);

    let (status, order) = call(
        app.clone(),
        Method::POST,
        "/api/orders".to_string(),
        Some(json!({ "plan": "doctor_pro", "amount_rub": 3900, "machine_hash": "machine-a" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let order_id = order["order_id"].as_str().unwrap().to_string();
    let access_token = order["order_access_token"].as_str().unwrap().to_string();
    assert_eq!(order["status"], "waiting_payment");

    let event_id = format!("evt-{order_id}");
    let callback = json!({ "order_id": order_id, "provider_event_id": event_id, "provider_payment_id": "pay-1", "provider": "manual", "status": "succeeded", "amount_rub": 3900, "callback_secret": "test-callback-secret" });
    let (status, body) = call(
        app.clone(),
        Method::POST,
        "/api/provider/callback".to_string(),
        Some(callback),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["duplicate"], false);

    let duplicate = json!({ "order_id": order_id, "provider_event_id": event_id, "provider_payment_id": "pay-1-dup", "provider": "manual", "status": "succeeded", "amount_rub": 3900, "callback_secret": "test-callback-secret" });
    let (status, body) = call(
        app.clone(),
        Method::POST,
        "/api/provider/callback".to_string(),
        Some(duplicate),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["duplicate"], true);

    let (status, body) = call_authorized(
        app.clone(),
        Method::GET,
        format!("/api/orders/{order_id}/status"),
        None,
        &access_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "paid");

    let (status, body) = call_authorized(
        app.clone(),
        Method::POST,
        format!("/api/orders/{order_id}/activate-machine"),
        Some(json!({ "machine_hash": "machine-a" })),
        &access_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "paid");
}

#[tokio::test]
async fn legacy_order_access_recovery_requires_admin_secret_and_is_one_time() {
    let state = AppState::try_new(base_config(None)).unwrap();
    let order_id = Uuid::new_v4();
    state
        .store
        .create_order_async(OrderRecord {
            id: order_id,
            plan: "doctor_pro".to_string(),
            amount_rub: 3_900,
            status: OrderStatus::Paid,
            machine_hash: Some("legacy-machine".to_string()),
            access_token_hash: None,
            created_at: OffsetDateTime::now_utc(),
        })
        .await
        .unwrap();
    let app = build_app(state);
    let uri = format!("/api/admin/orders/{order_id}/recover-access");
    let request = Some(json!({
        "machine_hash": "legacy-machine",
        "bind_missing_machine": false,
    }));

    let (status, _) = call(app.clone(), Method::POST, uri.clone(), request.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call_authorized(
        app.clone(),
        Method::POST,
        uri.clone(),
        request.clone(),
        "wrong-recovery-secret",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, recovered) = call_authorized(
        app.clone(),
        Method::POST,
        uri.clone(),
        request.clone(),
        "test-recovery-secret",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let access_token = recovered["order_access_token"].as_str().unwrap();
    let (status, body) = call_authorized(
        app.clone(),
        Method::GET,
        format!("/api/orders/{order_id}/status"),
        None,
        access_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "paid");
    let (status, _) = call_authorized(
        app,
        Method::POST,
        uri,
        request,
        "test-recovery-secret",
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn trusted_proxy_rate_limits_are_keyed_by_resolved_client_address() {
    let mut config = base_config(None);
    config.order_create_limit_per_hour = 1;
    config.trusted_proxies = crate::traffic_guard::TrustedProxyConfig::parse(
        Some("127.0.0.1/32"),
        true,
    )
    .unwrap();
    let app = build_app(AppState::try_new(config).unwrap());
    let peer: SocketAddr = "127.0.0.1:41000".parse().unwrap();
    let order = json!({
        "plan": "doctor_pro",
        "amount_rub": 3900,
        "machine_hash": "machine-owner",
    });
    assert_eq!(
        call_from_proxy(app.clone(), peer, Some("198.51.100.10"), order.clone()).await,
        StatusCode::OK
    );
    assert_eq!(
        call_from_proxy(app.clone(), peer, Some("198.51.100.10"), order.clone()).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        call_from_proxy(app.clone(), peer, Some("198.51.100.11"), order.clone()).await,
        StatusCode::OK
    );
    assert_eq!(
        call_from_proxy(app, peer, None, order).await,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn order_status_and_activation_require_the_per_order_bearer_token() {
    let app = build_app(AppState::try_new(base_config(None)).unwrap());
    let (status, order) = call(
        app.clone(),
        Method::POST,
        "/api/orders".to_string(),
        Some(json!({ "plan": "doctor_pro", "amount_rub": 3900, "machine_hash": "machine-owner" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let order_id = order["order_id"].as_str().unwrap();
    let token = order["order_access_token"].as_str().unwrap();

    let (status, _) = call(
        app.clone(),
        Method::GET,
        format!("/api/orders/{order_id}/status"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = call_authorized(
        app.clone(),
        Method::GET,
        format!("/api/orders/{order_id}/status"),
        None,
        "wrong-token",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = call_authorized(
        app,
        Method::GET,
        format!("/api/orders/{order_id}/status"),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn first_activation_cannot_be_redirected_to_another_machine() {
    let app = build_app(AppState::try_new(base_config(None)).unwrap());
    let (status, order) = call(
        app.clone(),
        Method::POST,
        "/api/orders".to_string(),
        Some(json!({ "plan": "doctor_pro", "amount_rub": 3900, "machine_hash": "machine-owner" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let order_id = order["order_id"].as_str().unwrap().to_string();
    let token = order["order_access_token"].as_str().unwrap().to_string();
    let callback = json!({
        "order_id": order_id,
        "provider_event_id": format!("paid-{order_id}"),
        "provider_payment_id": "pay-owner",
        "provider": "manual",
        "status": "succeeded",
        "amount_rub": 3900,
        "callback_secret": "test-callback-secret"
    });
    assert_eq!(
        call(
            app.clone(),
            Method::POST,
            "/api/provider/callback".to_string(),
            Some(callback),
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, _) = call_authorized(
        app.clone(),
        Method::POST,
        format!("/api/orders/{order_id}/activate-machine"),
        Some(json!({ "machine_hash": "machine-attacker" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = call_authorized(
        app,
        Method::POST,
        format!("/api/orders/{order_id}/activate-machine"),
        Some(json!({ "machine_hash": "machine-owner" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn order_creation_rate_limit_is_enforced_without_touching_tests_or_prices() {
    let mut config = base_config(None);
    config.order_create_limit_per_hour = 1;
    let app = build_app(AppState::try_new(config).unwrap());
    let request = Some(json!({ "plan": "doctor_pro", "amount_rub": 3900, "machine_hash": "machine-a" }));
    assert_eq!(
        call(app.clone(), Method::POST, "/api/orders".to_string(), request.clone())
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(app, Method::POST, "/api/orders".to_string(), request)
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn memory_http_order_payment_activation_flow() {
    let app = build_app(AppState::try_new(base_config(None)).unwrap());
    assert_order_payment_activation_flow(app, "memory", false).await;
}

#[tokio::test]
async fn memory_readyz_is_not_ready_without_database() {
    let app = build_app(AppState::try_new(base_config(None)).unwrap());
    let (status, body) = call(app, Method::GET, "/readyz".to_string(), None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["storage_backend"], "memory");
}

#[tokio::test]
async fn order_rejects_client_side_price_forgery() {
    let app = build_app(AppState::try_new(base_config(None)).unwrap());
    let (status, _) = call(
        app,
        Method::POST,
        "/api/orders".to_string(),
        Some(json!({ "plan": "doctor_pro", "amount_rub": 1, "machine_hash": "machine-a" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unimplemented_external_provider_callback_is_rejected() {
    let app = build_app(AppState::try_new(base_config(None)).unwrap());
    let (status, order) = call(
        app.clone(),
        Method::POST,
        "/api/orders".to_string(),
        Some(json!({ "plan": "doctor_pro", "amount_rub": 3900, "machine_hash": "machine-a" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let order_id = order["order_id"].as_str().unwrap().to_string();
    let callback = json!({ "order_id": order_id, "provider_event_id": "fake-yoo", "provider_payment_id": "fake-pay", "provider": "yookassa", "status": "succeeded", "amount_rub": 3900, "callback_secret": "test-callback-secret" });
    let (status, _) = call(
        app,
        Method::POST,
        "/api/provider/callback".to_string(),
        Some(callback),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn license_issue_rejects_different_machine_hash() {
    let app = build_app(AppState::try_new(base_config(None)).unwrap());
    let order_id = create_paid_order(app.clone(), "machine-a").await;
    let (status, _) = call(
        app,
        Method::POST,
        format!("/api/orders/{order_id}/license"),
        Some(json!({ "machine_hash": "machine-b", "issue_token": "test-issue-secret" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn postgres_http_order_payment_activation_flow_when_database_url_is_present() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let app = tokio::task::spawn_blocking(move || {
        build_app(AppState::try_new(base_config(Some(database_url))).unwrap())
    })
    .await
    .unwrap();
    assert_order_payment_activation_flow(app.clone(), "postgres", true).await;
    std::mem::forget(app);
}

#[test]
fn postgres_runtime_migration_records_schema_version_when_database_url_is_present() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let store = PostgresStore::connect(&database_url).unwrap();
    assert_eq!(store.pool_size(), 4);
    let mut client = Client::connect(&database_url, NoTls).unwrap();
    for version in ["0001_license_schema", "0002_order_access_token"] {
        let row = client
            .query_one(
                "SELECT checksum FROM schema_migrations WHERE version = $1",
                &[&version],
            )
            .unwrap();
        let checksum: String = row.get(0);
        assert_eq!(checksum.len(), 64);
        assert!(checksum.chars().all(|value| value.is_ascii_hexdigit()));
    }
    let access_token_column: bool = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'license_orders' AND column_name = 'access_token_hash')",
            &[],
        )
        .unwrap()
        .get(0);
    assert!(access_token_column);
}
