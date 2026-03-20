use typeway_macros::documented_handler;

// Handler with no doc comments — should still generate a valid HandlerDoc.
#[documented_handler]
async fn bare_handler() -> &'static str {
    "bare"
}

fn main() {
    let _: typeway_core::HandlerDoc = BARE_HANDLER_DOC;
    assert_eq!(BARE_HANDLER_DOC.summary, "");
    assert_eq!(BARE_HANDLER_DOC.description, "");
    assert_eq!(BARE_HANDLER_DOC.operation_id, "bare_handler");
    assert!(BARE_HANDLER_DOC.tags.is_empty());
}
