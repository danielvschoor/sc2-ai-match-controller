use crate::api_reference::aiarena::aiarena_api_client::AiArenaApiClient;
use crate::state::ProxyState;
use common::api::errors::app_error::AppError;
use common::api::errors::download_error::DownloadError;
use common::axum::extract::State;
use common::axum::Json;
use common::bytes::Bytes;
use common::configuration::ac_config::ACConfig;
use common::models::bot_controller::PlayerNum;
use common::parking_lot::RwLock;
use common::tracing::{self};
use std::sync::Arc;

#[tracing::instrument]
pub async fn configuration(
    State(state): State<Arc<RwLock<ProxyState>>>,
) -> Result<Json<ACConfig>, AppError> {
    Ok(Json(state.read().settings.clone()))
}

pub async fn download_bot(
    State(state): State<Arc<RwLock<ProxyState>>>,
    //ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(player_num): Json<PlayerNum>,
) -> Result<Bytes, AppError> {
    // todo: Implement authorization
    // if !state.read().auth_whitelist.contains(&addr) {
    //     return Err(DownloadError::Unauthorized.into());
    // }
    let settings = state.read().settings.clone();

    let current_match = match state
        .read()
        .current_match
        .as_ref()
        .and_then(|x| x.aiarena_match.clone())
    {
        None => {
            return Err(DownloadError::Other("current_match is None".to_string()).into());
        }
        Some(m) => m,
    };
    let api = AiArenaApiClient::new(
        &settings.base_website_url,
        settings.api_token.as_ref().unwrap(),
    )
    .unwrap(); //Would've failed before this point already
    let download_url = match player_num {
        PlayerNum::One => current_match.bot1.bot_zip.clone(),
        PlayerNum::Two => current_match.bot2.bot_zip.clone(),
    };
    api.download_bot(&download_url)
        .await
        .map_err(|e| AppError::Download(DownloadError::Other(e.to_string())))
}

pub async fn download_map(State(state): State<Arc<RwLock<ProxyState>>>) -> Result<Bytes, AppError> {
    let settings = state.read().settings.clone();

    let current_match = match state
        .read()
        .current_match
        .as_ref()
        .and_then(|x| x.aiarena_match.clone())
    {
        None => {
            return Err(DownloadError::Other("current_match is None".to_string()).into());
        }
        Some(m) => m,
    };
    let api = AiArenaApiClient::new(
        &settings.base_website_url,
        settings.api_token.as_ref().unwrap(),
    )
    .unwrap(); //Would've failed before this point already
    let map_url = &current_match.map.file;
    api.download_map(map_url)
        .await
        .map_err(|e| AppError::Download(DownloadError::Other(e.to_string())))
}
