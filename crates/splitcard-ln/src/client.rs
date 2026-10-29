//! The only module that does HTTP: fetching LNURL-pay requests and invoices.
//!
//! Unless [`ClientConfig::allow_insecure_hosts`] is set, every request,
//! redirect and connection is checked against server-side request forgery:
//! HTTPS only, at most [`MAX_REDIRECTS`] redirects, and every address a host
//! resolves to must be public. The address check runs in the DNS resolver, so
//! it applies to the IP actually dialled, not just the hostname.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::redirect;
use thiserror::Error;
use url::{Host, Url};

use crate::{
    address::{LightningAddress, well_known_url},
    invoice::{CheckedInvoice, InvoiceError, Network, check_invoice},
    lnurl::{
        LnurlError, PayRequest, Settlement, build_callback_url, check_amount,
        parse_invoice_response, parse_pay_request, parse_verify_response, parse_verify_url,
        sats_to_msat,
    },
};

/// Most redirects followed for one request.
pub const MAX_REDIRECTS: usize = 3;
/// Default total time allowed for one request, including reading the body.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// Default largest response body accepted, in bytes.
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// Settings for [`LnClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    /// Total time allowed for one request, including reading the body.
    pub timeout: Duration,
    /// Largest response body accepted, in bytes; enforced while reading.
    pub max_response_bytes: usize,
    /// The network invoices must be for.
    pub network: Network,
    /// Turns off the HTTPS and public-address rules. Only for tests and local
    /// demos against a mock server; never enable this in production.
    pub allow_insecure_hosts: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            network: Network::default(),
            allow_insecure_hosts: false,
        }
    }
}

/// A failure while fetching from an LNURL service.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ClientError {
    /// The HTTP client could not be built.
    #[error("could not build the HTTP client: {message}")]
    Build {
        /// The underlying error.
        message: String,
    },
    /// A URL or redirect target is not HTTPS.
    #[error("refusing non-HTTPS URL {url}")]
    InsecureScheme {
        /// The refused URL.
        url: String,
    },
    /// A URL has no host.
    #[error("URL {url} has no host")]
    MissingHost {
        /// The refused URL.
        url: String,
    },
    /// A host is, or resolves to, a loopback, private, link-local or other
    /// non-public address.
    #[error("refusing {host}: it resolves to non-public address {ip}")]
    ForbiddenAddress {
        /// The host as written in the URL.
        host: String,
        /// The non-public address.
        ip: IpAddr,
    },
    /// A host did not resolve to any address.
    #[error("{host} did not resolve to any address")]
    Unresolvable {
        /// The host as written in the URL.
        host: String,
    },
    /// More than [`MAX_REDIRECTS`] redirects.
    #[error("more than 3 redirects")]
    TooManyRedirects,
    /// The request did not finish within the configured timeout.
    #[error("request timed out")]
    Timeout,
    /// The response body exceeded the configured limit.
    #[error("response is larger than {limit} bytes")]
    ResponseTooLarge {
        /// The configured limit, in bytes.
        limit: usize,
    },
    /// The response body is not UTF-8.
    #[error("response is not valid UTF-8")]
    InvalidUtf8,
    /// The server answered with a non-success status and no LNURL error.
    #[error("server returned HTTP {status}")]
    HttpStatus {
        /// The HTTP status code.
        status: u16,
    },
    /// Any other transport failure (connection refused, TLS, DNS lookup).
    #[error("request failed: {message}")]
    Request {
        /// The underlying error chain.
        message: String,
    },
    /// The LNURL response or amount was invalid.
    #[error(transparent)]
    Lnurl(#[from] LnurlError),
    /// The returned invoice failed a check.
    #[error(transparent)]
    Invoice(#[from] InvoiceError),
}

/// A checked invoice and, if the provider gave one, its LUD-21 verify URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedInvoice {
    /// The invoice, which passed every check in [`check_invoice`].
    pub invoice: CheckedInvoice,
    /// Where to ask whether the invoice was paid (LUD-21), if offered.
    pub verify_url: Option<Url>,
}

/// Fetches LNURL-pay requests and invoices. It never pays and holds no wallet
/// credentials.
#[derive(Debug, Clone)]
pub struct LnClient {
    http: reqwest::Client,
    config: ClientConfig,
}

impl LnClient {
    /// Builds a client from its configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::Build`] if the TLS backend cannot be initialised.
    pub fn new(config: ClientConfig) -> Result<Self, ClientError> {
        let allow_insecure = config.allow_insecure_hosts;
        let policy = redirect::Policy::custom(move |attempt| {
            match check_redirect(attempt.previous().len(), attempt.url(), allow_insecure) {
                Ok(()) => attempt.follow(),
                Err(refusal) => attempt.error(refusal),
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
        let http = builder.build().map_err(|error| ClientError::Build {
            message: error_chain(&error),
        })?;
        Ok(Self { http, config })
    }

    /// The client's configuration.
    #[must_use]
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Fetches the LNURL-pay request for a Lightning Address.
    ///
    /// # Errors
    ///
    /// Returns a [`ClientError`] for a refused URL, transport failure or
    /// invalid pay request.
    pub async fn fetch_pay_request(
        &self,
        address: &LightningAddress,
    ) -> Result<PayRequest, ClientError> {
        self.fetch_pay_request_url(well_known_url(address)).await
    }

    /// Fetches an LNURL-pay request from a URL, under the same rules as
    /// [`LnClient::fetch_pay_request`].
    ///
    /// # Errors
    ///
    /// Returns a [`ClientError`] for a refused URL, transport failure or
    /// invalid pay request.
    pub async fn fetch_pay_request_url(&self, url: Url) -> Result<PayRequest, ClientError> {
        let body = self.get(url).await?;
        Ok(parse_pay_request(&body, self.config.allow_insecure_hosts)?)
    }

    /// Requests an invoice for exactly `amount_sats` and checks it.
    ///
    /// The amount is checked against the pay request's limits before the
    /// callback is called, and the invoice is returned only if it passes every
    /// check in [`check_invoice`] at `now_unix`.
    ///
    /// # Errors
    ///
    /// Returns a [`ClientError`] for an out-of-range amount, refused URL,
    /// transport failure, LNURL error or failed invoice check.
    pub async fn request_invoice(
        &self,
        pay_request: &PayRequest,
        amount_sats: u64,
        now_unix: u64,
    ) -> Result<CheckedInvoice, ClientError> {
        Ok(self
            .request_invoice_with_verify(pay_request, amount_sats, now_unix)
            .await?
            .invoice)
    }

    /// Like [`LnClient::request_invoice`], also keeping the LUD-21 verify URL
    /// if the callback response has a usable one (see [`parse_verify_url`]).
    ///
    /// # Errors
    ///
    /// The same as [`LnClient::request_invoice`].
    pub async fn request_invoice_with_verify(
        &self,
        pay_request: &PayRequest,
        amount_sats: u64,
        now_unix: u64,
    ) -> Result<FetchedInvoice, ClientError> {
        let amount_msat = sats_to_msat(amount_sats)?;
        check_amount(pay_request, amount_msat)?;
        let url = build_callback_url(&pay_request.callback, amount_msat);
        let body = self.get(url).await?;
        let bolt11 = parse_invoice_response(&body)?;
        let invoice = check_invoice(
            &bolt11,
            amount_msat,
            &pay_request.metadata,
            self.config.network,
            now_unix,
        )?;
        Ok(FetchedInvoice {
            invoice,
            verify_url: parse_verify_url(&body, self.config.allow_insecure_hosts),
        })
    }

    /// Asks a LUD-21 verify URL whether its invoice was paid, under the same
    /// network rules as every other request.
    ///
    /// # Errors
    ///
    /// Returns a [`ClientError`] for a refused URL, transport failure or
    /// invalid verify response.
    pub async fn check_settlement(&self, verify_url: &Url) -> Result<Settlement, ClientError> {
        let body = self.get(verify_url.clone()).await?;
        Ok(parse_verify_response(&body)?)
    }

    /// GETs a URL and returns its body, enforcing the size limit while reading.
    async fn get(&self, url: Url) -> Result<String, ClientError> {
        check_url(&url, self.config.allow_insecure_hosts)?;
        let limit = self.config.max_response_bytes;
        let mut response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|error| classify(&error))?;
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(ClientError::ResponseTooLarge { limit });
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| classify(&error))? {
            if body.len().saturating_add(chunk.len()) > limit {
                return Err(ClientError::ResponseTooLarge { limit });
            }
            body.extend_from_slice(&chunk);
        }
        let body = String::from_utf8(body).map_err(|_| ClientError::InvalidUtf8)?;
        if !status.is_success() {
            // Prefer the service's own `{ "status": "ERROR", "reason" }` when it sends one.
            if let Err(error @ LnurlError::Service { .. }) = parse_invoice_response(&body) {
                return Err(error.into());
            }
            return Err(ClientError::HttpStatus {
                status: status.as_u16(),
            });
        }
        Ok(body)
    }
}

/// Checks a URL before requesting it: HTTPS, a host, and no non-public IP literal.
/// Hostnames are checked again at connection time by [`PublicOnlyResolver`].
fn check_url(url: &Url, allow_insecure: bool) -> Result<(), ClientError> {
    if allow_insecure {
        return Ok(());
    }
    if url.scheme() != "https" {
        return Err(ClientError::InsecureScheme {
            url: url.to_string(),
        });
    }
    let ip = match url.host() {
        None => {
            return Err(ClientError::MissingHost {
                url: url.to_string(),
            });
        }
        Some(Host::Domain(_)) => return Ok(()),
        Some(Host::Ipv4(ip)) => IpAddr::V4(ip),
        Some(Host::Ipv6(ip)) => IpAddr::V6(ip),
    };
    if is_public_ip(ip) {
        Ok(())
    } else {
        Err(ClientError::ForbiddenAddress {
            host: url.host_str().unwrap_or_default().to_owned(),
            ip,
        })
    }
}

/// Decides whether to follow a redirect to `url` after `previous` requests.
fn check_redirect(previous: usize, url: &Url, allow_insecure: bool) -> Result<(), ClientError> {
    if previous > MAX_REDIRECTS {
        return Err(ClientError::TooManyRedirects);
    }
    check_url(url, allow_insecure)
}

/// Resolves hostnames and refuses any that resolve to a non-public address.
struct PublicOnlyResolver;

impl Resolve for PublicOnlyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            if addrs.is_empty() {
                return Err(ClientError::Unresolvable { host }.into());
            }
            // Refuse the host outright if any address is non-public.
            if let Some(addr) = addrs.iter().find(|addr| !is_public_ip(addr.ip())) {
                return Err(ClientError::ForbiddenAddress {
                    host,
                    ip: addr.ip(),
                }
                .into());
            }
            let addrs: Addrs = Box::new(addrs.into_iter());
            Ok(addrs)
        })
    }
}

/// Maps a reqwest error to a [`ClientError`], surfacing refusals raised by the
/// resolver or redirect policy.
fn classify(error: &reqwest::Error) -> ClientError {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = source {
        if let Some(refusal) = current.downcast_ref::<ClientError>() {
            return refusal.clone();
        }
        source = current.source();
    }
    if error.is_timeout() {
        ClientError::Timeout
    } else if error.is_redirect() {
        ClientError::TooManyRedirects
    } else {
        ClientError::Request {
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

/// Whether an address is publicly routable. Loopback, private, link-local,
/// shared, documentation, benchmarking, multicast, reserved and unspecified
/// ranges are not; IPv6 addresses embedding an IPv4 address are judged by it.
#[must_use]
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    let reserved = a == 0                                // "this network"
        || (a == 100 && (64..=127).contains(&b))         // shared address space (CGNAT)
        || (a == 192 && b == 0 && c == 0)                // IETF protocol assignments
        || (a == 198 && (b == 18 || b == 19))            // benchmarking
        || a >= 240; // reserved and broadcast
    !(reserved
        || ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_documentation()
        || ip.is_multicast()
        || ip.is_broadcast())
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_ipv4(v4);
    }
    let segments = ip.segments();
    let embedded_v4 = |high: u16, low: u16| {
        let [a, b] = high.to_be_bytes();
        let [c, d] = low.to_be_bytes();
        Ipv4Addr::new(a, b, c, d)
    };
    // NAT64 (64:ff9b::/96) and 6to4 (2002::/16) reach an embedded IPv4 address.
    if segments[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        return is_public_ipv4(embedded_v4(segments[6], segments[7]));
    }
    if segments[0] == 0x2002 {
        return is_public_ipv4(embedded_v4(segments[1], segments[2]));
    }
    let global_unicast = segments[0] & 0xe000 == 0x2000; // 2000::/3
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8; // 2001:db8::/32
    let protocol_assignments = segments[0] == 0x2001 && segments[1] < 0x0200; // 2001::/23, incl. Teredo
    global_unicast && !documentation && !protocol_assignments
}

#[cfg(test)]
mod tests {
    use super::{ClientError, MAX_REDIRECTS, check_redirect, check_url, is_public_ip};
    use std::net::IpAddr;
    use url::Url;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap_or_else(|error| panic!("test URL must parse: {error}"))
    }

    fn ip(text: &str) -> IpAddr {
        text.parse()
            .unwrap_or_else(|error| panic!("test IP must parse: {error}"))
    }

    #[test]
    fn public_addresses_are_allowed() {
        for public in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "2606:4700::1111",
            "2002:0808:0808::1",
        ] {
            assert!(is_public_ip(ip(public)), "{public} is public");
        }
    }

    #[test]
    fn non_public_addresses_are_refused() {
        for private in [
            "0.0.0.0",
            "0.1.2.3",
            "127.0.0.1",
            "127.255.255.254",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "192.0.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::a00:1",
            "2002:7f00:0001::1",
            "2001:db8::1",
            "2001::1",
        ] {
            assert!(!is_public_ip(ip(private)), "{private} must be refused");
        }
    }

    #[test]
    fn urls_must_be_https_with_public_literal_hosts() {
        assert!(check_url(&url("https://example.com/x"), false).is_ok());
        assert!(check_url(&url("https://1.1.1.1/x"), false).is_ok());
        assert!(matches!(
            check_url(&url("http://example.com/x"), false),
            Err(ClientError::InsecureScheme { .. })
        ));
        for private in [
            "https://127.0.0.1/x",
            "https://10.1.2.3/x",
            "https://192.168.0.10/x",
            "https://[::1]/x",
        ] {
            assert!(
                matches!(
                    check_url(&url(private), false),
                    Err(ClientError::ForbiddenAddress { .. })
                ),
                "{private} must be refused"
            );
        }
        assert!(check_url(&url("http://127.0.0.1/x"), true).is_ok());
    }

    #[test]
    fn redirects_are_rechecked_and_limited() {
        let public = url("https://example.com/next");
        assert!(check_redirect(1, &public, false).is_ok());
        assert!(check_redirect(MAX_REDIRECTS, &public, false).is_ok());
        assert_eq!(
            check_redirect(MAX_REDIRECTS + 1, &public, false),
            Err(ClientError::TooManyRedirects)
        );
        assert!(matches!(
            check_redirect(1, &url("https://192.168.1.1/admin"), false),
            Err(ClientError::ForbiddenAddress { .. })
        ));
        assert!(matches!(
            check_redirect(1, &url("https://10.0.0.5/"), false),
            Err(ClientError::ForbiddenAddress { .. })
        ));
        assert!(matches!(
            check_redirect(1, &url("http://example.com/"), false),
            Err(ClientError::InsecureScheme { .. })
        ));
        // The redirect limit holds even when insecure hosts are allowed.
        assert_eq!(
            check_redirect(MAX_REDIRECTS + 1, &url("http://127.0.0.1/"), true),
            Err(ClientError::TooManyRedirects)
        );
    }
}
