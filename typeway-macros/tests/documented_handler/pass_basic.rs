use typeway_macros::documented_handler;

/// List all users.
///
/// Returns a paginated list of users with optional filtering.
#[documented_handler]
async fn list_users() -> &'static str {
    "users"
}

/// Get a user by ID.
#[documented_handler(tags = "users")]
async fn get_user() -> &'static str {
    "user"
}

/// Create a user.
///
/// Creates a new user in the database.
#[documented_handler(tags = "users, admin")]
async fn create_user() -> &'static str {
    "created"
}

fn main() {
    // Verify the generated constants exist and have the right types.
    let _: typeway_core::HandlerDoc = LIST_USERS_DOC;
    let _: typeway_core::HandlerDoc = GET_USER_DOC;
    let _: typeway_core::HandlerDoc = CREATE_USER_DOC;

    // Verify values.
    assert_eq!(LIST_USERS_DOC.summary, "List all users.");
    assert_eq!(
        LIST_USERS_DOC.description,
        "Returns a paginated list of users with optional filtering."
    );
    assert_eq!(LIST_USERS_DOC.operation_id, "list_users");
    assert!(LIST_USERS_DOC.tags.is_empty());

    assert_eq!(GET_USER_DOC.summary, "Get a user by ID.");
    assert_eq!(GET_USER_DOC.description, "");
    assert_eq!(GET_USER_DOC.operation_id, "get_user");
    assert_eq!(GET_USER_DOC.tags, &["users"]);

    assert_eq!(CREATE_USER_DOC.summary, "Create a user.");
    assert_eq!(CREATE_USER_DOC.description, "Creates a new user in the database.");
    assert_eq!(CREATE_USER_DOC.operation_id, "create_user");
    assert_eq!(CREATE_USER_DOC.tags, &["users", "admin"]);
}
