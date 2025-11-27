use std::env;

fn get_tempo_rpc_url() -> String {
    env::var("TEMPO_RPC_URL")
        .unwrap_or_else(|_| "https://eng:zealous-mayer@rpc.testnet.tempo.xyz".to_string())
}

casttest!(tempo_erc20_transfer_with_fee_token, |_prj, cmd| {
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
});

casttest!(tempo_erc20_approve_with_fee_token, |_prj, cmd| {
    let eth_rpc_url = get_tempo_rpc_url();

    cmd.args([
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
});

casttest!(tempo_erc20_send_with_fee_token, |_prj, cmd| {
    let eth_rpc_url = get_tempo_rpc_url();

    cmd.args([
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

casttest!(tempo_erc20_mktx_with_fee_token, |_prj, cmd| {
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
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    ]);
    cmd.assert_success().stdout_eq(str![[r#"
0x77f88682a5bd820b37018504a817c80182534e9486a2ee8faf9a840f7a2c64ca3d51209f9a02081d8084d09de08ac0c09420c00000000000000000000000000000000000038080a075f84c48323e8f51fac0828e5400e3de93d2c136d13c14661ad7e2d9c7cd7866a06bfe57817d176b58865ffeb2e85be68937e6be47e76595858a4b5cf5c283d7a0

"#]]);
});
