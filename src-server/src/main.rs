mod auth;
mod config;
mod db;
mod routes;
mod state;

use axum::{Router, middleware};
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = config::load();
    tracing::info!("Starting Nalu sync server...");
    tracing::info!("Port: {}", config.port);
    tracing::info!("Data dir: {}", config.data_dir.display());
    tracing::info!("Pairing code: {}", config.pairing_code);

    let app_state = state::AppState::new(&config)
        .await
        .expect("Failed to initialize server state");

    let public = routes::public_router();
    let protected = routes::protected_router().route_layer(middleware::from_fn_with_state(
        app_state.clone(),
        auth::auth_middleware,
    ));

    let app: Router = Router::new()
        .merge(public)
        .merge(protected)
        .layer(CorsLayer::permissive())
        .with_state(app_state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("Server listening on {}", addr);

    axum::serve(listener, app).await.unwrap();
}
