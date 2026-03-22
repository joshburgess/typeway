// Versioned API with change markers compiles and satisfies ApiSpec.

use typeway::prelude::*;
use typeway_core::versioning::*;

typeway_path!(type UsersPath = "users");
typeway_path!(type ProfilePath = "profile");
typeway_path!(type SettingsPath = "settings");

#[derive(serde::Serialize, serde::Deserialize)]
struct UserV1 { name: String }
#[derive(serde::Serialize, serde::Deserialize)]
struct UserV2 { name: String, email: String }
#[derive(serde::Serialize, serde::Deserialize)]
struct Profile { bio: String }
#[derive(serde::Serialize, serde::Deserialize)]
struct Settings { theme: String }

type V1 = (
    GetEndpoint<UsersPath, Vec<UserV1>>,
    GetEndpoint<SettingsPath, Settings>,
);

type V2Changes = (
    Added<GetEndpoint<ProfilePath, Profile>>,
    Replaced<GetEndpoint<UsersPath, Vec<UserV1>>, GetEndpoint<UsersPath, Vec<UserV2>>>,
    Deprecated<GetEndpoint<SettingsPath, Settings>>,
);

type V2Resolved = (
    GetEndpoint<UsersPath, Vec<UserV2>>,
    GetEndpoint<SettingsPath, Settings>,
    GetEndpoint<ProfilePath, Profile>,
);

type V2 = VersionedApi<V1, V2Changes, V2Resolved>;

fn _check() {
    fn _assert_api<T: typeway_core::ApiSpec>() {}
    fn _assert_changelog<T: ApiChangelog>() {}

    // VersionedApi satisfies ApiSpec.
    _assert_api::<V2>();

    // Change markers satisfy ApiChangelog.
    _assert_changelog::<V2Changes>();

    // Verify counts at compile time.
    const _: () = assert!(
        <V2Changes as ApiChangelog>::ADDED == 1
            && <V2Changes as ApiChangelog>::REPLACED == 1
            && <V2Changes as ApiChangelog>::DEPRECATED == 1
            && <V2Changes as ApiChangelog>::REMOVED == 0
    );
}

fn main() {}
