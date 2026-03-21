//! OpenAPI integration — serves `/openapi.json` and `/docs` (Swagger UI).
//!
//! Enabled with `feature = "openapi"`. Adds [`Server::with_openapi`](crate::server::Server::with_openapi) which
//! registers two additional routes on the server.

use std::sync::Arc;

use bytes::Bytes;

use crate::body::{body_from_bytes, body_from_string};
use crate::handler::{BoxedHandler, ResponseFuture};

/// Embedded API documentation HTML page.
///
/// This is a self-contained page that renders the OpenAPI spec without any
/// external CDN dependencies. The `{{SPEC_JSON}}` placeholder is replaced
/// at startup with the actual JSON spec. The page provides:
/// - Collapsible endpoint listing grouped by path
/// - Method badges (GET, POST, PUT, DELETE, PATCH)
/// - Parameter and schema display
/// - Request body and response schema display
const DOCS_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{{TITLE}} — API Documentation</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#f8f9fa;color:#333;line-height:1.6}
.container{max-width:960px;margin:0 auto;padding:24px}
h1{font-size:1.8em;margin-bottom:4px}
.version{color:#666;font-size:0.9em;margin-bottom:24px}
.path-group{margin-bottom:16px;border:1px solid #dee2e6;border-radius:8px;overflow:hidden;background:#fff}
.path-header{padding:12px 16px;font-family:monospace;font-size:1.05em;font-weight:600;background:#f1f3f5;cursor:pointer;user-select:none}
.path-header:hover{background:#e9ecef}
.operation{padding:12px 16px;border-top:1px solid #eee}
.method{display:inline-block;padding:2px 8px;border-radius:4px;color:#fff;font-size:0.75em;font-weight:700;text-transform:uppercase;margin-right:8px;vertical-align:middle}
.method-get{background:#61affe}.method-post{background:#49cc90}.method-put{background:#fca130}
.method-delete{background:#f93e3e}.method-patch{background:#50e3c2}.method-head{background:#9012fe}.method-options{background:#0d5aa7}
.params,.req-body,.responses{margin-top:8px;font-size:0.9em}
.params table{width:100%;border-collapse:collapse;margin-top:4px}
.params td,.params th{padding:4px 8px;border:1px solid #eee;text-align:left;font-size:0.85em}
.params th{background:#f8f9fa}
.schema-type{color:#666;font-family:monospace;font-size:0.85em}
.section-label{font-weight:600;color:#555;font-size:0.85em;margin-top:8px}
details>summary{cursor:pointer;list-style:none}
details>summary::-webkit-details-marker{display:none}
details>summary::before{content:'▶ ';font-size:0.7em;color:#999}
details[open]>summary::before{content:'▼ '}
</style>
</head>
<body>
<div class="container">
<h1>{{TITLE}}</h1>
<div class="version">Version {{VERSION}} · OpenAPI 3.1</div>
<div id="app"></div>
</div>
<script>
const spec = {{SPEC_JSON}};
const app = document.getElementById('app');
const methods = ['get','post','put','delete','patch','head','options'];
Object.entries(spec.paths || {}).forEach(([path, item]) => {
  const group = document.createElement('div');
  group.className = 'path-group';
  const ops = methods.filter(m => item[m]);
  const header = document.createElement('details');
  header.open = true;
  const summary = document.createElement('summary');
  summary.className = 'path-header';
  summary.textContent = path;
  header.appendChild(summary);
  ops.forEach(m => {
    const op = item[m];
    const div = document.createElement('div');
    div.className = 'operation';
    let html = '<span class="method method-' + m + '">' + m + '</span>';
    if (op.summary) html += '<strong>' + op.summary + '</strong>';
    if (op.parameters && op.parameters.length) {
      html += '<div class="params"><div class="section-label">Parameters</div><table><tr><th>Name</th><th>In</th><th>Type</th><th>Required</th></tr>';
      op.parameters.forEach(p => {
        const t = p.schema ? (p.schema.type || 'any') : 'any';
        html += '<tr><td>' + p.name + '</td><td>' + p.in + '</td><td class="schema-type">' + t + '</td><td>' + (p.required ? 'yes' : 'no') + '</td></tr>';
      });
      html += '</table></div>';
    }
    if (op.requestBody) {
      const ct = Object.keys(op.requestBody.content || {})[0] || '';
      const s = op.requestBody.content?.[ct]?.schema;
      html += '<div class="req-body"><div class="section-label">Request Body</div> <span class="schema-type">' + ct + (s ? ' · ' + (s.type || 'object') : '') + '</span></div>';
    }
    if (op.responses) {
      html += '<div class="responses"><div class="section-label">Responses</div>';
      Object.entries(op.responses).forEach(([code, resp]) => {
        const ct = Object.keys(resp.content || {})[0] || '';
        const s = resp.content?.[ct]?.schema;
        html += ' <span class="schema-type">' + code + (s ? ' · ' + (s.type || 'object') : '') + '</span>';
      });
      html += '</div>';
    }
    div.innerHTML = html;
    header.appendChild(div);
  });
  group.appendChild(header);
  app.appendChild(group);
});
</script>
</body>
</html>"##;

/// Create a boxed handler that returns a fixed JSON response.
pub(crate) fn spec_handler(spec_json: Arc<String>) -> BoxedHandler {
    std::sync::Arc::new(move |_parts, _body| -> ResponseFuture {
        let json = spec_json.clone();
        Box::pin(async move {
            let body = body_from_bytes(Bytes::from(json.as_bytes().to_vec()));
            let mut res = http::Response::new(body);
            res.headers_mut().insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
            res
        })
    })
}

/// Create a boxed handler that returns the embedded API docs page.
pub(crate) fn docs_handler(title: &str, version: &str, spec_json: &str) -> BoxedHandler {
    let html = DOCS_HTML
        .replace("{{TITLE}}", title)
        .replace("{{VERSION}}", version)
        .replace("{{SPEC_JSON}}", spec_json);
    let html = Arc::new(html);
    std::sync::Arc::new(move |_parts, _body| -> ResponseFuture {
        let html = html.clone();
        Box::pin(async move {
            let body = body_from_string(html.to_string());
            let mut res = http::Response::new(body);
            res.headers_mut().insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("text/html; charset=utf-8"),
            );
            res
        })
    })
}

/// Match function for a fixed path (no captures).
pub(crate) fn exact_match(expected: &'static [&'static str]) -> crate::router::MatchFn {
    Box::new(move |segments| segments == expected)
}

// ---------------------------------------------------------------------------
// EndpointToOperation impls for server-side wrapper types
// ---------------------------------------------------------------------------

#[cfg(feature = "openapi")]
mod openapi_impls {
    use typeway_openapi::spec::{Operation, SecurityRequirement};
    use typeway_openapi::EndpointToOperation;

    use crate::auth::Protected;
    use crate::typed::Validated;

    /// `Protected<Auth, E>` delegates to the inner endpoint but adds a
    /// bearer security requirement to the operation.
    impl<Auth, E: EndpointToOperation> EndpointToOperation for Protected<Auth, E> {
        fn path_pattern() -> String {
            E::path_pattern()
        }
        fn method() -> http::Method {
            E::method()
        }
        fn to_operation() -> Operation {
            let mut op = E::to_operation();
            op.security.push(SecurityRequirement::bearer());
            op
        }
    }

    /// `Validated<V, E>` is transparent to OpenAPI — validation is an
    /// internal implementation detail.
    impl<V: Send + Sync + 'static, E: EndpointToOperation> EndpointToOperation
        for Validated<V, E>
    {
        fn path_pattern() -> String {
            E::path_pattern()
        }
        fn method() -> http::Method {
            E::method()
        }
        fn to_operation() -> Operation {
            E::to_operation()
        }
    }
}
