//! Session type primitives for protocol-typed WebSocket channels.
//!
//! Session types encode a communication protocol as a sequence of send/receive
//! operations at the type level. Each operation consumes the current channel
//! state and produces a new one at the next protocol step. Rust's ownership
//! system enforces linearity: you cannot use a channel in the wrong state
//! because the old state has been moved.
//!
//! # Example
//!
//! A chat room protocol where the server sends a welcome, then loops
//! receiving messages and broadcasting responses:
//!
//! ```
//! use typeway_core::session::*;
//!
//! // Server-side protocol:
//! // 1. Send a welcome message
//! // 2. Enter a loop:
//! //    a. Receive a chat message
//! //    b. Send a broadcast
//! //    c. Loop back
//! type ChatProtocol = Send<String, Rec<Recv<String, Send<String, Var>>>>;
//! ```
//!
//! # Combinators
//!
//! - [`Send<T, Next>`] — send a message of type `T`, continue with `Next`
//! - [`Recv<T, Next>`] — receive a message of type `T`, continue with `Next`
//! - [`Offer<L, R>`] — offer a choice; the remote peer decides the branch
//! - [`Select<L, R>`] — select a branch; the local side decides
//! - [`End`] — protocol termination
//! - [`Rec<Body>`] / [`Var`] — recursive protocol (loop back to enclosing `Rec`)

use std::marker::PhantomData;

/// Send a message of type `T`, then continue with protocol `Next`.
pub struct Send<T, Next>(PhantomData<(T, Next)>);

/// Receive a message of type `T`, then continue with protocol `Next`.
pub struct Recv<T, Next>(PhantomData<(T, Next)>);

/// Offer a choice between two protocol branches.
///
/// The remote peer decides which branch to take by sending a branch
/// selection message. Use with [`TypedWebSocket::offer`] on the server.
pub struct Offer<Left, Right>(PhantomData<(Left, Right)>);

/// Select one of two protocol branches.
///
/// The local side decides which branch to take. Use with
/// [`TypedWebSocket::select_left`] or [`TypedWebSocket::select_right`].
pub struct Select<Left, Right>(PhantomData<(Left, Right)>);

/// Protocol termination. No further messages can be sent or received.
pub struct End;

/// Recursive protocol marker.
///
/// Wraps a protocol body that eventually contains [`Var`] to loop back.
/// When the protocol reaches `Var`, it restarts from the `Rec` body.
///
/// # Example
///
/// ```
/// use typeway_core::session::*;
///
/// // A protocol that repeatedly receives a String and sends a String back:
/// type EchoLoop = Rec<Recv<String, Send<String, Var>>>;
/// ```
pub struct Rec<Body>(PhantomData<Body>);

/// Variable referencing the enclosing [`Rec`].
///
/// When reached, the protocol loops back to the `Rec` body.
pub struct Var;

/// Marker trait for valid session types.
///
/// Implemented for all session type combinators. Used as a bound to ensure
/// that only well-formed protocol types are accepted by typed channels.
pub trait SessionType {}

impl<T, N: SessionType> SessionType for Send<T, N> {}
impl<T, N: SessionType> SessionType for Recv<T, N> {}
impl<L: SessionType, R: SessionType> SessionType for Offer<L, R> {}
impl<L: SessionType, R: SessionType> SessionType for Select<L, R> {}
impl SessionType for End {}
impl<B: SessionType> SessionType for Rec<B> {}
impl SessionType for Var {}

/// Compute the dual (mirror) of a session type.
///
/// The dual swaps `Send` with `Recv` and `Offer` with `Select`, which
/// represents the protocol from the other peer's perspective.
pub trait Dual {
    /// The dual session type.
    type Output: SessionType;
}

impl<T, N: Dual> Dual for Send<T, N>
where
    N::Output: SessionType,
{
    type Output = Recv<T, N::Output>;
}

impl<T, N: Dual> Dual for Recv<T, N>
where
    N::Output: SessionType,
{
    type Output = Send<T, N::Output>;
}

impl<L: Dual, R: Dual> Dual for Offer<L, R>
where
    L::Output: SessionType,
    R::Output: SessionType,
{
    type Output = Select<L::Output, R::Output>;
}

impl<L: Dual, R: Dual> Dual for Select<L, R>
where
    L::Output: SessionType,
    R::Output: SessionType,
{
    type Output = Offer<L::Output, R::Output>;
}

impl Dual for End {
    type Output = End;
}

impl<B: Dual> Dual for Rec<B>
where
    B::Output: SessionType,
{
    type Output = Rec<B::Output>;
}

impl Dual for Var {
    type Output = Var;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify SessionType is implemented for all combinators.
    fn assert_session_type<S: SessionType>() {}

    #[test]
    fn session_type_impls() {
        assert_session_type::<End>();
        assert_session_type::<Var>();
        assert_session_type::<Send<String, End>>();
        assert_session_type::<Recv<u32, End>>();
        assert_session_type::<Offer<End, End>>();
        assert_session_type::<Select<End, End>>();
        assert_session_type::<Rec<Send<String, Var>>>();
    }

    #[test]
    fn complex_protocol_compiles() {
        // Chat room protocol:
        // Server sends welcome, then offers:
        //   Left: receive chat msg, send broadcast, loop
        //   Right: receive leave msg, end
        type ChatProtocol = Send<
            String, // welcome
            Recv<
                String, // join msg
                Offer<
                    Recv<String, Send<String, Rec<Var>>>, // chat loop
                    Recv<String, End>,                     // leave
                >,
            >,
        >;
        assert_session_type::<ChatProtocol>();
    }

    // Verify Dual trait works.
    fn assert_dual<S: Dual>()
    where
        S::Output: SessionType,
    {
    }

    #[test]
    fn dual_send_is_recv() {
        // Dual of Send<T, End> is Recv<T, End>
        fn check<S: Dual<Output = Recv<String, End>>>() {}
        check::<Send<String, End>>();
    }

    #[test]
    fn dual_offer_is_select() {
        fn check<S: Dual<Output = Select<End, End>>>() {}
        check::<Offer<End, End>>();
    }

    #[test]
    fn dual_complex() {
        assert_dual::<Send<String, Recv<u32, End>>>();
        assert_dual::<Rec<Send<String, Recv<String, Var>>>>();
    }
}
