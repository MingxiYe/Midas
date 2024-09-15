# Midas

# Build
Midas is build on top of LibAFL. Environment of Rust is necessary.

```
cd metafuzz
cargo build --release
```

# Run
You can try the following script.

```
## try it out with the following script
./Midas -o -c ETH --onchain-etherscan-api-key 25Q37J4HSDZRN67QI9XEMN634GKW7W1184 -f --flashloan-price-oracle onchain -t 0xb40b6608B2743E691C9B54DdBDEe7bf03cd79f1c --onchain-block-number 17504368 --target-txn-hash 0x2667e09b617e3bac4fa05f7f4d90dc7e4ede550b058549add90704231b8d6568 --spec-id Latest
```