// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {PoolId} from "../../lib/v4-core/src/types/PoolId.sol";
import {IPoolManager} from "../../lib/v4-core/src/interfaces/IPoolManager.sol";
import {PoolKey} from "../../lib/v4-core/src/types/PoolKey.sol";
import {TickMath} from "../../lib/v4-core/src/libraries/TickMath.sol";
import {BalanceDelta} from "../../lib/v4-core/src/types/BalanceDelta.sol";
import {FullMath} from "../../lib/v4-core/src/libraries/FullMath.sol";
import {FixedPoint96} from "../../lib/v4-core/src/libraries/FixedPoint96.sol";

library VirtualOrderExecutor {
    using VirtualOrderExecutor for ExecutionState;

    struct ExecutionState {
        uint256 totalSellRate0;
        uint256 totalSellRate1;
        uint256 lastExecutionBlock;
        uint256 cumulativeVolume0;
        uint256 cumulativeVolume1;
        uint256 totalRewards;
        bool isExecuting;
    }

    struct ExecutionResult {
        uint256 amount0Executed;
        uint256 amount1Executed;
        uint256 amount0Received;
        uint256 amount1Received;
        uint256 gasUsed;
        uint256 executionTime;
        bool success;
    }

    struct ExecutionParams {
        uint256 maxGasPerExecution;
        uint256 minExecutionInterval;
        uint256 maxPriceImpact;
        uint256 slippageTolerance;
    }

    // Constants
    uint256 private constant MAX_EXECUTION_GAS = 500000;
    uint256 private constant MIN_EXECUTION_INTERVAL = 1;
    uint256 private constant MAX_PRICE_IMPACT_BPS = 1000; // 10%
    uint256 private constant SLIPPAGE_TOLERANCE_BPS = 100; // 1%

    // Events
    event VirtualOrderExecuted(
        PoolId indexed poolId,
        uint256 amount0,
        uint256 amount1,
        uint256 blocksPassed,
        uint256 gasUsed
    );

    event ExecutionFailed(
        PoolId indexed poolId,
        string reason,
        uint256 blockNumber
    );

    event LargeOrderDetected(
        PoolId indexed poolId,
        uint256 orderSize,
        uint256 poolLiquidity
    );

    event ExecutionParametersUpdated(
        PoolId indexed poolId,
        ExecutionParams params
    );

    /**
     * @notice Execute virtual orders with safety checks
     * @param state The execution state
     * @param poolManager The pool manager contract
     * @param key The pool key
     * @param blocksPassed Number of blocks since last execution
     * @param stylusCalculator Address of Stylus calculator
     * @return result Execution result
     */
    function executeVirtualOrders(
        ExecutionState storage state,
        IPoolManager poolManager,
        PoolKey memory key,
        uint256 blocksPassed,
        address stylusCalculator
    ) internal returns (ExecutionResult memory result) {
        // Safety checks
        if (state.isExecuting) {
            result.success = false;
            return result;
        }

        if (blocksPassed < MIN_EXECUTION_INTERVAL) {
            result.success = false;
            return result;
        }

        uint256 startGas = gasleft();
        state.isExecuting = true;

        try
            VirtualOrderExecutor._executeVirtualOrdersInternal(
                state,
                poolManager,
                key,
                blocksPassed,
                stylusCalculator
            )
        returns (ExecutionResult memory execResult) {
            result = execResult;
            result.success = true;
        } catch Error(string memory reason) {
            result.success = false;
            emit ExecutionFailed(key.toId(), reason, block.number);
        } catch {
            result.success = false;
            emit ExecutionFailed(key.toId(), "Unknown error", block.number);
        }

        state.isExecuting = false;
        result.gasUsed = startGas - gasleft();
        result.executionTime = block.timestamp;

        emit VirtualOrderExecuted(
            key.toId(),
            result.amount0Executed,
            result.amount1Executed,
            blocksPassed,
            result.gasUsed
        );
    }

    /**
     * @notice Internal execution function
     * @param state The execution state
     * @param poolManager The pool manager contract
     * @param key The pool key
     * @param blocksPassed Number of blocks since last execution
     * @param stylusCalculator Address of Stylus calculator
     * @return result Execution result
     */
    function _executeVirtualOrdersInternal(
        ExecutionState storage state,
        IPoolManager poolManager,
        PoolKey memory key,
        uint256 blocksPassed,
        address stylusCalculator
    ) internal returns (ExecutionResult memory result) {
        // Get current pool state
        (uint256 reserve0, uint256 reserve1) = _getPoolReserves(
            poolManager,
            key
        );

        // Calculate virtual trades using Stylus
        (
            result.amount0Executed,
            result.amount1Executed
        ) = _callStylusCalculator(
            stylusCalculator,
            state.totalSellRate0,
            state.totalSellRate1,
            blocksPassed,
            reserve0,
            reserve1
        );

        // Validate execution amounts
        _validateExecutionAmounts(
            result.amount0Executed,
            result.amount1Executed,
            reserve0,
            reserve1
        );

        // Execute trades
        if (result.amount0Executed > 0) {
            result.amount1Received = _executeSwap(
                poolManager,
                key,
                true, // zeroForOne
                result.amount0Executed
            );
        }

        if (result.amount1Executed > 0) {
            result.amount0Received = _executeSwap(
                poolManager,
                key,
                false, // oneForZero
                result.amount1Executed
            );
        }

        // Update state
        state.cumulativeVolume0 += result.amount0Executed;
        state.cumulativeVolume1 += result.amount1Executed;
        state.lastExecutionBlock = block.number;
    }

    /**
     * @notice Call Stylus calculator for complex math
     * @param calculator Stylus calculator address
     * @param sellRate0 Sell rate for token 0
     * @param sellRate1 Sell rate for token 1
     * @param blocks Number of blocks
     * @param reserve0 Current reserve 0
     * @param reserve1 Current reserve 1
     * @return amount0 Amount of token 0 to execute
     * @return amount1 Amount of token 1 to execute
     */
    function _callStylusCalculator(
        address calculator,
        uint256 sellRate0,
        uint256 sellRate1,
        uint256 blocks,
        uint256 reserve0,
        uint256 reserve1
    ) internal returns (uint256 amount0, uint256 amount1) {
        bytes memory callData = abi.encodeWithSignature(
            "calculateVirtualTrades(uint256,uint256,uint256,uint256,uint256)",
            sellRate0,
            sellRate1,
            blocks,
            reserve0,
            reserve1
        );

        (bool success, bytes memory result) = calculator.call(callData);
        require(success, "Stylus calculation failed");

        (amount0, amount1) = abi.decode(result, (uint256, uint256));
    }

    /**
     * @notice Execute a single swap through the pool manager
     * @param poolManager The pool manager
     * @param key The pool key
     * @param zeroForOne Direction of swap
     * @param amountIn Amount to swap
     * @return amountOut Amount received
     */
    function _executeSwap(
        IPoolManager poolManager,
        PoolKey memory key,
        bool zeroForOne,
        uint256 amountIn
    ) internal returns (uint256 amountOut) {
        IPoolManager.SwapParams memory params = IPoolManager.SwapParams({
            zeroForOne: zeroForOne,
            amountSpecified: -int256(amountIn), // Exact input
            sqrtPriceLimitX96: zeroForOne
                ? TickMath.MIN_SQRT_RATIO + 1
                : TickMath.MAX_SQRT_RATIO - 1
        });

        BalanceDelta delta = poolManager.swap(key, params, "");
        amountOut = uint256(
            int256(zeroForOne ? -delta.amount1() : -delta.amount0())
        );
    }

    /**
     * @notice Get current pool reserves
     * @param poolManager The pool manager
     * @param key The pool key
     * @return reserve0 Reserve of token 0
     * @return reserve1 Reserve of token 1
     */
    function _getPoolReserves(
        IPoolManager poolManager,
        PoolKey memory key
    ) internal view returns (uint256 reserve0, uint256 reserve1) {
        PoolId poolId = key.toId();
        (uint160 sqrtPriceX96, , ) = poolManager.getSlot0(poolId);
        uint128 liquidity = poolManager.getLiquidity(poolId);

        // Calculate reserves from price and liquidity
        reserve0 = _getAmount0FromLiquidity(sqrtPriceX96, liquidity);
        reserve1 = _getAmount1FromLiquidity(sqrtPriceX96, liquidity);
    }

    /**
     * @notice Validate execution amounts don't exceed safety limits
     * @param amount0 Amount of token 0 to execute
     * @param amount1 Amount of token 1 to execute
     * @param reserve0 Current reserve 0
     * @param reserve1 Current reserve 1
     */
    function _validateExecutionAmounts(
        uint256 amount0,
        uint256 amount1,
        uint256 reserve0,
        uint256 reserve1
    ) internal pure {
        // Check maximum execution size (10% of reserves)
        require(amount0 <= reserve0 / 10, "Execution too large for token0");
        require(amount1 <= reserve1 / 10, "Execution too large for token1");

        // Check minimum execution size
        require(amount0 > 0 || amount1 > 0, "No execution amount");
    }

    /**
     * @notice Calculate optimal execution size based on pool conditions
     * @param state The execution state
     * @param reserve0 Current reserve 0
     * @param reserve1 Current reserve 1
     * @param blocksPassed Number of blocks passed
     * @return optimalAmount0 Optimal amount for token 0
     * @return optimalAmount1 Optimal amount for token 1
     */
    function calculateOptimalExecution(
        ExecutionState storage state,
        uint256 reserve0,
        uint256 reserve1,
        uint256 blocksPassed
    ) internal view returns (uint256 optimalAmount0, uint256 optimalAmount1) {
        // Base execution amounts
        uint256 baseAmount0 = state.totalSellRate0 * blocksPassed;
        uint256 baseAmount1 = state.totalSellRate1 * blocksPassed;

        // Apply safety scaling based on pool size
        optimalAmount0 = _applySafetyScaling(baseAmount0, reserve0);
        optimalAmount1 = _applySafetyScaling(baseAmount1, reserve1);
    }

    /**
     * @notice Apply safety scaling to execution amount
     * @param baseAmount Base execution amount
     * @param reserve Pool reserve
     * @return scaledAmount Scaled execution amount
     */
    function _applySafetyScaling(
        uint256 baseAmount,
        uint256 reserve
    ) internal pure returns (uint256 scaledAmount) {
        // Scale down if execution would be too large
        if (baseAmount > reserve / 20) {
            // More than 5% of pool
            scaledAmount = reserve / 20; // Cap at 5%
        } else {
            scaledAmount = baseAmount;
        }
    }

    /**
     * @notice Calculate price impact of execution
     * @param amountIn Input amount
     * @param reserveIn Input reserve
     * @param reserveOut Output reserve
     * @return impact Price impact in basis points
     */
    function calculatePriceImpact(
        uint256 amountIn,
        uint256 reserveIn,
        uint256 reserveOut
    ) internal pure returns (uint256 impact) {
        if (amountIn == 0) return 0;

        uint256 k = reserveIn * reserveOut;
        uint256 newReserveIn = reserveIn + amountIn;
        uint256 newReserveOut = k / newReserveIn;
        uint256 amountOut = reserveOut - newReserveOut;

        uint256 expectedOut = (amountIn * reserveOut) / reserveIn;

        if (expectedOut > amountOut) {
            impact = ((expectedOut - amountOut) * 10000) / expectedOut;
        }
    }

    /**
     * @notice Check if execution should be delayed due to high impact
     * @param amount0 Token 0 amount
     * @param amount1 Token 1 amount
     * @param reserve0 Token 0 reserve
     * @param reserve1 Token 1 reserve
     * @return shouldDelay Whether execution should be delayed
     */
    function shouldDelayExecution(
        uint256 amount0,
        uint256 amount1,
        uint256 reserve0,
        uint256 reserve1
    ) internal pure returns (bool shouldDelay) {
        uint256 impact0 = calculatePriceImpact(amount0, reserve0, reserve1);
        uint256 impact1 = calculatePriceImpact(amount1, reserve1, reserve0);

        return impact0 > MAX_PRICE_IMPACT_BPS || impact1 > MAX_PRICE_IMPACT_BPS;
    }

    /**
     * @notice Get execution statistics
     * @param state The execution state
     * @return stats Execution statistics
     */
    function getExecutionStats(
        ExecutionState storage state
    )
        internal
        view
        returns (
            uint256 totalVolume0,
            uint256 totalVolume1,
            uint256 lastExecution,
            uint256 currentSellRates,
            bool isActive
        )
    {
        return (
            state.cumulativeVolume0,
            state.cumulativeVolume1,
            state.lastExecutionBlock,
            state.totalSellRate0 + state.totalSellRate1,
            state.totalSellRate0 > 0 || state.totalSellRate1 > 0
        );
    }

    /**
     * @notice Estimate gas cost for execution
     * @param amount0 Token 0 amount to execute
     * @param amount1 Token 1 amount to execute
     * @return estimatedGas Estimated gas cost
     */
    function estimateExecutionGas(
        uint256 amount0,
        uint256 amount1
    ) internal pure returns (uint256 estimatedGas) {
        uint256 baseGas = 100000; // Base execution gas

        if (amount0 > 0) baseGas += 150000; // Swap gas
        if (amount1 > 0) baseGas += 150000; // Swap gas

        estimatedGas = baseGas;
    }

    /**
     * @notice Emergency stop execution
     * @param state The execution state
     */
    function emergencyStop(ExecutionState storage state) internal {
        state.isExecuting = false;
        state.totalSellRate0 = 0;
        state.totalSellRate1 = 0;
    }

    // Helper functions for liquidity calculations
    function _getAmount0FromLiquidity(
        uint160 sqrtPriceX96,
        uint128 liquidity
    ) internal pure returns (uint256) {
        return
            FullMath.mulDiv(
                uint256(liquidity) << FixedPoint96.RESOLUTION,
                FixedPoint96.Q96,
                sqrtPriceX96
            );
    }

    function _getAmount1FromLiquidity(
        uint160 sqrtPriceX96,
        uint128 liquidity
    ) internal pure returns (uint256) {
        return FullMath.mulDiv(liquidity, sqrtPriceX96, FixedPoint96.Q96);
    }
}
