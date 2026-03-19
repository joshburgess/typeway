//! Intermediate representation shared by both parse and emit stages.

use syn::{Ident, Pat, Type};

/// The full API model extracted from source code.
#[derive(Debug)]
pub struct ApiModel {
    /// All discovered endpoints.
    pub endpoints: Vec<EndpointModel>,
    /// Application state type, if any (e.g., `AppState`).
    pub state_type: Option<Type>,
    /// Tower layer expressions, in order of application.
    pub layers: Vec<syn::Expr>,
    /// Additional items (structs, impls, etc.) to pass through unchanged.
    pub passthrough_items: Vec<syn::Item>,
    /// Use statements that need rewriting.
    pub use_items: Vec<syn::ItemUse>,
    /// Nest prefix, if detected (e.g., `"/api/v1"`).
    pub prefix: Option<String>,
    /// Warnings produced during parsing (e.g., unsupported patterns).
    pub warnings: Vec<String>,
}

/// A single HTTP endpoint.
#[derive(Debug)]
pub struct EndpointModel {
    pub method: HttpMethod,
    pub path: PathModel,
    pub handler: HandlerModel,
    pub request_body: Option<Type>,
    pub response_type: Type,
}

/// HTTP methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl HttpMethod {
    /// Parse from an Axum routing function name.
    pub fn from_axum_method_name(name: &str) -> Option<Self> {
        match name {
            "get" => Some(Self::Get),
            "post" => Some(Self::Post),
            "put" => Some(Self::Put),
            "delete" => Some(Self::Delete),
            "patch" => Some(Self::Patch),
            "head" => Some(Self::Head),
            "options" => Some(Self::Options),
            _ => None,
        }
    }

    /// The Typeway endpoint type prefix (e.g., "GetEndpoint").
    pub fn typeway_endpoint_name(&self) -> &'static str {
        match self {
            Self::Get => "GetEndpoint",
            Self::Post => "PostEndpoint",
            Self::Put => "PutEndpoint",
            Self::Delete => "DeleteEndpoint",
            Self::Patch => "PatchEndpoint",
            Self::Head => "GetEndpoint",    // HEAD uses GET endpoint type
            Self::Options => "GetEndpoint", // OPTIONS uses GET endpoint type
        }
    }

    /// Whether this method typically has a request body.
    pub fn has_body(&self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch)
    }

    /// Axum routing function name.
    pub fn axum_fn_name(&self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Put => "put",
            Self::Delete => "delete",
            Self::Patch => "patch",
            Self::Head => "head",
            Self::Options => "options",
        }
    }
}

/// A URL path pattern.
#[derive(Debug, Clone)]
pub struct PathModel {
    /// The raw path string, e.g., "/users/{id}/posts".
    pub raw_pattern: String,
    /// Parsed path segments.
    pub segments: Vec<PathSegment>,
    /// Generated typeway type name, e.g., `UsersByIdPath`.
    pub typeway_type_name: Ident,
}

impl PathModel {
    /// Parse an Axum-style path string into segments.
    ///
    /// Capture types are left as `None` — they must be filled in
    /// from handler signature analysis.
    pub fn from_axum_path(raw: &str) -> Self {
        let segments: Vec<PathSegment> = raw
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|seg| {
                if let Some(name) = seg
                    .strip_prefix('{')
                    .and_then(|s| s.strip_suffix('}'))
                {
                    // Also handle Axum's :param syntax
                    PathSegment::Capture {
                        name: name.to_string(),
                        ty: None,
                    }
                } else if let Some(name) = seg.strip_prefix(':') {
                    PathSegment::Capture {
                        name: name.to_string(),
                        ty: None,
                    }
                } else {
                    PathSegment::Literal(seg.to_string())
                }
            })
            .collect();

        let type_name = generate_path_type_name(&segments);
        let type_ident = Ident::new(&type_name, proc_macro2::Span::call_site());

        PathModel {
            raw_pattern: raw.to_string(),
            segments,
            typeway_type_name: type_ident,
        }
    }

    /// Number of capture segments.
    pub fn capture_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| matches!(s, PathSegment::Capture { .. }))
            .count()
    }
}

/// A single segment of a URL path.
#[derive(Debug, Clone)]
pub enum PathSegment {
    /// A literal segment, e.g., "users".
    Literal(String),
    /// A captured segment with a name and (optionally resolved) type.
    Capture {
        name: String,
        ty: Option<Box<Type>>,
    },
}

/// A handler function.
#[derive(Debug)]
pub struct HandlerModel {
    /// Function name.
    pub name: Ident,
    /// Whether the function is async.
    pub is_async: bool,
    /// Extracted extractor arguments.
    pub extractors: Vec<ExtractorModel>,
    /// Return type.
    pub return_type: Type,
    /// Function body statements.
    pub body: Vec<syn::Stmt>,
    /// Any attributes on the function (e.g., #[handler]).
    pub attrs: Vec<syn::Attribute>,
}

/// An extractor argument in a handler.
#[derive(Debug)]
pub struct ExtractorModel {
    /// What kind of extractor this is.
    pub kind: ExtractorKind,
    /// The original pattern (e.g., `Path(id)` or `State(state)`).
    pub pattern: Pat,
    /// The full wrapper type (e.g., `Path<u32>`, `State<AppState>`).
    pub full_type: Type,
    /// The inner type (e.g., `u32` inside `Path<u32>`).
    pub inner_type: Option<Type>,
    /// The variable name extracted from the pattern, if any.
    pub var_name: Option<Ident>,
}

/// Categories of extractors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractorKind {
    Path,
    State,
    Json,
    Query,
    Header,
    Extension,
    HeaderMap,
    Method,
    Uri,
    Bytes,
    RawString,
    /// An extractor not recognized by the tool.
    Unknown,
}

impl ExtractorKind {
    /// Classify an extractor from its type path.
    pub fn from_type_path(path: &syn::Path) -> Self {
        let last = match path.segments.last() {
            Some(seg) => seg.ident.to_string(),
            None => return Self::Unknown,
        };
        match last.as_str() {
            "Path" => Self::Path,
            "State" => Self::State,
            "Json" => Self::Json,
            "Query" => Self::Query,
            "Header" => Self::Header,
            "Extension" => Self::Extension,
            "HeaderMap" => Self::HeaderMap,
            "Method" => Self::Method,
            "Uri" => Self::Uri,
            "Bytes" => Self::Bytes,
            "String" => Self::RawString,
            _ => Self::Unknown,
        }
    }
}

/// Generate a PascalCase type name from path segments.
///
/// "/users/{id}/posts/{post_id}" → "UsersByIdPostsByPostIdPath"
/// "/users" → "UsersPath"
/// "/health" → "HealthPath"
fn generate_path_type_name(segments: &[PathSegment]) -> String {
    let mut parts = Vec::new();
    for seg in segments {
        match seg {
            PathSegment::Literal(s) => {
                parts.push(to_pascal_case(s));
            }
            PathSegment::Capture { name, .. } => {
                parts.push(format!("By{}", to_pascal_case(name)));
            }
        }
    }
    if parts.is_empty() {
        "RootPath".to_string()
    } else {
        format!("{}Path", parts.join(""))
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .flat_map(|word| {
            let mut chars = word.chars();
            let first = chars.next()?.to_uppercase().to_string();
            Some(first + &chars.collect::<String>())
        })
        .collect()
}
