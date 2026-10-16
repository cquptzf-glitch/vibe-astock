use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretSetRequest<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretKeyRequest<'a> {
    pub key: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct SecretValueResponse {
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SecretPresenceResponse {
    pub present: bool,
}

#[derive(Debug, Deserialize)]
pub struct SecretMutationResponse {
    pub ok: bool,
}
