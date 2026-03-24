use crate::{signer::WalletSigner, tempo::TempoAccessKeyConfig, utils, wallet_raw::RawWalletOpts};
use alloy_primitives::Address;
use clap::Parser;
use eyre::Result;
use serde::Serialize;

/// The wallet options can either be:
/// 1. Raw (via private key / mnemonic file, see `RawWallet`)
/// 2. Keystore (via file path)
/// 3. Ledger
/// 4. Trezor
/// 5. AWS KMS
/// 6. Google Cloud KMS
/// 7. Turnkey
/// 8. Browser wallet
/// 9. Access key (signs on behalf of a root account)
#[derive(Clone, Debug, Default, Serialize, Parser)]
#[command(next_help_heading = "Wallet options", about = None, long_about = None)]
pub struct WalletOpts {
    /// The sender account.
    #[arg(
        long,
        short,
        value_name = "ADDRESS",
        help_heading = "Wallet options - raw",
        env = "ETH_FROM"
    )]
    pub from: Option<Address>,

    #[command(flatten)]
    pub raw: RawWalletOpts,

    /// Use an access key to sign on behalf of a root account.
    ///
    /// The access key is a delegated key that can sign transactions for the root account.
    /// Requires --tempo.root-account to specify the account the key signs for.
    #[arg(
        long = "tempo.access-key",
        help_heading = "Wallet options - access key",
        value_name = "PRIVATE_KEY",
        env = "TEMPO_ACCESS_KEY"
    )]
    pub access_key: Option<String>,

    /// The root account address that the access key signs on behalf of.
    ///
    /// Required when using --tempo.access-key. This is the account that holds the funds
    /// and whose nonce is used for the transaction.
    #[arg(
        long = "tempo.root-account",
        help_heading = "Wallet options - access key",
        value_name = "ADDRESS",
        requires = "access_key",
        env = "TEMPO_ROOT_ACCOUNT"
    )]
    pub root_account: Option<Address>,

    /// Use the keystore in the given folder or file.
    #[arg(
        long = "keystore",
        help_heading = "Wallet options - keystore",
        value_name = "PATH",
        env = "ETH_KEYSTORE"
    )]
    pub keystore_path: Option<String>,

    /// Use a keystore from the default keystores folder (~/.foundry/keystores) by its filename
    #[arg(
        long = "account",
        help_heading = "Wallet options - keystore",
        value_name = "ACCOUNT_NAME",
        env = "ETH_KEYSTORE_ACCOUNT",
        conflicts_with = "keystore_path"
    )]
    pub keystore_account_name: Option<String>,

    /// The keystore password.
    ///
    /// Used with --keystore.
    #[arg(
        long = "password",
        help_heading = "Wallet options - keystore",
        requires = "keystore_path",
        value_name = "PASSWORD"
    )]
    pub keystore_password: Option<String>,

    /// The keystore password file path.
    ///
    /// Used with --keystore.
    #[arg(
        long = "password-file",
        help_heading = "Wallet options - keystore",
        requires = "keystore_path",
        value_name = "PASSWORD_FILE",
        env = "ETH_PASSWORD"
    )]
    pub keystore_password_file: Option<String>,

    /// Use a Ledger hardware wallet.
    #[arg(long, short, help_heading = "Wallet options - hardware wallet")]
    pub ledger: bool,

    /// Use a Trezor hardware wallet.
    #[arg(long, short, help_heading = "Wallet options - hardware wallet")]
    pub trezor: bool,

    /// Use AWS Key Management Service.
    ///
    /// Ensure the AWS_KMS_KEY_ID environment variable is set.
    #[arg(long, help_heading = "Wallet options - remote", hide = !cfg!(feature = "aws-kms"))]
    pub aws: bool,

    /// Use Google Cloud Key Management Service.
    ///
    /// Ensure the following environment variables are set: GCP_PROJECT_ID, GCP_LOCATION,
    /// GCP_KEY_RING, GCP_KEY_NAME, GCP_KEY_VERSION.
    ///
    /// See: <https://cloud.google.com/kms/docs>
    #[arg(long, help_heading = "Wallet options - remote", hide = !cfg!(feature = "gcp-kms"))]
    pub gcp: bool,

    /// Use Turnkey.
    ///
    /// Ensure the following environment variables are set: TURNKEY_API_PRIVATE_KEY,
    /// TURNKEY_ORGANIZATION_ID, TURNKEY_ADDRESS.
    ///
    /// See: <https://docs.turnkey.com/getting-started/quickstart>
    #[arg(long, help_heading = "Wallet options - remote", hide = !cfg!(feature = "turnkey"))]
    pub turnkey: bool,

    /// Use a browser wallet.
    #[arg(long, help_heading = "Wallet options - browser")]
    pub browser: bool,

    /// Port for the browser wallet server.
    #[arg(
        long,
        help_heading = "Wallet options - browser",
        value_name = "PORT",
        default_value = "9545",
        requires = "browser"
    )]
    pub browser_port: u16,

    /// Whether to open the browser for wallet connection.
    #[arg(
        long,
        help_heading = "Wallet options - browser",
        default_value_t = false,
        requires = "browser"
    )]
    pub browser_disable_open: bool,

    /// Enable development mode for the browser wallet.
    /// This relaxes certain security features for local development.
    ///
    /// **WARNING**: This should only be used in a development environment.
    #[arg(long, help_heading = "Wallet options - browser", hide = true)]
    pub browser_development: bool,
}

/// Access key configuration for signing on behalf of a root account.
#[derive(Clone, Debug)]
pub struct AccessKeyConfig {
    /// The root account address that the access key signs on behalf of.
    pub root_account: Address,
    /// The access key's address (derived from its private key).
    pub key_id: Address,
}

impl WalletOpts {
    /// Returns the access key configuration if an access key is being used.
    ///
    /// When using an access key:
    /// - Transactions should use `root_account` as the sender (`from`)
    /// - The `key_id` should be set on the transaction for Keychain signature wrapping
    pub fn access_key_config(&self) -> Option<AccessKeyConfig> {
        if let (Some(access_key), Some(root_account)) = (&self.access_key, self.root_account) {
            // Derive the access key address from the private key
            if let Ok(key_id) = self.derive_access_key_address(access_key) {
                return Some(AccessKeyConfig { root_account, key_id });
            }
        }
        None
    }

    /// Derives the address from an access key private key.
    fn derive_access_key_address(&self, private_key: &str) -> Result<Address> {
        use alloy_primitives::hex;
        let key_bytes: alloy_primitives::B256 = hex::FromHex::from_hex(private_key)?;
        let signer = alloy_signer_local::PrivateKeySigner::from_bytes(&key_bytes)?;
        Ok(alloy_signer::Signer::address(&signer))
    }

    /// Returns true if an access key is being used.
    pub fn is_access_key(&self) -> bool {
        self.access_key.is_some() && self.root_account.is_some()
    }

    /// Attempts to resolve a signer from the configured wallet options.
    ///
    /// Returns the signer and, for Tempo keychain mode, a [`TempoAccessKeyConfig`] describing the
    /// root wallet and provisioning data.
    ///
    /// Returns `Ok((None, None))` if no wallet option was configured and no Tempo fallback
    /// matched.
    pub async fn maybe_signer(
        &self,
    ) -> Result<(Option<WalletSigner>, Option<TempoAccessKeyConfig>)> {
        trace!("start finding signer (maybe)");

        let get_env = |key: &str| {
            std::env::var(key)
                .map_err(|_| eyre::eyre!("{key} environment variable is required for signer"))
        };

        let signer = if let Some(access_key) = &self.access_key {
            use alloy_primitives::hex;
            let key_bytes: alloy_primitives::B256 = hex::FromHex::from_hex(access_key)
                .map_err(|e| eyre::eyre!("Failed to decode access key: {e}"))?;
            WalletSigner::from_private_key(&key_bytes)?
        } else if self.ledger {
            utils::create_ledger_signer(self.raw.hd_path.as_deref(), self.raw.mnemonic_index)
                .await?
        } else if self.trezor {
            utils::create_trezor_signer(self.raw.hd_path.as_deref(), self.raw.mnemonic_index)
                .await?
        } else if self.aws {
            let key_id = get_env("AWS_KMS_KEY_ID")?;
            WalletSigner::from_aws(key_id).await?
        } else if self.gcp {
            let project_id = get_env("GCP_PROJECT_ID")?;
            let location = get_env("GCP_LOCATION")?;
            let keyring = get_env("GCP_KEY_RING")?;
            let key_name = get_env("GCP_KEY_NAME")?;
            let key_version = get_env("GCP_KEY_VERSION")?
                .parse()
                .map_err(|_| eyre::eyre!("GCP_KEY_VERSION could not be parsed into u64"))?;
            WalletSigner::from_gcp(project_id, location, keyring, key_name, key_version).await?
        } else if self.turnkey {
            let api_private_key = get_env("TURNKEY_API_PRIVATE_KEY")?;
            let organization_id = get_env("TURNKEY_ORGANIZATION_ID")?;
            let address_str = get_env("TURNKEY_ADDRESS")?;
            let address = address_str.parse().map_err(|_| {
                eyre::eyre!("TURNKEY_ADDRESS could not be parsed as an Ethereum address")
            })?;
            WalletSigner::from_turnkey(api_private_key, organization_id, address)?
        } else if self.browser {
            WalletSigner::from_browser(
                self.browser_port,
                !self.browser_disable_open,
                self.browser_development,
            )
            .await?
        } else if let Some(raw_wallet) = self.raw.signer()? {
            raw_wallet
        } else if let Some(path) = utils::maybe_get_keystore_path(
            self.keystore_path.as_deref(),
            self.keystore_account_name.as_deref(),
        )? {
            let (maybe_signer, maybe_pending) = utils::create_keystore_signer(
                &path,
                self.keystore_password.as_deref(),
                self.keystore_password_file.as_deref(),
            )?;
            if let Some(pending) = maybe_pending {
                pending.unlock()?
            } else if let Some(signer) = maybe_signer {
                signer
            } else {
                unreachable!()
            }
        } else {
            // No explicit wallet option was provided. Try Tempo wallet as a fallback
            // if `--from` is set.
            if let Some(from) = self.from {
                match crate::tempo::lookup_signer(from)? {
                    crate::tempo::TempoLookup::Direct(signer) => {
                        return Ok((Some(signer), None));
                    }
                    crate::tempo::TempoLookup::Keychain(signer, config) => {
                        return Ok((Some(signer), Some(*config)));
                    }
                    crate::tempo::TempoLookup::NotFound => {}
                }
            }

            return Ok((None, None));
        };

        Ok((Some(signer), None))
    }

    pub async fn signer(&self) -> Result<WalletSigner> {
        trace!("start finding signer");

        let get_env = |key: &str| {
            std::env::var(key)
                .map_err(|_| eyre::eyre!("{key} environment variable is required for signer"))
        };

        // Handle access key first - it uses a local signer with the access key private key
        let signer = if let Some(access_key) = &self.access_key {
            use alloy_primitives::hex;
            let key_bytes: alloy_primitives::B256 = hex::FromHex::from_hex(access_key)
                .map_err(|e| eyre::eyre!("Failed to decode access key: {e}"))?;
            WalletSigner::from_private_key(&key_bytes)?
        } else if self.ledger {
            utils::create_ledger_signer(self.raw.hd_path.as_deref(), self.raw.mnemonic_index)
                .await?
        } else if self.trezor {
            utils::create_trezor_signer(self.raw.hd_path.as_deref(), self.raw.mnemonic_index)
                .await?
        } else if self.aws {
            let key_id = get_env("AWS_KMS_KEY_ID")?;
            WalletSigner::from_aws(key_id).await?
        } else if self.gcp {
            let project_id = get_env("GCP_PROJECT_ID")?;
            let location = get_env("GCP_LOCATION")?;
            let keyring = get_env("GCP_KEY_RING")?;
            let key_name = get_env("GCP_KEY_NAME")?;
            let key_version = get_env("GCP_KEY_VERSION")?
                .parse()
                .map_err(|_| eyre::eyre!("GCP_KEY_VERSION could not be parsed into u64"))?;
            WalletSigner::from_gcp(project_id, location, keyring, key_name, key_version).await?
        } else if self.turnkey {
            let api_private_key = get_env("TURNKEY_API_PRIVATE_KEY")?;
            let organization_id = get_env("TURNKEY_ORGANIZATION_ID")?;
            let address_str = get_env("TURNKEY_ADDRESS")?;
            let address = address_str.parse().map_err(|_| {
                eyre::eyre!("TURNKEY_ADDRESS could not be parsed as an Ethereum address")
            })?;
            WalletSigner::from_turnkey(api_private_key, organization_id, address)?
        } else if self.browser {
            WalletSigner::from_browser(
                self.browser_port,
                !self.browser_disable_open,
                self.browser_development,
            )
            .await?
        } else if let Some(raw_wallet) = self.raw.signer()? {
            raw_wallet
        } else if let Some(path) = utils::maybe_get_keystore_path(
            self.keystore_path.as_deref(),
            self.keystore_account_name.as_deref(),
        )? {
            let (maybe_signer, maybe_pending) = utils::create_keystore_signer(
                &path,
                self.keystore_password.as_deref(),
                self.keystore_password_file.as_deref(),
            )?;
            if let Some(pending) = maybe_pending {
                pending.unlock()?
            } else if let Some(signer) = maybe_signer {
                signer
            } else {
                unreachable!()
            }
        } else {
            eyre::bail!(
                "\
Error accessing local wallet. Did you pass a keystore, hardware wallet, private key or mnemonic?

Run the command with --help flag for more information or use the corresponding CLI
flag to set your key via:

--keystore
--interactive
--private-key
--mnemonic-path
--tempo.access-key (with --tempo.root-account for Tempo access keys)
--aws
--gcp
--turnkey
--trezor
--ledger
--browser

Alternatively, when using the `cast send` or `cast mktx` commands with a local node
or RPC that has unlocked accounts, the --unlocked or --ethsign flags can be used,
respectively. The sender address can be specified by setting the `ETH_FROM` environment
variable to the desired unlocked account address, or by providing the address directly
using the --from flag."
            )
        };

        Ok(signer)
    }
}

impl From<RawWalletOpts> for WalletOpts {
    fn from(options: RawWalletOpts) -> Self {
        Self { raw: options, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_signer::Signer;
    use std::{path::Path, str::FromStr};

    #[tokio::test]
    async fn find_keystore() {
        let keystore =
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../cast/tests/fixtures/keystore"));
        let keystore_file = keystore
            .join("UTC--2022-12-20T10-30-43.591916000Z--ec554aeafe75601aaab43bd4621a22284db566c2");
        let password_file = keystore.join("password-ec554");
        let wallet: WalletOpts = WalletOpts::parse_from([
            "foundry-cli",
            "--from",
            "560d246fcddc9ea98a8b032c9a2f474efb493c28",
            "--keystore",
            keystore_file.to_str().unwrap(),
            "--password-file",
            password_file.to_str().unwrap(),
        ]);
        let signer = wallet.signer().await.unwrap();
        assert_eq!(
            signer.address(),
            Address::from_str("ec554aeafe75601aaab43bd4621a22284db566c2").unwrap()
        );
    }

    #[tokio::test]
    async fn illformed_private_key_generates_user_friendly_error() {
        let wallet = WalletOpts {
            raw: RawWalletOpts {
                interactive: false,
                private_key: Some("123".to_string()),
                mnemonic: None,
                mnemonic_passphrase: None,
                hd_path: None,
                mnemonic_index: 0,
            },
            from: None,
            access_key: None,
            root_account: None,
            keystore_path: None,
            keystore_account_name: None,
            keystore_password: None,
            keystore_password_file: None,
            ledger: false,
            trezor: false,
            aws: false,
            gcp: false,
            turnkey: false,
            browser: false,
            browser_port: 9545,
            browser_development: false,
            browser_disable_open: false,
        };
        match wallet.signer().await {
            Ok(_) => {
                panic!("illformed private key shouldn't decode")
            }
            Err(x) => {
                assert!(
                    x.to_string().contains("Failed to decode private key"),
                    "Error message is not user-friendly"
                );
            }
        }
    }

    #[tokio::test]
    async fn access_key_signer_works() {
        // Test private key (well-known test key, do not use in production!)
        let test_key = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
        let expected_address =
            Address::from_str("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266").unwrap();
        let root_account = Address::from_str("1234567890123456789012345678901234567890").unwrap();

        let wallet = WalletOpts {
            raw: RawWalletOpts::default(),
            from: None,
            access_key: Some(test_key.to_string()),
            root_account: Some(root_account),
            keystore_path: None,
            keystore_account_name: None,
            keystore_password: None,
            keystore_password_file: None,
            ledger: false,
            trezor: false,
            aws: false,
            gcp: false,
            turnkey: false,
            browser: false,
            browser_port: 9545,
            browser_development: false,
            browser_disable_open: false,
        };

        // Test that the signer is created with the access key
        let signer = wallet.signer().await.unwrap();
        assert_eq!(signer.address(), expected_address);

        // Test access_key_config
        let config = wallet.access_key_config().unwrap();
        assert_eq!(config.root_account, root_account);
        assert_eq!(config.key_id, expected_address);

        // Test is_access_key
        assert!(wallet.is_access_key());
    }
}
