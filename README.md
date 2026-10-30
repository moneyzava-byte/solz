# SOLZ

Mined with SOL, never minted. This is the source of the SOLZ program on Solana mainnet.

* Program: `33jmYNoZWJJB74EaQkcVNXBVka6sst5gzWhqZBAUPtTp`
* $SOLZ mint: `9PCAbwogmpxzTr3Sv3oQWeG5e62BMiEmMM9aH6hiR17i`
* Upgrade authority: none. The program is locked and can never change.
* Site: https://solz.money · X: https://x.com/SolzMoney

## Verify it yourself

Built with `solana-cargo-build-sbf 3.1.10` (platform tools v1.52):

```
cargo build-sbf --manifest-path programs/solz/Cargo.toml -- --locked
```

The deployed program hash (trailing zeros stripped, the way `solana-verify get-program-hash` reports it):

```
a21530a4b2e801b1f307999547d7c10b106389fb6654fa7c4c57ef4deb557f40
```

Compare with the chain:

```
solana-verify get-program-hash -u mainnet-beta 33jmYNoZWJJB74EaQkcVNXBVka6sst5gzWhqZBAUPtTp
solana-verify get-executable-hash target/deploy/solz.so
```

`programs/raymock` is a local test stand in for Raydium and is not deployed.
