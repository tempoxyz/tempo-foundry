pub use tempo_contracts::precompiles::is_iso4217_currency;

pub fn iso4217_warning_message(currency: &str) -> String {
    format!(
        r#"Warning: "{currency}" is not a recognized ISO 4217 currency code.

If the token you are trying to deploy is a fiat-backed stablecoin, Tempo strongly
recommends that the currency code field be the ISO-4217 currency code of the fiat
currency your token tracks (e.g. "USD", "EUR", "GBP").

The currency field is IMMUTABLE after token creation and affects fee payment
eligibility, DEX routing, and quote token pairing. Only "USD"-denominated tokens
can be used to pay transaction fees on Tempo.

Learn more:
  • Tempo TIP-20 docs: https://docs.tempo.xyz/protocol/tip20/overview
  • ISO 4217 standard: https://www.iso.org/iso-4217-currency-codes.html"#
    )
}
