use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

/// A single holiday entry (shared across all providers)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Holiday {
    /// Date in "YYYY-MM-DD" format
    pub date: String,
    /// Holiday name
    pub name: String,
    /// Holiday type (e.g., "national", "local", "observance")
    #[serde(default = "default_holiday_type")]
    pub holiday_type: String,
}

fn default_holiday_type() -> String { "national".into() }

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

/// Trait for loading holidays from different sources
pub trait HolidayProvider {
    /// Load holidays for a given country and year.
    /// Returns a map of date -> holiday name.
    fn load(&self, country_code: &str, year: i32) -> Result<HashMap<NaiveDate, String>>;
}

// ---------------------------------------------------------------------------
// Local file provider (bundled JSON assets)
// ---------------------------------------------------------------------------

/// Loads holidays from JSON files bundled in assets/holidays/
pub struct LocalFileProvider {
    assets_dir: PathBuf,
}

impl Default for LocalFileProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalFileProvider {
    pub fn new() -> Self {
        let assets_dir = Self::find_assets_dir();
        Self { assets_dir }
    }

    fn find_assets_dir() -> PathBuf {
        let dev_path = PathBuf::from("assets/holidays");
        if dev_path.exists() {
            return dev_path;
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let exe_path = dir.join("assets/holidays");
                if exe_path.exists() {
                    return exe_path;
                }
            }
        }

        dev_path
    }
}

impl HolidayProvider for LocalFileProvider {
    fn load(&self, country_code: &str, year: i32) -> Result<HashMap<NaiveDate, String>> {
        let mut holidays = HashMap::new();
        let file_path = self.assets_dir.join(format!("{}.json", country_code.to_uppercase()));

        if !file_path.exists() {
            return Ok(holidays);
        }

        let content = fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read holiday file {:?}", file_path))?;

        let entries: Vec<Holiday> = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse holiday file {:?}", file_path))?;

        for entry in entries {
            if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
                if date.year() == year {
                    holidays.insert(date, entry.name);
                }
            }
        }

        Ok(holidays)
    }
}

// ---------------------------------------------------------------------------
// Calendarific API provider (with local caching)
// ---------------------------------------------------------------------------

const CALENDARIFIC_BASE_URL: &str = "https://calendarific.com/api/v2/holidays";
/// Cache TTL: 24 hours (free tier = 1000 req/day, one fetch per country+year)
const CACHE_TTL: Duration = Duration::from_secs(86_400);

/// Calendarific API response deserialization types
#[derive(Debug, Deserialize)]
struct CalendarificResponse {
    meta: CalendarificMeta,
    response: CalendarificData,
}

#[derive(Debug, Deserialize)]
struct CalendarificMeta {
    code: u16,
}

#[derive(Debug, Deserialize)]
struct CalendarificData {
    holidays: Vec<CalendarificHoliday>,
}

#[derive(Debug, Deserialize)]
struct CalendarificHoliday {
    name: String,
    date: CalendarificDate,
    #[serde(rename = "type", default)]
    holiday_type: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CalendarificDate {
    iso: String,
}

/// Fetches holidays from the Calendarific API with local file caching.
///
/// Cache location: ~/.local/share/solstice/holidays/{COUNTRY}_{YEAR}.json
/// Cache is valid for 24 hours to stay within the free tier (1000 req/day).
pub struct CalendarificProvider {
    api_key: String,
    cache_dir: PathBuf,
}

impl CalendarificProvider {
    pub fn new(api_key: String) -> Self {
        let cache_dir = crate::config::data_dir().join("holidays");
        Self { api_key, cache_dir }
    }

    /// Get the cache file path for a country+year combination
    fn cache_path(&self, country_code: &str, year: i32) -> PathBuf {
        self.cache_dir.join(format!("{}_{}.json", country_code.to_uppercase(), year))
    }

    /// Check if a cache file exists and is fresh (within TTL)
    fn cache_is_fresh(&self, path: &PathBuf) -> bool {
        if let Ok(metadata) = fs::metadata(path) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = SystemTime::now().duration_since(modified) {
                    return age < CACHE_TTL;
                }
            }
        }
        false
    }

    /// Read holidays from the local cache file
    fn read_cache(&self, path: &PathBuf, year: i32) -> Result<HashMap<NaiveDate, String>> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read cache {:?}", path))?;

        let entries: Vec<Holiday> = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse cache {:?}", path))?;

        let mut holidays = HashMap::new();
        for entry in entries {
            if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
                if date.year() == year {
                    holidays.insert(date, entry.name);
                }
            }
        }
        Ok(holidays)
    }

    /// Write holidays to the local cache file
    fn write_cache(&self, path: &PathBuf, holidays: &[Holiday]) -> Result<()> {
        fs::create_dir_all(&self.cache_dir)
            .with_context(|| format!("Failed to create cache dir {:?}", self.cache_dir))?;

        let content = serde_json::to_string_pretty(holidays)
            .context("Failed to serialize holidays for cache")?;

        fs::write(path, content)
            .with_context(|| format!("Failed to write cache {:?}", path))?;

        Ok(())
    }

    /// Fetch holidays from the Calendarific API
    fn fetch_from_api(&self, country_code: &str, year: i32) -> Result<Vec<Holiday>> {
        let url = format!(
            "{}?api_key={}&country={}&year={}&type=national,local",
            CALENDARIFIC_BASE_URL, self.api_key, country_code.to_uppercase(), year
        );

        let response = reqwest::blocking::get(&url)
            .with_context(|| format!("Failed to fetch holidays from Calendarific for {} {}", country_code, year))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            anyhow::bail!("Calendarific API rate limit reached (1000 req/day free tier). Using cached data if available.");
        }
        if !status.is_success() {
            anyhow::bail!("Calendarific API returned status {}: {}", status, response.text().unwrap_or_default());
        }

        let api_response: CalendarificResponse = response.json()
            .context("Failed to parse Calendarific API response")?;

        if api_response.meta.code != 200 {
            anyhow::bail!("Calendarific API error code: {}", api_response.meta.code);
        }

        let holidays: Vec<Holiday> = api_response.response.holidays.into_iter().map(|h| {
            let holiday_type = h.holiday_type.first()
                .cloned()
                .unwrap_or_else(|| "national".into());

            Holiday {
                date: h.date.iso,
                name: h.name,
                holiday_type,
            }
        }).collect();

        Ok(holidays)
    }
}

impl HolidayProvider for CalendarificProvider {
    fn load(&self, country_code: &str, year: i32) -> Result<HashMap<NaiveDate, String>> {
        let cache_path = self.cache_path(country_code, year);

        // Try fresh cache first
        if self.cache_is_fresh(&cache_path) {
            return self.read_cache(&cache_path, year);
        }

        // Fetch from API
        match self.fetch_from_api(country_code, year) {
            Ok(holidays) => {
                // Cache the result for next time
                let _ = self.write_cache(&cache_path, &holidays);

                let mut map = HashMap::new();
                for h in &holidays {
                    if let Ok(date) = NaiveDate::parse_from_str(&h.date, "%Y-%m-%d") {
                        if date.year() == year {
                            map.insert(date, h.name.clone());
                        }
                    }
                }
                Ok(map)
            }
            Err(api_err) => {
                // Fallback: try stale cache if API failed
                if cache_path.exists() {
                    eprintln!("Calendarific API error: {}. Using stale cache.", api_err);
                    return self.read_cache(&cache_path, year);
                }
                Err(api_err)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Provider factory
// ---------------------------------------------------------------------------

/// Create the appropriate holiday provider based on configuration.
///
/// - If `calendarific_api_key` is set → CalendarificProvider (API + cache)
/// - Otherwise → LocalFileProvider (bundled JSON files)
pub fn create_provider(api_key: Option<&str>) -> Box<dyn HolidayProvider> {
    match api_key {
        Some(key) if !key.is_empty() => {
            Box::new(CalendarificProvider::new(key.to_string()))
        }
        _ => {
            Box::new(LocalFileProvider::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_provider_missing_country() {
        let provider = LocalFileProvider::new();
        let result = provider.load("XX", 2026);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_create_provider_without_key() {
        let provider = create_provider(None);
        // Should return a LocalFileProvider — just verify it doesn't panic
        let result = provider.load("XX", 2026);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_with_empty_key() {
        let provider = create_provider(Some(""));
        let result = provider.load("XX", 2026);
        assert!(result.is_ok());
    }

    #[test]
    fn test_holiday_serialization() {
        let holiday = Holiday {
            date: "2026-12-25".into(),
            name: "Christmas".into(),
            holiday_type: "national".into(),
        };
        let json = serde_json::to_string(&holiday).unwrap();
        let parsed: Holiday = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Christmas");
        assert_eq!(parsed.date, "2026-12-25");
    }
}
