//! Type-level path segment encoding using heterogeneous lists.
//!
//! Paths are encoded as HLists of [`Lit`] (literal) and [`Capture`] segments.
//! The [`PathSpec`] trait computes the tuple of captured types, and
//! [`ExtractPath`] provides runtime parsing.
//!
//! # Example
//!
//! ```ignore
//! // /users/:u32/posts — encoded as an HList
//! type UserPosts = HCons<Lit<users_lit>, HCons<Capture<u32>, HCons<Lit<posts_lit>, HNil>>>;
//!
//! // PathSpec computes Captures = (u32,)
//! // ExtractPath::extract(&["users", "42", "posts"]) returns Some((42u32,))
//! ```

use std::marker::PhantomData;
use std::str::FromStr;

/// Heterogeneous list terminator.
pub struct HNil;

/// Heterogeneous list cons cell.
///
/// `H` is the head element (a path segment type), `T` is the tail (another
/// `HCons` or `HNil`).
pub struct HCons<H, T>(PhantomData<(H, T)>);

/// Trait implemented by literal path segment marker types.
///
/// Each unique path literal (e.g. `"users"`, `"posts"`) is represented as a
/// zero-sized type implementing this trait. The `path!` proc-macro generates
/// these automatically.
pub trait LitSegment {
    /// The string value this literal matches against.
    const VALUE: &'static str;
}

/// A literal path segment wrapper in the HList.
///
/// `S` is a marker type implementing [`LitSegment`]. Matches exactly the
/// string `S::VALUE` in the URL.
pub struct Lit<S: LitSegment>(PhantomData<S>);

/// A captured path segment of type `T`.
///
/// Parsed from the URL at runtime using [`FromStr`]. The captured value
/// appears in the handler's arguments.
pub struct Capture<T>(PhantomData<T>);

/// Matches all remaining path segments as a `Vec<String>`.
///
/// Must appear only at the tail of a path HList (i.e., `HCons<CaptureRest, HNil>`).
pub struct CaptureRest;

// ---------------------------------------------------------------------------
// Prepend: type-level cons on tuples
// ---------------------------------------------------------------------------

/// Prepends a type `T` onto a tuple, producing a tuple one element larger.
///
/// Used by [`PathSpec`] to accumulate captured types from left to right
/// as the HList is traversed.
pub trait Prepend<T> {
    /// The resulting tuple with `T` prepended.
    type Output;
}

macro_rules! impl_prepend {
    () => {
        impl<T> Prepend<T> for () {
            type Output = (T,);
        }
    };
    ($first:ident $(, $rest:ident)*) => {
        impl<T, $first $(, $rest)*> Prepend<T> for ($first, $($rest,)*) {
            type Output = (T, $first, $($rest,)*);
        }
    };
}

impl_prepend!();
impl_prepend!(A);
impl_prepend!(A, B);
impl_prepend!(A, B, C);
impl_prepend!(A, B, C, D);
impl_prepend!(A, B, C, D, E);
impl_prepend!(A, B, C, D, E, F);
impl_prepend!(A, B, C, D, E, F, G);

// ---------------------------------------------------------------------------
// PathSpec: type-level capture extraction
// ---------------------------------------------------------------------------

/// Computes the tuple of captured types from a path HList.
///
/// This is a type-level catamorphism (fold) over the path structure.
/// Literal segments contribute nothing; capture segments prepend their
/// type to the accumulator.
pub trait PathSpec {
    /// The tuple of captured segment types.
    ///
    /// For example, `HCons<Lit<users>, HCons<Capture<u32>, HNil>>` has
    /// `Captures = (u32,)`.
    type Captures;
}

impl PathSpec for HNil {
    type Captures = ();
}

impl<S: LitSegment, T: PathSpec> PathSpec for HCons<Lit<S>, T> {
    type Captures = T::Captures;
}

impl<U, T: PathSpec> PathSpec for HCons<Capture<U>, T>
where
    T::Captures: Prepend<U>,
{
    type Captures = <T::Captures as Prepend<U>>::Output;
}

impl<T: PathSpec> PathSpec for HCons<CaptureRest, T> {
    type Captures = (Vec<String>,);
}

// ---------------------------------------------------------------------------
// ExtractPath: runtime path parsing
// ---------------------------------------------------------------------------

/// Runtime path parsing, paired with the compile-time [`PathSpec`].
///
/// Implementations match URL path segments against the type-level pattern
/// and extract captured values.
pub trait ExtractPath: PathSpec {
    /// Attempt to parse `segments` according to this path pattern.
    ///
    /// Returns `Some(captures)` if the segments match, `None` otherwise.
    fn extract(segments: &[&str]) -> Option<Self::Captures>;

    /// The OpenAPI-format path pattern string, e.g. `"/users/{id}"`.
    fn pattern() -> String;
}

impl ExtractPath for HNil {
    fn extract(segments: &[&str]) -> Option<()> {
        if segments.is_empty() {
            Some(())
        } else {
            None
        }
    }

    fn pattern() -> String {
        String::new()
    }
}

impl<S: LitSegment, T: ExtractPath> ExtractPath for HCons<Lit<S>, T> {
    fn extract(segments: &[&str]) -> Option<T::Captures> {
        match segments.first() {
            Some(&seg) if seg == S::VALUE => T::extract(&segments[1..]),
            _ => None,
        }
    }

    fn pattern() -> String {
        format!("/{}{}", S::VALUE, T::pattern())
    }
}

impl<U, T> ExtractPath for HCons<Capture<U>, T>
where
    U: FromStr,
    T: ExtractPath,
    T::Captures: Prepend<U>,
    <T::Captures as Prepend<U>>::Output: CapturesPrepend<U, T::Captures>,
{
    fn extract(segments: &[&str]) -> Option<<T::Captures as Prepend<U>>::Output> {
        let seg = segments.first()?;
        let val = U::from_str(seg).ok()?;
        let rest = T::extract(&segments[1..])?;
        Some(<T::Captures as Prepend<U>>::Output::prepend(val, rest))
    }

    fn pattern() -> String {
        format!("/{{}}{}", T::pattern())
    }
}

impl ExtractPath for HCons<CaptureRest, HNil> {
    fn extract(segments: &[&str]) -> Option<(Vec<String>,)> {
        Some((segments.iter().map(|s| s.to_string()).collect(),))
    }

    fn pattern() -> String {
        "/{*rest}".to_string()
    }
}

// ---------------------------------------------------------------------------
// CapturesPrepend: runtime tuple prepend
// ---------------------------------------------------------------------------

/// Runtime counterpart of [`Prepend`] — constructs a tuple value by
/// prepending a value onto an existing tuple value.
pub trait CapturesPrepend<T, Rest> {
    fn prepend(val: T, rest: Rest) -> Self;
}

macro_rules! impl_captures_prepend {
    () => {
        impl<T> CapturesPrepend<T, ()> for (T,) {
            fn prepend(val: T, _rest: ()) -> Self {
                (val,)
            }
        }
    };
    ($($idx:tt : $ty:ident),+) => {
        impl<T, $($ty,)+> CapturesPrepend<T, ($($ty,)+)> for (T, $($ty,)+) {
            fn prepend(val: T, rest: ($($ty,)+)) -> Self {
                (val, $(rest.$idx,)+)
            }
        }
    };
}

impl_captures_prepend!();
impl_captures_prepend!(0: A);
impl_captures_prepend!(0: A, 1: B);
impl_captures_prepend!(0: A, 1: B, 2: C);
impl_captures_prepend!(0: A, 1: B, 2: C, 3: D);
impl_captures_prepend!(0: A, 1: B, 2: C, 3: D, 4: E);
impl_captures_prepend!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F);
impl_captures_prepend!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G);

#[cfg(test)]
#[allow(non_camel_case_types)]
mod tests {
    use super::*;

    // Test literal segments (lowercase names mimic proc-macro generated types)
    struct users;
    impl LitSegment for users {
        const VALUE: &'static str = "users";
    }

    struct posts;
    impl LitSegment for posts {
        const VALUE: &'static str = "posts";
    }

    // -- PathSpec compile-time assertions --

    fn assert_captures<P: PathSpec<Captures = C>, C>() {}

    #[test]
    fn pathspec_hnil() {
        assert_captures::<HNil, ()>();
    }

    #[test]
    fn pathspec_single_lit() {
        assert_captures::<HCons<Lit<users>, HNil>, ()>();
    }

    #[test]
    fn pathspec_lit_and_capture() {
        assert_captures::<HCons<Lit<users>, HCons<Capture<u32>, HNil>>, (u32,)>();
    }

    #[test]
    fn pathspec_two_captures() {
        type P = HCons<Capture<u32>, HCons<Lit<posts>, HCons<Capture<u32>, HNil>>>;
        assert_captures::<P, (u32, u32)>();
    }

    #[test]
    fn pathspec_mixed_captures() {
        type P =
            HCons<Lit<users>, HCons<Capture<u32>, HCons<Lit<posts>, HCons<Capture<String>, HNil>>>>;
        assert_captures::<P, (u32, String)>();
    }

    // -- ExtractPath runtime tests --

    #[test]
    fn extract_hnil() {
        assert_eq!(HNil::extract(&[]), Some(()));
        assert_eq!(HNil::extract(&["extra"]), None);
    }

    #[test]
    fn extract_single_lit() {
        type P = HCons<Lit<users>, HNil>;
        assert_eq!(P::extract(&["users"]), Some(()));
        assert_eq!(P::extract(&["posts"]), None);
        assert_eq!(P::extract(&[]), None);
    }

    #[test]
    fn extract_lit_and_capture_u32() {
        type P = HCons<Lit<users>, HCons<Capture<u32>, HNil>>;
        assert_eq!(P::extract(&["users", "42"]), Some((42u32,)));
        assert_eq!(P::extract(&["users", "abc"]), None);
        assert_eq!(P::extract(&["users"]), None);
        assert_eq!(P::extract(&["posts", "42"]), None);
    }

    #[test]
    fn extract_two_captures() {
        type P =
            HCons<Lit<users>, HCons<Capture<u32>, HCons<Lit<posts>, HCons<Capture<u32>, HNil>>>>;
        assert_eq!(
            P::extract(&["users", "42", "posts", "7"]),
            Some((42u32, 7u32))
        );
        assert_eq!(P::extract(&["users", "42", "posts"]), None);
        assert_eq!(P::extract(&["users", "abc", "posts", "7"]), None);
    }

    #[test]
    fn extract_trailing_segments_rejected() {
        type P = HCons<Lit<users>, HNil>;
        assert_eq!(P::extract(&["users", "extra"]), None);
    }

    #[test]
    fn extract_capture_rest() {
        type P = HCons<Lit<users>, HCons<CaptureRest, HNil>>;
        assert_eq!(
            P::extract(&["users", "a", "b", "c"]),
            Some((vec!["a".to_string(), "b".to_string(), "c".to_string()],))
        );
        assert_eq!(P::extract(&["users"]), Some((vec![],)));
    }

    // -- Pattern string tests --

    #[test]
    fn pattern_empty() {
        assert_eq!(HNil::pattern(), "");
    }

    #[test]
    fn pattern_single_lit() {
        type P = HCons<Lit<users>, HNil>;
        assert_eq!(P::pattern(), "/users");
    }

    #[test]
    fn pattern_lit_and_capture() {
        type P = HCons<Lit<users>, HCons<Capture<u32>, HNil>>;
        assert_eq!(P::pattern(), "/users/{}");
    }

    #[test]
    fn pattern_complex() {
        type P =
            HCons<Lit<users>, HCons<Capture<u32>, HCons<Lit<posts>, HCons<Capture<u32>, HNil>>>>;
        assert_eq!(P::pattern(), "/users/{}/posts/{}");
    }
}
