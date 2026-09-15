//! The limits layer: the Claude Code OAuth token out of the login Keychain
//! (via `/usr/bin/security`),
//! one `GET /api/oauth/usage` against the Anthropic API, and the parse into
//! [`crate::model::Usage`].
//!
//! We are a reader of somebody else's credential. Claude Code owns the token
//! and is the only thing allowed to refresh it, so an expired token is an
//! error we report ("run `claude`"), never something we try to fix. The token
//! itself is never logged, never put in an error message, and never leaves the
//! `Authorization` header.
//!
//! The response carries a lot of fields we do not use, and Anthropic adds more
//! over time, so every struct here is `#[serde(default)]` and tolerant of
//! unknown keys: a new field must never turn into a popover error.

use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use serde::Deserialize;

use crate::model::{Limit, LimitKind, Usage};

/// The usage endpoint. Undocumented, but it is what `claude` itself calls.
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// The OAuth beta the Claude Code token is scoped to.
const OAUTH_BETA: &str = "oauth-2025-04-20";
/// Keychain service holding the Claude Code credentials blob.
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const USER_AGENT: &str = "claudebar";
/// Short enough that a hung network does not hold the 5-minute poll open.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Everything that can go wrong between the Keychain and a parsed snapshot.
///
/// The variants are exactly the four states the popover's muted line knows how
/// to say; anything else collapses into [`UsageError::Bad`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageError {
    /// No Claude Code credentials in the Keychain at all.
    SignedOut,
    /// The stored token is past its `expiresAt`, or the API answered 401.
    TokenExpired,
    /// The request never reached the API (DNS, timeout, no route).
    Offline(String),
    /// The API answered 429: we asked too often. The last good numbers stay
    /// up and the next scheduled fetch tries again.
    RateLimited,
    /// The API answered with something we could not use.
    Bad(String),
}

impl UsageError {
    /// The popover-ready sentence for this failure, straight from the spec.
    pub fn message(&self) -> String {
        match self {
            UsageError::SignedOut => "Sign in with `claude` first".to_string(),
            UsageError::TokenExpired => "Token expired — run `claude` to refresh".to_string(),
            UsageError::Offline(_) => "Offline".to_string(),
            UsageError::RateLimited => "Rate limited — will retry".to_string(),
            UsageError::Bad(detail) => detail.clone(),
        }
    }
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsageError::Offline(detail) => write!(f, "Offline: {detail}"),
            other => f.write_str(&other.message()),
        }
    }
}

impl std::error::Error for UsageError {}

/// The Claude Code credential as it is stored in the Keychain.
#[derive(Debug, Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth", default)]
    oauth: Option<OauthCredential>,
}

#[derive(Debug, Default, Deserialize)]
struct OauthCredential {
    #[serde(rename = "accessToken", default)]
    access_token: String,
    /// Milliseconds since the epoch. Absent on some older writes, in which
    /// case we let the API be the judge of expiry.
    #[serde(rename = "expiresAt", default)]
    expires_at: Option<i64>,
    #[serde(rename = "subscriptionType", default)]
    subscription_type: Option<String>,
}

/// A token plus the plan we can read off the same blob, kept together so the
/// caller never has to touch the Keychain twice.
struct Token {
    access_token: String,
    plan: Option<String>,
}

/// Fetch the current limits. Blocking: callers run this on a background
/// thread.
pub fn fetch() -> Result<Usage, UsageError> {
    let token = read_token()?;
    let body = request(&token.access_token)?;
    let mut usage = parse(&body, Local::now())?;
    // The API does not name the plan, but the credential blob does.
    if usage.plan.is_none() {
        usage.plan = token.plan;
    }
    Ok(usage)
}

/// The macOS account name the Claude Code Keychain item is filed under.
///
/// `USER` is what a terminal-launched process sees; a bundled `.app` launched
/// from Finder may have no `USER`, so fall back to the home directory's name.
fn keychain_username() -> Option<String> {
    if let Ok(user) = std::env::var("USER") {
        if !user.is_empty() {
            return Some(user);
        }
    }
    let home = dirs::home_dir()?;
    Some(home.file_name()?.to_string_lossy().into_owned())
}

/// `security find-generic-password` exit status when the item does not exist
/// (`errSecItemNotFound`).
const SECURITY_NOT_FOUND: i32 = 44;

/// Read the credential blob out of the login Keychain.
///
/// This deliberately shells out to `/usr/bin/security` instead of calling the
/// Keychain API directly. Claude Code writes the item with that same tool, and
/// every time it refreshes the token (roughly every few hours) macOS resets
/// the item's partition list to `apple-tool:`, wiping the "Always Allow" the
/// user granted our bundle. A direct API read therefore re-prompts for the
/// login password after every refresh. Apple's own tool is in the surviving
/// partition, so reading through it never prompts.
fn read_keychain_blob(username: &str) -> Result<String, UsageError> {
    let output = std::process::Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            username,
            "-w",
        ])
        .output()
        .map_err(|e| UsageError::Bad(format!("running `security` failed: {e}")))?;
    match output.status.code() {
        Some(0) => {}
        Some(SECURITY_NOT_FOUND) => return Err(UsageError::SignedOut),
        Some(code) => {
            return Err(UsageError::Bad(format!(
                "reading the Keychain failed (security exit {code})"
            )))
        }
        None => {
            return Err(UsageError::Bad(
                "reading the Keychain failed (security killed)".into(),
            ))
        }
    }
    let secret = String::from_utf8(output.stdout)
        .map_err(|_| UsageError::Bad("the Claude Code credential is not UTF-8".into()))?;
    Ok(secret.trim_end_matches(['\n', '\r']).to_string())
}

/// Read and validate the stored credential. A missing item means signed out; a
/// past `expiresAt` means the user has to run `claude` again.
fn read_token() -> Result<Token, UsageError> {
    let username = keychain_username()
        .ok_or_else(|| UsageError::Bad("no macOS user name to look up".into()))?;
    let secret = read_keychain_blob(&username)?;

    // The blob is JSON, but never say what was in it: it holds the token.
    let credentials: Credentials = serde_json::from_str(&secret)
        .map_err(|_| UsageError::Bad("the Claude Code credential is not JSON".into()))?;
    let oauth = credentials.oauth.ok_or(UsageError::SignedOut)?;
    if oauth.access_token.is_empty() {
        return Err(UsageError::SignedOut);
    }
    if let Some(expires_at) = oauth.expires_at {
        if expires_at <= Utc::now().timestamp_millis() {
            return Err(UsageError::TokenExpired);
        }
    }
    Ok(Token {
        plan: oauth.subscription_type.as_deref().map(capitalise),
        access_token: oauth.access_token,
    })
}

/// One GET against the usage endpoint, returning the raw body.
fn request(access_token: &str) -> Result<String, UsageError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| UsageError::Bad(format!("building the HTTP client failed: {e}")))?;
    let response = client
        .get(USAGE_URL)
        .bearer_auth(access_token)
        .header("anthropic-beta", OAUTH_BETA)
        .send()
        .map_err(|e| UsageError::Offline(strip_url(&e.to_string())))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(UsageError::TokenExpired);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(UsageError::RateLimited);
    }
    if !status.is_success() {
        return Err(UsageError::Bad(format!("The API answered {}", status)));
    }
    response
        .text()
        .map_err(|e| UsageError::Offline(strip_url(&e.to_string())))
}

/// reqwest puts the request URL in its error text; the usage URL is harmless
/// but the message is shown to a human, so keep it to the cause.
fn strip_url(message: &str) -> String {
    match message.split_once(": ") {
        Some((_, rest)) if message.starts_with("error sending request") => rest.to_string(),
        _ => message.to_string(),
    }
}

/// "max" -> "Max". The plan is shown next to "Claude" in the header.
fn capitalise(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The response shape, restricted to what the rings need.
#[derive(Debug, Default, Deserialize)]
struct UsageResponse {
    #[serde(default)]
    five_hour: Option<Window>,
    #[serde(default)]
    seven_day: Option<Window>,
    /// Present on newer responses; the model-scoped rings come from here.
    #[serde(default)]
    limits: Vec<LimitEntry>,
    /// Not currently sent, but harmless to accept if it ever is.
    #[serde(default)]
    subscription_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Window {
    #[serde(default)]
    utilization: Option<f32>,
    #[serde(default)]
    resets_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct LimitEntry {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    percent: Option<f32>,
    #[serde(default)]
    resets_at: Option<String>,
    #[serde(default)]
    scope: Option<Scope>,
}

#[derive(Debug, Default, Deserialize)]
struct Scope {
    #[serde(default)]
    model: Option<ScopeModel>,
}

#[derive(Debug, Default, Deserialize)]
struct ScopeModel {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

/// Parse a usage response into the display model.
///
/// Pure, so the whole mapping is testable without a token: session from
/// `five_hour`, week from `seven_day`, then one ring per `weekly_scoped` entry
/// in the order the API listed them.
pub fn parse(json: &str, fetched_at: DateTime<Local>) -> Result<Usage, UsageError> {
    let response: UsageResponse = serde_json::from_str(json)
        .map_err(|e| UsageError::Bad(format!("The API sent something unexpected: {e}")))?;

    let mut limits = Vec::new();
    if let Some(window) = &response.five_hour {
        limits.push(Limit {
            kind: LimitKind::Session,
            percent: window.utilization.unwrap_or(0.0),
            resets_at: parse_time(window.resets_at.as_deref()),
        });
    }
    if let Some(window) = &response.seven_day {
        limits.push(Limit {
            kind: LimitKind::Weekly,
            percent: window.utilization.unwrap_or(0.0),
            resets_at: parse_time(window.resets_at.as_deref()),
        });
    }
    for entry in &response.limits {
        if entry.kind != "weekly_scoped" {
            continue;
        }
        // A scoped limit with no model name has nothing to label a ring with.
        let Some(name) = entry
            .scope
            .as_ref()
            .and_then(|s| s.model.as_ref())
            .and_then(|m| m.display_name.clone())
            .or_else(|| {
                entry
                    .scope
                    .as_ref()
                    .and_then(|s| s.model.as_ref())
                    .and_then(|m| m.id.as_deref())
                    .map(crate::model::model_display_name)
            })
        else {
            continue;
        };
        limits.push(Limit {
            kind: LimitKind::Model(name),
            percent: entry.percent.unwrap_or(0.0),
            resets_at: parse_time(entry.resets_at.as_deref()),
        });
    }

    if limits.is_empty() {
        return Err(UsageError::Bad("The API reported no limits".into()));
    }

    Ok(Usage {
        limits,
        plan: response.subscription_type.as_deref().map(capitalise),
        fetched_at,
    })
}

/// RFC 3339 as the API writes it. A reset we cannot read is simply absent —
/// the caption is optional and one bad timestamp must not fail the fetch.
fn parse_time(value: Option<&str>) -> Option<DateTime<Utc>> {
    let value = value?;
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// The exact response from the spec.
    const SPEC_JSON: &str = r#"
    {"five_hour": {"utilization": 2.0, "resets_at": "2026-09-12T22:29:59.705239+00:00"},
     "seven_day": {"utilization": 18.0, "resets_at": "2026-09-16T23:59:59.705258+00:00"},
     "limits": [
       {"kind": "session", "percent": 2, "resets_at": "2026-09-12T22:29:59.705239+00:00", "scope": null},
       {"kind": "weekly_all", "percent": 18, "resets_at": "2026-09-16T23:59:59.705258+00:00", "scope": null},
       {"kind": "weekly_scoped", "percent": 22, "resets_at": "2026-09-16T23:59:59.705258+00:00",
        "scope": {"model": {"id": null, "display_name": "Fable"}}}]}
    "#;

    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 9, 12, 13, 37, 0).unwrap()
    }

    #[test]
    fn the_spec_response_becomes_three_rings() {
        let usage = parse(SPEC_JSON, now()).unwrap();
        assert_eq!(usage.limits.len(), 3);
        assert_eq!(usage.limits[0].kind, LimitKind::Session);
        assert_eq!(usage.limits[0].percent, 2.0);
        assert_eq!(usage.limits[1].kind, LimitKind::Weekly);
        assert_eq!(usage.limits[1].percent, 18.0);
        assert_eq!(usage.limits[2].kind, LimitKind::Model("Fable".into()));
        assert_eq!(usage.limits[2].percent, 22.0);
        assert_eq!(usage.fetched_at, now());
        // The session ring's reset is the five_hour one, in UTC.
        let reset = usage.limits[0].resets_at.unwrap();
        assert_eq!(reset.to_rfc3339(), "2026-09-12T22:29:59.705239+00:00");
    }

    #[test]
    fn a_response_without_a_limits_array_still_has_session_and_week() {
        let json = r#"{"five_hour": {"utilization": 40.5, "resets_at": null},
                       "seven_day": {"utilization": 12.0}}"#;
        let usage = parse(json, now()).unwrap();
        assert_eq!(usage.limits.len(), 2);
        assert_eq!(usage.limits[0].percent, 40.5);
        assert!(usage.limits[0].resets_at.is_none());
        assert_eq!(usage.limits[1].kind, LimitKind::Weekly);
    }

    #[test]
    fn unknown_fields_and_nulls_are_ignored() {
        let json = r#"
        {"five_hour": {"utilization": 2.0, "resets_at": null, "overage_status": null,
                       "brand_new_field": {"deeply": ["nested", 1, null]}},
         "seven_day": null,
         "organization": null,
         "limits": [{"kind": "weekly_scoped", "percent": 7, "scope":
                       {"model": {"id": "claude-opus-5", "display_name": null}, "tier": null},
                     "future_key": 1}],
         "another_top_level": [1, 2, 3]}"#;
        let usage = parse(json, now()).unwrap();
        assert_eq!(usage.limits.len(), 2);
        // With no display_name we fall back to the model id, shortened.
        assert_eq!(usage.limits[1].kind, LimitKind::Model("Opus".into()));
        assert_eq!(usage.limits[1].percent, 7.0);
    }

    #[test]
    fn a_response_with_nothing_usable_is_an_error() {
        assert_eq!(
            parse("{}", now()),
            Err(UsageError::Bad("The API reported no limits".into()))
        );
        assert!(matches!(parse("not json", now()), Err(UsageError::Bad(_))));
    }

    #[test]
    fn error_messages_are_the_ones_the_popover_shows() {
        assert_eq!(
            UsageError::SignedOut.message(),
            "Sign in with `claude` first"
        );
        assert_eq!(
            UsageError::TokenExpired.message(),
            "Token expired — run `claude` to refresh"
        );
        assert_eq!(UsageError::Offline("dns error".into()).message(), "Offline");
        assert_eq!(UsageError::Bad("boom".into()).message(), "boom");
    }

    #[test]
    fn the_plan_is_the_subscription_type_capitalised() {
        assert_eq!(capitalise("max"), "Max");
        assert_eq!(capitalise("pro"), "Pro");
        assert_eq!(capitalise(""), "");
    }

    #[test]
    fn the_credential_blob_parses_the_way_claude_code_writes_it() {
        let raw = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-x","refreshToken":"r",
            "expiresAt":1789000000000,"scopes":["user:inference"],"subscriptionType":"max"}}"#;
        let credentials: Credentials = serde_json::from_str(raw).unwrap();
        let oauth = credentials.oauth.unwrap();
        assert_eq!(oauth.expires_at, Some(1789000000000));
        assert_eq!(oauth.subscription_type.as_deref(), Some("max"));
        assert!(!oauth.access_token.is_empty());
    }
}
