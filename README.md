<br>
<br>

<p align="center">
  <a href="https://tempo.xyz">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/tempoxyz/.github/refs/heads/main/assets/combomark-dark.svg">
      <img alt="tempo combomark" src="https://raw.githubusercontent.com/tempoxyz/.github/refs/heads/main/assets/combomark-bright.svg" width="auto" height="120">
    </picture>
  </a>
</p>

<br>
<br>

# Tempo Foundry

[Tempo](https://docs.tempo.xyz/) is a blockchain designed specifically for stablecoin payments. Its architecture focuses on high throughput, low cost, and features that financial institutions, payment service providers, and fintech platforms expect from modern payment infrastructure.

`Tempo Foundry` is a custom fork of [Foundry](https://github.com/foundry-rs/foundry) that integrates Tempo's payment-native protocol features directly into the familiar Foundry developer workflow.

This is a temporary required drop-in replacement for upstream Foundry while Tempo-specific features are being integrated into upstream Foundry, after which this fork will be deprecated.

Get started [here](https://docs.tempo.xyz/sdk/foundry) to use Tempo's features in Foundry.

## Installation

Tempo's Foundry fork is installed through the standard upstream `foundryup` using the `-n tempo` flag, no separate installer is required.

Getting started is very easy:

Install regular `foundryup`:

```bash
curl -L https://foundry.paradigm.xyz | bash
```

Or if you already have `foundryup` installed:

```bash
foundryup --update
```

Next, run:

```bash
foundryup -n tempo
```

It will automatically install the latest `nightly` release of the precompiled binaries: [`forge`](#forge) and [`cast`](#cast).

**Done!**

For additional details see the [installation guide](https://getfoundry.sh/getting-started/installation) in the [Foundry Docs][foundry-docs].

If you're experiencing any issues while installing, check out [Getting Help](#getting-help) and the [FAQ](https://getfoundry.sh/faq).

## Testnet

You can connect to Tempo's public testnet using the following details:

| Property           | Value                                |
| ------------------ | ------------------------------------ |
| **Network Name**   | Tempo Testnet (Moderato)             |
| **Currency**       | `USD`                                |
| **Chain ID**       | `42431`                              |
| **HTTP URL**       | `https://rpc.moderato.tempo.xyz`     |
| **WebSocket URL**  | `wss://rpc.moderato.tempo.xyz`       |
| **Block Explorer** | `https://explore.moderato.tempo.xyz` |

Next, grab some stablecoins to test with from Tempo's [Faucet](https://docs.tempo.xyz/quickstart/faucet#faucet).

Alternatively, use [`cast`](https://github.com/tempoxyz/tempo-foundry):

```bash
cast rpc tempo_fundAddress <ADDRESS> --rpc-url https://rpc.moderato.tempo.xyz
```

## Changeset

Key extensions:

- In `foundryup`:

  - `foundryup -n tempo`: download the latest `nightly` release of Tempo's fork of Foundry.
  - `foundryup -n tempo -i <TAG>`: download a specific `nightly` release by tag `nightly-<hash>`.

- In `forge`:

  - `forge init -n tempo`: adds a Tempo-specific `Mail` template showcasing a `TIP20` transfer with an attached memo.
  - `forge install tempoxyz/tempo-std`: like `forge-std`, a collection of helpful contracts and libraries for Tempo-specific testing and utilities.
  - `--fee-token` support: pay gas fees in any `TIP20` stablecoin.

- In `cast`:

  - `cast run`: updated to correctly process Tempo's system transactions when replaying a block.
  - `cast tip20`: alias to `cast erc20`.
  - `--fee-token` support: pay gas fees in any `TIP20` stablecoin.

- Additionally:

  - Support for local and forked simulation of the Tempo execution environment.
  - Support for Tempo's (stateful) precompiles and default contracts including labels in traces.
  - A custom `TempoEvm` extends `Revm`'s `Evm` to accommodate differences Tempo introduces to optimize for payments.

<br>
<br>

<div align="center">
  <img src=".github/assets/banner.png" alt="Foundry banner" />

&nbsp;

[![Github Actions][gha-badge]][gha-url] [![Telegram Chat][tg-badge]][tg-url] [![Telegram Support][tg-support-badge]][tg-support-url]

[gha-badge]: https://img.shields.io/github/actions/workflow/status/foundry-rs/foundry/test.yml?branch=master&style=flat-square
[gha-url]: https://github.com/foundry-rs/foundry/actions
[tg-badge]: https://img.shields.io/endpoint?color=neon&logo=telegram&label=chat&style=flat-square&url=https%3A%2F%2Ftg.sumanjay.workers.dev%2Ffoundry_rs
[tg-url]: https://t.me/foundry_rs
[tg-support-badge]: https://img.shields.io/endpoint?color=neon&logo=telegram&label=support&style=flat-square&url=https%3A%2F%2Ftg.sumanjay.workers.dev%2Ffoundry_support
[tg-support-url]: https://t.me/foundry_support

**[Install](https://getfoundry.sh/getting-started/installation)**
| [Docs][foundry-docs]
| [Benchmarks](https://www.getfoundry.sh/benchmarks)
| [Developer Guidelines](./docs/dev/README.md)
| [Contributing](./CONTRIBUTING.md)
| [Crate Docs](https://foundry-rs.github.io/foundry)

</div>

---

Blazing fast, portable and modular toolkit for Ethereum application development, written in Rust.

- [**Forge**](https://getfoundry.sh/forge) — Build, test, fuzz, debug and deploy Solidity contracts.
- [**Cast**](https://getfoundry.sh/cast) — Swiss Army knife for interacting with EVM smart contracts, sending transactions and getting chain data.
- [**Anvil**](https://getfoundry.sh/anvil) — Fast local Ethereum development node.
- [**Chisel**](https://getfoundry.sh/chisel) — Fast, utilitarian and verbose Solidity REPL.

![Demo](.github/assets/demo.gif)

## Installation

```sh
curl -L https://foundry.paradigm.xyz | bash
foundryup
```

See the [installation guide](https://getfoundry.sh/getting-started/installation) for more details.

## Getting Started

Initialize a new project, build and test:

```sh
forge init counter && cd counter
forge build
forge test
```

Interact with a live network:

```sh
cast block-number --rpc-url https://eth.merkle.io
cast balance vitalik.eth --ether --rpc-url https://eth.merkle.io
```

Fork mainnet locally:

```sh
anvil --fork-url https://eth.merkle.io
```

Read the [Foundry Docs][foundry-docs] to learn more.

## Contributing

Contributions are welcome and highly appreciated. To get started, check out the [contributing guidelines](./CONTRIBUTING.md).

Join our [Telegram][tg-url] to chat about the development of Foundry.

## Support

Having trouble? Check the [Foundry Docs][foundry-docs], join the [support Telegram][tg-support-url], or [open an issue](https://github.com/foundry-rs/foundry/issues/new).

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in these crates by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
</sub>

[foundry-docs]: https://getfoundry.sh
