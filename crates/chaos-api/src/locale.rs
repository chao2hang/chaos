//! Request locale from `Accept-Language`.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chaos_i18n::Locale;

/// Axum extractor: normalized locale for the current request.
#[derive(Debug, Clone, Copy)]
pub struct RequestLocale(pub Locale);

impl<S> FromRequestParts<S> for RequestLocale
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok());
        Ok(RequestLocale(Locale::from_accept_language(header)))
    }
}
