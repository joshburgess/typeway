//! Tests for session-typed WebSocket channels.
//!
//! These are primarily compile-time tests verifying that the type system
//! enforces correct protocol usage.

#![cfg(feature = "ws")]

use typeway_core::session::*;
use typeway_server::typed_ws::{Either, TypedWebSocket};

/// Verify that a `Send` state exposes `send` but not `recv`.
/// This is a compile-time property: the test just confirms the correct
/// usage compiles. Incorrect usage (calling recv on a Send state) would
/// be a compile error.
fn _compile_test_send_has_send_method(_ws: TypedWebSocket<Send<String, End>>) {
    // ws.send("hello".to_string()) would compile — it's an async fn so
    // we can't call it in a non-async context, but the method exists.
    // The important thing is that recv() is NOT available here.
    let _ = _ws;
}

fn _compile_test_recv_has_recv_method(_ws: TypedWebSocket<Recv<String, End>>) {
    // ws.recv() would compile. send() is NOT available.
    let _ = _ws;
}

fn _compile_test_end_has_close_method(_ws: TypedWebSocket<End>) {
    // ws.close() would compile. Neither send() nor recv() is available.
    let _ = _ws;
}

fn _compile_test_offer_has_offer_method(
    _ws: TypedWebSocket<Offer<Send<String, End>, Recv<u32, End>>>,
) {
    let _ = _ws;
}

fn _compile_test_select_has_select_methods(
    _ws: TypedWebSocket<Select<Send<String, End>, Recv<u32, End>>>,
) {
    let _ = _ws;
}

fn _compile_test_rec_has_enter(_ws: TypedWebSocket<Rec<Send<String, Var>>>) {
    let _entered: TypedWebSocket<Send<String, Var>> = _ws.enter();
}

/// Verify that a full protocol handler type-checks.
/// This function cannot actually run (no real WebSocket), but it must compile.
async fn _compile_test_full_protocol(ws: TypedWebSocket<Send<String, Recv<String, End>>>) {
    let ws = ws.send("hello".to_string()).await.unwrap();
    let (_msg, ws) = ws.recv().await.unwrap();
    ws.close().await.unwrap();
}

/// Verify that a recursive protocol type-checks.
async fn _compile_test_recursive_protocol(ws: TypedWebSocket<Rec<Recv<String, Send<String, Var>>>>) {
    let ws = ws.enter();
    let (msg, ws) = ws.recv().await.unwrap();
    let ws = ws.send(msg).await.unwrap();
    // At Var state, recurse back to Rec
    let _ws: TypedWebSocket<Rec<Recv<String, Send<String, Var>>>> =
        ws.recurse::<Recv<String, Send<String, Var>>>();
}

/// Verify that branching with Offer works at the type level.
async fn _compile_test_offer_branching(
    ws: TypedWebSocket<Offer<Send<String, End>, Recv<u32, End>>>,
) {
    match ws.offer().await.unwrap() {
        Either::Left(ws) => {
            let ws = ws.send("left branch".to_string()).await.unwrap();
            ws.close().await.unwrap();
        }
        Either::Right(ws) => {
            let (_val, ws) = ws.recv().await.unwrap();
            ws.close().await.unwrap();
        }
    }
}

/// Verify that Select works at the type level.
async fn _compile_test_select(ws: TypedWebSocket<Select<Send<String, End>, Recv<u32, End>>>) {
    // We can choose left...
    let ws = ws.select_left().await.unwrap();
    let ws = ws.send("chose left".to_string()).await.unwrap();
    ws.close().await.unwrap();
}

/// The protocol types compose correctly with the existing WebSocketUpgrade.
fn _compile_test_upgrade_typed(upgrade: typeway_server::ws::WebSocketUpgrade) {
    type Proto = Send<String, Recv<String, End>>;
    let _response = upgrade.on_upgrade_typed::<Proto, _, _>(|ws| async move {
        let ws = ws.send("hello".to_string()).await.unwrap();
        let (_reply, ws) = ws.recv().await.unwrap();
        ws.close().await.unwrap();
    });
}

#[test]
fn session_typed_ws_compiles() {
    // This test exists to confirm that the above compile-time tests
    // are actually checked by the compiler. If any of them had type errors,
    // this test file would fail to compile.
}
