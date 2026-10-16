#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(mobile)]
    #[error(transparent)]
    PluginInvoke(#[from] tauri::plugin::mobile::PluginInvokeError),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    #[cfg(desktop)]
    #[error("secure storage mobile backend is unavailable on desktop")]
    Unsupported,
}

pub type Result<T> = std::result::Result<T, Error>;
