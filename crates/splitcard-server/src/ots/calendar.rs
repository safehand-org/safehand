//! The only module that talks to OpenTimestamps calendars.
//!
//! Mirrors the reference client (`opentimestamps/calendar.py`): `POST
//! <calendar>/digest` with the raw digest submits it, `GET
//! <calendar>/timestamp/<commitment hex>` fetches an upgrade (404 means "not
//! yet"), both with `Accept: application/vnd.opentimestamps.v1`. Responses
//! are returned as raw bytes; [`super::format`] parses them.
//!
//! Requests are HTTPS only, time out, stop reading past
//! [`MAX_CALENDAR_RESPONSE_BYTES`], follow redirects only to HTTPS on the same
//! host, and connect only to public addresses.

use std::{future::Future, net::SocketAddr, time::Duration};

use reqwest::{
    StatusCode,
    dns::{Addrs, Name, Resolve, Resolving},
    header::{ACCEPT, USER_AGENT},
    redirect,
};
use splitcard_ln::client::is_public_ip;
use thiserror::Error;
use url::{Host, Url};

use super::format::MAX_CALENDAR_RESPONSE_BYTES;

/// Media type the reference client asks calendars for.
pub const OTS_MEDIA_TYPE: &str = "application/vnd.opentimestamps.v1";
/// Most redirects followed for one request.
pub const MAX_REDIRECTS: usize = 3;

/// A failed calendar request. "Not yet" is not an error.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CalendarError {
    /// The URL is not HTTPS, has no host, or names a non-public address.
    #[error("refused calendar URL {url}")]
    RefusedUrl {
        /// The URL.
        url: String,
    },
    /// The request timed out.
    #[error("calendar request timed out")]
    Timeout,
    /// The response was larger than allowed.
    #[error("calendar response is larger than {limit} bytes")]
    ResponseTooLarge {
        /// The limit in bytes.
        limit: usize,
    },
    /// The calendar answered with an unexpected HTTP status.
    #[error("calendar returned HTTP {status}")]
    HttpStatus {
        /// The status code.
        status: u16,
    },
    /// The request failed in transport.
    #[error("calendar request failed: {message}")]
    Request {
        /// The underlying error chain.
        message: String,
    },
}

/// Talks to calendars; implemented over HTTP by [`HttpCalendar`] and by fakes
/// in tests.
pub trait CalendarClient: Send + Sync + 'static {
    /// Submits a digest; returns the raw timestamp tree for it.
    fn submit(
        &self,
        calendar: &Url,
        digest: [u8; 32],
    ) -> impl Future<Output = Result<Vec<u8>, CalendarError>> + Send;

    /// Asks for the upgraded tree for a commitment; `Ok(None)` means the
    /// calendar has nothing newer yet.
    fn upgrade(
        &self,
        calendar: &Url,
        commitment: &[u8],
    ) -> impl Future<Output = Result<Option<Vec<u8>>, CalendarError>> + Send;
}

/// Settings for [`HttpCalendar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpCalendarConfig {
    /// Total time for one request, including reading the body.
    pub timeout: Duration,
    /// Allow plain HTTP and private addresses. Tests only.
    pub allow_insecure_hosts: bool,
}

impl Default for HttpCalendarConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(15),
            allow_insecure_hosts: false,
        }
    }
}

/// [`CalendarClient`] over HTTPS.
#[derive(Debug, Clone)]
pub struct HttpCalendar {
    http: reqwest::Client,
    allow_insecure: bool,
}

impl HttpCalendar {
    /// Builds the client.
    ///
    /// # Errors
    ///
    /// [`CalendarError::Request`] if the TLS backend cannot be initialised.
    pub fn new(config: &HttpCalendarConfig) -> Result<Self, CalendarError> {
        let allow_insecure = config.allow_insecure_hosts;
        let policy = redirect::Policy::custom(move |attempt| {
            let same_host = attempt
                .previous()
                .first()
                .is_some_and(|first| first.host_str() == attempt.url().host_str());
            if attempt.previous().len() > MAX_REDIRECTS
                || !same_host
                || check_url(attempt.url(), allow_insecure).is_err()
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        });
        // A system proxy would resolve hosts itself, bypassing the address check.
        let mut builder = reqwest::Client::builder()
            .timeout(config.timeout)
            .redirect(policy)
            .no_proxy();
        if !allow_insecure {
            builder = builder.dns_resolver(PublicOnlyResolver);
        }
        let http = builder.build().map_err(|error| CalendarError::Request {
            message: error_chain(&error),
        })?;
        Ok(Self {
            http,
            allow_insecure,
        })
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<(StatusCode, Vec<u8>), CalendarError> {
        let limit = MAX_CALENDAR_RESPONSE_BYTES;
        let mut response = request
            .header(ACCEPT, OTS_MEDIA_TYPE)
            .header(
                USER_AGENT,
                concat!("splitcard-server/", env!("CARGO_PKG_VERSION")),
            )
            .send()
            .await
            .map_err(|error| classify(&error))?;
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(CalendarError::ResponseTooLarge { limit });
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| classify(&error))? {
            if body.len().saturating_add(chunk.len()) > limit {
                return Err(CalendarError::ResponseTooLarge { limit });
            }
            body.extend_from_slice(&chunk);
        }
        Ok((status, body))
    }
}

/// `<calendar>/digest`, resolved like the reference client's `urljoin`.
///
/// # Errors
///
/// [`CalendarError::RefusedUrl`] if the result cannot be built.
pub fn digest_url(calendar: &Url) -> Result<Url, CalendarError> {
    calendar.join("digest").map_err(|_| refused(calendar))
}

/// `<calendar>/timestamp/<commitment hex>`, resolved like `urljoin`.
///
/// # Errors
///
/// [`CalendarError::RefusedUrl`] if the result cannot be built.
pub fn timestamp_url(calendar: &Url, commitment: &[u8]) -> Result<Url, CalendarError> {
    calendar
        .join(&format!("timestamp/{}", hex::encode(commitment)))
        .map_err(|_| refused(calendar))
}

impl CalendarClient for HttpCalendar {
    async fn submit(&self, calendar: &Url, digest: [u8; 32]) -> Result<Vec<u8>, CalendarError> {
        let url = digest_url(calendar)?;
        check_url(&url, self.allow_insecure)?;
        match self.send(self.http.post(url).body(digest.to_vec())).await? {
            (StatusCode::OK, body) => Ok(body),
            (status, _) => Err(CalendarError::HttpStatus {
                status: status.as_u16(),
            }),
        }
    }

    async fn upgrade(
        &self,
        calendar: &Url,
        commitment: &[u8],
    ) -> Result<Option<Vec<u8>>, CalendarError> {
        let url = timestamp_url(calendar, commitment)?;
        check_url(&url, self.allow_insecure)?;
        match self.send(self.http.get(url)).await? {
            (StatusCode::OK, body) => Ok(Some(body)),
            (StatusCode::NOT_FOUND, _) => Ok(None),
            (status, _) => Err(CalendarError::HttpStatus {
                status: status.as_u16(),
            }),
        }
    }
}

fn refused(url: &Url) -> CalendarError {
    CalendarError::RefusedUrl {
        url: url.to_string(),
    }
}

/// HTTPS, a host, and no non-public IP literal. Hostnames are checked again
/// at connection time by [`PublicOnlyResolver`].
fn check_url(url: &Url, allow_insecure: bool) -> Result<(), CalendarError> {
    if allow_insecure {
        return Ok(());
    }
    let public = match url.host() {
        _ if url.scheme() != "https" => false,
        None => false,
        Some(Host::Domain(_)) => true,
        Some(Host::Ipv4(ip)) => is_public_ip(ip.into()),
        Some(Host::Ipv6(ip)) => is_public_ip(ip.into()),
    };
    if public { Ok(()) } else { Err(refused(url)) }
}

/// Resolves hostnames and refuses any with a non-public address.
struct PublicOnlyResolver;

impl Resolve for PublicOnlyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            if addrs.is_empty() || addrs.iter().any(|addr| !is_public_ip(addr.ip())) {
                return Err(format!("{host} does not resolve only to public addresses").into());
            }
            let addrs: Addrs = Box::new(addrs.into_iter());
            Ok(addrs)
        })
    }
}

fn classify(error: &reqwest::Error) -> CalendarError {
    if error.is_timeout() {
        CalendarError::Timeout
    } else {
        CalendarError::Request {
            message: error_chain(error),
        }
    }
}

fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(current) = source {
        message.push_str(": ");
        message.push_str(&current.to_string());
        source = current.source();
    }
    message
}
