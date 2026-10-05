//! API controllers.
//!
//! In dev builds, [`AxumRouter`] is an [`utoipa_axum::router::OpenApiRouter`] so
//! that route handlers can register `OpenAPI` metadata. In test and release builds
//! it is a plain [`axum::Router`].

pub mod user;

/// A plain [`axum::Router`] in test and release builds.
#[cfg(any(test, not(debug_assertions)))]
pub type AxumRouter = axum::Router;

/// An [`utoipa_axum::router::OpenApiRouter`] in non-test dev builds.
#[cfg(all(not(test), debug_assertions))]
pub type AxumRouter = utoipa_axum::router::OpenApiRouter;
