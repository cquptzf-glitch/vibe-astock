use tauri::{AppHandle, Runtime, plugin::PluginApi};

pub fn init<R: Runtime, C>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<VibeShare<R>>
where
    C: serde::de::DeserializeOwned,
{
    Ok(VibeShare(std::marker::PhantomData))
}

pub struct VibeShare<R: Runtime>(std::marker::PhantomData<R>);

impl<R: Runtime> VibeShare<R> {
    pub fn share_text(
        &self,
        _title: Option<&str>,
        _text: &str,
        _chooser_title: Option<&str>,
    ) -> crate::Result<()> {
        Err(crate::Error::Unsupported)
    }

    pub fn share_file(
        &self,
        _title: Option<&str>,
        _name: &str,
        _mime: &str,
        _data_base64: &str,
        _chooser_title: Option<&str>,
    ) -> crate::Result<()> {
        Err(crate::Error::Unsupported)
    }
}
