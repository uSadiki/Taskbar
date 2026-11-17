//! Weather from Open-Meteo (no API key). The location comes from a city
//! search in Settings. Refreshes every 30 minutes; the last answer is cached
//! on disk for startup and for when the network is down.

use std::sync::OnceLock;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::net;
use crate::settings::{SettingsStore, WeatherLocation};

const REFRESH: Duration = Duration::from_secs(30 * 60);
const CACHE: &str = "weather";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Current {
    pub temperature: f64,
    pub feels_like: f64,
    pub humidity: f64,
    /// Millimetres in the current hour.
    pub precipitation: f64,
    /// WMO weather code.
    pub code: u32,
    /// Metres per second, and the direction it blows from in degrees.
    pub wind_speed: f64,
    pub wind_direction: f64,
    pub is_day: bool,
    pub uv_index: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hour {
    /// Local time at the location, `YYYY-MM-DDTHH:MM`.
    pub time: String,
    pub temperature: f64,
    pub code: u32,
    /// Chance of precipitation, percent.
    pub precipitation_chance: f64,
    pub is_day: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub code: u32,
    pub high: f64,
    pub low: f64,
    pub precipitation_chance: f64,
    pub sunrise: String,
    pub sunset: String,
    pub uv_index_max: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forecast {
    pub location: String,
    pub fahrenheit: bool,
    pub current: Current,
    pub hours: Vec<Hour>,
    pub days: Vec<Day>,
    /// Unix seconds when this was fetched.
    pub fetched_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherState {
    /// False until a city is chosen in Settings.
    pub configured: bool,
    pub forecast: Option<Forecast>,
    /// Why the last refresh failed, if it did. The forecast, if any, is then
    /// the last good one.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub region: Option<String>,
    pub country: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
}

fn number(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

fn column<'a>(block: &'a Value, name: &str) -> &'a [Value] {
    block[name].as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// Turn Open-Meteo's forecast response into a `Forecast`. Returns `None` if
/// the response has no current conditions.
pub fn parse_forecast(json: &Value, location: &str, fahrenheit: bool, now: u64) -> Option<Forecast> {
    let c = json.get("current")?;
    let current = Current {
        temperature: c.get("temperature_2m")?.as_f64()?,
        feels_like: number(&c["apparent_temperature"]),
        humidity: number(&c["relative_humidity_2m"]),
        precipitation: number(&c["precipitation"]),
        code: c["weather_code"].as_u64().unwrap_or(0) as u32,
        wind_speed: number(&c["wind_speed_10m"]),
        wind_direction: number(&c["wind_direction_10m"]),
        is_day: c["is_day"].as_u64().unwrap_or(1) == 1,
        uv_index: number(&c["uv_index"]),
    };

    // Each block is a set of parallel columns; rows are zipped by index.
    let h = &json["hourly"];
    let hours = column(h, "time")
        .iter()
        .enumerate()
        .filter_map(|(i, time)| {
            Some(Hour {
                time: time.as_str()?.to_string(),
                temperature: column(h, "temperature_2m").get(i)?.as_f64()?,
                code: column(h, "weather_code").get(i).and_then(Value::as_u64).unwrap_or(0) as u32,
                precipitation_chance: column(h, "precipitation_probability")
                    .get(i)
                    .map(number)
                    .unwrap_or(0.0),
                is_day: column(h, "is_day").get(i).and_then(Value::as_u64).unwrap_or(1) == 1,
            })
        })
        .collect();

    let d = &json["daily"];
    let text = |name: &str, i: usize| {
        column(d, name).get(i).and_then(Value::as_str).unwrap_or_default().to_string()
    };
    let days = column(d, "time")
        .iter()
        .enumerate()
        .filter_map(|(i, date)| {
            Some(Day {
                date: date.as_str()?.to_string(),
                code: column(d, "weather_code").get(i).and_then(Value::as_u64).unwrap_or(0) as u32,
                high: column(d, "temperature_2m_max").get(i)?.as_f64()?,
                low: column(d, "temperature_2m_min").get(i)?.as_f64()?,
                precipitation_chance: column(d, "precipitation_probability_max")
                    .get(i)
                    .map(number)
                    .unwrap_or(0.0),
                sunrise: text("sunrise", i),
                sunset: text("sunset", i),
                uv_index_max: column(d, "uv_index_max").get(i).map(number).unwrap_or(0.0),
            })
        })
        .collect();

    Some(Forecast { location: location.to_string(), fahrenheit, current, hours, days, fetched_at: now })
}

/// Turn Open-Meteo's geocoding response into places.
pub fn parse_places(json: &Value) -> Vec<Place> {
    json["results"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|r| {
            Some(Place {
                name: r["name"].as_str()?.to_string(),
                region: r["admin1"].as_str().map(str::to_string),
                country: r["country"].as_str().map(str::to_string),
                latitude: r["latitude"].as_f64()?,
                longitude: r["longitude"].as_f64()?,
            })
        })
        .collect()
}

async fn fetch(location: &WeatherLocation, fahrenheit: bool) -> Result<Forecast, String> {
    let mut query = vec![
        ("latitude", location.latitude.to_string()),
        ("longitude", location.longitude.to_string()),
        (
            "current",
            "temperature_2m,apparent_temperature,relative_humidity_2m,precipitation,weather_code,\
             wind_speed_10m,wind_direction_10m,is_day,uv_index"
                .into(),
        ),
        ("hourly", "temperature_2m,weather_code,precipitation_probability,is_day".into()),
        (
            "daily",
            "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,\
             sunrise,sunset,uv_index_max"
                .into(),
        ),
        ("timezone", "auto".into()),
        ("forecast_days", "7".into()),
        ("forecast_hours", "24".into()),
        ("wind_speed_unit", "ms".into()),
    ];
    if fahrenheit {
        query.push(("temperature_unit", "fahrenheit".into()));
    }
    let json: Value = net::client()
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&query)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| net::describe(&e))?
        .json()
        .await
        .map_err(|e| net::describe(&e))?;
    parse_forecast(&json, &location.name, fahrenheit, net::now_unix())
        .ok_or_else(|| "The forecast could not be read".to_string())
}

/// City search for Settings.
pub async fn search(query: String) -> Result<Vec<Place>, String> {
    let query = query.trim().to_string();
    if query.len() < 2 {
        return Ok(Vec::new());
    }
    let json: Value = net::client()
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[("name", query.as_str()), ("count", "8"), ("language", "en"), ("format", "json")])
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| net::describe(&e))?
        .json()
        .await
        .map_err(|e| net::describe(&e))?;
    Ok(parse_places(&json))
}

/// Approximate location from the public IP address. Only runs when the user
/// asks for it in Settings: it sends the IP address to ipwho.is.
pub async fn locate() -> Result<Place, String> {
    let json: Value = net::client()
        .get("https://ipwho.is/")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| net::describe(&e))?
        .json()
        .await
        .map_err(|e| net::describe(&e))?;
    if json["success"].as_bool() == Some(false) {
        return Err("The location service could not place this connection".into());
    }
    Ok(Place {
        name: json["city"].as_str().ok_or("The location service gave no city")?.to_string(),
        region: json["region"].as_str().map(str::to_string),
        country: json["country"].as_str().map(str::to_string),
        latitude: json["latitude"].as_f64().ok_or("The location service gave no position")?,
        longitude: json["longitude"].as_f64().ok_or("The location service gave no position")?,
    })
}

static STATE: Mutex<Option<WeatherState>> = Mutex::new(None);
static WAKE: OnceLock<Notify> = OnceLock::new();

pub fn state() -> WeatherState {
    STATE.lock().clone().unwrap_or_default()
}

fn publish(app: &AppHandle, next: WeatherState) {
    let changed = STATE.lock().replace(next.clone()).as_ref() != Some(&next);
    if changed {
        let _ = app.emit("weather://changed", next);
    }
}

/// Refresh now: after a settings change, or when the user asks.
pub fn refresh() {
    if let Some(wake) = WAKE.get() {
        wake.notify_one();
    }
}

async fn refresh_once(app: &AppHandle) {
    let settings = app.state::<SettingsStore>().get().weather;
    let Some(location) = settings.location else {
        publish(app, WeatherState::default());
        return;
    };
    // Keep showing what we have (or the cached copy) while fetching.
    let previous = state().forecast.or_else(|| net::cache_read::<Forecast>(CACHE)).filter(|f| {
        f.location == location.name && f.fahrenheit == settings.fahrenheit
    });
    match fetch(&location, settings.fahrenheit).await {
        Ok(forecast) => {
            net::cache_write(CACHE, &forecast);
            publish(app, WeatherState { configured: true, forecast: Some(forecast), error: None });
        }
        Err(error) => {
            log::warn!("weather: {error}");
            publish(app, WeatherState { configured: true, forecast: previous, error: Some(error) });
        }
    }
}

pub fn start(app: &AppHandle) {
    let wake = WAKE.get_or_init(Notify::new);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            refresh_once(&app).await;
            tokio::select! {
                () = tokio::time::sleep(REFRESH) => {}
                () = wake.notified() => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "current": {
                "time": "2026-11-17T01:15", "temperature_2m": 3.4, "apparent_temperature": -0.6,
                "relative_humidity_2m": 87, "precipitation": 0.2, "weather_code": 61,
                "wind_speed_10m": 4.1, "wind_direction_10m": 230, "is_day": 0, "uv_index": 0.0
            },
            "hourly": {
                "time": ["2026-11-17T01:00", "2026-11-17T02:00", "2026-11-17T03:00"],
                "temperature_2m": [3.4, 3.1, 2.8],
                "weather_code": [61, 3, 71],
                "precipitation_probability": [80, 35, null],
                "is_day": [0, 0, 0]
            },
            "daily": {
                "time": ["2026-11-17", "2026-11-18"],
                "weather_code": [61, 3],
                "temperature_2m_max": [5.2, 4.0],
                "temperature_2m_min": [1.1, -0.5],
                "precipitation_probability_max": [85, 20],
                "sunrise": ["2026-11-17T08:21", "2026-11-17T08:23"],
                "sunset": ["2026-11-17T15:42", "2026-11-17T15:40"],
                "uv_index_max": [0.4, 0.5]
            }
        })
    }

    #[test]
    fn parses_current_conditions() {
        let forecast = parse_forecast(&sample(), "Oslo", false, 1000).unwrap();
        assert_eq!(forecast.location, "Oslo");
        assert_eq!(forecast.fetched_at, 1000);
        let c = &forecast.current;
        assert_eq!((c.temperature, c.feels_like, c.humidity), (3.4, -0.6, 87.0));
        assert_eq!((c.code, c.wind_speed, c.wind_direction), (61, 4.1, 230.0));
        assert!(!c.is_day);
    }

    #[test]
    fn zips_hourly_and_daily_columns() {
        let forecast = parse_forecast(&sample(), "Oslo", false, 0).unwrap();
        assert_eq!(forecast.hours.len(), 3);
        assert_eq!(forecast.hours[1].time, "2026-11-17T02:00");
        assert_eq!((forecast.hours[1].temperature, forecast.hours[1].code), (3.1, 3));
        // A missing probability reads as zero rather than dropping the hour.
        assert_eq!(forecast.hours[2].precipitation_chance, 0.0);

        assert_eq!(forecast.days.len(), 2);
        let day = &forecast.days[0];
        assert_eq!((day.high, day.low, day.precipitation_chance), (5.2, 1.1, 85.0));
        assert_eq!((day.sunrise.as_str(), day.sunset.as_str()), ("2026-11-17T08:21", "2026-11-17T15:42"));
    }

    #[test]
    fn a_response_without_current_conditions_is_rejected() {
        assert!(parse_forecast(&json!({ "hourly": {} }), "Oslo", false, 0).is_none());
        assert!(parse_forecast(&json!({ "current": { "weather_code": 1 } }), "Oslo", false, 0).is_none());
    }

    #[test]
    fn short_columns_drop_only_the_incomplete_rows() {
        let mut data = sample();
        data["hourly"]["temperature_2m"] = json!([3.4]);
        let forecast = parse_forecast(&data, "Oslo", false, 0).unwrap();
        assert_eq!(forecast.hours.len(), 1);
    }

    #[test]
    fn parses_city_search_results() {
        let places = parse_places(&json!({ "results": [
            { "name": "Oslo", "admin1": "Oslo", "country": "Norway", "latitude": 59.91, "longitude": 10.75 },
            { "name": "Oslo", "country": "United States", "latitude": 48.19, "longitude": -97.13 },
            { "name": "No coordinates" }
        ]}));
        assert_eq!(places.len(), 2);
        assert_eq!(places[0].country.as_deref(), Some("Norway"));
        assert_eq!(places[1].region, None);
    }

    #[test]
    fn an_empty_search_has_no_places() {
        assert!(parse_places(&json!({})).is_empty());
        assert!(parse_places(&json!({ "results": [] })).is_empty());
    }
}
