//! Shared outbound HTTP clients.
//!
//! A `reqwest::Client` owns a connection pool and a TLS session cache, so
//! building one per call discards both and repeats TLS setup on every request.
//! Callers therefore keep one `LazyLock<Option<Client>>` per distinct
//! configuration.
//!
//! The first dereference always happens inside a request handler or a startup
//! task, so a Tokio runtime is running when the client is constructed.

use std::time::Duration;

/// Build a client with `timeout` and `user_agent` applied.
///
/// Returns `None` when the client cannot be constructed — in practice when the
/// TLS backend fails to initialise. This is deliberately fallible rather than
/// papered over: the obvious fallback, `reqwest::Client::new()`, is itself
/// `build().expect(...)` and so panics in exactly the situation that lands here.
/// A panic inside a `LazyLock` initializer poisons the static for the rest of
/// the process, which would take out every later request that shares it; an
/// `Option` instead lets each caller fail only the request in front of it.
pub fn build_client(timeout: Duration, user_agent: &str) -> Option<reqwest::Client> {
    match reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(user_agent)
        .build()
    {
        Ok(client) => Some(client),
        Err(error) => {
            tracing::error!(%error, "failed to build http client");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_client_for_each_configuration() {
        // Exercises the builder so a broken configuration fails here and not on
        // the first request that needs it.
        assert!(build_client(Duration::from_secs(15), "chaos-api/test").is_some());
        assert!(build_client(Duration::from_secs(300), "chaos-self-update/test").is_some());
    }
}
