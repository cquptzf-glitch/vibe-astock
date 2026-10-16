use serde::de::DeserializeOwned;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::models::{
    SecretKeyRequest, SecretMutationResponse, SecretPresenceResponse, SecretSetRequest,
    SecretValueResponse,
};

const PLUGIN_IDENTIFIER: &str = "com.vibeastock.securestorage";

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<SecureStorage<R>> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "SecureStoragePlugin")?;
    Ok(SecureStorage(handle))
}

pub struct SecureStorage<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> SecureStorage<R> {
    pub fn set(&self, key: &str, value: &str) -> crate::Result<()> {
        let response: SecretMutationResponse = self
            .0
            .run_mobile_plugin("setSecret", SecretSetRequest { key, value })?;
        let _ = response.ok;
        Ok(())
    }

    pub fn get(&self, key: &str) -> crate::Result<Option<String>> {
        let response: SecretValueResponse = self
            .0
            .run_mobile_plugin("getSecret", SecretKeyRequest { key })?;
        Ok(response.value)
    }

    pub fn contains(&self, key: &str) -> crate::Result<bool> {
        let response: SecretPresenceResponse = self
            .0
            .run_mobile_plugin("containsSecret", SecretKeyRequest { key })?;
        Ok(response.present)
    }

    pub fn delete(&self, key: &str) -> crate::Result<()> {
        let response: SecretMutationResponse = self
            .0
            .run_mobile_plugin("deleteSecret", SecretKeyRequest { key })?;
        let _ = response.ok;
        Ok(())
    }
}
