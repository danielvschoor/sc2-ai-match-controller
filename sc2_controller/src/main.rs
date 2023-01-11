#[cfg(feature = "swagger")]
mod docs;
mod routes;

#[cfg(feature = "swagger")]
use crate::docs::ApiDoc;
use crate::routes::{download_controller_log, find_map, start_sc2, terminate_sc2};
use common::api::health;
use common::api::process::{shutdown, stats, stats_host, status, terminate_all};
use common::api::process::{stats_all, ProcessMap};
use common::api::state::AppState;
use common::axum::http::Request;
use common::axum::response::Response;
use common::axum::routing::{get, post};
use common::axum::{error_handling::HandleErrorLayer, http::StatusCode, Router};
use common::configuration::{get_config_from_proxy, get_host_url, get_proxy_url_from_env};
use common::logging::init_logging;
use common::tower::{BoxError, ServiceBuilder};
use common::tower_http::trace::TraceLayer;
use common::tracing::Span;
#[cfg(feature = "swagger")]
use common::utoipa::OpenApi;
#[cfg(feature = "swagger")]
use common::utoipa_swagger_ui::SwaggerUi;
use common::{axum, tower, tracing};
use common::{tokio, tracing_appender};
use std::path::Path;
use std::str::FromStr;
use std::{net::SocketAddr, time::Duration};

static PREFIX: &str = "ACSC2";

#[tokio::main]
async fn main() {
    let host_url = get_host_url(PREFIX, 8083);

    let proxy_url = get_proxy_url_from_env(PREFIX);
    let config_url = format!("http://{}/configuration", proxy_url);
    let health_url = format!("http://{}/health", proxy_url);

    let settings = get_config_from_proxy(config_url, health_url, PREFIX)
        .await
        .unwrap(); //panic if we can't get the config

    let log_level = &settings.logging_level;
    let env_log =
        std::env::var("RUST_LOG").unwrap_or_else(|_| format!("info,sc2_controller={}", log_level));

    let log_path = format!("{}/sc2_controller", &settings.log_root);
    let log_file = "sc2_controller.log";
    let full_path = Path::new(&log_path).join(log_file);
    if full_path.exists() {
        common::tokio::fs::remove_file(full_path).await.unwrap();
    }
    let (non_blocking_stdout, _guard) = tracing_appender::non_blocking(std::io::stdout());
    let non_blocking_file = tracing_appender::rolling::never(&log_path, log_file);
    init_logging(&env_log, non_blocking_stdout, non_blocking_file);

    let process_map = ProcessMap::default();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
    let state = AppState {
        process_map,
        settings,
        shutdown_sender: tx,
        extra_info: Default::default(),
    };
    #[allow(unused_mut)]
    let mut router = Router::<AppState>::new();
    #[cfg(feature = "swagger")]
    {
        router = router
            .merge(SwaggerUi::new("/swagger-ui/").url("/api-doc/openapi.json", ApiDoc::openapi()));
    }

    // Compose the routes
    let app = router
        .route("/start", post(start_sc2))
        .route("/stats/:port", get(stats))
        .route("/stats/host", get(stats_host))
        .route("/stats_all", get(stats_all))
        .route("/status/:port", get(status))
        .route("/terminate/:port", post(terminate_sc2))
        .route("/terminate_all", post(terminate_all))
        .route("/shutdown", post(shutdown))
        .route("/find_map/:map_name", get(find_map))
        .route("/download/controller_log", get(download_controller_log))
        .layer(
            TraceLayer::new_for_http()
                .on_request(|request: &Request<_>, _span: &Span| {
                    tracing::debug!("started {} {}", request.method(), request.uri().path());
                })
                .on_response(|_response: &Response, latency: Duration, _span: &Span| {
                    tracing::debug!("response generated in {:?}", latency);
                }),
        )
        .route("/health", get(health))
        // Add middleware to all routes
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|error: BoxError| async move {
                    if error.is::<tower::timeout::error::Elapsed>() {
                        Ok(StatusCode::REQUEST_TIMEOUT)
                    } else {
                        Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Unhandled internal error: {}", error),
                        ))
                    }
                }))
                .timeout(Duration::from_secs(120))
                .layer(TraceLayer::new_for_http())
                .into_inner(),
        )
        .with_state(state.clone());

    let addr = SocketAddr::from_str(&host_url).unwrap();
    tracing::debug!("listening on {}", addr);
    let graceful_server = axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async {
            let _ = rx.recv().await;
        });
    if let Err(e) = graceful_server.await {
        tracing::error!("server error: {}", e);
    }
}
