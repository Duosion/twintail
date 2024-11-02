use super::aes::{decrypt, encrypt};
use crate::error::ApiError;
use serde::{de::DeserializeOwned, Serialize};

/// Convert an AES & msgpack encoded slice into something that implements the trait ``serde::de::DeserializeOwned``
pub fn from_slice<T>(slice: &[u8]) -> Result<T, ApiError>
where
    T: DeserializeOwned,
{
    // decrypt from AES
    let decrypted = decrypt(slice)
        .map_err(|_| ApiError::AesMsgpack("error when decrypting AES encoded body".into()))?;

    // deserialize from msgpack
    let deserialized: T = rmp_serde::from_slice(&decrypted)?;

    Ok(deserialized)
}

/// Convert something that implements the trait ``serde::Serialize`` into an AES & msgpack encoded value.
pub fn into_vec<T>(value: &T) -> Result<Vec<u8>, ApiError>
where
    T: Serialize,
{
    // serialize & encrypt
    let serialized = rmp_serde::to_vec_named(value)?;
    Ok(encrypt(&serialized))
}

#[cfg(test)]
mod tests {
    use crate::models::api::GameVersion;

    use super::*;

    #[test]
    fn test_aes_msgpack() {
        let game_version = GameVersion {
            profile: "production".into(),
            assetbundle_host_hash: "cf2d2388".into(),
            domain: "production-game-api.sekai.colorfulpalette.org".into(),
        };

        // serialize & encrypt
        let aes_encoded = into_vec(&game_version).unwrap();

        // decrypt & deserialize
        let decrypted_game_version: GameVersion = from_slice(&aes_encoded).unwrap();
        assert_eq!(game_version, decrypted_game_version)
    }
}
