//! shirita-web: Axum adaptation layer (REST + SSE + static files + authentication)

pub mod auth;
pub mod embed;
pub mod generations;
pub mod provider_select;
pub mod routes;
pub mod state;

pub use generations::Generations;
pub use provider_select::{
    provider_from_env, provider_kind, resolve_provider, resolve_provider_config, ProviderKind,
};
pub use state::AppState;

/// Create an HTTP client shared across all processes. Call each of the two entry points (web/tauri) once and store the results in `AppState.http_client`.
pub fn new_http_client() -> reqwest::Client {
    // A connect timeout bounds time spent reaching a dead/slow provider without
    // capping the *total* request: generation streams an SSE response that can
    // legitimately run for minutes, so a whole-request `.timeout()` would
    // truncate long replies. Per-request read timeouts are applied where the
    // response is bounded (e.g. /provider/models).
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default()
}

use axum::extract::DefaultBodyLimit;
use axum::http::{header, Method};
use axum::routing::{delete, post, put};
use axum::{middleware, routing::get, Router};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

/// Test-only helper: seed a `tester` user + a long-lived session whose token is
/// the fixed `secret-token` that the integration tests send as
/// `Authorization: Bearer secret-token`, so the suite can exercise
/// session-protected routes without each test logging in. `#[doc(hidden)]` —
/// not part of the supported public API.
#[doc(hidden)]
pub async fn seed_test_session<S: shirita_core::Storage + ?Sized>(storage: &S) {
    use shirita_core::{AuthSessionRecord, User};
    if storage.get_user_by_username("tester").await.ok().flatten().is_none() {
        let _ = storage.create_user(&User::new("tester", String::new())).await;
    }
    if let Some(user) = storage.get_user_by_username("tester").await.ok().flatten() {
        let _ = storage.delete_auth_session("secret-token").await;
        let expires_at = shirita_core::iso_now_plus_days(365);
        let _ = storage
            .create_auth_session(&AuthSessionRecord::new("secret-token", &user.id, &expires_at))
            .await;
    }
}

/// Set up application routing. `/`, `/health`, and `GET /assets/*` are publicly accessible; `/api/*` goes through the Bearer middleware.
pub fn app(state: AppState) -> Router {
    let assets_dir = state.config.assets_dir.clone();

    let protected = Router::new()
        .route("/ping", get(routes::ping::ping))
        .route(
            "/sessions",
            get(routes::sessions::list_sessions).post(routes::sessions::create_session),
        )
        .route("/sessions/import", post(routes::sessions::import_session))
        .route("/sessions/reorder", put(routes::sessions::reorder_sessions))
        .route(
            "/sessions/{id}",
            get(routes::sessions::get_session)
                .patch(routes::sessions::patch_session)
                .delete(routes::sessions::delete_session),
        )
        .route("/sessions/{id}/duplicate", post(routes::sessions::duplicate_session))
        .route("/sessions/{id}/export", get(routes::sessions::export_session))
        .route("/sessions/{id}/identity", get(routes::sessions::get_session_identity))
        .route(
            "/sessions/{id}/messages",
            get(routes::sessions::list_messages).post(routes::chat::send),
        )
        .route(
            "/sessions/{id}/messages/{msg_id}",
            put(routes::messages::edit_message).delete(routes::messages::delete_message),
        )
        .route(
            "/sessions/{id}/active-leaf",
            put(routes::messages::set_active_leaf),
        )
        .route(
            "/sessions/{id}/messages/{msg_id}/regenerate",
            post(routes::chat::regenerate_message),
        )
        .route("/sessions/{id}/abort", post(routes::chat::abort_message))
        .route("/sessions/{id}/fork", post(routes::messages::fork_session))
        .route("/sessions/{id}/mounts", put(routes::sessions::set_mounts))
        .route("/sessions/{id}/packs", get(routes::sessions::get_packs).put(routes::sessions::set_packs))
        .route("/sessions/{id}/panels", get(routes::sessions::get_panels))
        .route(
            "/sessions/{id}/local-definitions/{def_id}",
            put(routes::local_overrides::set_local_definition)
                .delete(routes::local_overrides::clear_local_definition),
        )
        .route(
            "/sessions/{id}/local-definitions/{def_id}/promote",
            post(routes::local_overrides::promote_local_definition),
        )
        .route(
            "/sessions/{id}/materialize-nodes",
            post(routes::local_overrides::materialize_nodes),
        )
        .route(
            "/sessions/{id}/materialize-pack",
            post(routes::local_overrides::materialize_pack_nodes),
        )
        .route("/sessions/{id}/state", get(routes::variables::get_state))
        .route("/sessions/{id}/local-variables", put(routes::variables::set_local_variables))
        .route("/sessions/{id}/state-updates", post(routes::variables::apply_state_updates))
        .route(
            "/definitions",
            get(routes::definitions::list).post(routes::definitions::create),
        )
        .route(
            "/definitions/{id}",
            get(routes::definitions::get)
                .put(routes::definitions::update)
                .delete(routes::definitions::delete),
        )
        .route("/definitions/{id}/export", get(routes::export::export_definition))
        .route("/templates/{id}/export", get(routes::export::export_template))
        .route("/templates", get(routes::templates::list).post(routes::templates::create))
        .route("/templates/{id}", get(routes::templates::get).put(routes::templates::update).delete(routes::templates::delete))
        .route("/templates/{id}/duplicate", post(routes::templates::duplicate))
        .route("/templates/{id}/orphan-definitions", get(routes::templates::orphan_definitions))
        .route("/templates/{id}/nodes", get(routes::prompt_nodes::list_nodes).post(routes::prompt_nodes::create_node))
        .route("/nodes/{id}", put(routes::prompt_nodes::update_node).delete(routes::prompt_nodes::delete_node))
        .route("/templates/{id}/nodes/reorder", put(routes::prompt_nodes::reorder_nodes))
        .route("/packs", get(routes::packs::list).post(routes::packs::create))
        .route("/packs/{id}", get(routes::packs::get).put(routes::packs::update).delete(routes::packs::delete))
        .route("/packs/{id}/duplicate", post(routes::packs::duplicate))
        .route("/packs/{id}/orphan-definitions", get(routes::packs::orphan_definitions))
        .route("/packs/{id}/export", get(routes::export::export_pack))
        .route("/packs/{id}/nodes", get(routes::prompt_nodes::list_nodes).post(routes::prompt_nodes::create_node))
        .route("/packs/{id}/nodes/reorder", put(routes::prompt_nodes::reorder_nodes))
        .route("/types", get(routes::types::list).post(routes::types::create))
        .route("/types/{id}", delete(routes::types::delete))
        .route("/import", post(routes::import_export::import).layer(DefaultBodyLimit::max(16 * 1024 * 1024)))
        .route("/regex-rules/scopes", get(routes::regex_rules::list_regex_scopes))
        .route("/settings", get(routes::settings::get_all).put(routes::settings::update_all))
        .route("/agent-settings", get(routes::agent_settings::get_global).put(routes::agent_settings::put_global).delete(routes::agent_settings::reset_global))
        .route("/sessions/{id}/agent-settings", get(routes::agent_settings::get_session).put(routes::agent_settings::put_session).delete(routes::agent_settings::reset_session))
        .route("/provider/test", post(routes::provider::test_connection))
        .route("/provider/models", get(routes::provider::list_models))
        // Image uploads routinely exceed axum's 2 MiB default; allow up to 16 MiB
        // (matching the /import bundle limit).
        .route(
            "/assets",
            get(routes::assets::list)
                .post(routes::assets::upload)
                .layer(DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route("/assets/{id}", put(routes::assets::rename).delete(routes::assets::delete))
        // Session-protected auth routes. The public login route is mounted
        // outside this gate (merged below) so unauthenticated clients can log in.
        .route("/auth/logout", post(routes::auth::logout))
        .route("/auth/me", get(routes::auth::me))
        .route("/auth/password", post(routes::auth::change_password))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_session,
        ));

    // Public login (no session gate) merged with the protected sub-router
    // (disjoint paths — `protected` has no `/auth/login`), then nested under
    // /api together so CORS/prefix are consistent.
    let api = Router::new()
        .route("/auth/login", post(routes::auth::login))
        .merge(protected);

    let router = Router::new()
        .route("/health", get(routes::health::health))
        .nest("/api", api);

    #[cfg(feature = "embed-ui")]
    let router = router
        .route("/", get(embed::serve_index))
        .route("/static/{*path}", get(embed::serve_static))
        .fallback(embed::spa_fallback);

    #[cfg(not(feature = "embed-ui"))]
    let router = router.route("/", get(routes::index::index));

    // Resolve state, then merge in the token-gated /assets router. ServeDir is
    // behind `require_asset_access`, which accepts `?t=<token>` (so `<img>` can
    // authenticate) or a Bearer header.
    let router = router.with_state(state.clone());
    let assets = Router::new()
        .nest_service("/assets", ServeDir::new(assets_dir))
        .layer(middleware::from_fn_with_state(state.clone(), auth::require_asset_access));
    let router = router.merge(assets);

    // Request logging with the session token redacted from the URI — the token
    // rides in `/assets/<file>?t=<token>`, so the span records method + a
    // redacted path only. Quiet at the default `info` filter; enabling debug
    // shows method + redacted path, never the token itself.
    router.layer(
        TraceLayer::new_for_http().make_span_with(|req: &axum::extract::Request| {
            tracing::info_span!(
                "request",
                method = %req.method(),
                uri = %auth::redact_query(req.uri())
            )
        }),
    )
}

/// Origin of the desktop WebView:
/// - Production (`tauri build`, custom-protocol): `tauri://localhost` (Linux/macOS)/
///   `https://tauri.localhost` (Windows);
/// - Development (`tauri dev`, loading devUrl): `http://localhost:<port>`.
/// The embedded server is bound to 127.0.0.1 and protected by Bearer authentication, so allowing access to any port on localhost/127.0.0.1 is safe.
fn is_desktop_origin(origin: &header::HeaderValue) -> bool {
    let o = origin.as_bytes();
    o == b"tauri://localhost"
        || o == b"https://tauri.localhost"
        || o == b"http://tauri.localhost"
        || o.starts_with(b"http://localhost:")
        || o.starts_with(b"http://127.0.0.1:")
}

/// For desktop (embedded server) only: Wrap `app()` with CORS to allow the Tauri WebView origin.
/// CorsLayer acts as the outermost layer—it short-circuits the `OPTIONS` preflight response, preventing it from entering Bearer authentication;
/// Actual requests pass through CORS → auth → handler, and the response is supplemented with `Access-Control-Allow-Origin` on the way back.
pub fn app_with_cors(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _req| is_desktop_origin(origin)))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
    app(state).layer(cors)
}
