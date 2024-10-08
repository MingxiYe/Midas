use crate::evm::abi::{A256InnerType, AArray, AEmpty, BoxedABI, A256};
use crate::evm::onchain::endpoints::Chain;
use crate::evm::types::{EVMAddress, EVMU256};
use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum UniswapProvider {
    PancakeSwap,
    SushiSwap,
    UniswapV2,
    UniswapV3,
    Biswap,
}

impl FromStr for UniswapProvider {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pancakeswap" => Ok(Self::PancakeSwap),
            "pancakeswapv2" => Ok(Self::PancakeSwap),
            "sushiswap" => Ok(Self::SushiSwap),
            "uniswapv2" => Ok(Self::UniswapV2),
            "uniswapv3" => Ok(Self::UniswapV3),
            "biswap" => Ok(Self::Biswap),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct UniswapInfo {
    pub pool_fee: usize,
    pub router: EVMAddress,
    pub factory: EVMAddress,
    pub init_code_hash: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct PairContext {
    pub pair_address: EVMAddress,
    pub next_hop: EVMAddress,
    pub side: u8,
    pub uniswap_info: Arc<UniswapInfo>,
    pub initial_reserves: (EVMU256, EVMU256),
}

#[derive(Clone, Debug, Default)]
pub struct PathContext {
    pub route: Vec<Rc<RefCell<PairContext>>>,
    pub final_pegged_ratio: EVMU256,
    pub final_pegged_pair: Rc<RefCell<Option<PairContext>>>,
}

#[derive(Clone, Debug, Default)]
pub struct TokenContext {
    pub swaps: Vec<PathContext>,
    pub is_weth: bool,
    pub weth_address: EVMAddress,
    pub address: EVMAddress,
}

static mut WETH_MAX: EVMU256 = EVMU256::ZERO;

// swapExactETHForTokensSupportingFeeOnTransferTokens
pub fn generate_uniswap_router_buy(
    token: &TokenContext,
    path_idx: usize,
    amount_in: EVMU256,
    to: EVMAddress,
) -> Option<(BoxedABI, EVMU256, EVMAddress)> {
    unsafe {
        WETH_MAX = EVMU256::from(10).pow(EVMU256::from(24));
    }
    // function swapExactETHForTokensSupportingFeeOnTransferTokens(
    //     uint amountOutMin,
    //     address[] calldata path,
    //     address to,
    //     uint deadline
    // )
    if token.is_weth {
        let mut abi = BoxedABI::new(Box::new(AEmpty {}));
        abi.function = [0xd0, 0xe3, 0x0d, 0xb0]; // deposit
                                                 // EVMU256::from(perct) * unsafe {WETH_MAX}
        Some((abi, amount_in, token.weth_address))
    } else {
        if token.swaps.is_empty() {
            return None;
        }
        let path_ctx = &token.swaps[path_idx % token.swaps.len()];
        // let amount_in = path_ctx.get_amount_in(perct, reserve);
        let mut path: Vec<EVMAddress> = path_ctx
            .route
            .iter()
            .rev()
            .map(|pair| pair.deref().borrow().next_hop)
            .collect();
        // when it is pegged token or weth
        if path.is_empty() || path[0] != token.weth_address {
            path.insert(0, token.weth_address);
        }
        path.insert(path.len(), token.address);
        let mut abi = BoxedABI::new(Box::new(AArray {
            data: vec![
                BoxedABI::new(Box::new(A256 {
                    data: vec![0; 32],
                    is_address: false,
                    dont_mutate: false,
                    inner_type: A256InnerType::Uint,
                })),
                BoxedABI::new(Box::new(AArray {
                    data: path
                        .iter()
                        .map(|addr| {
                            BoxedABI::new(Box::new(A256 {
                                data: addr.as_bytes().to_vec(),
                                is_address: true,
                                dont_mutate: false,
                                inner_type: A256InnerType::Address,
                            }))
                        })
                        .collect(),
                    dynamic_size: true,
                })),
                BoxedABI::new(Box::new(A256 {
                    data: to.0.to_vec(),
                    is_address: true,
                    dont_mutate: false,
                    inner_type: A256InnerType::Address,
                })),
                BoxedABI::new(Box::new(A256 {
                    data: vec![0xff; 32],
                    is_address: false,
                    dont_mutate: false,
                    inner_type: A256InnerType::Uint,
                })),
            ],
            dynamic_size: false,
        }));
        abi.function = [0xb6, 0xf9, 0xde, 0x95]; // swapExactETHForTokensSupportingFeeOnTransferTokens

        match path_ctx.final_pegged_pair.deref().borrow().as_ref() {
            None => Some((
                abi,
                amount_in,
                path_ctx
                    .route
                    .last()
                    .unwrap()
                    .deref()
                    .borrow()
                    .uniswap_info
                    .router,
            )),
            Some(info) => Some((abi, amount_in, info.uniswap_info.router)),
        }
    }
}

// swapExactTokensForETHSupportingFeeOnTransferTokens
pub fn generate_uniswap_router_sell(
    token: &TokenContext,
    path_idx: usize,
    amount_in: EVMU256,
    to: EVMAddress,
) -> Option<Vec<(BoxedABI, EVMU256, EVMAddress)>> {
    unsafe {
        WETH_MAX = EVMU256::from(10).pow(EVMU256::from(24));
    }
    // function swapExactTokensForETHSupportingFeeOnTransferTokens(
    //     uint amountIn,
    //     uint amountOutMin,
    //     address[] calldata path,
    //     address to,
    //     uint deadline
    // )
    let amount: [u8; 32] = amount_in.to_be_bytes();
    let mut abi_amount = BoxedABI::new(Box::new(A256 {
        data: amount.to_vec(),
        is_address: false,
        dont_mutate: false,
        inner_type: A256InnerType::Uint,
    }));

    if token.is_weth {
        abi_amount.function = [0x2e, 0x1a, 0x7d, 0x4d]; // withdraw
        Some(vec![(abi_amount, EVMU256::ZERO, token.weth_address)])
    } else {
        if token.swaps.is_empty() {
            return None;
        }
        let path_ctx = &token.swaps[path_idx % token.swaps.len()];
        // let amount_in = path_ctx.get_amount_in(perct, reserve);
        let mut path: Vec<EVMAddress> = path_ctx
            .route
            .iter()
            .map(|pair| pair.deref().borrow().next_hop)
            .collect();
        // when it is pegged token or weth
        if path.is_empty() || *path.last().unwrap() != token.weth_address {
            path.push(token.weth_address);
        }
        path.insert(0, token.address);
        let mut sell_abi = BoxedABI::new(Box::new(AArray {
            data: vec![
                abi_amount,
                BoxedABI::new(Box::new(A256 {
                    data: vec![0; 32],
                    is_address: false,
                    dont_mutate: false,
                    inner_type: A256InnerType::Uint,
                })),
                BoxedABI::new(Box::new(AArray {
                    data: path
                        .iter()
                        .map(|addr| {
                            BoxedABI::new(Box::new(A256 {
                                data: addr.as_bytes().to_vec(),
                                is_address: true,
                                dont_mutate: false,
                                inner_type: A256InnerType::Address,
                            }))
                        })
                        .collect(),
                    dynamic_size: true,
                })),
                BoxedABI::new(Box::new(A256 {
                    data: to.0.to_vec(),
                    is_address: true,
                    dont_mutate: false,
                    inner_type: A256InnerType::Address,
                })),
                BoxedABI::new(Box::new(A256 {
                    data: vec![0xff; 32],
                    is_address: false,
                    dont_mutate: false,
                    inner_type: A256InnerType::Uint,
                })),
            ],
            dynamic_size: false,
        }));
        sell_abi.function = [0x79, 0x1a, 0xc9, 0x47]; // swapExactTokensForETHSupportingFeeOnTransferTokens

        let router = match path_ctx.final_pegged_pair.deref().borrow().as_ref() {
            None => {
                path_ctx
                    .route
                    .last()
                    .unwrap()
                    .deref()
                    .borrow()
                    .uniswap_info
                    .router
            }
            Some(info) => info.uniswap_info.router,
        };

        let mut approve_abi = BoxedABI::new(Box::new(AArray {
            data: vec![
                BoxedABI::new(Box::new(A256 {
                    data: router.0.to_vec(),
                    is_address: true,
                    dont_mutate: false,
                    inner_type: A256InnerType::Address,
                })),
                BoxedABI::new(Box::new(A256 {
                    data: vec![0xff; 32],
                    is_address: false,
                    dont_mutate: false,
                    inner_type: A256InnerType::Uint,
                })),
            ],
            dynamic_size: false,
        }));

        approve_abi.function = [0x09, 0x5e, 0xa7, 0xb3]; // approve

        Some(vec![
            (approve_abi, EVMU256::ZERO, token.address),
            (sell_abi, EVMU256::ZERO, router),
        ])
    }
}

pub fn get_uniswap_info(provider: &UniswapProvider, chain: &Chain) -> UniswapInfo {
    match (provider, chain) {
        (&UniswapProvider::UniswapV2, &Chain::BSC) => UniswapInfo {
            pool_fee: 25,
            router: EVMAddress::from_str("0x10ed43c718714eb63d5aa57b78b54704e256024e").unwrap(),
            factory: EVMAddress::from_str("0xca143ce32fe78f1f7019d7d551a6402fc5350c73").unwrap(),
            init_code_hash: hex::decode(
                "00fb7f630766e6a796048ea87d01acd3068e8ff67d078148a3fa3f4a84f69bd5",
            )
            .unwrap(),
        },
        (&UniswapProvider::PancakeSwap, &Chain::BSC) => UniswapInfo {
            pool_fee: 25,
            router: EVMAddress::from_str("0x10ed43c718714eb63d5aa57b78b54704e256024e").unwrap(),
            factory: EVMAddress::from_str("0xca143ce32fe78f1f7019d7d551a6402fc5350c73").unwrap(),
            init_code_hash: hex::decode(
                "00fb7f630766e6a796048ea87d01acd3068e8ff67d078148a3fa3f4a84f69bd5",
            )
            .unwrap(),
        },
        (&UniswapProvider::UniswapV2, &Chain::ETH) => UniswapInfo {
            pool_fee: 3,
            router: EVMAddress::from_str("0x7a250d5630b4cf539739df2c5dacb4c659f2488d").unwrap(),
            factory: EVMAddress::from_str("0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f").unwrap(),
            init_code_hash: hex::decode(
                "96e8ac4277198ff8b6f785478aa9a39f403cb768dd02cbee326c3e7da348845f",
            )
            .unwrap(),
        },
        _ => panic!(
            "Uniswap provider {:?} @ chain {:?} not supported",
            provider, chain
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evm::onchain::endpoints::Chain;
    use std::str::FromStr;
    use tracing::debug;

    macro_rules! wrap {
        ($x: expr) => {
            Rc::new(RefCell::new($x))
        };
    }

    #[test]
    fn test_uniswap_sell() {
        let t1 = TokenContext {
            swaps: vec![PathContext {
                route: vec![wrap!(PairContext {
                    pair_address: EVMAddress::from_str(
                        "0x0000000000000000000000000000000000000000"
                    )
                    .unwrap(),
                    side: 0,
                    uniswap_info: Arc::new(get_uniswap_info(
                        &UniswapProvider::PancakeSwap,
                        &Chain::BSC
                    )),
                    initial_reserves: (Default::default(), Default::default()),
                    next_hop: EVMAddress::from_str("0x1100000000000000000000000000000000000000")
                        .unwrap(),
                })],
                final_pegged_ratio: EVMU256::from(1),
                final_pegged_pair: Rc::new(RefCell::new(None)),
            }],
            is_weth: false,
            weth_address: EVMAddress::from_str("0xee00000000000000000000000000000000000000")
                .unwrap(),
            address: EVMAddress::from_str("0xff00000000000000000000000000000000000000").unwrap(),
        };

        let plan = generate_uniswap_router_sell(
            &t1,
            0,
            EVMU256::from(10000),
            EVMAddress::from_str("0x2300000000000000000000000000000000000000").unwrap(),
        );
        debug!(
            "plan: {:?}",
            plan.unwrap()
                .iter()
                .map(|x| hex::encode(x.0.get_bytes()))
                .collect::<Vec<_>>()
        );
    }
}
