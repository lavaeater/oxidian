//! The one HTTP client every request in this crate goes through.
//!
//! It exists for Android. `reqwest` 0.13 verifies certificates with
//! `rustls-platform-verifier` under *every* one of its rustls features — there
//! is no webpki-roots feature to pick instead — and on Android that verifier
//! panics on first use unless the app has initialised it:
//!
//! ```text
//! Expect rustls-platform-verifier to be initialized
//! ```
//!
//! Initialising it is not an option here. It calls into the JVM through a small
//! Kotlin component that has to be added to the app's Gradle build (see that
//! crate's Android section), and `dx`'s Android template has no hook for
//! shipping one — so the fix would live outside the Rust workspace entirely.
//!
//! Instead Android verifies against the Mozilla root set compiled into the
//! binary (`webpki-root-certs`), via `tls_certs_only`, which is the one builder
//! path in reqwest 0.13 that bypasses the platform verifier. The trade-off is
//! that a user-installed or enterprise CA in the Android trust store is not
//! honoured; for an app that talks to github.com and gitlab.com that costs
//! nothing real, and the alternative is an app that cannot make one HTTPS
//! request.
//!
//! Everything else — desktop, and the browser, where the fetch API does TLS —
//! keeps the default, so this narrows to the platform that is actually broken.
//!
//! Sharing one client is the other half: a `Client` owns the connection pool,
//! so building one per request (as this crate used to) threw away every kept
//! connection and re-ran a TLS handshake per call.

use std::sync::OnceLock;

use reqwest::Client;

/// The process-wide client. Cloning it is cheap — it's an `Arc` around the pool.
pub(crate) fn client() -> Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(build).clone()
}

#[cfg(target_os = "android")]
fn build() -> Client {
    let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .filter_map(|der| reqwest::Certificate::from_der(der).ok());
    Client::builder()
        .tls_certs_only(roots)
        .build()
        // The roots are compiled-in constants, so a failure here is a bug in
        // this build, not a condition any user can hit or recover from.
        .expect("the built-in Mozilla root certificates should parse")
}

#[cfg(not(target_os = "android"))]
fn build() -> Client {
    Client::new()
}
