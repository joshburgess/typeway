//! Integration tests for session type primitives.

use typeway_core::session::*;

fn assert_session_type<S: SessionType>() {}
fn assert_dual<S: Dual>()
where
    S::Output: SessionType,
{
}

#[test]
fn all_combinators_implement_session_type() {
    assert_session_type::<End>();
    assert_session_type::<Var>();
    assert_session_type::<Send<String, End>>();
    assert_session_type::<Recv<u32, End>>();
    assert_session_type::<Offer<End, End>>();
    assert_session_type::<Select<End, End>>();
    assert_session_type::<Rec<Send<String, Var>>>();
}

#[test]
fn nested_protocol_compiles() {
    assert_session_type::<Send<String, Recv<u32, Send<Vec<u8>, End>>>>();
}

#[test]
fn complex_chat_protocol_compiles() {
    // A realistic chat room protocol from the server's perspective:
    //
    // 1. Receive a JoinMsg (String)
    // 2. Send a WelcomeMsg (String)
    // 3. Offer a choice:
    //    Left:  Receive ChatMsg (String), Send BroadcastMsg (String), recurse
    //    Right: Receive LeaveMsg (String), End
    //
    // Using String as message type stand-in for all named types.
    type ChatProtocol = Send<
        String, // JoinMsg
        Recv<
            String, // WelcomeMsg
            Offer<
                Recv<
                    String, // ChatMsg
                    Send<
                        String,   // BroadcastMsg
                        Rec<Var>, // loop back
                    >,
                >,
                Recv<String, End>, // LeaveMsg, then done
            >,
        >,
    >;

    assert_session_type::<ChatProtocol>();
}

#[test]
fn recursive_echo_protocol_compiles() {
    // Echo server: repeatedly receive a message and send it back.
    type EchoProtocol = Rec<Recv<String, Send<String, Var>>>;
    assert_session_type::<EchoProtocol>();
}

#[test]
fn deeply_nested_offer_compiles() {
    // Offer within offer — multi-level branching.
    type MultiOffer =
        Offer<Offer<Send<u32, End>, Recv<u64, End>>, Select<Send<String, End>, Recv<Vec<u8>, End>>>;
    assert_session_type::<MultiOffer>();
}

#[test]
fn dual_preserves_session_type() {
    assert_dual::<End>();
    assert_dual::<Send<String, End>>();
    assert_dual::<Recv<u32, End>>();
    assert_dual::<Offer<End, End>>();
    assert_dual::<Select<End, End>>();
    assert_dual::<Rec<Send<String, Recv<String, Var>>>>();
}

#[test]
fn dual_of_send_is_recv() {
    fn check_dual<S: Dual<Output = Recv<String, End>>>() {}
    check_dual::<Send<String, End>>();
}

#[test]
fn dual_of_recv_is_send() {
    fn check_dual<S: Dual<Output = Send<u32, End>>>() {}
    check_dual::<Recv<u32, End>>();
}

#[test]
fn dual_of_offer_is_select() {
    fn check_dual<S: Dual<Output = Select<End, End>>>() {}
    check_dual::<Offer<End, End>>();
}

#[test]
fn dual_of_select_is_offer() {
    fn check_dual<S: Dual<Output = Offer<End, End>>>() {}
    check_dual::<Select<End, End>>();
}
