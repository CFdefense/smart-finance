//! Standalone binary that generates `docs/openapi.json` from the API definition.
//!
//! Used in CI to produce the `OpenAPI` spec for GitHub Pages deployment.
//! Requires no database, no environment variables, and no running server.
//!
//! NOTE: This is a first-pass implementation. Task 3 restructures to a lib+bin
//! crate so this binary can import `ApiDoc` directly from the library.

use std::{fs, io::Write, path::PathBuf};
use utoipa::{
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
    Modify, OpenApi,
};

/// Mirrors `SecurityAddon` in `src/swagger.rs`.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "auth-token",
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                    "auth-token",
                    "An HTTP-only private cookie encoding user id and expiry.",
                ))),
            );
        }
    }
}

/// Mirrors `ApiDoc` in `src/swagger.rs` — kept in sync manually.
/// Task 3 will replace this with an import from the library crate.
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    security((), ("auth-token" = [])),
    info(
        title = "Smart Finance API",
        description = "The public API documentation for the Smart Finance web application."
    ),
    servers(
        (url = "http://localhost:3001", description = "Local development server")
    )
)]
struct ApiDoc;

fn main() {
    let doc = ApiDoc::openapi();
    let json = doc
        .to_pretty_json()
        .expect("Could not serialise OpenAPI doc");

    let docs_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs");
    fs::create_dir_all(&docs_path).expect("Could not create docs directory");

    let out_path = docs_path.join("openapi.json");
    let mut file = fs::File::create(&out_path).expect("Could not create openapi.json");
    file.write_all(json.as_bytes())
        .expect("Could not write openapi.json");

    println!("Written: {}", out_path.display());
}
