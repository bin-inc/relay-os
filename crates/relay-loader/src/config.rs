use core::str::FromStr;

use relay_abi::GptGuid;

use crate::ConfigError;

const MAX_CONFIG_SIZE: usize = 4096;
const ROOT_GUID_KEY: &[u8] = b"root-guid";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoaderConfig {
    pub root_guid: GptGuid,
}

pub fn parse_config(bytes: &[u8]) -> Result<LoaderConfig, ConfigError> {
    if bytes.len() > MAX_CONFIG_SIZE {
        return Err(ConfigError::TooLarge);
    }
    if bytes.contains(&0) {
        return Err(ConfigError::NulByte);
    }
    if !bytes.is_ascii() {
        return Err(ConfigError::NonAscii);
    }
    if bytes.is_empty() {
        return Err(ConfigError::MissingRootGuid);
    }
    if !bytes.ends_with(b"\n") {
        return Err(ConfigError::MissingTrailingNewline);
    }

    let lines = &bytes[..bytes.len() - 1];
    if lines.is_empty() {
        return Err(ConfigError::MissingRootGuid);
    }

    let mut root_guid = None;
    for line in lines.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            return Err(ConfigError::InvalidFormat);
        }
        if line.iter().any(u8::is_ascii_whitespace) {
            return Err(ConfigError::Whitespace);
        }
        let Some(separator) = line.iter().position(|byte| *byte == b'=') else {
            return Err(ConfigError::InvalidFormat);
        };
        let (key, value_with_separator) = line.split_at(separator);
        let value = value_with_separator
            .get(1..)
            .ok_or(ConfigError::InvalidFormat)?;
        if key != ROOT_GUID_KEY {
            return Err(ConfigError::UnknownKey);
        }
        if root_guid.is_some() {
            return Err(ConfigError::DuplicateRootGuid);
        }

        let value = core::str::from_utf8(value).map_err(|_| ConfigError::NonAscii)?;
        let guid = GptGuid::from_str(value).map_err(|_| ConfigError::InvalidRootGuid)?;
        root_guid = Some(guid);
    }

    root_guid
        .map(|root_guid| LoaderConfig { root_guid })
        .ok_or(ConfigError::MissingRootGuid)
}
