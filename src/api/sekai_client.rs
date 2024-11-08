use super::{
    headers::Headers,
    url::{
        global_provider::GlobalUrlProvider, japan_provider::JapanUrlProvider,
        server_provider::ServerUrlProvider, UrlProvider,
    },
};
use crate::{
    constants::{header, strings},
    crypto::aes_msgpack,
    error::ApiError,
    models::{
        api::{
            AssetbundleInfo, GameVersion, SystemInfo, UserAuthRequest, UserAuthResponse,
            UserRequest, UserSignup,
        },
        enums::{Platform, Server},
    },
};
use reqwest::{Client, StatusCode};

/// A simple struct that stores information about the game's app.
pub struct SekaiApp {
    version: String,
    hash: String,
    platform: Platform,
}

impl SekaiApp {
    pub fn new(version: String, hash: String, platform: Platform) -> Self {
        Self {
            version,
            hash,
            platform,
        }
    }
}

/// An API client that interfaces with the game's servers, providing various functions to query endpoints.
pub struct SekaiClient<T: UrlProvider> {
    headers: Headers,
    client: Client,
    url_provider: T,
    server: Server,
    pub app: SekaiApp,
}

impl SekaiClient<ServerUrlProvider> {
    /// Creates a new SekaiClient that uses a ServerUrlProvider based on the passed ``server`` value.
    pub async fn new(
        version: String,
        hash: String,
        platform: Platform,
        server: Server,
    ) -> Result<Self, ApiError> {
        let provider = match server {
            Server::Japan => ServerUrlProvider::Japan(JapanUrlProvider::default()),
            Server::Global => ServerUrlProvider::Global(GlobalUrlProvider::default()),
        };

        Self::new_with_url_provider(version, hash, platform, server, provider).await
    }
}

impl<T: UrlProvider> SekaiClient<T> {
    /// Creates a new SekaiClient that uses a specific url provider.
    pub async fn new_with_url_provider(
        version: String,
        hash: String,
        platform: Platform,
        server: Server,
        url_provider: T,
    ) -> Result<Self, ApiError> {
        let headers = Headers::builder()?
            .version(&version)
            .hash(&hash)
            .platform(&platform)
            .build()?;

        let mut client = Self {
            headers,
            client: Client::new(),
            url_provider,
            server,
            app: SekaiApp::new(version, hash, platform),
        };

        // save the cloudfront signature only if required
        if client.url_provider.issue_signature().is_some() {
            client.issue_signature().await?;
        }

        Ok(client)
    }

    /// Performs a request to [`constants::url::sekai::ISSUE_SIGNATURE`].
    ///
    /// This endpoint responds with a CloudFront cookie value,
    /// which we need in order to communicate with the CDN.
    ///
    /// The function will automatically assign this cookie value to its Headers.
    async fn issue_signature(&mut self) -> Result<(), ApiError> {
        let url = self
            .url_provider
            .issue_signature()
            .ok_or(ApiError::MissingUrl("issue_signature".to_string()))?;

        let request = self
            .client
            .post(url)
            .body(b"ffa3bd6214f33fe73cb72fee2262bedb".to_vec())
            .headers(self.headers.get_map());

        match request.send().await?.error_for_status() {
            Ok(mut response) => {
                // set the cookie that is inside of issue_signature_response
                let set_cookie_header = response
                    .headers_mut()
                    .remove(header::name::SET_COOKIE)
                    .ok_or(ApiError::InvalidRequest(
                        strings::api::error::SET_COOKIE_NOT_FOUND.into(),
                    ))?;
                self.headers.insert(header::name::COOKIE, set_cookie_header);
                Ok(())
            }
            Err(err) => Err(ApiError::InvalidRequest(err.to_string())),
        }
    }

    /// Performs a request to [`constants::url::sekai::GAME_VERSION`].
    ///
    /// This endpoint will respond with info about the game version that the URL corresponds to.
    /// The version, hash, and platform values that this [`SekaiClient`] was created with determine this.
    ///
    /// Returns the parsed GameVersion data if it was found.
    pub async fn get_game_version(&self) -> Result<GameVersion, ApiError> {
        let request = self
            .client
            .get(
                self.url_provider
                    .game_version(&self.app.version, &self.app.hash),
            )
            .headers(self.headers.get_map());

        match request.send().await?.error_for_status() {
            Ok(response) => {
                let bytes = response.bytes().await?;
                Ok(aes_msgpack::from_slice(&bytes)?)
            }
            Err(err) => match err.status() {
                Some(StatusCode::FORBIDDEN) => Err(ApiError::InvalidRequest(
                    strings::api::error::INVALID_HASH_VERSION.into(),
                )),
                _ => Err(ApiError::InvalidRequest(err.to_string())),
            },
        }
    }

    /// Performs a request to [`constants::url::sekai::USER`].
    ///
    /// This endpoint will sign up for a new account, returning the account's default data
    /// and a credential to login later.
    ///
    /// This function will return a portion of this response; the user_registration info
    /// and the credential.
    pub async fn user_signup(&self) -> Result<UserSignup, ApiError> {
        let request_body = aes_msgpack::into_vec(&UserRequest {
            platform: self.app.platform.clone(),
            device_model: header::value::DEVICE_MODEL.into(),
            operating_system: header::value::OPERATING_SYSTEM.into(),
        })?;

        let request = self
            .client
            .post(self.url_provider.user())
            .headers(self.headers.get_map())
            .body(request_body);

        match request.send().await?.error_for_status() {
            Ok(response) => {
                let bytes = response.bytes().await?;
                Ok(aes_msgpack::from_slice(&bytes)?)
            }
            Err(err) => match err.status() {
                Some(StatusCode::UPGRADE_REQUIRED) => Err(ApiError::InvalidRequest(
                    strings::api::error::UPGRADE_REQUIRED.into(),
                )),
                _ => Err(ApiError::InvalidRequest(err.to_string())),
            },
        }
    }

    /// Performs a request to [`constants::url::sekai::user_auth`]
    ///
    /// This endpoint will create and respond with a login session for
    /// a specific user when given a valid login credential.
    ///
    /// It also contains relative URLs to the split suite master files along
    /// with the current data, asset, and multiplay versions.
    ///
    /// This function will store the session token as a header
    /// and respond with the entire response from the server.
    pub async fn user_login(
        &mut self,
        user_id: usize,
        credential: String,
    ) -> Result<UserAuthResponse, ApiError> {
        let request_body = aes_msgpack::into_vec(&UserAuthRequest {
            credential,
            device_id: None,
        })?;

        let request = self
            .client
            .put(self.url_provider.user_auth(user_id))
            .headers(self.headers.get_map())
            .body(request_body);

        match request.send().await?.error_for_status() {
            Ok(response) => {
                // parse body
                let bytes = response.bytes().await?;
                let auth_response: UserAuthResponse = aes_msgpack::from_slice(&bytes)?;

                // insert session token
                self.headers
                    .insert_str(header::name::SESSION_TOKEN, &auth_response.session_token)?;

                Ok(auth_response)
            }
            Err(err) => Err(ApiError::InvalidRequest(err.to_string())),
        }
    }

    /// Performs a request to [`constants::url::sekai::ASSETBUNDLE_INFO`]
    ///
    /// This endpoint responds with information about the assetbundles available to download
    /// for the asset version provided to it.
    ///
    /// This endpoint requires that the cloudfront cookies have been set.
    ///
    /// Returns the assetbundle info provided by the endpoint.
    pub async fn get_assetbundle_info(
        &self,
        asset_version: &str,
        asstbundle_host_hash: &str,
    ) -> Result<AssetbundleInfo, ApiError> {
        let request = self
            .client
            .get(self.url_provider.assetbundle_info(
                asstbundle_host_hash,
                asset_version,
                &self.app.platform,
            ))
            .headers(self.headers.get_map());

        match request.send().await?.error_for_status() {
            Ok(response) => {
                // parse body
                let bytes = response.bytes().await?;
                Ok(aes_msgpack::from_slice(&bytes)?)
            }
            Err(err) => match err.status() {
                Some(StatusCode::FORBIDDEN) => Err(ApiError::InvalidRequest(
                    strings::api::error::FORBIDDEN_ASSETBUNDLE_INFO.into(),
                )),
                _ => Err(ApiError::InvalidRequest(err.to_string())),
            },
        }
    }

    /// Performs a request to [`constants::url::sekai::SYSTEM`]
    ///
    /// This endpoint reports information about the current status of the game server
    /// including available app and asset versions.
    ///
    /// This endpoint requires that the cloudfront cookies have been set.
    ///
    /// This function responds with this information
    pub async fn get_system(&self) -> Result<SystemInfo, ApiError> {
        let request = self
            .client
            .get(self.url_provider.system())
            .headers(self.headers.get_map());

        match request.send().await?.error_for_status() {
            Ok(response) => {
                // parse body
                let bytes = response.bytes().await?;
                Ok(aes_msgpack::from_slice(&bytes)?)
            }
            Err(err) => Err(ApiError::InvalidRequest(err.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::url::test_provider::TestUrlProvider, models::api::AppVersion};

    const SIGNATURE_COOKIE_VALUE: &str = "signature_cookie";

    async fn get_client(server_url: String) -> SekaiClient<TestUrlProvider> {
        SekaiClient::new_with_url_provider(
            "3.9".to_string(),
            "393939".to_string(),
            Platform::Android,
            Server::Japan,
            TestUrlProvider::new(server_url),
        )
        .await
        .unwrap()
    }

    async fn get_server() -> mockito::ServerGuard {
        let mut server = mockito::Server::new_async().await;

        server
            .mock("POST", "/api/signature")
            .with_status(200)
            .with_header(header::name::SET_COOKIE, SIGNATURE_COOKIE_VALUE)
            .create_async()
            .await;

        server
    }

    #[tokio::test]
    async fn test_get_system() {
        let mut server = get_server().await;

        // create body
        let mock_system_info = SystemInfo {
            server_date: 1730780277695,
            timezone: "Asia/Tokyo".into(),
            profile: "production".into(),
            maintenance_status: "maintenance_out".into(),
            app_versions: vec![AppVersion {
                system_profile: "production".into(),
                app_version: "4.0.5".into(),
                multi_play_version: "miku".into(),
                asset_version: "4.0.5.10".into(),
                app_version_status: "available".into(),
            }],
        };
        let mock_body = aes_msgpack::into_vec(&mock_system_info).unwrap();

        let mock = server
            .mock("GET", "/api/system")
            .with_status(200)
            .with_body(&mock_body)
            .create_async()
            .await;

        let client = get_client(server.url()).await;

        let result = client.get_system().await;

        mock.assert();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), mock_system_info)
    }

    #[tokio::test]
    async fn test_issue_signature() {
        let server = get_server().await;

        let client = get_client(server.url()).await;

        assert_eq!(
            client.headers.0.get(header::name::COOKIE).unwrap(),
            SIGNATURE_COOKIE_VALUE
        )
    }
}
