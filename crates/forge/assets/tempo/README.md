## Tempo Foundry

**Foundry is a blazing fast, portable and modular toolkit for Ethereum application development written in Rust.**

Since Tempo introduces several protocol-level features that extend beyond Ethereum's vanilla EVM implementation — including custom precompiles, new transaction types, fee payments in stablecoins, and native account abstraction — a custom `forge` and `cast` built from [Tempo's fork](https://github.com/tempoxyz/tempo-foundry) of [Foundry](https://github.com/foundry-rs/foundry) is required.

Tempo's fork of Foundry consists of:

- **Forge**: Ethereum testing framework (like Truffle, Hardhat and DappTools).
- **Cast**: Swiss army knife for interacting with EVM smart contracts, sending transactions and getting chain data.

## Documentation

https://book.getfoundry.sh/

## Usage

### Build

```shell
$ forge build
```

### Test

```shell
$ forge test
```

### Format

```shell
$ forge fmt
```

### Gas Snapshots

```shell
$ forge snapshot
```

### Deploy

```shell
$ forge script script/Mail.s.sol:MailScript --rpc-url <YOUR_RPC_URL> --private-key <YOUR_PRIVATE_KEY>
```

### Cast

```shell
$ cast <subcommand>
```

### Help

```shell
$ forge --help
$ cast --help
```
