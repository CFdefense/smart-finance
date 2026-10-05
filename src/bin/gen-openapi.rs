//! Standalone binary that generates `docs/openapi.json` from the live API definition.
//!
//! Used in CI to produce the OpenAPI spec for GitHub Pages deployment.
//! Requires no database, no environment variables, and no running server.

use smart_finance::swagger::ApiDoc;
use std::{fs, io::Write, path::PathBuf};
use utoipa::OpenApi;

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
