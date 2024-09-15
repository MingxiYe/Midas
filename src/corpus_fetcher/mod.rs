use crate::evm::{
    input::ConciseEVMInput,
    onchain::endpoints::{Chain, OnChainConfig},
};
use crate::generic_vm::vm_executor::ExecutionResult;
use clap::Parser;
use std::{
    fs::File,
    io::{Read, Write},
    str::FromStr,
};

/// CLI for Midas for fetching historical txn
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct CorpusFetcherArgs {
    /// Historical transaction file. If specified, will fetch txn hash
    /// from the file, line by line
    #[arg(long, default_value = "")]
    txn_hash_file: String,

    /// Concise input file. If specified, will store concise input to
    /// the file, line by line
    #[arg(long, default_value = "initial_corpus/txn_replayable")]
    concise_txn_file: String,

    /// Onchain - Chain type (ETH, BSC, POLYGON, MUMBAI)
    #[arg(short, long)]
    chain_type: Option<String>,

    /// Onchain - Block number (Default: 0 / latest)
    #[arg(long, short = 'b')]
    onchain_block_number: Option<u64>,

    /// Onchain Customize - RPC endpoint URL (Default: inferred from
    /// chain-type), Example: https://rpc.ankr.com/eth
    #[arg(long, short = 'u')]
    onchain_url: Option<String>,

    /// Onchain Customize - Chain ID (Default: inferred from chain-type)
    #[arg(long, short = 'i')]
    onchain_chain_id: Option<u32>,

    /// Onchain Customize - Block explorer URL (Default: inferred from
    /// chain-type), Example: https://api.etherscan.io/api
    #[arg(long, short = 'e')]
    onchain_explorer_url: Option<String>,

    /// Onchain Customize - Chain name (used as Moralis handle of chain)
    /// (Default: inferred from chain-type)
    #[arg(long, short = 'n')]
    onchain_chain_name: Option<String>,

    /// Onchain Etherscan API Key (Default: None)
    #[arg(long, short = 'k')]
    onchain_etherscan_api_key: Option<String>,
}

pub fn corpus_fetch_main(args: CorpusFetcherArgs) {
    if args.txn_hash_file == "" || args.concise_txn_file == "" {
        panic!("incorrect file path");
    }

    let mut contents = String::new();
    File::open(args.txn_hash_file)
        .expect("fail to open txn hash file")
        .read_to_string(&mut contents)
        .expect("fail to read txn hash file");

    if !args.chain_type.is_some() && !args.onchain_url.is_some() {
        panic!("onchain config not specified");
    }
    let mut onchain = match args.chain_type {
        Some(chain_str) => {
            let chain = Chain::from_str(&chain_str).expect("Invalid chain type");
            let block_number = args.onchain_block_number.unwrap_or(0);
            OnChainConfig::new(chain, block_number)
        }
        None => OnChainConfig::new_raw(
            args.onchain_url
                .expect("You need to either specify chain type or chain rpc"),
            args.onchain_chain_id
                .expect("You need to either specify chain type or chain id"),
            args.onchain_block_number.unwrap_or(0),
            args.onchain_explorer_url
                .expect("You need to either specify chain type or block explorer url"),
            args.onchain_chain_name
                .expect("You need to either specify chain type or chain name"),
        ),
    };
    let etherscan_api_key = match args.onchain_etherscan_api_key {
        Some(v) => v,
        None => std::env::var("ETHERSCAN_API_KEY").unwrap_or_default(),
    };
    if !etherscan_api_key.is_empty() {
        onchain.etherscan_api_key = etherscan_api_key
            .split(',')
            .map(|s| s.to_string())
            .collect();
    }

    let mut txn_list = String::new();
    for hash in contents.split('\n') {
        let txn = onchain.fetch_transaction_by_hash(hash.to_string());
        if txn.is_none() {
            continue;
        }
        let ci = ConciseEVMInput::from_input::<_, Vec<u8>>(
            txn.as_ref().expect(&format!("txn ({hash}) not found")),
            &ExecutionResult::empty_result(),
        );
        let ci_serialize = serde_json::to_vec(&ci).expect("Failed to deserialize concise input");
        let txn_json_text = String::from_utf8(ci_serialize).expect("utf-8 failed");
        if txn_list == "" {
            txn_list = txn_json_text;
        } else {
            txn_list = format!("{}\n{}", txn_list, txn_json_text);
        }
    }
    let mut corpus_file = File::create(args.concise_txn_file).unwrap();
    corpus_file.write_all(txn_list.as_bytes()).unwrap();
}
