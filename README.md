# Time-Weighted Average Market Maker (TWAMM)

A sophisticated implementation of a Time-Weighted Average Market Maker built on Arbitrum using Stylus (Rust) for complex mathematical computations and Solidity for the main contract logic. This project integrates with Uniswap V4 to provide efficient long-term order execution with minimal price impact.

## 🚀 Overview

TWAMM (Time-Weighted Average Market Maker) is an advanced AMM mechanism that allows users to place long-term orders that execute gradually over time, reducing price impact and providing better execution for large trades. This implementation leverages Stylus for gas-efficient mathematical computations while maintaining compatibility with Uniswap V4's hook system.

## 📍 Deployed Contract

**Contract Address**: `0x3409488afe731fb8e270251f267e1a822f774ec9`  
**Network**: Arbitrum Sepolia  
**Deployment TX**: `0x5dfb3ded0423130d63522661d96ca145771c896e8edec62fe53e760cc6660151`  
**Activation TX**: `0x79a2c0b042ce319a6837fc15921a38edf82c7a661565dab553510bdf7a473438`  
**Contract Size**: 16.4 KiB (16,776 bytes)  
**WASM Size**: 62.0 KiB (63,467 bytes)

> **Note**: This is the comprehensive TWAMM contract with full mathematical utilities and order execution system. Deployed on Arbitrum Sepolia testnet. For mainnet deployment, additional security audits and testing are recommended.

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

#### 2. Stylus Contract (`/tmm/`)

- **lib.rs**: Main Stylus contract with comprehensive TWAMM functionality
- **twamm_math.rs**: Advanced mathematical utilities and TWAMM formulas (695 lines)
- **order_execution.rs**: Complete order management and execution system (687 lines)
- **main.rs**: Contract entry point and ABI export functionality

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

## 📋 Contract Functions

### Core TWAMM Functions

```solidity
// Calculate virtual trades for TWAMM
function calculateVirtualTrades(
    uint256 sellRate0,
    uint256 sellRate1,
    uint256 blocksElapsed,
    uint256 reserve0,
    uint256 reserve1
) external returns (uint256 amount0Out, uint256 amount1Out);

// Execute virtual orders with advanced math
function executeVirtualOrders(
    uint256 poolReserve0,
    uint256 poolReserve1,
    uint256 sellRate0,
    uint256 sellRate1,
    uint256 blocksElapsed
) external returns (uint256 amount0Out, uint256 amount1Out);

// Calculate price impact
function calculatePriceImpact(
    uint256 tradeSize,
    uint256 reserveIn,
    uint256 reserveOut
) external pure returns (uint256 impact);

// Advanced price impact with time weighting
function calculateAdvancedPriceImpact(
    uint256 tradeSize,
    uint256 reserveIn,
    uint256 reserveOut,
    uint256 timeWeightingFactor
) external pure returns (uint256 weightedImpact);
```

### Order Management Functions

```solidity
// Create a new long-term order
function createLongTermOrder(
    address owner,
    uint8 direction,    // 0 = SellToken0, 1 = SellToken1
    uint256 sellRate,
    uint256 blocks,
    uint256 currentBlock
) external returns (uint256 orderId);

// Get order statistics
function getTotalOrdersCreated() external view returns (uint256);
function getTotalOrdersExecuted() external view returns (uint256);
function getNextOrderId() external view returns (uint256);
```

### Quality and Statistics Functions

```solidity
// Calculate execution quality score (0-100)
function calculateExecutionQuality(
    uint256 expectedAmount,
    uint256 actualAmount,
    uint256 priceImpact
) external pure returns (uint256 score);

// Get comprehensive statistics
function getTotalCalculations() external view returns (uint256);
function getTotalVolumeProcessed() external view returns (uint256);
function getTotalFeesCollected() external view returns (uint256);

// Reset all statistics
function resetStatistics() external;
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
git clone https://github.com/OFUZORCHUKWUEMEKE/Time-Weighted-Average-Market-Maker
cd Time-Weighted-Average-Market-Maker/twamm
```

2. **Install dependencies**

```bash
# Install Foundry dependencies
forge install

# Install Stylus dependencies
cd tmm
cargo build
```

3. **Build contracts**

```bash
# Build Solidity contracts
forge build

# Build Stylus contracts
cd tmm
cargo stylus build
```

### Testing

```bash
# Run Solidity tests
forge test

# Run Stylus tests
cd tmm
cargo test

# Run comprehensive test suite
cargo test -- --nocapture
```

### Deployment

```bash
# Deploy to Arbitrum Sepolia
cd tmm
cargo stylus deploy \
  --endpoint='https://arbitrum-sepolia.infura.io/v3/YOUR_INFURA_KEY' \
  --private-key="YOUR_PRIVATE_KEY" \
  --no-verify

# Cache contract for cheaper calls
cargo stylus cache bid 3409488afe731fb8e270251f267e1a822f774ec9 0
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

### Advanced Mathematical Functions

The deployed contract includes comprehensive mathematical utilities:

- **Square Root**: Newton's method with high precision
- **Exponential Functions**: Taylor series expansion
- **Logarithmic Functions**: Newton's method implementation
- **Power Calculations**: Efficient exponentiation
- **Compound Interest**: Time-weighted calculations
- **TWAP Calculations**: Time-weighted average price
- **MEV Protection Scoring**: Front-running protection metrics
- **Gas Cost Estimation**: Execution cost prediction
- **Time Decay Factors**: Order aging calculations

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
- **Stylus Optimization**: Complex math operations in Rust for gas efficiency
- **Contract Size**: 16.4 KiB optimized for deployment

### Contract Capabilities

The deployed contract provides:

- **40+ Mathematical Functions**: Comprehensive TWAMM calculations
- **Order Management System**: Complete order lifecycle management
- **Quality Scoring**: 0-100 execution quality metrics
- **Statistics Tracking**: Real-time performance monitoring
- **MEV Protection**: Front-running protection mechanisms
- **Gas Optimization**: Efficient execution with minimal costs

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
