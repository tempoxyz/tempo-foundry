pub use tempo_contracts::precompiles::is_iso4217_currency;

/// Returns a warning message for non-ISO 4217 currency codes used in TIP-20 token creation.
pub fn iso4217_warning_message(currency: &str) -> String {
    let hyperlink = |url: &str| format!("\x1b]8;;{url}\x1b\\{url}\x1b]8;;\x1b\\");
    let tip20_docs = hyperlink("https://docs.tempo.xyz/protocol/tip20/overview");
    let iso_docs = hyperlink("https://www.iso.org/iso-4217-currency-codes.html");

    format!(
        "\"{currency}\" is not a recognized ISO 4217 currency code.\n\
         \n\
         If the token you are trying to deploy is a fiat-backed stablecoin, Tempo strongly\n\
         recommends that the currency code field be the ISO-4217 currency code of the fiat\n\
         currency your token tracks (e.g. \"USD\", \"EUR\", \"GBP\").\n\
         \n\
         The currency field is IMMUTABLE after token creation and affects fee payment\n\
         eligibility, DEX routing, and quote token pairing. Only \"USD\"-denominated tokens\n\
         can be used to pay transaction fees on Tempo.\n\
         \n\
         Learn more:\n  \
         - Tempo TIP-20 docs: {tip20_docs}\n  \
         - ISO 4217 standard: {iso_docs}"
    )
}
