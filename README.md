# Time-Weighted Average Market Maker (TWAMM)

A sophisticated implementation of a Time-Weighted Average Market Maker built on Arbitrum using Stylus (Rust) for complex mathematical computations and Solidity for the main contract logic. This project integrates with Uniswap V4 to provide efficient long-term order execution with minimal price impact.

## 🚀 Overview

TWAMM (Time-Weighted Average Market Maker) is an advanced AMM mechanism that allows users to place long-term orders that execute gradually over time, reducing price impact and providing better execution for large trades. This implementation leverages Stylus for gas-efficient mathematical computations while maintaining compatibility with Uniswap V4's hook system.

## ✨ Key Features

- **Time-Weighted Execution**: Orders execute gradually over specified time periods
- **Minimal Price Impact**: Reduces market impact through continuous execution
- **Stylus Integration**: Complex mathematical calculations performed in Rust for gas efficiency
- **Uniswap V4 Compatibility**: Built as a hook for seamless integration
- **Bidirectional Trading**: Supports both token0→token1 and token1→token0 trades
- **Virtual Order Processing**: Efficient batch processing of pending orders
- **MEV Protection**: Time-distributed execution reduces MEV opportunities
- **Emergency Controls**: Safety mechanisms for order cancellation and withdrawal

## 🏗️ Architecture

### Core Components

#### 1. Solidity Contracts (`/contracts/`)

- **TWAMMHook.sol**: Main hook contract implementing Uniswap V4 integration
- **TWAMMStorage.sol**: Storage contract managing orders and pool states
- **VirtualOrderExecutor.sol**: Library for executing virtual orders safely

#### 2. Stylus Contract (`/stylus/`)

- **lib.rs**: Main Stylus contract with TWAMM calculation logic
- **twamm_math.rs**: Advanced mathematical utilities and TWAMM formulas
- **order_execution.rs**: Order management and execution logic

### Mathematical Model

The implementation uses Paradigm's TWAMM research with closed-form solutions for:

- **Unidirectional Trading**: Single-direction order execution
- **Bidirectional Trading**: Net flow calculation for opposing orders
- **Time Weighting**: Gradual execution over time periods
- **Price Impact Calculation**: Real-time impact assessment
- **Execution Quality Scoring**: Performance metrics for order execution

## 🔧 Technical Specifications

### Order Parameters

- **Minimum Order Amount**: 0.001 ETH
- **Maximum Order Blocks**: 100,000 blocks
- **Minimum Order Blocks**: 10 blocks
- **Execution Reward**: 0.001 ETH for order executors

### Gas Optimization

- **Stylus Integration**: Complex math operations in Rust
- **Batch Processing**: Efficient virtual order execution
- **Gas Estimation**: Built-in gas cost calculation
- **Execution Intervals**: Configurable execution frequency

## 📋 Usage

### Submitting an Order

```solidity
// Submit a long-term order
uint256 orderId = twammHook.submitOrder{value: executionReward}(
    poolKey,      // Pool to trade in
    amount,       // Total amount to sell
    zeroForOne,   // Direction (true = token0→token1)
    blocks        // Duration in blocks
);
```

### Canceling an Order

```solidity
// Cancel an existing order
twammHook.cancelOrder(orderId);
```

### Executing Virtual Orders

```solidity
// Execute pending virtual orders (anyone can call)
twammHook.executePendingOrders(poolKey);
```

## 🛠️ Development Setup

### Prerequisites

- Rust (latest stable)
- Foundry
- Node.js (for testing)
- Solana CLI tools

### Installation

1. **Clone the repository**

```bash
git clone <repository-url>
cd Time-Weighted-Average-Market-Maker
```

2. **Install dependencies**

```bash
# Install Foundry dependencies
forge install

# Install Stylus dependencies
cd stylus
cargo build
```

3. **Build contracts**

```bash
# Build Solidity contracts
forge build

# Build Stylus contracts
cd stylus
cargo stylus build
```

### Testing

```bash
# Run Solidity tests
forge test

# Run Stylus tests
cd stylus
cargo test
```

## 📊 Mathematical Implementation

### Core TWAMM Formula

The implementation uses the closed-form solution for TWAMM:

```
new_reserve_out = k / (reserve_in + amount_in * time_factor)
```

Where:

- `k` = constant product (reserve_in × reserve_out)
- `time_factor` = time weighting factor based on order size and duration
- `amount_in` = total amount to be sold over time period

### Time Weighting

The time weighting factor implements:

```
factor = 0.8 + 0.2 * (size_ratio / (1 + size_ratio))
```

This provides better execution for larger orders while maintaining efficiency.

### Bidirectional Trading

For opposing orders, the system calculates net flows:

```
net_flow = max(sell_0_value_in_1 - sell_1, sell_1 - sell_0_value_in_1)
```

## 🔒 Security Features

- **Reentrancy Protection**: All external functions protected
- **Input Validation**: Comprehensive parameter validation
- **Overflow Protection**: Safe math operations throughout
- **Access Control**: Owner-only functions for critical operations
- **Emergency Controls**: Order cancellation and withdrawal mechanisms
- **Price Impact Limits**: Maximum execution size constraints

## 📈 Performance Metrics

### Execution Quality Scoring

- **Amount Ratio**: 0-50 points based on expected vs actual execution
- **Price Impact**: 0-50 points based on market impact
- **Total Score**: 0-100 quality rating

### Gas Efficiency

- **Base Gas**: ~100,000 gas for virtual execution
- **Per Block**: ~1,000 additional gas per block executed
- **Per Order**: ~5,000 additional gas per active order

## 🚨 Risk Considerations

- **Smart Contract Risk**: Code is in development, not audited
- **Market Risk**: Price movements during order execution
- **Liquidity Risk**: Pool liquidity changes affecting execution
- **MEV Risk**: Potential front-running of large orders
- **Gas Risk**: High gas costs during network congestion

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Submit a pull request

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.

## 🙏 Acknowledgments

- [Paradigm Research](https://www.paradigm.xyz/2021/07/twamm) for TWAMM research
- [Uniswap V4](https://github.com/Uniswap/v4-core) for the hook system
- [Stylus](https://stylus-lang.org/) for Rust-based smart contracts

## 📞 Support

For questions and support:

- Create an issue in the repository
- Join our community discussions
- Review the documentation

---

**⚠️ Disclaimer**: This software is experimental and not audited. Use at your own risk. The authors are not responsible for any financial losses.
