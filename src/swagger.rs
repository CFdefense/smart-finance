//! Swagger / `OpenAPI` documentation configuration.

#![allow(unused_qualifications)] // utoipa nest!() macro forces qualified paths

use utoipa::{
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
    Modify, OpenApi,
};

#[cfg(all(not(test), debug_assertions))]
use axum::Router;
#[cfg(all(not(test), debug_assertions))]
use std::{fs, io::Write, path::PathBuf};
#[cfg(all(not(test), debug_assertions))]
use utoipa_axum::router::OpenApiRouter;
#[cfg(all(not(test), debug_assertions))]
use utoipa_swagger_ui::SwaggerUi;

/// Security scheme modifier — adds the `auth-token` cookie scheme to the `OpenAPI` spec.
///
/// Constructed only by the `#[openapi(modifiers(...))]` macro on [`ApiDoc`].
#[allow(dead_code)] // only instantiated by the #[derive(OpenApi)] macro — never constructed directly
pub struct SecurityAddon;

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

/// Root `OpenAPI` document for the Smart Finance API.
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    security((), ("auth-token" = [])),
    info(
        title = "Smart Finance API",
        description = "The public API documentation for the Smart Finance web application."
    ),
    nest(
        (path = "/api/user", api = crate::controllers::user::UserApiDoc)
    ),
    servers(
        (url = "http://localhost:3001", description = "Local development server")
    )
)]
#[allow(dead_code)] // only instantiated by the #[derive(OpenApi)] macro — never constructed directly
pub struct ApiDoc;

/// Merges Swagger UI into the router and writes `docs/openapi.json` to disk.
///
/// Only compiled in non-test debug builds.
///
/// # Panics
///
/// Panics if the docs directory or `openapi.json` file cannot be created or written.
#[cfg(all(not(test), debug_assertions))]
pub fn merge_swagger(router: OpenApiRouter) -> Router {
    let doc = ApiDoc::openapi();

    let docs_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs");
    fs::create_dir_all(&docs_path).expect("Could not create docs directory");
    let mut file =
        fs::File::create(docs_path.join("openapi.json")).expect("Could not create openapi.json");
    file.write_all(
        doc.to_pretty_json()
            .expect("Could not serialise OpenAPI doc")
            .as_bytes(),
    )
    .expect("Could not write openapi.json");

    let (router, api) = OpenApiRouter::with_openapi(doc)
        .merge(router)
        .split_for_parts();
    router.merge(SwaggerUi::new("/swagger").url("/docs/openapi.json", api))
}
