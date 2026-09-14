//! The client against a fake core.
//!
//! A real WebSocket server on a real port, answering the way the core does:
//! `core.subscribe` first, then `event/snapshot`, a delta, a binary frame and
//! `event/flush`. Testing against this rather than against a mock of the
//! client's own transport is what catches a framing mistake.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use godwinmix_client::{Client, Event, UI_EVENTS};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

/// Start the fake core. Answers one client, then stops.
async fn fake_core() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a free port");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (socket, _) = listener.accept().await.expect("a client");
        let mut ws = tokio_tungstenite::accept_async(socket).await.expect("a handshake");
        while let Some(Ok(message)) = ws.next().await {
            let Message::Text(text) = message else { continue };
            let call: Value = serde_json::from_str(&text).expect("a JSON-RPC request");
            let id = call["id"].clone();
            match call["method"].as_str().unwrap_or("") {
                "core.subscribe" => {
                    reply(&mut ws, id, json!({"seq": 41, "events": ["*"], "ignored_ext": []})).await;
                    notify(&mut ws, "event/snapshot", snapshot()).await;
                    notify(
                        &mut ws,
                        "event/source.state",
                        json!({"source": "cam2", "state": "live"}),
                    )
                    .await;
                    notify(
                        &mut ws,
                        "event/multiview.layout",
                        json!({"id": 7, "width": 640, "height": 360, "cells": [
                            {"index": 0, "source": "cam1", "x": 0, "y": 0, "w": 320, "h": 180}
                        ]}),
                    )
                    .await;
                    let mut frame = Vec::new();
                    frame.extend_from_slice(&3u32.to_le_bytes());
                    frame.extend_from_slice(&7u32.to_le_bytes());
                    frame.extend_from_slice(&1500u64.to_le_bytes());
                    frame.extend_from_slice(&[0xff, 0xd8, 0xff, 0xd9]);
                    ws.send(Message::Binary(frame.into())).await.unwrap();
                    notify(&mut ws, "event/flush", json!({"seq": 44})).await;
                }
                "program.take" => {
                    let source = call["params"]["source"].clone();
                    if source.as_str() == Some("cam9") {
                        let error = json!({
                            "code": -32004,
                            "message": "no source cam9. This core has cam1 and cam2.",
                            "data": {"retryable": false}
                        });
                        let body = json!({"jsonrpc": "2.0", "id": id, "error": error});
                        ws.send(Message::Text(body.to_string().into())).await.unwrap();
                        continue;
                    }
                    reply(&mut ws, id, json!({"program": source, "running_time_ms": 1600})).await;
                    notify(&mut ws, "event/program.took", json!({"source": source})).await;
                    notify(&mut ws, "event/flush", json!({"seq": 45})).await;
                }
                other => reply(&mut ws, id, json!({"echo": other})).await,
            }
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn snapshot() -> Value {
    json!({
        "seq": 42,
        "state": {
            "program": "cam1",
            "running_time_ms": 1500,
            "uptime_secs": 12,
            "backend": {},
            "multiview": {},
            "outputs": [],
            "sources": [
                {"id": "cam1", "name": "Camera 1", "uri": "rtmp://a", "state": "live",
                 "has_video": true, "has_audio": true},
                {"id": "cam2", "name": "Camera 2", "uri": "rtmp://b", "state": "connecting",
                 "has_video": true, "has_audio": true}
            ]
        }
    })
}

type Socket = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

async fn reply(ws: &mut Socket, id: Value, result: Value) {
    let body = json!({"jsonrpc": "2.0", "id": id, "result": result});
    ws.send(Message::Text(body.to_string().into())).await.unwrap();
}

async fn notify(ws: &mut Socket, method: &str, params: Value) {
    let body = json!({"jsonrpc": "2.0", "method": method, "params": params});
    ws.send(Message::Text(body.to_string().into())).await.unwrap();
}

#[tokio::test]
async fn subscribe_then_snapshot_then_flush() {
    let base = fake_core().await;
    let client = Client::connect(&base, Some("t")).await.expect("connect");
    let result = client.subscribe(&UI_EVENTS, json!({})).await.expect("subscribe");
    assert_eq!(result.seq, 41);

    let seq = tokio::time::timeout(Duration::from_secs(5), client.next_flush())
        .await
        .expect("a flush within five seconds")
        .expect("a flush");
    assert_eq!(seq, 44);

    let state = client.state();
    assert_eq!(state.program(), Some("cam1"));
    assert_eq!(state.status.sources.len(), 2);
    // The delta that arrived after the snapshot has been folded in.
    assert_eq!(state.source("cam2").map(|s| s.state.as_str()), Some("live"));
    assert_eq!(state.layout.as_ref().map(|l| l.id), Some(7));
}

#[tokio::test]
async fn a_take_moves_the_store_and_the_event_arrives() {
    let base = fake_core().await;
    let client = Client::connect(&base, None).await.expect("connect");
    let mut events = client.events();
    client.subscribe(&UI_EVENTS, json!({})).await.expect("subscribe");
    client.next_flush().await.expect("the first flush");

    let program = client.take(Some("cam2")).await.expect("take");
    assert_eq!(program.program.as_deref(), Some("cam2"));

    let mut took = None;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
            Ok(Ok(Event::ProgramTook(ev))) => {
                took = ev.source;
                break;
            }
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }
    assert_eq!(took.as_deref(), Some("cam2"));
    assert_eq!(client.state().program(), Some("cam2"));
}

#[tokio::test]
async fn a_binary_frame_arrives_with_its_header_read() {
    let base = fake_core().await;
    let client = Client::connect(&base, None).await.expect("connect");
    let mut events = client.events();
    client.subscribe(&UI_EVENTS, json!({"multiview": {"fps": 4}})).await.expect("subscribe");

    let mut seen = None;
    for _ in 0..20 {
        match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
            Ok(Ok(Event::MultiviewFrame(frame))) => {
                seen = Some(frame);
                break;
            }
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }
    let frame = seen.expect("a mosaic frame");
    assert_eq!(frame.seq, 3);
    assert_eq!(frame.layout, 7);
    assert_eq!(frame.running_time_ms, 1500);
    assert_eq!(frame.jpeg, vec![0xff, 0xd8, 0xff, 0xd9]);

    // The layout the frame names is the one the store holds, so cutting a cell
    // out of the sheet is safe.
    let state = client.state();
    let layout = state.layout.as_ref().expect("a layout");
    let cell = frame.cell(layout, "cam1").expect("cam1 has a cell");
    assert_eq!((cell.x, cell.y, cell.w, cell.h), (0, 0, 320, 180));
}

#[tokio::test]
async fn a_refusal_keeps_its_shape() {
    let base = fake_core().await;
    let client = Client::connect(&base, None).await.expect("connect");
    let error = client.take(Some("cam9")).await.expect_err("cam9 does not exist");
    assert_eq!(error.code(), Some(godwinmix_client::codes::NOT_FOUND));
    assert!(!error.retryable());
    assert_eq!(error.next_step(), Some("This core has cam1 and cam2."));
}

#[tokio::test]
async fn a_call_on_a_closed_socket_answers_rather_than_hanging() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
        // Take the first call and then go away without answering it, which is
        // what a core being restarted mid show looks like.
        let _ = ws.next().await;
        let _ = ws.close(None).await;
    });
    let client = Client::connect(&format!("http://127.0.0.1:{port}"), None).await.unwrap();
    let error = client.take(Some("cam1")).await.expect_err("the socket went away");
    assert_eq!(error, godwinmix_client::Error::Closed);
    assert!(error.retryable());
}
