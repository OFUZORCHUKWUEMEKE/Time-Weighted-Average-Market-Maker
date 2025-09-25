// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {PoolKey} from "../../lib/v4-core/src/types/PoolKey.sol";
import {PoolId} from "../../lib/v4-core/src/types/PoolId.sol";
// import "./TWAMMStorage.sol";

interface ITWAMMHook {
    /**
     * @notice Submit a new TWAMM order
     * @param key The pool key
     * @param amount The total amount to sell
     * @param zeroForOne Direction of trade (true = token0->token1)
     * @param blocks Number of blocks over which to execute the order
     * @return orderId The unique identifier for the order
     */
    function submitOrder(
        PoolKey calldata key,
        uint256 amount,
        bool zeroForOne,
        uint256 blocks
    ) external payable returns (uint256 orderId);

    /**
     * @notice Cancel an existing order
     * @param orderId The order to cancel
     */
    function cancelOrder(uint256 orderId) external;

    /**
     * @notice Execute pending virtual orders for a pool
     * @param key The pool key
     */
    function executePendingOrders(PoolKey calldata key) external;

    /**
     * @notice Get order details
     * @param orderId The order ID
     * @return order The order struct
     */
    function getOrder(uint256 orderId) external view returns (TWAMMStorage.Order memory order);

    /**
     * @notice Get pool state
     * @param poolId The pool ID
     * @return state The pool state
     */
    function getPoolState(PoolId poolId) external view returns (TWAMMStorage.PoolState memory state);

    /**
     * @notice Get all active orders for a pool
     * @param poolId The pool ID
     * @return orderIds Array of active order IDs
     */
    function getActiveOrders(PoolId poolId) external view returns (uint256[] memory orderIds);

    /**
     * @notice Get all orders for a user
     * @param user The user address
     * @return orderIds Array of user's order IDs
     */
    function getUserOrders(address user) external view returns (uint256[] memory orderIds);

    /**
     * @notice Get the amount that can be executed for an order
     * @param orderId The order ID
     * @return amount The executable amount
     */
    function getExecutableAmount(uint256 orderId) external view returns (uint256 amount);

    /**
     * @notice Emergency withdrawal of order (with penalty)
     * @param orderId The order ID to withdraw
     */
    function emergencyWithdraw(uint256 orderId) external;

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
}