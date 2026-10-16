use tauri::{AppHandle, Runtime, plugin::PluginApi};

pub fn init<R: Runtime, C>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<SecureStorage<R>>
where
    C: serde::de::DeserializeOwned,
{
    Ok(SecureStorage(std::marker::PhantomData))
}

pub struct SecureStorage<R: Runtime>(std::marker::PhantomData<R>);

impl<R: Runtime> SecureStorage<R> {
    pub fn set(&self, _key: &str, _value: &str) -> crate::Result<()> {
        Err(crate::Error::Unsupported)
    }

    pub fn get(&self, _key: &str) -> crate::Result<Option<String>> {
        Err(crate::Error::Unsupported)
    }

    pub fn contains(&self, _key: &str) -> crate::Result<bool> {
        Err(crate::Error::Unsupported)
    }

    pub fn delete(&self, _key: &str) -> crate::Result<()> {
        Err(crate::Error::Unsupported)
    }
}
