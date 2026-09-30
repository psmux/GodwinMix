//! A plain HTTP request to a show, and its answer back, streamed.
//!
//! An MJPEG preview or a PCM monitor is an answer that never ends: its body
//! is handed on chunk by chunk as the show writes it, and when the browser
//! goes away the connection to the show is dropped with it, which is what
//! tells the show to stop building the stream. An upload goes the other way
//! the same way.

use super::super::state::Station;
use super::{show_in, without_show};
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderName, StatusCode};
use axum::response::{IntoResponse, Response};
use godwinmix_protocol::error::RpcError;
use std::sync::Arc;
use tracing::debug;

/// Headers that belong to one hop and are not passed on.
const HOP: &[&str] = &["connection", "keep-alive", "proxy-connection", "transfer-encoding", "te", "trailer", "upgrade", "host", "content-length"];

fn passed(name: &HeaderName) -> bool {
    !HOP.contains(&name.as_str())
}

/// The URL on the show for a request the station received.
pub fn upstream_url(scheme: &str, addr: std::net::SocketAddr, path: &str, query: Option<&str>) -> String {
    match without_show(query) {
        Some(q) => format!("{scheme}://{addr}{path}?{q}"),
        None => format!("{scheme}://{addr}{path}"),
    }
}

/// Pass `req` to the show it names and hand back what it answers.
pub async fn forward(st: Arc<Station>, req: Request) -> Response {
    let show = show_in(req.uri().query()).unwrap_or_else(|| st.first());
    let addr = match st.addr_of(&show).await {
        Ok(a) => a,
        Err(e) => return refusal(&e),
    };
    let url = upstream_url("http", addr, req.uri().path(), req.uri().query());
    let (parts, body) = req.into_parts();
    let mut out = st.http.request(parts.method.clone(), &url);
    for (name, value) in parts.headers.iter().filter(|(n, _)| passed(n)) {
        out = out.header(name, value);
    }
    let out = out.body(reqwest::Body::wrap_stream(body.into_data_stream()));
    match out.send().await {
        Ok(answer) => {
            let status = answer.status();
            let mut headers = HeaderMap::new();
            for (name, value) in answer.headers().iter().filter(|(n, _)| passed(n)) {
                headers.append(name.clone(), value.clone());
            }
            let body = Body::from_stream(answer.bytes_stream());
            let mut response = Response::new(body);
            *response.status_mut() = status;
            *response.headers_mut() = headers;
            response
        }
        Err(e) => {
            debug!(%show, %url, error = %e, "a show did not answer a relayed request");
            let e = RpcError::internal(format!(
                "show {show} did not answer ({e}). It may be restarting; show.list says how it is."
            ))
            .with("show", show.as_str());
            (StatusCode::BAD_GATEWAY, axum::Json(e.body(&godwinmix_protocol::trace::new_id()))).into_response()
        }
    }
}

/// An error the station decided on, in the shape `/api/v1` answers with.
pub fn refusal(e: &RpcError) -> Response {
    crate::control::rest::error_response(e, &godwinmix_protocol::trace::new_id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_on_the_show_keeps_the_path_and_the_query_but_not_the_show() {
        let addr = "127.0.0.1:5000".parse().unwrap();
        assert_eq!(upstream_url("http", addr, "/mjpeg/program", Some("show=b&fps=5")), "http://127.0.0.1:5000/mjpeg/program?fps=5");
        assert_eq!(upstream_url("ws", addr, "/rpc", None), "ws://127.0.0.1:5000/rpc");
        assert!(!passed(&HeaderName::from_static("host")));
        assert!(passed(&HeaderName::from_static("authorization")));
    }
}
