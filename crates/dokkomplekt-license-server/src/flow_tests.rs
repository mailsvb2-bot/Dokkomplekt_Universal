use super::{build_app, config::ServerConfig, state::AppState};
use axum::{
    body::{to_bytes, Body},
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        Method, Request, StatusCode,
    },
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use tower::ServiceExt;

fn config(database_url: Option<String>) -> ServerConfig {
    ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "http://127.0.0.1:8787".to_string(),
        issuer_id: "test-issuer".to_string(),
        issuer_key_b64: Some(STANDARD.encode([0u8; 32])),
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

async fn emulate(
    app: Router,
    storage_backend: &str,
    database_connected: bool,
    ready_status: StatusCode,
) {
    let (status, health) = call(app.clone(), Method::GET, "/healthz".to_string(), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["storage_backend"], storage_backend);
    assert_eq!(health["database_connected"], database_connected);

    let (status, ready) = call(app.clone(), Method::GET, "/readyz".to_string(), None).await;
    assert_eq!(status, ready_status);
    assert_eq!(ready["storage_backend"], storage_backend);

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
    let (status, payment) = call(
        app.clone(),
        Method::POST,
        "/api/provider/callback".to_string(),
        Some(callback),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(payment["duplicate"], false);

    let duplicate = json!({ "order_id": order_id, "provider_event_id": event_id, "provider_payment_id": "pay-1-dup", "provider": "manual", "status": "succeeded", "amount_rub": 3900, "callback_secret": "test-callback-secret" });
    let (status, payment) = call(
        app.clone(),
        Method::POST,
        "/api/provider/callback".to_string(),
        Some(duplicate),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(payment["duplicate"], true);

    let (status, state) = call_authorized(
        app.clone(),
        Method::GET,
        format!("/api/orders/{order_id}/status"),
        None,
        &access_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(state["status"], "paid");

    let (status, activation) = call_authorized(
        app.clone(),
        Method::POST,
        format!("/api/orders/{order_id}/activate-machine"),
        Some(json!({ "machine_hash": "machine-a" })),
        &access_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(activation["status"], "paid");
    let first_activation_id = activation["activation_id"].as_str().unwrap().to_string();

    let (status, repeated_activation) = call_authorized(
        app.clone(),
        Method::POST,
        format!("/api/orders/{order_id}/activate-machine"),
        Some(json!({ "machine_hash": "machine-a" })),
        &access_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated_activation["activation_id"], first_activation_id);
    assert_eq!(repeated_activation["message"], "already_activated");

    let (status, second_activation) = call_authorized(
        app.clone(),
        Method::POST,
        format!("/api/orders/{order_id}/activate-machine"),
        Some(json!({ "machine_hash": "machine-b" })),
        &access_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(second_activation["activation_id"], first_activation_id);

    let issue_body = json!({ "owner_name": "User", "organization_name": "Org", "machine_hash": "machine-b", "issue_token": "test-issue-secret" });
    let (status, license) = call(
        app.clone(),
        Method::POST,
        format!("/api/orders/{order_id}/license"),
        Some(issue_body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let license_id = license["license"]["payload"]["license_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(license_id.starts_with("DKK-"));
    assert_eq!(license["license"]["payload"]["order_id"], order_id);
    assert_eq!(
        license["license"]["payload"]["allowed_machines"],
        json!(["machine-a", "machine-b"])
    );

    let (status, state) = call_authorized(
        app.clone(),
        Method::GET,
        format!("/api/orders/{order_id}/status"),
        None,
        &access_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(state["status"], "license_issued");

    let (status, repeated) = call(
        app.clone(),
        Method::POST,
        format!("/api/orders/{order_id}/license"),
        Some(issue_body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated["license"]["payload"]["license_id"], license_id);
}

#[tokio::test]
async fn memory_flow() {
    emulate(
        build_app(AppState::try_new(config(None)).unwrap()),
        "memory",
        false,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
}

#[tokio::test]
async fn postgres_flow_when_database_url_is_present() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let app = tokio::task::spawn_blocking(move || {
        build_app(AppState::try_new(config(Some(database_url))).unwrap())
    })
    .await
    .unwrap();
    emulate(app.clone(), "postgres", true, StatusCode::OK).await;
    std::mem::forget(app);
}
