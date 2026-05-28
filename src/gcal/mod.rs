//! Google Calendar sync module
//!
//! OAuth2 Device Authorization Grant (RFC 8628) for read-only Google Calendar
//! sync. Fetches events via Calendar API v3 with recurring event expansion
//! and upserts into the local SQLite EventStore.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use oauth2::basic::{BasicClient, BasicTokenType};
use oauth2::{
    AuthUrl, ClientId, ClientSecret, DeviceAuthorizationUrl, EmptyExtraTokenFields, Scope,
    StandardDeviceAuthorizationResponse, StandardTokenResponse, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Type alias for the token response we work with
pub type TokenResponse_ = StandardTokenResponse<EmptyExtraTokenFields, BasicTokenType>;

/// Wrapper that stores the OAuth2 token alongside the timestamp it was obtained,
/// enabling accurate expiry calculations. Without this, `expires_in()` only returns
/// the original TTL (e.g. 3600s) regardless of how much time has actually elapsed.
#[derive(Debug, Serialize, Deserialize)]
pub struct StoredToken {
    pub token: TokenResponse_,
    pub obtained_at: DateTime<Utc>,
}

impl StoredToken {
    /// Calculate the remaining time before the access token expires.
    /// Returns None if the token has no expiry information.
    pub fn remaining_time(&self) -> Option<std::time::Duration> {
        let expires_in = self.token.expires_in()?;
        let elapsed = Utc::now().signed_duration_since(self.obtained_at);
        let elapsed_secs = elapsed.num_seconds().max(0) as u64;
        let total_secs = expires_in.as_secs();

        if elapsed_secs >= total_secs {
            Some(std::time::Duration::ZERO)
        } else {
            Some(std::time::Duration::from_secs(total_secs - elapsed_secs))
        }
    }

    /// Check if the token is still fresh (more than `buffer_secs` remaining).
    pub fn is_fresh(&self, buffer_secs: u64) -> bool {
        match self.remaining_time() {
            Some(remaining) => remaining.as_secs() > buffer_secs,
            None => true, // No expiry info — assume valid
        }
    }
}

/// Intermediate type from Google Calendar API response before conversion to Event.
/// Represents a single calendar event as parsed from the API JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct GCalEvent {
    pub gcal_id: String,
    pub title: String,
    pub description: Option<String>,
    pub date: NaiveDate,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

/// Parse a single Google Calendar event JSON value into a GCalEvent.
///
/// Handles both all-day events (with `date` field) and timed events (with `dateTime` field).
/// Timed events are converted to the specified IANA timezone.
pub fn parse_event(value: &serde_json::Value, timezone: &str) -> Result<GCalEvent> {
    let gcal_id = value
        .get("id")
        .and_then(|v| v.as_str())
        .context("Event missing 'id' field")?
        .to_string();

    let title = value
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("(No title)")
        .to_string();

    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Parse start — either all-day (date) or timed (dateTime)
    let start = value.get("start").context("Event missing 'start' field")?;

    let (date, start_time, end_time) = if let Some(date_str) = start.get("date").and_then(|v| v.as_str()) {
        // All-day event: date is "YYYY-MM-DD"
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .with_context(|| format!("Invalid all-day date: {}", date_str))?;
        (date, None, None)
    } else if let Some(datetime_str) = start.get("dateTime").and_then(|v| v.as_str()) {
        // Timed event: dateTime is RFC3339
        let dt = chrono::DateTime::parse_from_rfc3339(datetime_str)
            .with_context(|| format!("Invalid dateTime: {}", datetime_str))?;

        // Convert to target timezone
        let tz: chrono_tz::Tz = timezone
            .parse()
            .with_context(|| format!("Invalid timezone: {}", timezone))?;
        let local_dt = dt.with_timezone(&tz);

        let date = local_dt.date_naive();
        let start_time = Some(local_dt.format("%H:%M").to_string());

        // Parse end time
        let end_time = value
            .get("end")
            .and_then(|e| e.get("dateTime"))
            .and_then(|v| v.as_str())
            .and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(s).ok().map(|dt| {
                    dt.with_timezone(&tz).format("%H:%M").to_string()
                })
            });

        (date, start_time, end_time)
    } else {
        anyhow::bail!("Event has neither 'date' nor 'dateTime' in start");
    };

    Ok(GCalEvent {
        gcal_id,
        title,
        description,
        date,
        start_time,
        end_time,
    })
}

/// Token storage path: ~/.config/solstice/gcal_token.json
pub fn token_path() -> PathBuf {
    crate::config::config_dir().join("gcal_token.json")
}

/// Google Calendar sync orchestrator.
/// Handles OAuth2 Device Authorization Grant, token management, and event fetching.
pub struct GCalSync {
    client: BasicClient<
        oauth2::EndpointSet,
        oauth2::EndpointSet,
        oauth2::EndpointNotSet,
        oauth2::EndpointNotSet,
        oauth2::EndpointSet,
    >,
    http: reqwest::blocking::Client,
    token_path: PathBuf,
    sleep_fn: Box<dyn Fn(Duration)>,
    calendar_api_base_url: String,
}

impl GCalSync {
    /// Create a new GCalSync instance with OAuth2 client configured for Google Calendar.
    ///
    /// # Arguments
    /// * `client_id` - Google OAuth2 Client ID
    /// * `client_secret` - Google OAuth2 Client Secret
    pub fn new(client_id: &str, client_secret: &str) -> Result<Self> {
        let client = BasicClient::new(ClientId::new(client_id.to_string()))
            .set_client_secret(ClientSecret::new(client_secret.to_string()))
            .set_auth_uri(
                AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())
                    .context("Invalid auth URL")?,
            )
            .set_token_uri(
                TokenUrl::new("https://oauth2.googleapis.com/token".to_string())
                    .context("Invalid token URL")?,
            )
            .set_device_authorization_url(
                DeviceAuthorizationUrl::new("https://oauth2.googleapis.com/device/code".to_string())
                    .context("Invalid device auth URL")?,
            );

        let http = reqwest::blocking::Client::new();
        let token_path = token_path();

        Ok(Self {
            client,
            http,
            token_path,
            sleep_fn: Box::new(std::thread::sleep),
            calendar_api_base_url: "https://www.googleapis.com".into(),
        })
    }

    /// Load token from disk. Returns None if file doesn't exist.
    /// Returns a `StoredToken` that includes the timestamp the token was obtained.
    pub fn load_token(&self) -> Result<Option<StoredToken>> {
        if !self.token_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&self.token_path)
            .with_context(|| format!("Failed to read token file: {:?}", self.token_path))?;

        // Try new format first (StoredToken with obtained_at)
        if let Ok(stored) = serde_json::from_str::<StoredToken>(&content) {
            return Ok(Some(stored));
        }

        // Fallback: legacy format (raw TokenResponse_ without timestamp)
        // Treat as freshly obtained so it gets used but will refresh on next run
        let token: TokenResponse_ = serde_json::from_str(&content)
            .context("Failed to deserialize token file")?;

        Ok(Some(StoredToken {
            token,
            obtained_at: Utc::now(),
        }))
    }

    /// Save token to disk with restrictive permissions (0600).
    /// Wraps the token in a `StoredToken` with the current timestamp.
    pub fn save_token(&self, token: &TokenResponse_) -> Result<()> {
        let stored = StoredToken {
            token: token.clone(),
            obtained_at: Utc::now(),
        };

        if let Some(parent) = self.token_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create token directory: {:?}", parent))?;
        }

        let content = serde_json::to_string_pretty(&stored)
            .context("Failed to serialize token")?;

        std::fs::write(&self.token_path, content)
            .with_context(|| format!("Failed to write token file: {:?}", self.token_path))?;

        // Set restrictive permissions on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.token_path, perms)
                .context("Failed to set token file permissions")?;
        }

        Ok(())
    }

    /// Authenticate via OAuth2 Device Authorization Grant.
    /// Prints verification URL and user code, then polls until authorized.
    ///
    /// `timeout` is the polling timeout passed to the OAuth2 client.
    /// Pass `None` for the default (5 minutes), or `Some(Duration)` for a custom timeout.
    pub fn authenticate(&self, timeout: Option<Duration>) -> Result<()> {
        let details: StandardDeviceAuthorizationResponse = self
            .client
            .exchange_device_code()
            .add_scope(Scope::new("https://www.googleapis.com/auth/calendar.readonly".to_string()))
            .request(&self.http)
            .context("Failed to request device code")?;

        println!(
            "\n🔐 Google Calendar Authentication Required\n\
             \n\
             Open this URL in your browser:\n\
             {}\n\
             \n\
             And enter this code: {}\n\
             \n\
             Waiting for authorization...",
            details.verification_uri(),
            details.user_code().secret()
        );

        let sleep_fn = &self.sleep_fn;
        let token = self
            .client
            .exchange_device_access_token(&details)
            .request(&self.http, sleep_fn, timeout)
            .context("Failed to exchange device code for token")?;

        self.save_token(&token)?;
        println!("✅ Authentication successful! Token saved.\n");

        Ok(())
    }

    /// Refresh an expired access token using the refresh token.
    pub fn refresh_token(&self, token: &TokenResponse_) -> Result<TokenResponse_> {
        let refresh_token = token
            .refresh_token()
            .context("No refresh token available")?;

        let new_token = self
            .client
            .exchange_refresh_token(refresh_token)
            .request(&self.http)
            .context("Failed to refresh token")?;

        Ok(new_token)
    }

    /// Fetch events from Google Calendar API v3 within the given time window.
    /// Uses singleEvents=true for recurring event expansion and paginates via nextPageToken.
    ///
    /// On 401 Unauthorized, automatically refreshes the token and retries once.
    pub fn fetch_events(
        &self,
        token: &mut TokenResponse_,
        time_min: DateTime<Utc>,
        time_max: DateTime<Utc>,
        timezone: &str,
    ) -> Result<Vec<GCalEvent>> {
        self.fetch_events_inner(token, time_min, time_max, timezone, true)
    }

    /// Inner fetch loop with optional 401 retry.
    fn fetch_events_inner(
        &self,
        token: &mut TokenResponse_,
        time_min: DateTime<Utc>,
        time_max: DateTime<Utc>,
        timezone: &str,
        allow_retry: bool,
    ) -> Result<Vec<GCalEvent>> {
        let mut all_events = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = format!(
                "{}/calendar/v3/calendars/primary/events\
                 ?singleEvents=true&orderBy=startTime&maxResults=2500\
                 &timeMin={}&timeMax={}",
                self.calendar_api_base_url,
                time_min.to_rfc3339(),
                time_max.to_rfc3339(),
            );

            if let Some(ref pt) = page_token {
                url.push_str(&format!("&pageToken={}", pt));
            }

            let response = self
                .http
                .get(&url)
                .bearer_auth(token.access_token().secret())
                .send()
                .context("Failed to fetch calendar events")?;

            let status = response.status();

            // Reactive 401 handling: refresh token and retry once
            if status == reqwest::StatusCode::UNAUTHORIZED && allow_retry {
                eprintln!("🔄 Access token expired (401), refreshing...");
                match self.refresh_token(token) {
                    Ok(new_token) => {
                        self.save_token(&new_token)?;
                        *token = new_token;
                        // Retry the entire fetch with the new token (no more retries)
                        return self.fetch_events_inner(token, time_min, time_max, timezone, false);
                    }
                    Err(e) => {
                        anyhow::bail!(
                            "Calendar API returned 401 and token refresh failed: {}. \
                             Please re-authenticate with `solstice sync`.",
                            e
                        );
                    }
                }
            }

            if !status.is_success() {
                let body = response.text().unwrap_or_default();
                anyhow::bail!("Calendar API error ({}): {}", status, body);
            }

            let body: serde_json::Value = response.json().context("Failed to parse API response")?;

            if let Some(items) = body.get("items").and_then(|v| v.as_array()) {
                for item in items {
                    match parse_event(item, timezone) {
                        Ok(event) => all_events.push(event),
                        Err(e) => eprintln!("⚠️  Skipping event: {}", e),
                    }
                }
            }

            // Check for next page
            page_token = body
                .get("nextPageToken")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if page_token.is_none() {
                break;
            }
        }

        Ok(all_events)
    }

    /// Load token from disk, refresh if expired, or authenticate from scratch.
    /// Returns a valid access token.
    ///
    /// Uses `StoredToken.obtained_at` to calculate actual remaining time,
    /// not the raw `expires_in()` which only returns the original TTL.
    pub fn load_or_refresh_token(&self) -> Result<TokenResponse_> {
        match self.load_token()? {
            Some(stored) => {
                // Check if token is fresh (more than 5 minutes remaining)
                if stored.is_fresh(300) {
                    return Ok(stored.token);
                }

                // Token expired or expiring soon — try refresh
                println!("🔄 Token expired or expiring soon, refreshing...");
                match self.refresh_token(&stored.token) {
                    Ok(new_token) => {
                        self.save_token(&new_token)?;
                        Ok(new_token)
                    }
                    Err(e) => {
                        eprintln!("⚠️  Token refresh failed: {}. Re-authenticating...", e);
                        // Refresh token revoked — re-authenticate from scratch
                        self.authenticate(None)?;
                        let stored = self.load_token()?
                            .context("Token file missing after authentication")?;
                        Ok(stored.token)
                    }
                }
            }
            None => {
                // No token file — authenticate from scratch
                self.authenticate(None)?;
                let stored = self.load_token()?
                    .context("Token file missing after authentication")?;
                Ok(stored.token)
            }
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl GCalSync {
    /// Create a GCalSync instance with full dependency injection for testing.
    ///
    /// # Arguments
    /// * `client_id` - Google OAuth2 Client ID
    /// * `client_secret` - Google OAuth2 Client Secret
    /// * `http_client` - Reqwest blocking client (e.g., configured for wiremock)
    /// * `oauth2_token_url` - OAuth2 token endpoint URL
    /// * `oauth2_device_url` - OAuth2 device authorization URL
    /// * `calendar_api_base_url` - Google Calendar API base URL
    /// * `token_path` - Path to store/load the OAuth2 token
    /// * `sleep_fn` - Sleep function for polling (use `|_| {}` to skip real sleeps)
    pub fn new_for_test(
        client_id: &str,
        client_secret: &str,
        http_client: reqwest::blocking::Client,
        oauth2_token_url: &str,
        oauth2_device_url: &str,
        calendar_api_base_url: &str,
        token_path: PathBuf,
        sleep_fn: Box<dyn Fn(Duration)>,
    ) -> Result<Self> {
        let client = BasicClient::new(ClientId::new(client_id.to_string()))
            .set_client_secret(ClientSecret::new(client_secret.to_string()))
            .set_auth_uri(
                AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())
                    .context("Invalid auth URL")?,
            )
            .set_token_uri(
                TokenUrl::new(oauth2_token_url.to_string())
                    .context("Invalid token URL")?,
            )
            .set_device_authorization_url(
                DeviceAuthorizationUrl::new(oauth2_device_url.to_string())
                    .context("Invalid device auth URL")?,
            );

        Ok(Self {
            client,
            http: http_client,
            token_path,
            sleep_fn,
            calendar_api_base_url: calendar_api_base_url.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use oauth2::basic::BasicTokenType;
    use oauth2::{AccessToken, EmptyExtraTokenFields, RefreshToken, StandardTokenResponse};
    use std::time::Duration;

    #[test]
    fn test_gcal_event_all_day() {
        let event = GCalEvent {
            gcal_id: "abc123".into(),
            title: "Team Offsite".into(),
            description: None,
            date: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
            start_time: None,
            end_time: None,
        };
        assert_eq!(event.gcal_id, "abc123");
        assert_eq!(event.title, "Team Offsite");
        assert!(event.description.is_none());
        assert_eq!(event.date, NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        assert!(event.start_time.is_none());
        assert!(event.end_time.is_none());
    }

    #[test]
    fn test_gcal_event_timed_with_description() {
        let event = GCalEvent {
            gcal_id: "xyz789".into(),
            title: "Sprint Planning".into(),
            description: Some("Q3 planning session".into()),
            date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            start_time: Some("09:00".into()),
            end_time: Some("10:30".into()),
        };
        assert_eq!(event.gcal_id, "xyz789");
        assert_eq!(event.description.as_deref(), Some("Q3 planning session"));
        assert_eq!(event.start_time.as_deref(), Some("09:00"));
        assert_eq!(event.end_time.as_deref(), Some("10:30"));
    }

    #[test]
    fn test_gcal_sync_new_configures_client() {
        let sync = GCalSync::new("test-client-id", "test-client-secret").unwrap();
        assert_eq!(sync.token_path, token_path());
    }

    #[test]
    fn test_token_save_and_load_roundtrip() {
        let tmp_dir = std::env::temp_dir().join(format!("solstice-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let token_file = tmp_dir.join("gcal_token.json");

        let mut sync = GCalSync::new("test-id", "test-secret").unwrap();
        sync.token_path = token_file.clone();

        let mut token = StandardTokenResponse::new(
            AccessToken::new("test-access-token".to_string()),
            BasicTokenType::Bearer,
            EmptyExtraTokenFields {},
        );
        token.set_refresh_token(Some(RefreshToken::new("test-refresh-token".to_string())));
        token.set_expires_in(Some(&Duration::from_secs(3600)));

        // Save
        sync.save_token(&token).unwrap();
        assert!(token_file.exists());

        // Load — now returns StoredToken
        let stored = sync.load_token().unwrap().expect("Token should exist");
        assert_eq!(stored.token.access_token().secret(), "test-access-token");
        assert_eq!(stored.token.refresh_token().unwrap().secret(), "test-refresh-token");
        // obtained_at should be very recent (within last 2 seconds)
        let elapsed = Utc::now().signed_duration_since(stored.obtained_at);
        assert!(elapsed.num_seconds() < 2, "obtained_at should be recent");

        // Cleanup
        std::fs::remove_dir_all(&tmp_dir).ok();
    }

    #[test]
    fn test_token_load_missing_file_returns_none() {
        let tmp_dir = std::env::temp_dir().join(format!("solstice-test-missing-{}", std::process::id()));
        let token_file = tmp_dir.join("nonexistent_token.json");

        let mut sync = GCalSync::new("test-id", "test-secret").unwrap();
        sync.token_path = token_file;

        let result = sync.load_token().unwrap();
        assert!(result.is_none(), "Missing token file should return None");
    }

    #[test]
    fn test_stored_token_remaining_time_fresh() {
        let mut token = StandardTokenResponse::new(
            AccessToken::new("test".to_string()),
            BasicTokenType::Bearer,
            EmptyExtraTokenFields {},
        );
        token.set_expires_in(Some(&Duration::from_secs(3600)));

        let stored = StoredToken {
            token,
            obtained_at: Utc::now(), // Just obtained
        };

        let remaining = stored.remaining_time().unwrap();
        // Should be close to 3600 seconds (within 2s tolerance)
        assert!(remaining.as_secs() > 3598, "Fresh token should have ~3600s remaining, got {}", remaining.as_secs());
        assert!(stored.is_fresh(300), "Fresh token should be fresh with 300s buffer");
    }

    #[test]
    fn test_stored_token_remaining_time_expired() {
        let mut token = StandardTokenResponse::new(
            AccessToken::new("test".to_string()),
            BasicTokenType::Bearer,
            EmptyExtraTokenFields {},
        );
        token.set_expires_in(Some(&Duration::from_secs(3600)));

        let stored = StoredToken {
            token,
            obtained_at: Utc::now() - chrono::Duration::seconds(7200), // Obtained 2 hours ago
        };

        let remaining = stored.remaining_time().unwrap();
        assert_eq!(remaining.as_secs(), 0, "Expired token should have 0 remaining");
        assert!(!stored.is_fresh(300), "Expired token should not be fresh");
    }

    #[test]
    fn test_stored_token_remaining_time_no_expiry() {
        let token = StandardTokenResponse::new(
            AccessToken::new("test".to_string()),
            BasicTokenType::Bearer,
            EmptyExtraTokenFields {},
        );
        // No expires_in set

        let stored = StoredToken {
            token,
            obtained_at: Utc::now(),
        };

        assert!(stored.remaining_time().is_none(), "No expiry info should return None");
        assert!(stored.is_fresh(300), "No expiry info should assume valid");
    }

    #[test]
    fn test_token_load_legacy_format_fallback() {
        let tmp_dir = std::env::temp_dir().join(format!("solstice-test-legacy-{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let token_file = tmp_dir.join("gcal_token.json");

        // Write a legacy format token (raw TokenResponse_ without obtained_at)
        let mut token = StandardTokenResponse::new(
            AccessToken::new("legacy-access-token".to_string()),
            BasicTokenType::Bearer,
            EmptyExtraTokenFields {},
        );
        token.set_refresh_token(Some(RefreshToken::new("legacy-refresh".to_string())));
        token.set_expires_in(Some(&Duration::from_secs(3600)));

        let legacy_json = serde_json::to_string_pretty(&token).unwrap();
        std::fs::write(&token_file, &legacy_json).unwrap();

        let mut sync = GCalSync::new("test-id", "test-secret").unwrap();
        sync.token_path = token_file.clone();

        // Should load legacy format and wrap in StoredToken with current timestamp
        let stored = sync.load_token().unwrap().expect("Legacy token should load");
        assert_eq!(stored.token.access_token().secret(), "legacy-access-token");
        assert_eq!(stored.token.refresh_token().unwrap().secret(), "legacy-refresh");
        // obtained_at should be now (fallback)
        let elapsed = Utc::now().signed_duration_since(stored.obtained_at);
        assert!(elapsed.num_seconds() < 2, "Legacy fallback should set obtained_at to now");

        // Cleanup
        std::fs::remove_dir_all(&tmp_dir).ok();
    }

    #[test]
    fn test_parse_event_all_day() {
        let json = serde_json::json!({
            "id": "event-allday-1",
            "summary": "Company Holiday",
            "description": "Office closed",
            "start": {
                "date": "2026-12-25"
            },
            "end": {
                "date": "2026-12-25"
            }
        });

        let event = parse_event(&json, "America/Bogota").unwrap();
        assert_eq!(event.gcal_id, "event-allday-1");
        assert_eq!(event.title, "Company Holiday");
        assert_eq!(event.description.as_deref(), Some("Office closed"));
        assert_eq!(event.date, NaiveDate::from_ymd_opt(2026, 12, 25).unwrap());
        assert!(event.start_time.is_none(), "All-day events should have no start_time");
        assert!(event.end_time.is_none(), "All-day events should have no end_time");
    }

    #[test]
    fn test_parse_event_timed_with_timezone() {
        let json = serde_json::json!({
            "id": "event-timed-1",
            "summary": "Team Standup",
            "start": {
                "dateTime": "2026-06-15T14:00:00Z",
                "timeZone": "UTC"
            },
            "end": {
                "dateTime": "2026-06-15T14:30:00Z",
                "timeZone": "UTC"
            }
        });

        // UTC 14:00 → America/Bogota (UTC-5) = 09:00
        let event = parse_event(&json, "America/Bogota").unwrap();
        assert_eq!(event.gcal_id, "event-timed-1");
        assert_eq!(event.title, "Team Standup");
        assert_eq!(event.date, NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        assert_eq!(event.start_time.as_deref(), Some("09:00"));
        assert_eq!(event.end_time.as_deref(), Some("09:30"));
    }

    #[test]
    fn test_parse_event_missing_description() {
        let json = serde_json::json!({
            "id": "event-no-desc",
            "summary": "Quick Sync",
            "start": {
                "date": "2026-08-01"
            }
        });

        let event = parse_event(&json, "America/Bogota").unwrap();
        assert_eq!(event.gcal_id, "event-no-desc");
        assert!(event.description.is_none());
    }

    #[test]
    fn test_parse_event_missing_summary_uses_default() {
        let json = serde_json::json!({
            "id": "event-no-title",
            "start": {
                "date": "2026-08-01"
            }
        });

        let event = parse_event(&json, "America/Bogota").unwrap();
        assert_eq!(event.title, "(No title)");
    }

    #[test]
    fn test_parse_event_missing_id_fails() {
        let json = serde_json::json!({
            "summary": "No ID event",
            "start": {
                "date": "2026-08-01"
            }
        });

        let result = parse_event(&json, "America/Bogota");
        assert!(result.is_err(), "Should fail when id is missing");
    }

    #[test]
    fn test_parse_event_timed_different_timezone() {
        // Event at 10:00 in US Eastern (UTC-4 in summer)
        let json = serde_json::json!({
            "id": "event-tz-2",
            "summary": "NY Meeting",
            "start": {
                "dateTime": "2026-07-01T10:00:00-04:00",
                "timeZone": "America/New_York"
            },
            "end": {
                "dateTime": "2026-07-01T11:00:00-04:00",
                "timeZone": "America/New_York"
            }
        });

        // 10:00 EDT (UTC-4) = 14:00 UTC = 09:00 Colombia (UTC-5)
        let event = parse_event(&json, "America/Bogota").unwrap();
        assert_eq!(event.start_time.as_deref(), Some("09:00"));
        assert_eq!(event.end_time.as_deref(), Some("10:00"));
    }
}
