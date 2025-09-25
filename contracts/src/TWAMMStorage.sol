// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {PoolId} from "../../lib/v4-core/src/types/PoolId.sol";
import {Currency} from "../../lib/v4-core/src/types/Currency.sol";
import {Ownable} from "../../lib/openzeppelin-contracts/contracts/access/Ownable.sol";

contract TWAMMStorage is Ownable {
    
    struct Order {
        uint256 id;
        address owner;
        PoolId poolId;
        uint256 originalAmount;
        uint256 remainingAmount;
        uint256 sellRate;
        uint256 startBlock;
        uint256 endBlock;
        uint256 lastExecutionBlock;
        bool zeroForOne;
        bool isActive;
        uint256 totalExecuted;
    }

    struct PoolState {
        bool initialized;
        uint256 sellRate0;
        uint256 sellRate1;
        uint256 lastVirtualOrderBlock;
        uint256 totalOrders0;
        uint256 totalOrders1;
        Currency currency0;
        Currency currency1;
    }

    uint256 private nextOrderId = 1;
    
    mapping(uint256 => Order) public orders;
    mapping(PoolId => PoolState) public poolStates;
    mapping(PoolId => uint256[]) public poolActiveOrders;
    mapping(address => uint256[]) public userOrders;
    mapping(uint256 => uint256) public orderIndexInPool;
    mapping(PoolId => mapping(bool => uint256)) public poolOrderCounts;

    event PoolInitialized(PoolId indexed poolId, Currency currency0, Currency currency1);
    event OrderCreated(
        uint256 indexed orderId,
        address indexed owner,
        PoolId indexed poolId,
        uint256 amount,
        bool zeroForOne
    );
    event OrderDeactivated(uint256 indexed orderId);
    event SellRateUpdated(PoolId indexed poolId, uint256 sellRate0, uint256 sellRate1);

    modifier onlyAuthorized() {
        require(msg.sender == owner() || _authorizedCallers[msg.sender], "Unauthorized");
        _;
    }

    mapping(address => bool) private _authorizedCallers;

    constructor() Ownable() {}

    function authorizeCaller(address caller) external onlyOwner {
        _authorizedCallers[caller] = true;
    }

    function revokeCaller(address caller) external onlyOwner {
        _authorizedCallers[caller] = false;
    }

    function initializePool(PoolId poolId) external onlyAuthorized {
        require(!poolStates[poolId].initialized, "Pool already initialized");
        
        poolStates[poolId] = PoolState({
            initialized: true,
            sellRate0: 0,
            sellRate1: 0,
            lastVirtualOrderBlock: block.number,
            totalOrders0: 0,
            totalOrders1: 0,
            currency0: Currency.wrap(address(0)),
            currency1: Currency.wrap(address(0))
        });

        emit PoolInitialized(poolId, poolStates[poolId].currency0, poolStates[poolId].currency1);
    }

    function setPoolCurrencies(
        PoolId poolId,
        Currency currency0,
        Currency currency1
    ) external onlyAuthorized {
        require(poolStates[poolId].initialized, "Pool not initialized");
        poolStates[poolId].currency0 = currency0;
        poolStates[poolId].currency1 = currency1;
    }

    function createOrder(
        address owner,
        PoolId poolId,
        uint256 amount,
        uint256 sellRate,
        uint256 startBlock,
        uint256 endBlock,
        bool zeroForOne
    ) external onlyAuthorized returns (uint256 orderId) {
        require(poolStates[poolId].initialized, "Pool not initialized");
        require(amount > 0, "Amount must be greater than 0");
        require(sellRate > 0, "Sell rate must be greater than 0");
        require(endBlock > startBlock, "End block must be after start block");

        orderId = nextOrderId++;

        orders[orderId] = Order({
            id: orderId,
            owner: owner,
            poolId: poolId,
            originalAmount: amount,
            remainingAmount: amount,
            sellRate: sellRate,
            startBlock: startBlock,
            endBlock: endBlock,
            lastExecutionBlock: startBlock,
            zeroForOne: zeroForOne,
            isActive: true,
            totalExecuted: 0
        });

        poolActiveOrders[poolId].push(orderId);
        orderIndexInPool[orderId] = poolActiveOrders[poolId].length - 1;
        
        userOrders[owner].push(orderId);

        if (zeroForOne) {
            poolStates[poolId].totalOrders0++;
        } else {
            poolStates[poolId].totalOrders1++;
        }

        poolOrderCounts[poolId][zeroForOne]++;

        emit OrderCreated(orderId, owner, poolId, amount, zeroForOne);
    }

    function updateSellRate(
        PoolId poolId,
        uint256 sellRate,
        bool zeroForOne,
        bool isAdd
    ) external onlyAuthorized {
        require(poolStates[poolId].initialized, "Pool not initialized");

        if (zeroForOne) {
            if (isAdd) {
                poolStates[poolId].sellRate0 += sellRate;
            } else {
                poolStates[poolId].sellRate0 = poolStates[poolId].sellRate0 >= sellRate 
                    ? poolStates[poolId].sellRate0 - sellRate 
                    : 0;
            }
        } else {
            if (isAdd) {
                poolStates[poolId].sellRate1 += sellRate;
            } else {
                poolStates[poolId].sellRate1 = poolStates[poolId].sellRate1 >= sellRate 
                    ? poolStates[poolId].sellRate1 - sellRate 
                    : 0;
            }
        }

        emit SellRateUpdated(poolId, poolStates[poolId].sellRate0, poolStates[poolId].sellRate1);
    }

    function updateLastVirtualOrderBlock(
        PoolId poolId,
        uint256 blockNumber
    ) external onlyAuthorized {
        require(poolStates[poolId].initialized, "Pool not initialized");
        poolStates[poolId].lastVirtualOrderBlock = blockNumber;
    }

    function updateOrderExecution(
        uint256 orderId,
        uint256 executedAmount
    ) external onlyAuthorized {
        require(orders[orderId].isActive, "Order not active");
        
        Order storage order = orders[orderId];
        
        if (executedAmount >= order.remainingAmount) {
            order.totalExecuted += order.remainingAmount;
            order.remainingAmount = 0;
        } else {
            order.totalExecuted += executedAmount;
            order.remainingAmount -= executedAmount;
        }
        
        order.lastExecutionBlock = block.number;
    }

    function deactivateOrder(uint256 orderId) external onlyAuthorized {
        require(orders[orderId].isActive, "Order already inactive");
        
        Order storage order = orders[orderId];
        order.isActive = false;

        _removeOrderFromPool(orderId, order.poolId);

        if (order.zeroForOne) {
            poolStates[order.poolId].totalOrders0--;
        } else {
            poolStates[order.poolId].totalOrders1--;
        }

        poolOrderCounts[order.poolId][order.zeroForOne]--;

        emit OrderDeactivated(orderId);
    }

    function _removeOrderFromPool(uint256 orderId, PoolId poolId) internal {
        uint256 orderIndex = orderIndexInPool[orderId];
        uint256[] storage activeOrders = poolActiveOrders[poolId];
        
        if (orderIndex < activeOrders.length && activeOrders[orderIndex] == orderId) {
            uint256 lastOrderId = activeOrders[activeOrders.length - 1];
            activeOrders[orderIndex] = lastOrderId;
            orderIndexInPool[lastOrderId] = orderIndex;
            activeOrders.pop();
            delete orderIndexInPool[orderId];
        }
    }

    function getOrder(uint256 orderId) external view returns (Order memory) {
        return orders[orderId];
    }

    function getOrderOwner(uint256 orderId) external view returns (address) {
        return orders[orderId].owner;
    }

    function getPoolState(PoolId poolId) external view returns (PoolState memory) {
        return poolStates[poolId];
    }

    function getPoolCurrency0(PoolId poolId) external view returns (Currency) {
        return poolStates[poolId].currency0;
    }

    function getPoolCurrency1(PoolId poolId) external view returns (Currency) {
        return poolStates[poolId].currency1;
    }

    function getActiveOrders(PoolId poolId) external view returns (uint256[] memory) {
        return poolActiveOrders[poolId];
    }

    function getUserOrders(address user) external view returns (uint256[] memory) {
        return userOrders[user];
    }

    function getActiveOrdersCount(PoolId poolId) external view returns (uint256) {
        return poolActiveOrders[poolId].length;
    }

    function getOrderCountByDirection(PoolId poolId, bool zeroForOne) external view returns (uint256) {
        return poolOrderCounts[poolId][zeroForOne];
    }

    function isPoolInitialized(PoolId poolId) external view returns (bool) {
        return poolStates[poolId].initialized;
    }

    function getNextOrderId() external view returns (uint256) {
        return nextOrderId;
    }

    function getTotalExecutedAmount(uint256 orderId) external view returns (uint256) {
        return orders[orderId].totalExecuted;
    }

    function getRemainingAmount(uint256 orderId) external view returns (uint256) {
        return orders[orderId].remainingAmount;
    }

    function isOrderActive(uint256 orderId) external view returns (bool) {
        return orders[orderId].isActive;
    }

    function getOrderStartBlock(uint256 orderId) external view returns (uint256) {
        return orders[orderId].startBlock;
    }

    function getOrderEndBlock(uint256 orderId) external view returns (uint256) {
        return orders[orderId].endBlock;
    }

    function getOrderLastExecutionBlock(uint256 orderId) external view returns (uint256) {
        return orders[orderId].lastExecutionBlock;
    }

    function getCurrentSellRates(PoolId poolId) external view returns (uint256 sellRate0, uint256 sellRate1) {
        PoolState memory state = poolStates[poolId];
        return (state.sellRate0, state.sellRate1);
    }

    function getActiveOrdersByDirection(
        PoolId poolId, 
        bool zeroForOne
    ) external view returns (uint256[] memory) {
        uint256[] memory allActiveOrders = poolActiveOrders[poolId];
        uint256 count = 0;
        
        for (uint256 i = 0; i < allActiveOrders.length; i++) {
            if (orders[allActiveOrders[i]].zeroForOne == zeroForOne && orders[allActiveOrders[i]].isActive) {
                count++;
            }
        }
        
        uint256[] memory filteredOrders = new uint256[](count);
        uint256 index = 0;
        
        for (uint256 i = 0; i < allActiveOrders.length; i++) {
            if (orders[allActiveOrders[i]].zeroForOne == zeroForOne && orders[allActiveOrders[i]].isActive) {
                filteredOrders[index] = allActiveOrders[i];
                index++;
            }
        }
        
        return filteredOrders;
    }

    function batchUpdateOrders(
        uint256[] calldata orderIds,
        uint256[] calldata executedAmounts
    ) external onlyAuthorized {
        require(orderIds.length == executedAmounts.length, "Array length mismatch");
        
        for (uint256 i = 0; i < orderIds.length; i++) {
            if (orders[orderIds[i]].isActive) {
                updateOrderExecution(orderIds[i], executedAmounts[i]);
            }
        }
    }

    function emergencyDeactivateOrder(uint256 orderId) external onlyOwner {
        require(orders[orderId].isActive, "Order already inactive");
        
        Order storage order = orders[orderId];
        order.isActive = false;

        _removeOrderFromPool(orderId, order.poolId);

        emit OrderDeactivated(orderId);
    }

    function getPoolSummary(PoolId poolId) external view returns (
        bool initialized,
        uint256 totalActiveOrders,
        uint256 sellRate0,
        uint256 sellRate1,
        uint256 lastExecutionBlock
    ) {
        PoolState memory state = poolStates[poolId];
        return (
            state.initialized,
            poolActiveOrders[poolId].length,
            state.sellRate0,
            state.sellRate1,
            state.lastVirtualOrderBlock
        );
    }
}