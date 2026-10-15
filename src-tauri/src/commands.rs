use tauri::State;
use vibe_astock_core::{AppCore, MobileApiRequest, MobileApiResponse};

#[tauri::command]
pub async fn mobile_api(
    state: State<'_, AppCore>,
    request: MobileApiRequest,
) -> Result<MobileApiResponse, String> {
    Ok(state.handle(request).await)
}
