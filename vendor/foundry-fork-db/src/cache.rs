//! Cache related abstraction

use alloy_chains::Chain;
use alloy_primitives::{Address, B256, U256, map::U256Map};
use parking_lot::RwLock;
use revm::{
    DatabaseCommit,
    context::BlockEnv,
    primitives::{KECCAK_EMPTY, StorageKeyMap, map::AddressHashMap},
    state::{Account, AccountInfo, AccountStatus},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap};
use std::{
    collections::BTreeSet,
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use tempo_revm::TempoBlockEnv;
use url::Url;

pub type StorageInfo = StorageKeyMap<U256>;

/// A shareable Block database
#[derive(Clone, Debug)]
pub struct BlockchainDb<B = BlockEnv> {
    /// Contains all the data
    db: Arc<MemDb>,
    /// metadata of the current config
    meta: Arc<RwLock<BlockchainDbMeta<B>>>,
    /// the cache that can be flushed
    cache: Arc<JsonBlockCacheDB<B>>,
}

impl<B: ForkBlockEnv> BlockchainDb<B> {
    /// Creates a new instance of the [BlockchainDb].
    ///
    /// If a `cache_path` is provided it attempts to load a previously stored [JsonBlockCacheData]
    /// and will try to use the cached entries it holds.
    ///
    /// This will return a new and empty [MemDb] if
    ///   - `cache_path` is `None`
    ///   - the file the `cache_path` points to, does not exist
    ///   - the file contains malformed data, or if it couldn't be read
    ///   - the provided `meta` differs from [BlockchainDbMeta] that's stored on disk
    pub fn new(meta: BlockchainDbMeta<B>, cache_path: Option<PathBuf>) -> Self {
        Self::new_db(meta, cache_path, false)
    }

    /// Creates a new instance of the [BlockchainDb] and skips check when comparing meta
    /// This is useful for offline-start mode when we don't want to fetch metadata of `block`.
    ///
    /// if a `cache_path` is provided it attempts to load a previously stored [JsonBlockCacheData]
    /// and will try to use the cached entries it holds.
    ///
    /// This will return a new and empty [MemDb] if
    ///   - `cache_path` is `None`
    ///   - the file the `cache_path` points to, does not exist
    ///   - the file contains malformed data, or if it couldn't be read
    ///   - the provided `meta` differs from [BlockchainDbMeta] that's stored on disk
    pub fn new_skip_check(meta: BlockchainDbMeta<B>, cache_path: Option<PathBuf>) -> Self {
        Self::new_db(meta, cache_path, true)
    }

    fn new_db(meta: BlockchainDbMeta<B>, cache_path: Option<PathBuf>, skip_check: bool) -> Self {
        trace!(target: "forge::cache", cache=?cache_path, "initialising blockchain db");
        // read cache and check if metadata matches
        let cache = cache_path
            .as_ref()
            .and_then(|p| {
                JsonBlockCacheDB::load(p).ok().filter(|cache| {
                    if skip_check {
                        return true;
                    }
                    let mut existing = cache.meta().write();
                    existing.hosts.extend(meta.hosts.clone());
                    if meta != *existing {
                        warn!(target: "cache", "non-matching block metadata");
                        false
                    } else {
                        true
                    }
                })
            })
            .unwrap_or_else(|| JsonBlockCacheDB::new(Arc::new(RwLock::new(meta)), cache_path));

        Self { db: Arc::clone(cache.db()), meta: Arc::clone(cache.meta()), cache: Arc::new(cache) }
    }

    /// Returns the map that holds the account related info
    pub fn accounts(&self) -> &RwLock<AddressHashMap<AccountInfo>> {
        &self.db.accounts
    }

    /// Returns the map that holds the storage related info
    pub fn storage(&self) -> &RwLock<AddressHashMap<StorageInfo>> {
        &self.db.storage
    }

    /// Returns the map that holds all the block hashes
    pub fn block_hashes(&self) -> &RwLock<U256Map<B256>> {
        &self.db.block_hashes
    }

    /// Returns the Env related metadata
    pub const fn meta(&self) -> &Arc<RwLock<BlockchainDbMeta<B>>> {
        &self.meta
    }

    /// Returns the inner cache
    pub const fn cache(&self) -> &Arc<JsonBlockCacheDB<B>> {
        &self.cache
    }

    /// Returns the underlying storage
    pub const fn db(&self) -> &Arc<MemDb> {
        &self.db
    }
}

/// A helper trait for serializing the `block_env` field of [`BlockchainDbMeta`].
///
/// This exists because some block environment types (e.g. [`TempoBlockEnv`]) do not implement
/// [`Serialize`] directly, so their serializable representation is handled here.
pub trait SerializeBlockEnv: Clone {
    fn serialize_block_env<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error>;
}

impl SerializeBlockEnv for BlockEnv {
    fn serialize_block_env<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.serialize(serializer)
    }
}

impl SerializeBlockEnv for TempoBlockEnv {
    fn serialize_block_env<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.inner.serialize(serializer)
    }
}

/// A helper trait for deserializing the `block_env` field of [`BlockchainDbMeta`].
///
/// This exists because some block environment types (e.g. [`TempoBlockEnv`]) do not implement
/// [`Deserialize`] directly, so their construction from a stored [`BlockEnv`] is handled here.
pub trait DeserializeBlockEnv: Sized {
    fn from_owned_block_env(block_env: BlockEnv) -> Self;
}

impl DeserializeBlockEnv for BlockEnv {
    fn from_owned_block_env(block_env: BlockEnv) -> Self {
        block_env
    }
}

impl DeserializeBlockEnv for TempoBlockEnv {
    fn from_owned_block_env(block_env: BlockEnv) -> Self {
        Self { inner: block_env, timestamp_millis_part: 0 }
    }
}

/// Marker trait for block environment types that can be used with the forking backend.
///
/// Automatically implemented for any `B: SerializeBlockEnv + DeserializeBlockEnv + Clone +
/// PartialEq + Send + Sync + 'static`.
pub trait ForkBlockEnv:
    SerializeBlockEnv + DeserializeBlockEnv + Clone + PartialEq + Send + Sync + 'static
{
}

impl<B: SerializeBlockEnv + DeserializeBlockEnv + Clone + PartialEq + Send + Sync + 'static>
    ForkBlockEnv for B
{
}

/// relevant identifying markers in the context of [BlockchainDb]
#[derive(Clone, Debug, Default, Eq)]
pub struct BlockchainDbMeta<B> {
    /// The chain of the blockchain of the block environment
    pub chain: Option<Chain>,
    /// The block environment
    pub block_env: B,
    /// All the hosts used to connect to
    pub hosts: BTreeSet<String>,
}

impl<B> BlockchainDbMeta<B> {
    /// Creates a new instance
    pub fn new(block_env: B, url: String) -> Self {
        let host = Url::parse(&url)
            .ok()
            .and_then(|url| url.host().map(|host| host.to_string()))
            .unwrap_or(url);

        Self { chain: None, block_env, hosts: BTreeSet::from([host]) }
    }

    /// Infers the host from the provided url and adds it to the set of hosts
    pub fn with_url(mut self, url: &str) -> Self {
        let host = Url::parse(url)
            .ok()
            .and_then(|url| url.host().map(|host| host.to_string()))
            .unwrap_or(url.to_string());
        self.hosts.insert(host);
        self
    }

    /// Sets the [Chain] of this instance
    pub const fn set_chain(mut self, chain: Chain) -> Self {
        self.chain = Some(chain);
        self
    }

    /// Sets the block environment of this instance
    pub fn set_block_env(mut self, block_env: B) -> Self {
        self.block_env = block_env;
        self
    }
}

// ignore hosts to not invalidate the cache when different endpoints are used, as it's commonly the
// case for http vs ws endpoints
impl<B: PartialEq> PartialEq for BlockchainDbMeta<B> {
    fn eq(&self, other: &Self) -> bool {
        self.block_env == other.block_env
    }
}

impl<B: SerializeBlockEnv> Serialize for BlockchainDbMeta<B> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        struct BlockEnvWrapper<'a, B>(&'a B);
        impl<'a, B: SerializeBlockEnv> Serialize for BlockEnvWrapper<'a, B> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.serialize_block_env(serializer)
            }
        }

        let field_count = if self.chain.is_some() { 3 } else { 2 };
        let mut s = serializer.serialize_struct("BlockchainDbMeta", field_count)?;
        if let Some(chain) = &self.chain {
            s.serialize_field("chain", chain)?;
        }
        s.serialize_field("block_env", &BlockEnvWrapper(&self.block_env))?;
        s.serialize_field("hosts", &self.hosts)?;
        s.end()
    }
}

/// A backwards compatible representation of [BlockEnv]
///
/// This prevents deserialization errors of cache files caused by breaking changes to the
/// default [BlockEnv], for example enabling an optional feature.
/// By hand rolling deserialize impl we can prevent cache file issues
struct BlockEnvBackwardsCompat {
    inner: BlockEnv,
}

impl<'de> Deserialize<'de> for BlockEnvBackwardsCompat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = serde_json::Value::deserialize(deserializer)?;

        // we check for any missing fields here
        if let Some(obj) = value.as_object_mut() {
            let default_value = serde_json::to_value(BlockEnv::default()).unwrap();
            for (key, value) in default_value.as_object().unwrap() {
                if !obj.contains_key(key) {
                    obj.insert(key.to_string(), value.clone());
                }
            }
        }

        let cfg_env: BlockEnv = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self { inner: cfg_env })
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Hosts {
    Multi(BTreeSet<String>),
    Single(String),
}

impl From<Hosts> for BTreeSet<String> {
    fn from(hosts: Hosts) -> Self {
        match hosts {
            Hosts::Multi(hosts) => hosts,
            Hosts::Single(host) => Self::from([host]),
        }
    }
}

impl<'de, B: DeserializeBlockEnv> Deserialize<'de> for BlockchainDbMeta<B> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Meta {
            chain: Option<Chain>,
            block_env: BlockEnvBackwardsCompat,
            #[serde(alias = "host")]
            hosts: Hosts,
        }

        let Meta { chain, block_env, hosts } = Meta::deserialize(deserializer)?;
        Ok(Self { chain, block_env: B::from_owned_block_env(block_env.inner), hosts: hosts.into() })
    }
}

/// In Memory cache containing all fetched accounts and storage slots
/// and their values from RPC
#[derive(Debug, Default)]
pub struct MemDb {
    /// Account related data
    pub accounts: RwLock<AddressHashMap<AccountInfo>>,
    /// Storage related data
    pub storage: RwLock<AddressHashMap<StorageInfo>>,
    /// All retrieved block hashes
    pub block_hashes: RwLock<U256Map<B256>>,
}

impl MemDb {
    /// Clears all data stored in this db
    pub fn clear(&self) {
        self.accounts.write().clear();
        self.storage.write().clear();
        self.block_hashes.write().clear();
    }

    // Inserts the account, replacing it if it exists already
    pub fn do_insert_account(&self, address: Address, account: AccountInfo) {
        self.accounts.write().insert(address, account);
    }

    /// The implementation of [DatabaseCommit::commit()]
    pub fn do_commit(&self, changes: AddressHashMap<Account>) {
        let mut storage = self.storage.write();
        let mut accounts = self.accounts.write();
        for (add, mut acc) in changes {
            if acc.is_empty() || acc.is_selfdestructed() {
                accounts.remove(&add);
                storage.remove(&add);
            } else {
                // insert account
                if let Some(code_hash) = acc
                    .info
                    .code
                    .as_ref()
                    .filter(|code| !code.is_empty())
                    .map(|code| code.hash_slow())
                {
                    acc.info.code_hash = code_hash;
                } else if acc.info.code_hash.is_zero() {
                    acc.info.code_hash = KECCAK_EMPTY;
                }
                accounts.insert(add, acc.info);

                let acc_storage = storage.entry(add).or_default();
                if acc.status.contains(AccountStatus::Created) {
                    acc_storage.clear();
                }
                for (index, value) in acc.storage {
                    if value.present_value().is_zero() {
                        acc_storage.remove(&index);
                    } else {
                        acc_storage.insert(index, value.present_value());
                    }
                }
                if acc_storage.is_empty() {
                    storage.remove(&add);
                }
            }
        }
    }
}

impl Clone for MemDb {
    fn clone(&self) -> Self {
        Self {
            storage: RwLock::new(self.storage.read().clone()),
            accounts: RwLock::new(self.accounts.read().clone()),
            block_hashes: RwLock::new(self.block_hashes.read().clone()),
        }
    }
}

impl DatabaseCommit for MemDb {
    fn commit(&mut self, changes: AddressHashMap<Account>) {
        self.do_commit(changes)
    }
}

/// A DB that stores the cached content in a json file
#[derive(Debug)]
pub struct JsonBlockCacheDB<B> {
    /// Where this cache file is stored.
    ///
    /// If this is a [None] then caching is disabled
    cache_path: Option<PathBuf>,
    /// Object that's stored in a json file
    data: JsonBlockCacheData<B>,
}

impl<B> JsonBlockCacheDB<B> {
    /// Creates a new instance.
    fn new(meta: Arc<RwLock<BlockchainDbMeta<B>>>, cache_path: Option<PathBuf>) -> Self {
        Self { cache_path, data: JsonBlockCacheData { meta, data: Arc::new(Default::default()) } }
    }

    /// Returns the [MemDb] it holds access to
    pub const fn db(&self) -> &Arc<MemDb> {
        &self.data.data
    }

    /// Metadata stored alongside the data
    pub const fn meta(&self) -> &Arc<RwLock<BlockchainDbMeta<B>>> {
        &self.data.meta
    }

    /// Returns `true` if this is a transient cache and nothing will be flushed
    pub const fn is_transient(&self) -> bool {
        self.cache_path.is_none()
    }

    /// Returns the cache path.
    pub fn cache_path(&self) -> Option<&Path> {
        self.cache_path.as_deref()
    }
}

impl<B: ForkBlockEnv> JsonBlockCacheDB<B> {
    /// Loads the contents of the diskmap file and returns the read object
    ///
    /// # Errors
    /// This will fail if
    ///   - the `path` does not exist
    ///   - the format does not match [JsonBlockCacheData]
    pub fn load(path: impl Into<PathBuf>) -> eyre::Result<Self> {
        let path = path.into();
        trace!(target: "cache", ?path, "reading json cache");
        let contents = std::fs::read_to_string(&path).inspect_err(|err| {
            warn!(?err, ?path, "Failed to read cache file");
        })?;
        let data = serde_json::from_str(&contents).inspect_err(|err| {
            warn!(target: "cache", ?err, ?path, "Failed to deserialize cache data");
        })?;
        trace!(target: "cache", ?path, "read json cache");
        Ok(Self { cache_path: Some(path), data })
    }
}

impl<B: SerializeBlockEnv> JsonBlockCacheDB<B> {
    /// Flushes the DB to disk if caching is enabled.
    #[instrument(level = "warn", skip_all, fields(path = ?self.cache_path))]
    pub fn flush(&self) {
        let Some(path) = &self.cache_path else { return };
        self.flush_to(path.as_path());
    }

    /// Flushes the DB to a specific file
    pub fn flush_to(&self, cache_path: &Path) {
        let path: &Path = cache_path;

        trace!(target: "cache", "saving json cache");

        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let file = match fs::File::create(path) {
            Ok(file) => file,
            Err(e) => return warn!(target: "cache", %e, "Failed to open json cache for writing"),
        };

        let mut writer = BufWriter::new(file);
        if let Err(e) = serde_json::to_writer(&mut writer, &self.data) {
            return warn!(target: "cache", %e, "Failed to write to json cache");
        }
        if let Err(e) = writer.flush() {
            return warn!(target: "cache", %e, "Failed to flush to json cache");
        }

        trace!(target: "cache", "saved json cache");
    }
}

/// The Data the [JsonBlockCacheDB] can read and flush
///
/// This will be deserialized in a JSON object with the keys:
/// `["meta", "accounts", "storage", "block_hashes"]`
#[derive(Debug)]
pub struct JsonBlockCacheData<B> {
    pub meta: Arc<RwLock<BlockchainDbMeta<B>>>,
    pub data: Arc<MemDb>,
}

impl<B: SerializeBlockEnv> Serialize for JsonBlockCacheData<B> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(4))?;

        map.serialize_entry("meta", &self.meta.read().clone())?;
        map.serialize_entry("accounts", &self.data.accounts.read().clone())?;
        map.serialize_entry("storage", &self.data.storage.read().clone())?;
        map.serialize_entry("block_hashes", &self.data.block_hashes.read().clone())?;

        map.end()
    }
}

impl<'de, B: DeserializeBlockEnv> Deserialize<'de> for JsonBlockCacheData<B> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Data<B> {
            meta: B,
            accounts: AddressHashMap<AccountInfo>,
            storage: AddressHashMap<StorageInfo>,
            block_hashes: U256Map<B256>,
        }

        let Data { meta, accounts, storage, block_hashes } =
            Data::<BlockchainDbMeta<B>>::deserialize(deserializer)?;

        Ok(Self {
            meta: Arc::new(RwLock::new(meta)),
            data: Arc::new(MemDb {
                accounts: RwLock::new(accounts),
                storage: RwLock::new(storage),
                block_hashes: RwLock::new(block_hashes),
            }),
        })
    }
}

/// A type that flushes a `JsonBlockCacheDB` on drop
///
/// This type intentionally does not implement `Clone` since it's intended that there's only once
/// instance that will flush the cache.
#[derive(Debug)]
pub struct FlushJsonBlockCacheDB<B: SerializeBlockEnv>(pub Arc<JsonBlockCacheDB<B>>);

impl<B: SerializeBlockEnv> Drop for FlushJsonBlockCacheDB<B> {
    fn drop(&mut self) {
        trace!(target: "fork::cache", "flushing cache");
        self.0.flush();
        trace!(target: "fork::cache", "flushed cache");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_deserialize_cache() {
        let s = r#"{
    "meta": {
        "cfg_env": {
            "chain_id": 1337,
            "perf_analyse_created_bytecodes": "Analyse",
            "limit_contract_code_size": 18446744073709551615,
            "memory_limit": 4294967295,
            "disable_block_gas_limit": false,
            "disable_eip3607": false,
            "disable_base_fee": false
        },
        "block_env": {
            "number": 15547871,
            "coinbase": "0x0000000000000000000000000000000000000000",
            "timestamp": 1663351871,
            "difficulty": "0x0",
            "basefee": 12448539171,
            "gas_limit": 30000000,
            "prevrandao": "0x0000000000000000000000000000000000000000000000000000000000000000"
        },
        "hosts": [
            "eth-mainnet.alchemyapi.io"
        ]
    },
    "accounts": {
        "0xb8ffc3cd6e7cf5a098a1c92f48009765b24088dc": {
            "balance": "0x0",
            "nonce": 10,
            "code_hash": "0x3ac64c95eedf82e5d821696a12daac0e1b22c8ee18a9fd688b00cfaf14550aad",
            "code": {
                "LegacyAnalyzed": {
                    "bytecode": "0x00",
                    "original_len": 0,
                    "jump_table": {
                      "order": "bitvec::order::Lsb0",
                      "head": {
                        "width": 8,
                        "index": 0
                      },
                      "bits": 1,
                      "data": [0]
                    }
                }
            }
        }
    },
    "storage": {
        "0xa354f35829ae975e850e23e9615b11da1b3dc4de": {
            "0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e564": "0x5553444320795661756c74000000000000000000000000000000000000000000",
            "0x10": "0x37fd60ff8346",
            "0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563": "0xb",
            "0x6": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "0x5": "0x36ff5b93162e",
            "0x14": "0x29d635a8e000",
            "0x11": "0x63224c73",
            "0x2": "0x6"
        }
    },
    "block_hashes": {
        "0xed3deb": "0xbf7be3174b261ea3c377b6aba4a1e05d5fae7eee7aab5691087c20cf353e9877",
        "0xed3de9": "0xba1c3648e0aee193e7d00dffe4e9a5e420016b4880455641085a4731c1d32eef",
        "0xed3de8": "0x61d1491c03a9295fb13395cca18b17b4fa5c64c6b8e56ee9cc0a70c3f6cf9855",
        "0xed3de7": "0xb54560b5baeccd18350d56a3bee4035432294dc9d2b7e02f157813e1dee3a0be",
        "0xed3dea": "0x816f124480b9661e1631c6ec9ee39350bda79f0cbfc911f925838d88e3d02e4b"
    }
}"#;

        let cache: JsonBlockCacheData<BlockEnv> = serde_json::from_str(s).unwrap();
        assert_eq!(cache.data.accounts.read().len(), 1);
        assert_eq!(cache.data.storage.read().len(), 1);
        assert_eq!(cache.data.block_hashes.read().len(), 5);

        let _s = serde_json::to_string(&cache).unwrap();
    }

    #[test]
    fn can_deserialize_cache_post_4844() {
        let s = r#"{
    "meta": {
        "cfg_env": {
            "chain_id": 1,
            "kzg_settings": "Default",
            "perf_analyse_created_bytecodes": "Analyse",
            "limit_contract_code_size": 18446744073709551615,
            "memory_limit": 134217728,
            "disable_block_gas_limit": false,
            "disable_eip3607": true,
            "disable_base_fee": false,
            "optimism": false
        },
        "block_env": {
            "number": 18651580,
            "coinbase": "0x4838b106fce9647bdf1e7877bf73ce8b0bad5f97",
            "timestamp": 1700950019,
            "gas_limit": 30000000,
            "basefee": 26886078239,
            "difficulty": "0xc6b1a299886016dea3865689f8393b9bf4d8f4fe8c0ad25f0058b3569297c057",
            "prevrandao": "0xc6b1a299886016dea3865689f8393b9bf4d8f4fe8c0ad25f0058b3569297c057",
            "blob_excess_gas_and_price": {
                "excess_blob_gas": 0,
                "blob_gasprice": 1
            }
        },
        "hosts": [
            "eth-mainnet.alchemyapi.io"
        ]
    },
    "accounts": {
        "0x4838b106fce9647bdf1e7877bf73ce8b0bad5f97": {
            "balance": "0x8e0c373cfcdfd0eb",
            "nonce": 128912,
            "code_hash": "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470",
            "code": {
                "LegacyAnalyzed": {
                    "bytecode": "0x00",
                    "original_len": 0,
                    "jump_table": {
                      "order": "bitvec::order::Lsb0",
                      "head": {
                        "width": 8,
                        "index": 0
                      },
                      "bits": 1,
                      "data": [0]
                    }
                }
            }
        }
    },
    "storage": {},
    "block_hashes": {}
}"#;

        let cache: JsonBlockCacheData<BlockEnv> = serde_json::from_str(s).unwrap();
        assert_eq!(cache.data.accounts.read().len(), 1);

        let _s = serde_json::to_string(&cache).unwrap();
    }

    #[test]
    fn roundtrip_meta_block_env() {
        let meta = BlockchainDbMeta {
            chain: Some(Chain::mainnet()),
            block_env: BlockEnv { number: U256::from(1u64), ..Default::default() },
            hosts: BTreeSet::from(["eth-mainnet.alchemyapi.io".to_string()]),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let recovered: BlockchainDbMeta<BlockEnv> = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, recovered);
    }

    #[test]
    fn roundtrip_meta_tempo_block_env() {
        let meta = BlockchainDbMeta {
            chain: Some(Chain::mainnet()),
            block_env: TempoBlockEnv {
                inner: BlockEnv { number: U256::from(1u64), ..Default::default() },
                timestamp_millis_part: 42,
            },
            hosts: BTreeSet::from(["eth-mainnet.alchemyapi.io".to_string()]),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let recovered: BlockchainDbMeta<TempoBlockEnv> = serde_json::from_str(&json).unwrap();
        // timestamp_millis_part is not serialized, so it resets to 0 on deserialization
        assert_eq!(meta.block_env.inner, recovered.block_env.inner);
        assert_eq!(recovered.block_env.timestamp_millis_part, 0);
    }

    #[test]
    fn can_return_cache_path_if_set() {
        // set
        let cache_db = JsonBlockCacheDB::<BlockEnv>::new(
            Arc::new(RwLock::new(BlockchainDbMeta::default())),
            Some(PathBuf::from("/tmp/foo")),
        );
        assert_eq!(Some(Path::new("/tmp/foo")), cache_db.cache_path());

        // unset
        let cache_db = JsonBlockCacheDB::<BlockEnv>::new(
            Arc::new(RwLock::new(BlockchainDbMeta::default())),
            None,
        );
        assert_eq!(None, cache_db.cache_path());
    }
}
