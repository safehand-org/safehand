//! The HTTP calendar client against a local one-shot HTTP server.

use std::sync::{Arc, Mutex};

use splitcard_server::ots::{
    calendar::{
        CalendarClient, CalendarError, HttpCalendar, HttpCalendarConfig, OTS_MEDIA_TYPE,
        digest_url, timestamp_url,
    },
    format::MAX_CALENDAR_RESPONSE_BYTES,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use url::Url;

/// Serves one canned response per connection and records each raw request.
async fn serve(responses: Vec<Vec<u8>>) -> (Url, Arc<Mutex<Vec<Vec<u8>>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base =
        Url::parse(&format!("http://{}", listener.local_addr().expect("addr"))).expect("url");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    tokio::spawn(async move {
        for response in responses {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            // Headers, then a body of Content-Length bytes.
            loop {
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                request.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&request).to_lowercase();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length || read == 0 {
                        break;
                    }
                } else if read == 0 {
                    break;
                }
            }
            recorded.lock().expect("lock").push(request);
            let _ = socket.write_all(&response).await;
            let _ = socket.shutdown().await;
        }
    });
    (base, requests)
}

fn response(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes = format!(
        "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n{headers}\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn insecure() -> HttpCalendar {
    HttpCalendar::new(&HttpCalendarConfig {
        allow_insecure_hosts: true,
        ..HttpCalendarConfig::default()
    })
    .expect("client")
}

#[test]
fn urls_are_joined_like_the_reference_client() {
    let pool = Url::parse("https://a.pool.opentimestamps.org").expect("url");
    assert_eq!(
        digest_url(&pool).expect("url").as_str(),
        "https://a.pool.opentimestamps.org/digest"
    );
    assert_eq!(
        timestamp_url(&pool, &[0xab, 0x01]).expect("url").as_str(),
        "https://a.pool.opentimestamps.org/timestamp/ab01"
    );
}

#[tokio::test]
async fn submit_posts_the_raw_digest_and_returns_the_body() {
    let (base, requests) = serve(vec![response("200 OK", "", b"\x00tree")]).await;
    let digest = [0x42_u8; 32];
    let body = insecure().submit(&base, digest).await.expect("accepted");
    assert_eq!(body, b"\x00tree");

    let request = requests.lock().expect("lock").remove(0);
    let text = String::from_utf8_lossy(&request).to_lowercase();
    assert!(text.starts_with("post /digest http/1.1"), "{text}");
    assert!(text.contains(&format!("accept: {OTS_MEDIA_TYPE}")));
    assert!(request.ends_with(&digest));
}

#[tokio::test]
async fn upgrade_maps_404_to_not_yet() {
    let (base, requests) = serve(vec![
        response(
            "404 Not Found",
            "",
            b"Pending confirmation in Bitcoin blockchain",
        ),
        response("200 OK", "", b"upgraded"),
        response("500 Internal Server Error", "", b""),
    ])
    .await;
    let client = insecure();
    assert_eq!(client.upgrade(&base, &[0xab]).await, Ok(None));
    assert_eq!(
        client.upgrade(&base, &[0xab]).await,
        Ok(Some(b"upgraded".to_vec()))
    );
    assert_eq!(
        client.upgrade(&base, &[0xab]).await,
        Err(CalendarError::HttpStatus { status: 500 })
    );
    let first = String::from_utf8_lossy(&requests.lock().expect("lock")[0]).to_lowercase();
    assert!(first.starts_with("get /timestamp/ab http/1.1"), "{first}");
}

#[tokio::test]
async fn oversized_responses_are_rejected() {
    let big = vec![0_u8; MAX_CALENDAR_RESPONSE_BYTES + 1];
    let (base, _) = serve(vec![response("200 OK", "", &big)]).await;
    assert_eq!(
        insecure().submit(&base, [0; 32]).await,
        Err(CalendarError::ResponseTooLarge {
            limit: MAX_CALENDAR_RESPONSE_BYTES
        })
    );
}

#[tokio::test]
async fn redirects_to_other_hosts_are_not_followed() {
    let (base, requests) = serve(vec![response(
        "302 Found",
        "location: http://localhost:1/digest\r\n",
        b"",
    )])
    .await;
    assert_eq!(
        insecure().submit(&base, [0; 32]).await,
        Err(CalendarError::HttpStatus { status: 302 })
    );
    assert_eq!(requests.lock().expect("lock").len(), 1);
}

#[tokio::test]
async fn plain_http_and_private_addresses_are_refused() {
    let secure = HttpCalendar::new(&HttpCalendarConfig::default()).expect("client");
    for calendar in [
        "http://a.pool.opentimestamps.org",
        "https://127.0.0.1",
        "https://[::1]",
    ] {
        let calendar = Url::parse(calendar).expect("url");
        assert!(matches!(
            secure.submit(&calendar, [0; 32]).await,
            Err(CalendarError::RefusedUrl { .. })
        ));
    }
}
