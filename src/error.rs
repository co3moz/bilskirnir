use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
#[derive(Debug)]
pub struct Error(pub StatusCode, pub String);
pub type Result<T> = std::result::Result<T, Error>;
impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        if let Some(sql) = e.downcast_ref::<rusqlite::Error>() {
            return if matches!(sql, rusqlite::Error::QueryReturnedNoRows) {
                missing()
            } else {
                Self(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            };
        }
        Self(StatusCode::BAD_REQUEST, format!("{e:#}"))
    }
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
pub fn missing() -> Error {
    Error(StatusCode::NOT_FOUND, "Record not found".into())
}
