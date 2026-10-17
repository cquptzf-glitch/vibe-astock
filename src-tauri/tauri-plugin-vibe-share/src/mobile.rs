use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

const PLUGIN_IDENTIFIER: &str = "com.vibeastock.share";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareTextRequest<'a> {
    title: Option<&'a str>,
    text: &'a str,
    chooser_title: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareFileRequest<'a> {
    title: Option<&'a str>,
    name: &'a str,
    mime: &'a str,
    data_base64: &'a str,
    chooser_title: Option<&'a str>,
}

#[derive(Deserialize)]
struct ShareResponse {
    ok: bool,
}

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<VibeShare<R>> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "SharePlugin")?;
    Ok(VibeShare(handle))
}

pub struct VibeShare<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> VibeShare<R> {
    pub fn share_text(
        &self,
        title: Option<&str>,
        text: &str,
        chooser_title: Option<&str>,
    ) -> crate::Result<()> {
        let response: ShareResponse = self.0.run_mobile_plugin(
            "shareText",
            ShareTextRequest {
                title,
                text,
                chooser_title,
            },
        )?;
        let _ = response.ok;
        Ok(())
    }

    pub fn share_file(
        &self,
        title: Option<&str>,
        name: &str,
        mime: &str,
        data_base64: &str,
        chooser_title: Option<&str>,
    ) -> crate::Result<()> {
        let response: ShareResponse = self.0.run_mobile_plugin(
            "shareFile",
            ShareFileRequest {
                title,
                name,
                mime,
                data_base64,
                chooser_title,
            },
        )?;
        let _ = response.ok;
        Ok(())
    }
}
