//! Errores de la API: siempre status + `{"error": "<código>", "message": "<texto en inglés>"}`
//! (docs/API.md, "Errors"). La UI traduce por el código. También envuelve los extractores de axum
//! (`Json`, `Path`, `Query`) para que sus rechazos salgan con ese formato.
use axum::{
    extract::{rejection::*, FromRequest, FromRequestParts},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    /// Código estable (`unauthorized`, `not_found`, `source_error`...).
    pub code: &'static str,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into() }
    }
    pub fn bad_request(m: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", m)
    }
    pub fn unauthorized(m: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", m)
    }
    pub fn forbidden(m: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", m)
    }
    pub fn not_found(m: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", m)
    }
    pub fn conflict(m: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", m)
    }
    pub fn too_large(m: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "too_large", m)
    }
    pub fn rate_limited(m: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", m)
    }
    /// La fuente de manga o Suwayomi devolvió un error.
    pub fn source(m: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, "source_error", m)
    }
    /// Suwayomi no responde.
    pub fn unavailable(m: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, "unavailable", m)
    }
    /// Error interno: se loguea el detalle y al cliente le llega un texto genérico.
    pub fn internal(e: impl std::fmt::Display) -> Self {
        tracing::error!("internal error: {e:#}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "Internal server error")
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self::internal(format!("{e:#}"))
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        Self::internal(e)
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        Self::internal(e)
    }
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    message: &'a str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, axum::Json(ErrorBody { error: self.code, message: &self.message })).into_response()
    }
}

/// Rechazo de un extractor: 413 si el cuerpo es muy grande, si no 400.
fn rejection(status: StatusCode, text: String) -> ApiError {
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::too_large(text)
    } else if status == StatusCode::NOT_FOUND {
        ApiError::not_found(text)
    } else {
        ApiError::new(StatusCode::BAD_REQUEST, "bad_request", text)
    }
}

impl From<JsonRejection> for ApiError {
    fn from(r: JsonRejection) -> Self {
        rejection(r.status(), format!("Invalid JSON body: {}", r.body_text()))
    }
}
impl From<PathRejection> for ApiError {
    fn from(r: PathRejection) -> Self {
        rejection(r.status(), format!("Invalid path: {}", r.body_text()))
    }
}
impl From<QueryRejection> for ApiError {
    fn from(r: QueryRejection) -> Self {
        rejection(r.status(), format!("Invalid query: {}", r.body_text()))
    }
}
impl From<axum::extract::multipart::MultipartRejection> for ApiError {
    fn from(r: axum::extract::multipart::MultipartRejection) -> Self {
        rejection(r.status(), format!("Invalid upload: {}", r.body_text()))
    }
}
impl From<axum::extract::multipart::MultipartError> for ApiError {
    fn from(r: axum::extract::multipart::MultipartError) -> Self {
        rejection(r.status(), format!("Upload failed: {}", r.body_text()))
    }
}

/// `axum::Json` con rechazo en formato API.
#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct Json<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

/// `axum::extract::Path` con rechazo en formato API.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct Path<T>(pub T);

/// `axum::extract::Query` con rechazo en formato API.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct Query<T>(pub T);
