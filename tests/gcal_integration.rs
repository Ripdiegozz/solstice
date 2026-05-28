//! Integration tests for Google Calendar Sync (wiremock-backed).
//!
//! All tests are `#[ignore]` — run via:
//!   cargo test --features test-utils --test gcal_integration -- --ignored
//! Requires the `test-utils` feature to be enabled.
//!
//! Each test starts a per-test wiremock server and tokio runtime.

#![cfg(feature = "test-utils")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{NaiveDate, Utc};
use oauth2::basic::BasicTokenType;
use oauth2::{AccessToken, EmptyExtraTokenFields, RefreshToken, StandardTokenResponse};
use oauth2::TokenResponse;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request as WiremockRequest, ResponseTemplate};

use solstice::events::{EventSource, EventStore};
use solstice::gcal::{parse_event, GCalSync, StoredToken, TokenResponse_};

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Create a `GCalSync` configured to use the given wiremock server.
fn make_gcal_sync(server: &MockServer, token_path: PathBuf) -> GCalSync {
    let uri = server.uri();
    GCalSync::new_for_test(
        "test-client-id",
        "test-client-secret",
        reqwest::blocking::Client::new(),
        &format!("{}/token", uri),
        &format!("{}/device/code", uri),
        &uri,
        token_path,
        Box::new(|_| {}),
    )
    .unwrap()
}

/// Build a minimal `TokenResponse_` with optional refresh token.
fn make_token(access: &str, refresh: Option<&str>) -> TokenResponse_ {
    let mut token = StandardTokenResponse::new(
        AccessToken::new(access.to_string()),
        BasicTokenType::Bearer,
        EmptyExtraTokenFields {},
    );
    if let Some(rt) = refresh {
        token.set_refresh_token(Some(RefreshToken::new(rt.to_string())));
    }
    token.set_expires_in(Some(&Duration::from_secs(3600)));
    token
}

/// Create a temporary token path for a test.
fn test_token_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("solstice-wiremock-{}", name));
    std::fs::create_dir_all(&dir).ok();
    dir.join("gcal_token.json")
}

/// Register a device code mock on the server.
async fn mock_device_code(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "abc",
            "user_code": "DEF-123",
            "verification_uri": "https://example.com",
            "expires_in": 1800,
            "interval": 5,
        })))
        .mount(server)
        .await;
}

/// Register a token exchange (success) mock on the server.
async fn mock_token_exchange(server: &MockServer, access_token: &str, refresh_token: &str) {
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": access_token,
            "token_type": "Bearer",
            "expires_in": 3600,
            "refresh_token": refresh_token,
        })))
        .mount(server)
        .await;
}

/// Register a token refresh (success) mock on the server.
async fn mock_token_refresh(server: &MockServer, access_token: &str) {
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": access_token,
            "token_type": "Bearer",
            "expires_in": 3600,
        })))
        .mount(server)
        .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// T1–T6: OAuth2 Device Code Grant / Token Refresh
// ═══════════════════════════════════════════════════════════════════════════════

/// T1: Device code grant flow success.
#[ignore]
#[test]
fn t1_device_code_grant_success() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());

    rt.block_on(async {
        mock_device_code(&server).await;
        mock_token_exchange(&server, "at1", "rt1").await;
    });

    let token_path = test_token_path("t1");
    let gcal = make_gcal_sync(&server, token_path.clone());
    gcal.authenticate(Some(Duration::from_secs(10))).unwrap();

    assert!(token_path.exists(), "token file should be saved");
    let stored: StoredToken =
        serde_json::from_str(&std::fs::read_to_string(&token_path).unwrap()).unwrap();
    assert_eq!(stored.token.access_token().secret(), "at1");
    assert_eq!(stored.token.refresh_token().unwrap().secret(), "rt1");
    std::fs::remove_dir_all(token_path.parent().unwrap()).ok();
}

/// T2: Token polling — immediate success (no retry needed).
#[ignore]
#[test]
fn t2_token_polling_immediate_success() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());

    rt.block_on(async {
        mock_device_code(&server).await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "immediate_at",
                "token_type": "Bearer",
                "expires_in": 3600,
            })))
            .mount(&server)
            .await;
    });

    let token_path = test_token_path("t2");
    let gcal = make_gcal_sync(&server, token_path.clone());
    gcal.authenticate(Some(Duration::from_secs(5))).unwrap();
    assert!(token_path.exists());
    std::fs::remove_dir_all(token_path.parent().unwrap()).ok();
}

/// T3: Token polling — retries then succeeds (2× authorization_pending → success).
#[ignore]
#[test]
fn t3_token_polling_retry_then_success() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());
    let call_count = Arc::new(AtomicU32::new(0));
    let cc = call_count.clone();

    rt.block_on(async {
        mock_device_code(&server).await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(move |_req: &WiremockRequest| {
                let c = cc.fetch_add(1, Ordering::SeqCst);
                if c < 2 {
                    ResponseTemplate::new(400)
                        .set_body_json(json!({"error": "authorization_pending"}))
                } else {
                    ResponseTemplate::new(200).set_body_json(json!({
                        "access_token": "retried_at",
                        "token_type": "Bearer",
                        "expires_in": 3600,
                        "refresh_token": "rt_retried",
                    }))
                }
            })
            .mount(&server)
            .await;
    });

    let token_path = test_token_path("t3");
    let gcal = make_gcal_sync(&server, token_path.clone());
    gcal.authenticate(Some(Duration::from_secs(10))).unwrap();

    assert!(token_path.exists());
    let stored: StoredToken =
        serde_json::from_str(&std::fs::read_to_string(&token_path).unwrap()).unwrap();
    assert_eq!(stored.token.access_token().secret(), "retried_at");
    assert_eq!(stored.token.refresh_token().unwrap().secret(), "rt_retried");
    std::fs::remove_dir_all(token_path.parent().unwrap()).ok();
}

/// T4: Token polling — timeout (always authorization_pending + short timeout).
#[ignore]
#[test]
fn t4_token_polling_timeout() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());

    rt.block_on(async {
        mock_device_code(&server).await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({"error": "authorization_pending"})),
            )
            .mount(&server)
            .await;
    });

    let token_path = test_token_path("t4");
    let gcal = make_gcal_sync(&server, token_path.clone());
    let result = gcal.authenticate(Some(Duration::from_millis(50)));
    assert!(result.is_err(), "authenticate should fail on timeout");
    std::fs::remove_dir_all(token_path.parent().unwrap()).ok();
}

/// T5: Token refresh — success.
#[ignore]
#[test]
fn t5_token_refresh_success() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());

    rt.block_on(async {
        mock_token_refresh(&server, "new_at").await;
    });

    let token_path = test_token_path("t5");
    let gcal = make_gcal_sync(&server, token_path.clone());
    let old_token = make_token("old_at", Some("refresh_tok"));

    let new_token = gcal.refresh_token(&old_token).unwrap();
    assert_eq!(new_token.access_token().secret(), "new_at");
    std::fs::remove_dir_all(token_path.parent().unwrap()).ok();
}

/// T6: Token refresh — invalid_grant error.
#[ignore]
#[test]
fn t6_token_refresh_invalid_grant() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());

    rt.block_on(async {
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({"error": "invalid_grant"})),
            )
            .mount(&server)
            .await;
    });

    let token_path = test_token_path("t6");
    let gcal = make_gcal_sync(&server, token_path.clone());
    let old_token = make_token("old_at", Some("bad_refresh"));

    let result = gcal.refresh_token(&old_token);
    assert!(result.is_err(), "refresh_token should fail on invalid_grant");
    std::fs::remove_dir_all(token_path.parent().unwrap()).ok();
}

// ═══════════════════════════════════════════════════════════════════════════════
// T7–T11: Calendar API
// ═══════════════════════════════════════════════════════════════════════════════

/// T7: Fetch events — success.
#[ignore]
#[test]
fn t7_fetch_events_success() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());

    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/calendar/v3/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    {
                        "id": "e1",
                        "summary": "Event 1",
                        "start": {"dateTime": "2026-06-01T10:00:00Z"},
                        "end": {"dateTime": "2026-06-01T11:00:00Z"},
                    }
                ]
            })))
            .mount(&server)
            .await;
    });

    let token_path = test_token_path("t7");
    let gcal = make_gcal_sync(&server, token_path.clone());
    let mut token = make_token("valid_at", None);
    let t_min = Utc::now() - chrono::Duration::days(1);
    let t_max = Utc::now() + chrono::Duration::days(1);

    let events = gcal
        .fetch_events(&mut token, t_min, t_max, "UTC")
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].gcal_id, "e1");
    assert_eq!(events[0].title, "Event 1");
    std::fs::remove_dir_all(token_path.parent().unwrap()).ok();
}

/// T8: 401 retry then success (token refresh succeeds → retry → events).
#[ignore]
#[test]
fn t8_401_retry_then_success() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());
    let call_count = Arc::new(AtomicU32::new(0));
    let cc = call_count.clone();

    rt.block_on(async {
        // Calendar API: first call 401, second call 200
        Mock::given(method("GET"))
            .and(path("/calendar/v3/calendars/primary/events"))
            .respond_with(move |_req: &WiremockRequest| {
                let c = cc.fetch_add(1, Ordering::SeqCst);
                if c == 0 {
                    ResponseTemplate::new(401)
                } else {
                    ResponseTemplate::new(200).set_body_json(json!({
                        "items": [
                            {
                                "id": "e_retried",
                                "summary": "Retried Event",
                                "start": {"dateTime": "2026-06-01T10:00:00Z"},
                                "end": {"dateTime": "2026-06-01T11:00:00Z"},
                            }
                        ]
                    }))
                }
            })
            .mount(&server)
            .await;

        // Token refresh succeeds
        mock_token_refresh(&server, "refreshed_at").await;
    });

    let token_path = test_token_path("t8");
    let gcal = make_gcal_sync(&server, token_path.clone());
    let mut token = make_token("expired_at", Some("rt_for_refresh"));
    let t_min = Utc::now() - chrono::Duration::days(1);
    let t_max = Utc::now() + chrono::Duration::days(1);

    let events = gcal
        .fetch_events(&mut token, t_min, t_max, "UTC")
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].gcal_id, "e_retried");
    // Token should have been refreshed
    assert_eq!(token.access_token().secret(), "refreshed_at");
    std::fs::remove_dir_all(token_path.parent().unwrap()).ok();
}

/// T9: 401 then refresh fails (invalid_grant).
#[ignore]
#[test]
fn t9_401_then_refresh_fails() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());

    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/calendar/v3/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({"error": "invalid_grant"})),
            )
            .mount(&server)
            .await;
    });

    let token_path = test_token_path("t9");
    let gcal = make_gcal_sync(&server, token_path.clone());
    let mut token = make_token("expired_at", Some("bad_refresh"));
    let t_min = Utc::now() - chrono::Duration::days(1);
    let t_max = Utc::now() + chrono::Duration::days(1);

    let result = gcal.fetch_events(&mut token, t_min, t_max, "UTC");
    assert!(result.is_err(), "should fail when 401 + refresh fails");
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("401") || err.contains("refresh"),
        "error should mention 401 or refresh: {}",
        err
    );
    std::fs::remove_dir_all(token_path.parent().unwrap()).ok();
}

/// T10: Calendar API — malformed response.
#[ignore]
#[test]
fn t10_calendar_api_malformed_response() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());

    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/calendar/v3/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{invalid json}"))
            .mount(&server)
            .await;
    });

    let token_path = test_token_path("t10");
    let gcal = make_gcal_sync(&server, token_path.clone());
    let mut token = make_token("valid_at", None);
    let t_min = Utc::now() - chrono::Duration::days(1);
    let t_max = Utc::now() + chrono::Duration::days(1);

    let result = gcal.fetch_events(&mut token, t_min, t_max, "UTC");
    assert!(result.is_err(), "should fail on malformed JSON");
    std::fs::remove_dir_all(token_path.parent().unwrap()).ok();
}

/// T11: Calendar API — network error (connection refused).
#[ignore]
#[test]
fn t11_calendar_api_network_error() {
    // Use a non-routable address (port 1 will refuse connection)
    let gcal = GCalSync::new_for_test(
        "test-id",
        "test-secret",
        reqwest::blocking::Client::new(),
        "http://127.0.0.1:1/token",
        "http://127.0.0.1:1/device/code",
        "http://127.0.0.1:1",
        test_token_path("t11"),
        Box::new(|_| {}),
    )
    .unwrap();

    let mut token = make_token("at", None);
    let t_min = Utc::now() - chrono::Duration::days(1);
    let t_max = Utc::now() + chrono::Duration::days(1);

    let result = gcal.fetch_events(&mut token, t_min, t_max, "UTC");
    assert!(result.is_err(), "should fail on connection refused");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T12–T13: Event Parsing
// ═══════════════════════════════════════════════════════════════════════════════

/// T12: Parse event — all fields present.
#[ignore]
#[test]
fn t12_parse_event_all_fields() {
    let json_val = json!({
        "id": "full-event-1",
        "summary": "Full Event",
        "description": "A detailed description",
        "start": {"dateTime": "2026-06-01T10:00:00Z"},
        "end": {"dateTime": "2026-06-01T11:30:00Z"},
    });
    let event = parse_event(&json_val, "America/Bogota").unwrap();
    assert_eq!(event.gcal_id, "full-event-1");
    assert_eq!(event.title, "Full Event");
    assert_eq!(event.description.as_deref(), Some("A detailed description"));
    assert_eq!(event.date, NaiveDate::from_ymd_opt(2026, 6, 1).unwrap());
    // 10:00 UTC = 05:00 Colombia (UTC-5)
    assert_eq!(event.start_time.as_deref(), Some("05:00"));
    assert_eq!(event.end_time.as_deref(), Some("06:30"));
}

/// T13: Parse event — all-day event (date only, no time).
#[ignore]
#[test]
fn t13_parse_event_all_day() {
    let json_val = json!({
        "id": "allday-1",
        "summary": "Company Holiday",
        "start": {"date": "2026-12-25"},
        "end": {"date": "2026-12-25"},
    });
    let event = parse_event(&json_val, "America/Bogota").unwrap();
    assert_eq!(event.gcal_id, "allday-1");
    assert_eq!(event.title, "Company Holiday");
    assert_eq!(event.date, NaiveDate::from_ymd_opt(2026, 12, 25).unwrap());
    assert!(event.start_time.is_none(), "all-day events have no start_time");
    assert!(event.end_time.is_none(), "all-day events have no end_time");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T14: End-to-End Sync
// ═══════════════════════════════════════════════════════════════════════════════

/// T14: Full sync — device code → token → fetch → upsert → verify.
#[ignore]
#[test]
fn t14_end_to_end_sync() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());

    rt.block_on(async {
        mock_device_code(&server).await;
        mock_token_exchange(&server, "e2e_at", "e2e_rt").await;
        Mock::given(method("GET"))
            .and(path("/calendar/v3/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    {
                        "id": "e2e-1",
                        "summary": "E2E Event 1",
                        "start": {"dateTime": "2026-06-01T10:00:00Z"},
                        "end": {"dateTime": "2026-06-01T11:00:00Z"},
                    },
                    {
                        "id": "e2e-2",
                        "summary": "E2E Event 2",
                        "start": {"dateTime": "2026-06-01T14:00:00Z"},
                        "end": {"dateTime": "2026-06-01T15:00:00Z"},
                    },
                ]
            })))
            .mount(&server)
            .await;
    });

    let token_path = test_token_path("t14");
    let gcal = make_gcal_sync(&server, token_path.clone());

    // Step 1: Authenticate
    gcal.authenticate(Some(Duration::from_secs(10))).unwrap();
    assert!(token_path.exists());

    // Step 2: Load saved token
    let stored = gcal
        .load_token()
        .unwrap()
        .expect("token should be saved after authenticate");
    let mut token = stored.token;

    // Step 3: Fetch events
    let t_min = Utc::now() - chrono::Duration::days(30);
    let t_max = Utc::now() + chrono::Duration::days(90);
    let events = gcal
        .fetch_events(&mut token, t_min, t_max, "UTC")
        .unwrap();
    assert_eq!(events.len(), 2);

    // Step 4: Upsert into EventStore
    let store = EventStore::open_in_memory().unwrap();
    for event in &events {
        store.upsert_gcal_event(event).unwrap();
    }

    // Step 5: Verify events in store with source=GoogleCalendar
    let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
    let stored_events = store.events_for_date(date).unwrap();
    assert_eq!(stored_events.len(), 2, "should have 2 events in store");
    for e in &stored_events {
        assert_eq!(
            e.source,
            EventSource::GoogleCalendar,
            "event source should be GoogleCalendar"
        );
    }

    std::fs::remove_dir_all(token_path.parent().unwrap()).ok();
}
