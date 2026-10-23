// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Product launch policy for ordinary fetches and body-less document loads.
//! Micron's separate native wire/envelope/body settings live in `nomadnet`.

use fetch::ClientFetchPolicy;
use mere_document_lanes::ResourceFetchPolicy;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserClientLimits {
    pub fetch: ClientFetchPolicy,
    pub resources: ResourceFetchPolicy,
}

impl BrowserClientLimits {
    pub fn from_environment() -> Result<Self, String> {
        Self::from_lookup(|name| match std::env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} must be a number")),
        })
    }

    fn from_lookup(
        mut get: impl FnMut(&str) -> Result<Option<String>, String>,
    ) -> Result<Self, String> {
        let mut result = Self::default();
        macro_rules! bytes {
            ($field:expr, $name:literal) => {
                $field = usize::try_from(number(&mut get, $name, $field as u64, false)?)
                    .map_err(|_| format!("{} exceeds the platform byte count", $name))?;
            };
        }
        bytes!(
            result.fetch.max_page_bytes,
            "TURNSTONE_FETCH_MAX_PAGE_BYTES"
        );
        bytes!(
            result.fetch.max_subresource_bytes,
            "TURNSTONE_FETCH_MAX_SUBRESOURCE_BYTES"
        );
        bytes!(
            result.fetch.max_submission_bytes,
            "TURNSTONE_FETCH_MAX_SUBMISSION_BYTES"
        );
        bytes!(
            result.fetch.max_submission_response_bytes,
            "TURNSTONE_FETCH_MAX_SUBMISSION_RESPONSE_BYTES"
        );
        bytes!(
            result.fetch.max_request_header_bytes,
            "TURNSTONE_FETCH_MAX_REQUEST_HEADER_BYTES"
        );
        bytes!(
            result.fetch.max_response_header_line_bytes,
            "TURNSTONE_FETCH_MAX_RESPONSE_HEADER_LINE_BYTES"
        );
        bytes!(
            result.resources.max_response_bytes,
            "TURNSTONE_RESOURCE_MAX_RESPONSE_BYTES"
        );
        result.fetch.timeout = Duration::from_millis(number(
            &mut get,
            "TURNSTONE_FETCH_TIMEOUT_MS",
            result.fetch.timeout.as_millis() as u64,
            false,
        )?);
        result.resources.timeout = Duration::from_millis(number(
            &mut get,
            "TURNSTONE_RESOURCE_TIMEOUT_MS",
            result.resources.timeout.as_millis() as u64,
            false,
        )?);
        result.fetch.max_redirects = u32::try_from(number(
            &mut get,
            "TURNSTONE_FETCH_MAX_SMOLWEB_REDIRECTS",
            result.fetch.max_redirects as u64,
            true,
        )?)
        .map_err(|_| {
            "TURNSTONE_FETCH_MAX_SMOLWEB_REDIRECTS exceeds a redirect count".to_string()
        })?;
        result.fetch.max_http_redirects = u32::try_from(number(
            &mut get,
            "TURNSTONE_FETCH_MAX_HTTP_REDIRECTS",
            result.fetch.max_http_redirects as u64,
            true,
        )?)
        .map_err(|_| "TURNSTONE_FETCH_MAX_HTTP_REDIRECTS exceeds a redirect count".to_string())?;
        result.resources.max_redirects = u32::try_from(number(
            &mut get,
            "TURNSTONE_RESOURCE_MAX_REDIRECTS",
            result.resources.max_redirects as u64,
            true,
        )?)
        .map_err(|_| "TURNSTONE_RESOURCE_MAX_REDIRECTS exceeds a redirect count".to_string())?;
        result.resources.max_concurrent_fetches = usize::try_from(number(
            &mut get,
            "TURNSTONE_RESOURCE_MAX_CONCURRENT_FETCHES",
            result.resources.max_concurrent_fetches as u64,
            true,
        )?)
        .map_err(|_| {
            "TURNSTONE_RESOURCE_MAX_CONCURRENT_FETCHES exceeds a request count".to_string()
        })?;
        result.fetch.validate()?;
        Ok(result)
    }
}

fn number(
    get: &mut impl FnMut(&str) -> Result<Option<String>, String>,
    name: &str,
    fallback: u64,
    allow_zero: bool,
) -> Result<u64, String> {
    let Some(value) = get(name)? else {
        return Ok(fallback);
    };
    value
        .parse::<u64>()
        .ok()
        .filter(|value| allow_zero || *value > 0)
        .ok_or_else(|| {
            format!(
                "{name} must be {}",
                if allow_zero {
                    "a nonnegative integer"
                } else {
                    "a positive integer"
                }
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_launch_limits_are_kept_and_invalid_values_are_not_silently_clamped() {
        let limits = BrowserClientLimits::from_lookup(|name| {
            Ok(match name {
                "TURNSTONE_FETCH_MAX_PAGE_BYTES" => Some("123".into()),
                "TURNSTONE_FETCH_TIMEOUT_MS" => Some("47".into()),
                "TURNSTONE_RESOURCE_MAX_CONCURRENT_FETCHES" => Some("0".into()),
                "TURNSTONE_FETCH_MAX_SMOLWEB_REDIRECTS" => Some("0".into()),
                _ => None,
            })
        })
        .unwrap();
        assert_eq!(limits.fetch.max_page_bytes, 123);
        assert_eq!(limits.fetch.timeout, Duration::from_millis(47));
        assert_eq!(limits.fetch.max_redirects, 0);
        assert_eq!(limits.fetch.max_http_redirects, 20);
        assert_eq!(limits.resources.max_concurrent_fetches, 0);
        assert!(
            BrowserClientLimits::from_lookup(|name| Ok(
                (name == "TURNSTONE_FETCH_MAX_PAGE_BYTES").then(|| "0".into())
            ))
            .is_err()
        );
        assert!(
            BrowserClientLimits::from_lookup(|name| Ok(
                (name == "TURNSTONE_RESOURCE_TIMEOUT_MS").then(|| "unlimited".into())
            ))
            .is_err()
        );
    }
}
