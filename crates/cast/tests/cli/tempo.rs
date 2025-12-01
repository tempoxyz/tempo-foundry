use std::env;

const TESTNET_URL: &str = "https://eng:zealous-mayer@rpc.testnet.tempo.xyz";

casttest!(tempo_erc20_send_with_fee_token, |_prj, cmd| {
    cmd.args([
        "erc20",
        "transfer",
        "--fee-token",
        "0x20c0000000000000000000000000000000000002",
        "0x20c0000000000000000000000000000000000001",
        "0x4ef5DFf69C1514f4Dbf85aA4F9D95F804F64275F",
        "1234567",
        "--rpc-url",
        TESTNET_URL,
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
        TESTNET_URL,
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
        TESTNET_URL,
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
    cmd.args([
        "mktx",
        "--fee-token",
        "0x20c0000000000000000000000000000000000003",
        "--rpc-url",
        TESTNET_URL,
        "0x86A2EE8FAf9A840F7a2c64CA3d51209F9A02081D",
        "increment()",
        "--private-key",
        "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    ]);
    cmd.assert_success().stdout_eq(str![[r#"
0x[..]

"#]]);
});

casttest!(tempo_cast_run_aa, |_prj, cmd| {
    cmd.args([
        "run",
        "0x6fb40b6ce389c4493512164fdf01d30a43554d6f70b4fad9dc8e7578b6a8eda2",
        "--rpc-url",
        TESTNET_URL,
    ]);
    cmd.assert_success().stdout_eq(str![[r#"
Executing previous transactions from the block.
Traces:
  [28449] Test::grantRole(0x114e74f6ea3bd819998f78687bfcb11b140da08e9b7d222fa9c1f1ba1f2aa122, 0x389077a7171cFb5613c009520B6Cf7cc74d77e06)
    └─ ← [Return]


Transaction successfully executed.
[GAS]

"#]]);
});

casttest!(tempo_cast_aa_receipt, |_prj, cmd| {
    cmd.args([
        "receipt",
        "0x6fb40b6ce389c4493512164fdf01d30a43554d6f70b4fad9dc8e7578b6a8eda2",
        "--rpc-url",
        TESTNET_URL,
    ]);
    cmd.assert_success().stdout_eq(str![[r#"

feeToken             0x20C0000000000000000000000000000000000001
feePayer             0x389077a7171cFb5613c009520B6Cf7cc74d77e06
blockHash            0x83d790916e4913d04a45b8f03e1d124cce164fd65a69e079e50a8fa30d7f8d44
blockNumber          2813073
contractAddress      
cumulativeGasUsed    50784
effectiveGasPrice    10000000001
from                 0x389077a7171cFb5613c009520B6Cf7cc74d77e06
gasUsed              50784
logs                 [{"address":"0x4e4f4e4345000000000000000000000000000000","topics":["0x213a3328d23da89e0b3bd14e43c4fdffe45f7cf41ccf635cb3883609da35bad5","0x000000000000000000000000389077a7171cfb5613c009520b6cf7cc74d77e06"],"data":"0x0000000000000000000000000000000000000000000000000000000000000002","blockHash":"0x83d790916e4913d04a45b8f03e1d124cce164fd65a69e079e50a8fa30d7f8d44","blockNumber":"0x2aec91","blockTimestamp":"0x692a0e0e","transactionHash":"0x6fb40b6ce389c4493512164fdf01d30a43554d6f70b4fad9dc8e7578b6a8eda2","transactionIndex":"0x0","logIndex":"0x0","removed":false},{"address":"0x20c0000000000000000000000000000000000001","topics":["0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef","0x000000000000000000000000389077a7171cfb5613c009520b6cf7cc74d77e06","0x000000000000000000000000feec000000000000000000000000000000000000"],"data":"0x00000000000000000000000000000000000000000000000000000000000001fc","blockHash":"0x83d790916e4913d04a45b8f03e1d124cce164fd65a69e079e50a8fa30d7f8d44","blockNumber":"0x2aec91","blockTimestamp":"0x692a0e0e","transactionHash":"0x6fb40b6ce389c4493512164fdf01d30a43554d6f70b4fad9dc8e7578b6a8eda2","transactionIndex":"0x0","logIndex":"0x1","removed":false}]
logsBloom            0x00000000000000000000000000000000000000000000000001000000000200000000000000000000000000000000000000000020000000000000000000000000000004000000000000000008000004000008000000000000000000000000000000000000000000000000000000000000000000000000000001000010000000000000000040000000000000000020000000000000100000000040001000000000000020000010000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000
root                 
status               false
transactionHash      0x6fb40b6ce389c4493512164fdf01d30a43554d6f70b4fad9dc8e7578b6a8eda2
transactionIndex     0
type                 AA
to                   0x20C000000000000000000000000000000000042a

"#]]);
});
