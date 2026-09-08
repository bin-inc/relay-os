use core::str::FromStr;
use relay_abi::GptGuid;
use relay_loader::{ConfigError, LoaderConfig, parse_config};

const ROOT_GUID: &str = "52454c41-5900-4000-8000-000000000003";

#[test]
fn config_accepts_only_the_canonical_root_guid_line() {
    assert_eq!(
        parse_config(b"root-guid=52454c41-5900-4000-8000-000000000003\n"),
        Ok(LoaderConfig {
            root_guid: GptGuid::from_str(ROOT_GUID).unwrap(),
        })
    );
}

#[test]
fn config_requires_exactly_one_root_guid() {
    assert!(matches!(
        parse_config(b""),
        Err(ConfigError::MissingRootGuid)
    ));
    assert!(matches!(
        parse_config(b"root-guid=bad\n"),
        Err(ConfigError::InvalidRootGuid)
    ));
    assert!(matches!(
        parse_config(b"root-guid=52454c41-5900-4000-8000-000000000003\nroot-guid=52454c41-5900-4000-8000-000000000003\n"),
        Err(ConfigError::DuplicateRootGuid)
    ));
}

#[test]
fn config_rejects_noncanonical_or_structurally_invalid_input() {
    for bytes in [
        b"root-guid=52454C41-5900-4000-8000-000000000003\n".as_slice(),
        b" root-guid=52454c41-5900-4000-8000-000000000003\n",
        b"root-guid =52454c41-5900-4000-8000-000000000003\n",
        b"root-guid=52454c41-5900-4000-8000-000000000003",
        b"root-guid=52454c41-5900-4000-8000-000000000003\nunknown=value\n",
        b"root-guid=52454c41-5900-4000-8000-000000000003\0\n",
        b"root-guid=52454c41-5900-4000-8000-000000000003\xff\n",
    ] {
        assert!(parse_config(bytes).is_err(), "{bytes:?}");
    }

    let oversized = [b'x'; 4097];
    assert_eq!(parse_config(&oversized), Err(ConfigError::TooLarge));
}
