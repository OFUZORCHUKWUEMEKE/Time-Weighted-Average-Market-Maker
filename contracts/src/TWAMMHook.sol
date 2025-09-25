// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {BaseHook} from "../../lib/v4-periphery/src/utils/BaseHook.sol";
import {Hooks} from "../../lib/v4-core/src/libraries/Hooks.sol";
import{IPoolManager} from "../../lib/v4-core/src/interfaces/IPoolManager.sol";
import{PoolKey} from "../../lib/v4-core/src/types/PoolKey.sol";
import{PoolId, PoolIdLibrary} from "../../lib/v4-core/src/types/PoolId.sol";
import{BeforeSwapDelta, BeforeSwapDeltaLibrary} from "../../lib/v4-core/src/types/BeforeSwapDelta.sol";
import {Currency, CurrencyLibrary} from "../../lib/v4-core/src/types/Currency.sol";
import {IERC20} from "../../lib/openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "../../lib/openzeppelin-contracts/contracts/token/ERC20/utils/SafeERC20.sol";
import "./TWAMMStorage.sol";
// import "./interfaces/ITWAMMHook.sol";
// import "../libraries/TWAMMMath.sol";
// import "../libraries/OrderPoolManager.sol";
import "../libraries/VirtualOrderExecutor.sol";

// import {ReentrancyGuard} from "../../lib/openzeppelin-contracts/contracts/security/ReentrancyGuard.sol";

contract TWAMMHook is BaseHook, ITWAMMHook {
    using PoolIdLibrary for PoolKey;
    using SafeERC20 for IERC20;
    using TWAMMMath for uint256;
    using OrderPoolManager for OrderPoolManager.OrderPool;
    using VirtualOrderExecutor for VirtualOrderExecutor.ExecutionState;

    // Stylus backend contract address
    address public immutable stylusCalculator;

    // Storage contract
    TWAMMStorage public immutable storage_;

    // Constants
    uint256 public constant MIN_ORDER_AMOUNT = 1e15; // 0.001 ETH minimum
    uint256 public constant MAX_ORDER_BLOCKS = 100000; // Max execution period
    uint256 public constant MIN_ORDER_BLOCKS = 10; // Min execution period
    uint256 public constant EXECUTION_REWARD = 1e15; // 0.001 ETH reward for executors

    // Events
    event OrderSubmitted(
        uint256 indexed orderId,
        address indexed owner,
        PoolId indexed poolId,
        uint256 amount,
        bool zeroForOne,
        uint256 blocks,
        uint256 sellRate
    );

    event OrderExecuted(
        uint256 indexed orderId,
        PoolId indexed poolId,
        uint256 amountIn,
        uint256 amountOut,
        address executor
    );

    event OrderCancelled(uint256 indexed orderId, address indexed owner);

    event VirtualTradesExecuted(
        PoolId indexed poolId,
        uint256 amount0,
        uint256 amount1,
        uint256 blocksPassed
    );

    modifier validPool(PoolKey calldata key) {
        require(
            address(poolManager.getPool(key.toId())) != address(0),
            "Pool not initialized"
        );
        _;
    }

    modifier onlyOrderOwner(uint256 orderId) {
        require(
            storage_.getOrderOwner(orderId) == msg.sender,
            "Not order owner"
        );
        _;
    }

    constructor(
        IPoolManager _poolManager,
        address _stylusCalculator,
        address _storage
    ) BaseHook(_poolManager) {
        stylusCalculator = _stylusCalculator;
        storage_ = TWAMMStorage(_storage);
    }

    function getHookPermissions()
        public
        pure
        override
        returns (Hooks.Permissions memory)
    {
        return
            Hooks.Permissions({
                beforeInitialize: true,
                afterInitialize: false,
                beforeAddLiquidity: false,
                afterAddLiquidity: false,
                beforeRemoveLiquidity: false,
                afterRemoveLiquidity: false,
                beforeSwap: true,
                afterSwap: false,
                beforeDonate: false,
                afterDonate: false,
                beforeSwapReturnDelta: false,
                afterSwapReturnDelta: false,
                afterAddLiquidityReturnDelta: false,
                afterRemoveLiquidityReturnDelta: false
            });
    }

    function beforeInitialize(
        address,
        PoolKey calldata key,
        uint160,
        bytes calldata
    ) external override returns (bytes4) {
        PoolId poolId = key.toId();
        storage_.initializePool(poolId);
        return BaseHook.beforeInitialize.selector;
    }

    function beforeSwap(
        address,
        PoolKey calldata key,
        IPoolManager.SwapParams calldata,
        bytes calldata
    ) external override returns (bytes4, BeforeSwapDelta, uint24) {
        PoolId poolId = key.toId();

        // Execute pending virtual orders before any swap
        _executePendingOrders(key, poolId);

        return (
            BaseHook.beforeSwap.selector,
            BeforeSwapDeltaLibrary.ZERO_DELTA,
            0
        );
    }

    function submitOrder(
        PoolKey calldata key,
        uint256 amount,
        bool zeroForOne,
        uint256 blocks
    ) external payable nonReentrant validPool(key) returns (uint256 orderId) {
        require(amount >= MIN_ORDER_AMOUNT, "Amount too small");
        require(
            blocks >= MIN_ORDER_BLOCKS && blocks <= MAX_ORDER_BLOCKS,
            "Invalid block range"
        );
        require(msg.value >= EXECUTION_REWARD, "Insufficient execution reward");

        PoolId poolId = key.toId();

        // Calculate sell rate (amount per block)
        uint256 sellRate = amount / blocks;
        require(sellRate > 0, "Sell rate too small");

        // Transfer tokens from user
        Currency currency = zeroForOne ? key.currency0 : key.currency1;
        IERC20(Currency.unwrap(currency)).safeTransferFrom(
            msg.sender,
            address(this),
            amount
        );

        // Create order
        orderId = storage_.createOrder(
            msg.sender,
            poolId,
            amount,
            sellRate,
            block.number,
            block.number + blocks,
            zeroForOne
        );

        // Update pool state
        storage_.updateSellRate(poolId, sellRate, zeroForOne, true);

        emit OrderSubmitted(
            orderId,
            msg.sender,
            poolId,
            amount,
            zeroForOne,
            blocks,
            sellRate
        );
    }

    function cancelOrder(
        uint256 orderId
    ) external nonReentrant onlyOrderOwner(orderId) {
        TWAMMStorage.Order memory order = storage_.getOrder(orderId);
        require(order.isActive, "Order not active");

        // Calculate remaining amount
        uint256 blocksPassed = block.number > order.lastExecutionBlock
            ? block.number - order.lastExecutionBlock
            : 0;
        uint256 executedAmount = blocksPassed * order.sellRate;
        uint256 remainingAmount = order.remainingAmount > executedAmount
            ? order.remainingAmount - executedAmount
            : 0;

        if (remainingAmount > 0) {
            // Return remaining tokens to user
            Currency currency = order.zeroForOne
                ? storage_.getPoolCurrency0(order.poolId)
                : storage_.getPoolCurrency1(order.poolId);
            IERC20(Currency.unwrap(currency)).safeTransfer(
                msg.sender,
                remainingAmount
            );
        }

        // Update pool state
        storage_.updateSellRate(
            order.poolId,
            order.sellRate,
            order.zeroForOne,
            false
        );

        // Mark order as inactive
        storage_.deactivateOrder(orderId);

        emit OrderCancelled(orderId, msg.sender);
    }

    function executePendingOrders(
        PoolKey calldata key
    ) external nonReentrant validPool(key) {
        PoolId poolId = key.toId();
        _executePendingOrders(key, poolId);

        // Reward executor
        payable(msg.sender).transfer(EXECUTION_REWARD);
    }

    function _executePendingOrders(
        PoolKey calldata key,
        PoolId poolId
    ) internal {
        TWAMMStorage.PoolState memory poolState = storage_.getPoolState(poolId);

        if (block.number <= poolState.lastVirtualOrderBlock) {
            return; // No time passed since last execution
        }

        uint256 blocksPassed = block.number - poolState.lastVirtualOrderBlock;

        // Skip if no active orders
        if (poolState.sellRate0 == 0 && poolState.sellRate1 == 0) {
            storage_.updateLastVirtualOrderBlock(poolId, block.number);
            return;
        }

        // Get current pool reserves
        (uint256 reserve0, uint256 reserve1) = _getPoolReserves(key);

        // Call Stylus backend for heavy mathematical computation
        (
            uint256 amount0ToSwap,
            uint256 amount1ToSwap
        ) = _calculateVirtualTrades(
                poolState.sellRate0,
                poolState.sellRate1,
                blocksPassed,
                reserve0,
                reserve1
            );

        // Execute swaps through pool manager
        if (amount0ToSwap > 0) {
            _executeSwap(key, true, amount0ToSwap);
        }

        if (amount1ToSwap > 0) {
            _executeSwap(key, false, amount1ToSwap);
        }

        // Update last execution block
        storage_.updateLastVirtualOrderBlock(poolId, block.number);

        // Update executed amounts for orders
        _updateOrderExecutions(
            poolId,
            amount0ToSwap,
            amount1ToSwap,
            blocksPassed
        );

        emit VirtualTradesExecuted(
            poolId,
            amount0ToSwap,
            amount1ToSwap,
            blocksPassed
        );
    }

    function _calculateVirtualTrades(
        uint256 sellRate0,
        uint256 sellRate1,
        uint256 blocksPassed,
        uint256 reserve0,
        uint256 reserve1
    ) internal returns (uint256 amount0, uint256 amount1) {
        // Call Stylus backend for precise mathematical computation
        bytes memory result = _callStylus(
            abi.encodeWithSignature(
                "calculateVirtualTrades(uint256,uint256,uint256,uint256,uint256)",
                sellRate0,
                sellRate1,
                blocksPassed,
                reserve0,
                reserve1
            )
        );

        return abi.decode(result, (uint256, uint256));
    }

    function _callStylus(bytes memory data) internal returns (bytes memory) {
        (bool success, bytes memory result) = stylusCalculator.call(data);
        require(success, "Stylus call failed");
        return result;
    }

    function _executeSwap(
        PoolKey calldata key,
        bool zeroForOne,
        uint256 amountIn
    ) internal {
        poolManager.unlock(abi.encode(key, zeroForOne, amountIn));
    }

    function unlockCallback(
        bytes calldata data
    ) external override returns (bytes memory) {
        require(msg.sender == address(poolManager), "Only pool manager");

        (PoolKey memory key, bool zeroForOne, uint256 amountIn) = abi.decode(
            data,
            (PoolKey, bool, uint256)
        );

        // Perform the swap
        IPoolManager.SwapParams memory params = IPoolManager.SwapParams({
            zeroForOne: zeroForOne,
            amountSpecified: -int256(amountIn), // Exact input
            sqrtPriceLimitX96: zeroForOne
                ? TickMath.MIN_SQRT_RATIO + 1
                : TickMath.MAX_SQRT_RATIO - 1
        });

        BalanceDelta delta = poolManager.swap(key, params, "");

        // Settle the swap
        if (zeroForOne) {
            poolManager.settle(key.currency0);
            poolManager.take(
                key.currency1,
                address(this),
                uint256(int256(-delta.amount1()))
            );
        } else {
            poolManager.settle(key.currency1);
            poolManager.take(
                key.currency0,
                address(this),
                uint256(int256(-delta.amount0()))
            );
        }

        return "";
    }

    function _getPoolReserves(
        PoolKey calldata key
    ) internal view returns (uint256 reserve0, uint256 reserve1) {
        PoolId poolId = key.toId();
        (uint160 sqrtPriceX96, int24 tick, ) = poolManager.getSlot0(poolId);
        uint128 liquidity = poolManager.getLiquidity(poolId);

        // Calculate reserves from sqrt price and liquidity
        reserve0 = TWAMMMath.getAmount0FromLiquidity(sqrtPriceX96, liquidity);
        reserve1 = TWAMMMath.getAmount1FromLiquidity(sqrtPriceX96, liquidity);
    }

    function _updateOrderExecutions(
        PoolId poolId,
        uint256 amount0Executed,
        uint256 amount1Executed,
        uint256 blocksPassed
    ) internal {
        // Update all active orders proportionally
        uint256[] memory activeOrders = storage_.getActiveOrders(poolId);

        for (uint256 i = 0; i < activeOrders.length; i++) {
            uint256 orderId = activeOrders[i];
            TWAMMStorage.Order memory order = storage_.getOrder(orderId);

            if (order.isActive) {
                uint256 executedAmount = order.sellRate * blocksPassed;
                storage_.updateOrderExecution(orderId, executedAmount);

                // Deactivate order if fully executed
                if (order.remainingAmount <= executedAmount) {
                    storage_.deactivateOrder(orderId);
                }
            }
        }
    }

    // View functions
    function getOrder(
        uint256 orderId
    ) external view returns (TWAMMStorage.Order memory) {
        return storage_.getOrder(orderId);
    }

    function getPoolState(
        PoolId poolId
    ) external view returns (TWAMMStorage.PoolState memory) {
        return storage_.getPoolState(poolId);
    }

    function getActiveOrders(
        PoolId poolId
    ) external view returns (uint256[] memory) {
        return storage_.getActiveOrders(poolId);
    }

    function getUserOrders(
        address user
    ) external view returns (uint256[] memory) {
        return storage_.getUserOrders(user);
    }

    function getExecutableAmount(
        uint256 orderId
    ) external view returns (uint256) {
        TWAMMStorage.Order memory order = storage_.getOrder(orderId);
        if (!order.isActive) return 0;

        uint256 blocksPassed = block.number > order.lastExecutionBlock
            ? block.number - order.lastExecutionBlock
            : 0;

        uint256 executableAmount = blocksPassed * order.sellRate;
        return
            executableAmount > order.remainingAmount
                ? order.remainingAmount
                : executableAmount;
    }

    // Emergency functions
    function emergencyWithdraw(
        uint256 orderId
    ) external onlyOrderOwner(orderId) {
        // Allow order owner to withdraw in emergency (with penalty)
        TWAMMStorage.Order memory order = storage_.getOrder(orderId);
        require(order.isActive, "Order not active");

        uint256 penalty = order.remainingAmount / 100; // 1% penalty
        uint256 withdrawAmount = order.remainingAmount - penalty;

        Currency currency = order.zeroForOne
            ? storage_.getPoolCurrency0(order.poolId)
            : storage_.getPoolCurrency1(order.poolId);

        IERC20(Currency.unwrap(currency)).safeTransfer(
            msg.sender,
            withdrawAmount
        );
        storage_.deactivateOrder(orderId);
    }

    receive() external payable {
        // Accept ETH for execution rewards
    }
}
