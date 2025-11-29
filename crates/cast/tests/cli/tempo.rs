use std::env;

fn get_tempo_rpc_url() -> String {
    env::var("TEMPO_RPC_URL")
        .unwrap_or_else(|_| "https://eng:zealous-mayer@rpc.testnet.tempo.xyz".to_string())
}

casttest!(tempo_erc20_send_with_fee_token, |_prj, cmd| {
    let eth_rpc_url = get_tempo_rpc_url();

    cmd.args([
        "erc20",
        "transfer",
        "--fee-token",
        "0x20c0000000000000000000000000000000000002",
        "0x20c0000000000000000000000000000000000001",
        "0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F",
        "1234567",
        "--rpc-url",
        eth_rpc_url.as_str(),
        "--private-key",
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    ]);
    cmd.assert_success().stdout_eq(str![[r#"

feeToken             0x20C0000000000000000000000000000000000002
feePayer             0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
blockHash            [..]
blockNumber          [..]
contractAddress      
cumulativeGasUsed    [..]
effectiveGasPrice    [..]
from                 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
gasUsed              [..]
logs                 [..]
logsBloom            [..]
root                 
status               true
transactionHash      [..]
transactionIndex     0
type                 FeeToken
to                   0x20C0000000000000000000000000000000000001

"#]]);

    cmd.cast_fuse().args([
        "erc20",
        "approve",
        "--fee-token",
        "0x20c0000000000000000000000000000000000003",
        "0x20c0000000000000000000000000000000000001",
        "0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F",
        "1234567",
        "--rpc-url",
        eth_rpc_url.as_str(),
        "--private-key",
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    ]);
    cmd.assert_success().stdout_eq(str![[r#"

feeToken             0x20C0000000000000000000000000000000000003
feePayer             0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
blockHash            [..]
blockNumber          [..]
contractAddress      
cumulativeGasUsed    [..]
effectiveGasPrice    [..]
from                 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
gasUsed              [..]
logs                 [..]
logsBloom            [..]
root                 
status               true
transactionHash      [..]
transactionIndex     0
type                 FeeToken
to                   0x20C0000000000000000000000000000000000001

"#]]);

    cmd.cast_fuse().args([
        "send",
        "--fee-token",
        "0x20c0000000000000000000000000000000000003",
        "--rpc-url",
        eth_rpc_url.as_str(),
        "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D",
        "increment()",
        "--private-key",
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    ]);
    cmd.assert_success().stdout_eq(str![[r#"

feeToken             0x20C0000000000000000000000000000000000003
feePayer             0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
blockHash            [..]
blockNumber          [..]
contractAddress      
cumulativeGasUsed    [..]
effectiveGasPrice    [..]
from                 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
gasUsed              [..]
logs                 [..]
logsBloom            [..]
root                 
status               true
transactionHash      [..]
transactionIndex     0
type                 FeeToken
to                   0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D

"#]]);
});

casttest!(tempo_mktx_with_fee_token, |_prj, cmd| {
    let eth_rpc_url = get_tempo_rpc_url();

    cmd.args([
        "mktx",
        "--fee-token",
        "0x20c0000000000000000000000000000000000003",
        "--rpc-url",
        eth_rpc_url.as_str(),
        "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D",
        "increment()",
        "--private-key",
        "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    ]);
    cmd.assert_success().stdout_eq(str![[r#"
0x[..]

"#]]);
});
