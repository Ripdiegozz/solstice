use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Returns the config directory: ~/.config/solstice/
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .expect("Could not determine config directory")
        .join("solstice")
}

/// Returns the data directory: ~/.local/share/solstice/
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .expect("Could not determine data directory")
        .join("solstice")
}

/// Theme color configuration (Catppuccin Mocha defaults)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    /// Base background color (hex)
    #[serde(default = "default_base")]
    pub base: String,
    /// Surface color for panels
    #[serde(default = "default_surface0")]
    pub surface: String,
    /// Primary text color
    #[serde(default = "default_text")]
    pub text: String,
    /// Accent/highlight color (mauve)
    #[serde(default = "default_accent")]
    pub accent: String,
    /// Today highlight color (peach)
    #[serde(default = "default_today")]
    pub today: String,
    /// Holiday marker color (red)
    #[serde(default = "default_holiday")]
    pub holiday: String,
    /// Event dot color (green)
    #[serde(default = "default_event")]
    pub event: String,
    /// Secondary text / muted (overlay0)
    #[serde(default = "default_muted")]
    pub muted: String,
}

fn default_base() -> String { "#1e1e2e".into() }
fn default_surface0() -> String { "#313244".into() }
fn default_text() -> String { "#cdd6f4".into() }
fn default_accent() -> String { "#cba6f7".into() }
fn default_today() -> String { "#fab387".into() }
fn default_holiday() -> String { "#f38ba8".into() }
fn default_event() -> String { "#a6e3a1".into() }
fn default_muted() -> String { "#6c7086".into() }

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            base: default_base(),
            surface: default_surface0(),
            text: default_text(),
            accent: default_accent(),
            today: default_today(),
            holiday: default_holiday(),
            event: default_event(),
            muted: default_muted(),
        }
    }
}

/// First day of the week
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FirstDayOfWeek {
    #[default]
    Sunday,
    Monday,
}

/// Application configuration loaded from TOML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// IANA timezone (e.g., "America/Bogota")
    #[serde(default = "default_timezone")]
    pub timezone: String,

    /// ISO 3166-1 alpha-2 country code for holidays
    #[serde(default = "default_country")]
    pub country_code: String,

    /// Date format string (chrono format specifiers)
    #[serde(default = "default_date_format")]
    pub date_format: String,

    /// Time format string (chrono format specifiers)
    #[serde(default = "default_time_format")]
    pub time_format: String,

    /// First day of the week
    #[serde(default)]
    pub first_day_of_week: FirstDayOfWeek,

    /// Number of upcoming events to show
    #[serde(default = "default_upcoming_count")]
    pub upcoming_event_count: usize,

    /// Calendarific API key for online holiday fetching (optional)
    /// If set, holidays are fetched from calendarific.com with local caching.
    /// If not set, falls back to bundled JSON files in assets/holidays/.
    #[serde(default)]
    pub calendarific_api_key: Option<String>,

    /// Google OAuth2 Client ID for Calendar sync (optional)
    #[serde(default)]
    pub gcal_client_id: Option<String>,

    /// Google OAuth2 Client Secret for Calendar sync (optional)
    #[serde(default)]
    pub gcal_client_secret: Option<String>,

    /// Theme colors
    #[serde(default)]
    pub theme: ThemeColors,
}

fn default_timezone() -> String { "America/Bogota".into() }
fn default_country() -> String { "CO".into() }
fn default_date_format() -> String { "%A, %B %d, %Y".into() }
fn default_time_format() -> String { "%H:%M:%S".into() }
fn default_upcoming_count() -> usize { 5 }

impl Default for Config {
    fn default() -> Self {
        Self {
            timezone: default_timezone(),
            country_code: default_country(),
            date_format: default_date_format(),
            time_format: default_time_format(),
            first_day_of_week: FirstDayOfWeek::default(),
            upcoming_event_count: default_upcoming_count(),
            calendarific_api_key: None,
            gcal_client_id: None,
            gcal_client_secret: None,
            theme: ThemeColors::default(),
        }
    }
}

impl Config {
    /// Load config from ~/.config/solstice/config.toml
    /// Falls back to defaults if file doesn't exist.
    pub fn load() -> Result<Self> {
        let config_path = config_dir().join("config.toml");

        if !config_path.exists() {
            // Create default config file
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config at {:?}", config_path))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config at {:?}", config_path))?;

        Ok(config)
    }

    /// Save current config to disk
    pub fn save(&self) -> Result<()> {
        let dir = config_dir();
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config directory {:?}", dir))?;

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        let path = dir.join("config.toml");
        fs::write(&path, content)
            .with_context(|| format!("Failed to write config to {:?}", path))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_gcal_fields_default_to_none() {
        let config = Config::default();
        assert!(config.gcal_client_id.is_none(), "gcal_client_id should default to None");
        assert!(config.gcal_client_secret.is_none(), "gcal_client_secret should default to None");
    }

    #[test]
    fn test_config_gcal_fields_from_toml() {
        let toml_str = r#"
            timezone = "America/Bogota"
            country_code = "CO"
            gcal_client_id = "my-client-id.apps.googleusercontent.com"
            gcal_client_secret = "GOCSPX-secret-value"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.gcal_client_id.as_deref(), Some("my-client-id.apps.googleusercontent.com"));
        assert_eq!(config.gcal_client_secret.as_deref(), Some("GOCSPX-secret-value"));
    }

    #[test]
    fn test_config_gcal_fields_absent_from_toml() {
        let toml_str = r#"
            timezone = "America/Bogota"
            country_code = "CO"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.gcal_client_id.is_none());
        assert!(config.gcal_client_secret.is_none());
    }
}
