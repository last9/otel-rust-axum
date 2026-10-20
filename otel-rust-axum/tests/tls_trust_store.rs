//! Smoke checks for the bundled TLS trust store (`webpki-roots` + `rustls-webpki`).
//!
//! These hit the public internet, so they are ignored by default. Run them after
//! any change to the TLS dependency set:
//!
//! ```sh
//! cargo test --all-features --test tls_trust_store -- --ignored
//! ```
#![cfg(feature = "reqwest")]

use otel_rust_axum::client::TracedClient;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(30);

/// Chains under the eMudhra intermediates whose private keys were disclosed on
/// 2026-10-20 (https://emudhra.we-have-a-problem.com/). A trust store that still
/// accepts this endpoint can be impersonated by anyone holding those keys.
#[tokio::test]
#[ignore = "network: run with --ignored"]
async fn rejects_compromised_emudhra_chain() {
    let err = TracedClient::new()
        .get("https://dv-g3b-emudhra.we-have-a-problem.com/")
        .timeout(TIMEOUT)
        .send()
        .await
        .expect_err("certificate chained to a compromised eMudhra CA was accepted");

    // reqwest's Debug output includes the rustls error from the source chain.
    let debug = format!("{err:?}");
    assert!(
        err.is_connect() && debug.contains("InvalidCertificate"),
        "expected a certificate validation failure, got: {debug}"
    );
}

/// Control: the same client must still complete a handshake with a normally
/// trusted endpoint, so the test above can't pass by rejecting everything.
#[tokio::test]
#[ignore = "network: run with --ignored"]
async fn accepts_trusted_endpoint() {
    let resp = TracedClient::new()
        .get("https://www.google.com/")
        .timeout(TIMEOUT)
        .send()
        .await
        .expect("handshake with a trusted endpoint failed");

    assert!(
        resp.status().is_success(),
        "unexpected status: {}",
        resp.status()
    );
}
