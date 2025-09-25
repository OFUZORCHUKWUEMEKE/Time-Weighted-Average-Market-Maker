use crate::twamm_math::{MathError, TWAMMath};
use alloc::vec::Vec;
use stylus_sdk::{
    alloy_primitives::{Address, U256},
    console,
};

/// Order execution logic for TWAMM
/// Handles long-term orders, virtual order processing, and execution scheduling

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OrderType {
    LongTerm = 0,
    Instant = 1,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OrderDirection {
    SellToken0 = 0,
    SellToken1 = 1,
}

#[derive(Clone, Copy, Debug)]
pub struct Order {
    pub id: U256,
    pub owner: Address,
    pub order_type: OrderType,
    pub direction: OrderDirection,
    pub sell_rate: U256,
    pub remaining_amount: U256,
    pub start_block: U256,
    pub end_block: U256,
    pub last_virtual_order_block: U256,
    pub accumulated_out: U256,
}

impl Default for Order {
    fn default() -> Self {
        Self {
            id: U256::ZERO,
            owner: Address::ZERO,
            order_type: OrderType::Instant,
            direction: OrderDirection::SellToken0,
            sell_rate: U256::ZERO,
            remaining_amount: U256::ZERO,
            start_block: U256::ZERO,
            end_block: U256::ZERO,
            last_virtual_order_block: U256::ZERO,
            accumulated_out: U256::ZERO,
        }
    }
}

/// Virtual order execution state
#[derive(Clone, Copy, Debug)]
pub struct VirtualOrderState {
    pub last_virtual_order_block: U256,
    pub sell_rate_0_to_1: U256,
    pub sell_rate_1_to_0: U256,
    pub order_block_interval: U256,
}

impl Default for VirtualOrderState {
    fn default() -> Self {
        Self {
            last_virtual_order_block: U256::ZERO,
            sell_rate_0_to_1: U256::ZERO,
            sell_rate_1_to_0: U256::ZERO,
            order_block_interval: U256::from(100u32), // Default 100 blocks
        }
    }
}

/// Order pool for managing active long-term orders
pub struct OrderPool {
    pub orders: Vec<Order>,
    pub next_order_id: U256,
    pub virtual_order_state: VirtualOrderState,
    pub total_sell_rate_0: U256,
    pub total_sell_rate_1: U256,
}

impl Default for OrderPool {
    fn default() -> Self {
        Self {
            orders: Vec::new(),
            next_order_id: U256::from(1u32),
            virtual_order_state: VirtualOrderState::default(),
            total_sell_rate_0: U256::ZERO,
            total_sell_rate_1: U256::ZERO,
        }
    }
}

/// Execution result for virtual orders
#[derive(Debug, Clone, Copy)]
pub struct VirtualExecutionResult {
    pub blocks_executed: U256,
    pub amount_0_sold: U256,
    pub amount_1_sold: U256,
    pub amount_0_received: U256,
    pub amount_1_received: U256,
    pub new_reserve_0: U256,
    pub new_reserve_1: U256,
    pub gas_used_estimate: U256,
}

impl OrderPool {
    /// Create a new long-term order
    pub fn create_long_term_order(
        &mut self,
        owner: Address,
        direction: OrderDirection,
        sell_amount: U256,
        duration_blocks: U256,
        current_block: U256,
    ) -> Result<U256, Vec<u8>> {
        if sell_amount == U256::ZERO || duration_blocks == U256::ZERO {
            return Err(b"Invalid order parameters".to_vec());
        }

        // Calculate sell rate
        let sell_rate = sell_amount
            .checked_div(duration_blocks)
            .ok_or(b"Division overflow".to_vec())?;

        let order = Order {
            id: self.next_order_id,
            owner,
            order_type: OrderType::LongTerm,
            direction,
            sell_rate,
            remaining_amount: sell_amount,
            start_block: current_block,
            end_block: current_block
                .checked_add(duration_blocks)
                .ok_or(b"Block overflow".to_vec())?,
            last_virtual_order_block: current_block,
            accumulated_out: U256::ZERO,
        };

        self.orders.push(order);

        // Update total sell rates
        match direction {
            OrderDirection::SellToken0 => {
                self.total_sell_rate_0 = self
                    .total_sell_rate_0
                    .checked_add(sell_rate)
                    .ok_or(b"Rate overflow".to_vec())?;
            }
            OrderDirection::SellToken1 => {
                self.total_sell_rate_1 = self
                    .total_sell_rate_1
                    .checked_add(sell_rate)
                    .ok_or(b"Rate overflow".to_vec())?;
            }
        }

        let order_id = self.next_order_id;
        self.next_order_id = self
            .next_order_id
            .checked_add(U256::from(1u32))
            .ok_or(b"Order ID overflow".to_vec())?;

        console!(
            "Created long-term order {} for {} blocks",
            order_id,
            duration_blocks
        );
        Ok(order_id)
    }

    /// Cancel an existing order
    pub fn cancel_order(&mut self, order_id: U256, caller: Address) -> Result<Order, Vec<u8>> {
        let order_index = self
            .orders
            .iter()
            .position(|order| order.id == order_id)
            .ok_or(b"Order not found".to_vec())?;

        let order = self.orders[order_index];

        // Check ownership
        if order.owner != caller {
            return Err(b"Not order owner".to_vec());
        }

        // Update total sell rates
        match order.direction {
            OrderDirection::SellToken0 => {
                self.total_sell_rate_0 = self
                    .total_sell_rate_0
                    .checked_sub(order.sell_rate)
                    .ok_or(b"Rate underflow".to_vec())?;
            }
            OrderDirection::SellToken1 => {
                self.total_sell_rate_1 = self
                    .total_sell_rate_1
                    .checked_sub(order.sell_rate)
                    .ok_or(b"Rate underflow".to_vec())?;
            }
        }

        // Remove order from active orders
        self.orders.remove(order_index);

        console!("Cancelled order {}", order_id);
        Ok(order)
    }

    /// Execute virtual orders up to current block
    pub fn execute_virtual_orders(
        &mut self,
        current_block: U256,
        current_reserve_0: U256,
        current_reserve_1: U256,
    ) -> Result<VirtualExecutionResult, Vec<u8>> {
        let last_block = self.virtual_order_state.last_virtual_order_block;

        if current_block <= last_block {
            // No execution needed
            return Ok(VirtualExecutionResult {
                blocks_executed: U256::ZERO,
                amount_0_sold: U256::ZERO,
                amount_1_sold: U256::ZERO,
                amount_0_received: U256::ZERO,
                amount_1_received: U256::ZERO,
                new_reserve_0: current_reserve_0,
                new_reserve_1: current_reserve_1,
                gas_used_estimate: U256::ZERO,
            });
        }

        let blocks_elapsed = current_block
            .checked_sub(last_block)
            .ok_or(b"Block calculation error".to_vec())?;

        // Get active sell rates at the time of execution
        let (active_sell_rate_0, active_sell_rate_1) =
            self.get_active_sell_rates(last_block, current_block)?;

        // Use closed-form solution to calculate virtual order execution
        let (new_reserve_0, new_reserve_1, amount_0_received, amount_1_received) =
            TWAMMath::execute_virtual_orders_closed_form(
                active_sell_rate_0,
                active_sell_rate_1,
                blocks_elapsed,
                current_reserve_0,
                current_reserve_1,
            )
            .map_err(|e| match e {
                MathError::Overflow => b"Math overflow in virtual execution".to_vec(),
                MathError::DivisionByZero => b"Division by zero in virtual execution".to_vec(),
                MathError::InvalidInput => b"Invalid input for virtual execution".to_vec(),
                MathError::ComputationFailed => b"Virtual execution computation failed".to_vec(),
            })?;

        // Update order states and remove completed orders
        self.update_orders_after_execution(
            blocks_elapsed,
            amount_0_received,
            amount_1_received,
            current_block,
        )?;

        // Update virtual order state
        self.virtual_order_state.last_virtual_order_block = current_block;

        let amount_0_sold = active_sell_rate_0
            .checked_mul(blocks_elapsed)
            .ok_or(b"Calculation overflow".to_vec())?;
        let amount_1_sold = active_sell_rate_1
            .checked_mul(blocks_elapsed)
            .ok_or(b"Calculation overflow".to_vec())?;

        // Estimate gas used (approximation based on blocks executed)
        let gas_estimate = blocks_elapsed
            .checked_mul(U256::from(21000u32))
            .ok_or(b"Gas calculation overflow".to_vec())?;

        console!("Executed virtual orders for {} blocks", blocks_elapsed);

        Ok(VirtualExecutionResult {
            blocks_executed: blocks_elapsed,
            amount_0_sold,
            amount_1_sold,
            amount_0_received,
            amount_1_received,
            new_reserve_0,
            new_reserve_1,
            gas_used_estimate: gas_estimate,
        })
    }

    /// Get active sell rates for a given time period
    fn get_active_sell_rates(
        &self,
        start_block: U256,
        end_block: U256,
    ) -> Result<(U256, U256), Vec<u8>> {
        let mut total_rate_0 = U256::ZERO;
        let mut total_rate_1 = U256::ZERO;

        for order in &self.orders {
            if order.order_type != OrderType::LongTerm {
                continue;
            }

            // Check if order is active during this period
            if order.end_block <= start_block || order.start_block >= end_block {
                continue; // Order not active in this period
            }

            // Calculate the effective rate for this time period
            let effective_start = order.start_block.max(start_block);
            let effective_end = order.end_block.min(end_block);

            if effective_end > effective_start {
                match order.direction {
                    OrderDirection::SellToken0 => {
                        total_rate_0 = total_rate_0
                            .checked_add(order.sell_rate)
                            .ok_or(b"Rate calculation overflow".to_vec())?;
                    }
                    OrderDirection::SellToken1 => {
                        total_rate_1 = total_rate_1
                            .checked_add(order.sell_rate)
                            .ok_or(b"Rate calculation overflow".to_vec())?;
                    }
                }
            }
        }

        Ok((total_rate_0, total_rate_1))
    }

    /// Update orders after virtual execution
    fn update_orders_after_execution(
        &mut self,
        blocks_elapsed: U256,
        amount_0_received: U256,
        amount_1_received: U256,
        current_block: U256,
    ) -> Result<(), Vec<u8>> {
        let mut orders_to_remove = Vec::new();

        for (index, order) in self.orders.iter_mut().enumerate() {
            if order.order_type != OrderType::LongTerm {
                continue;
            }

            // Check if order has expired
            if current_block >= order.end_block {
                orders_to_remove.push(index);
                continue;
            }

            // Update order state
            let amount_sold = order
                .sell_rate
                .checked_mul(blocks_elapsed)
                .ok_or(b"Amount calculation overflow".to_vec())?;

            order.remaining_amount = order
                .remaining_amount
                .checked_sub(amount_sold.min(order.remaining_amount))
                .unwrap_or(U256::ZERO);

            // Distribute received amounts proportionally
            let received_amount = match order.direction {
                OrderDirection::SellToken0 => {
                    // This order sold token0, received token1
                    if self.total_sell_rate_0 > U256::ZERO {
                        amount_1_received
                            .checked_mul(order.sell_rate)
                            .ok_or(b"Distribution calculation overflow".to_vec())?
                            .checked_div(self.total_sell_rate_0)
                            .ok_or(b"Distribution division error".to_vec())?
                    } else {
                        U256::ZERO
                    }
                }
                OrderDirection::SellToken1 => {
                    // This order sold token1, received token0
                    if self.total_sell_rate_1 > U256::ZERO {
                        amount_0_received
                            .checked_mul(order.sell_rate)
                            .ok_or(b"Distribution calculation overflow".to_vec())?
                            .checked_div(self.total_sell_rate_1)
                            .ok_or(b"Distribution division error".to_vec())?
                    } else {
                        U256::ZERO
                    }
                }
            };

            order.accumulated_out = order
                .accumulated_out
                .checked_add(received_amount)
                .ok_or(b"Accumulated amount overflow".to_vec())?;

            order.last_virtual_order_block = current_block;

            // Mark completed orders for removal
            if order.remaining_amount == U256::ZERO {
                orders_to_remove.push(index);
            }
        }

        // Remove completed orders (in reverse order to maintain indices)
        for &index in orders_to_remove.iter().rev() {
            let completed_order = self.orders.remove(index);

            // Update total sell rates
            match completed_order.direction {
                OrderDirection::SellToken0 => {
                    self.total_sell_rate_0 = self
                        .total_sell_rate_0
                        .checked_sub(completed_order.sell_rate)
                        .unwrap_or(U256::ZERO);
                }
                OrderDirection::SellToken1 => {
                    self.total_sell_rate_1 = self
                        .total_sell_rate_1
                        .checked_sub(completed_order.sell_rate)
                        .unwrap_or(U256::ZERO);
                }
            }

            console!("Completed order {}", completed_order.id);
        }

        Ok(())
    }

    /// Get order details by ID
    pub fn get_order(&self, order_id: U256) -> Option<Order> {
        self.orders
            .iter()
            .find(|order| order.id == order_id)
            .copied()
    }

    /// Get all orders for a specific owner
    pub fn get_orders_by_owner(&self, owner: Address) -> Vec<Order> {
        self.orders
            .iter()
            .filter(|order| order.owner == owner)
            .copied()
            .collect()
    }

    /// Get active orders count
    pub fn get_active_orders_count(&self) -> usize {
        self.orders.len()
    }

    /// Get current total sell rates
    pub fn get_current_sell_rates(&self) -> (U256, U256) {
        (self.total_sell_rate_0, self.total_sell_rate_1)
    }

    /// Check if virtual order execution is needed
    pub fn needs_virtual_order_execution(&self, current_block: U256) -> bool {
        if self.orders.is_empty() {
            return false;
        }

        let blocks_since_last = current_block
            .checked_sub(self.virtual_order_state.last_virtual_order_block)
            .unwrap_or(U256::ZERO);

        blocks_since_last >= self.virtual_order_state.order_block_interval
    }

    /// Update virtual order execution interval
    pub fn set_order_block_interval(&mut self, interval: U256) -> Result<(), Vec<u8>> {
        if interval == U256::ZERO {
            return Err(b"Invalid block interval".to_vec());
        }

        self.virtual_order_state.order_block_interval = interval;
        console!("Updated order block interval to {}", interval);
        Ok(())
    }

    /// Estimate gas cost for virtual order execution
    pub fn estimate_virtual_execution_gas(&self, current_block: U256) -> U256 {
        let blocks_since_last = current_block
            .checked_sub(self.virtual_order_state.last_virtual_order_block)
            .unwrap_or(U256::ZERO);

        // Base gas cost for virtual execution
        let base_gas = U256::from(50000u32);

        // Additional gas per block of execution
        let per_block_gas = U256::from(1000u32);
        let blocks_gas = blocks_since_last
            .checked_mul(per_block_gas)
            .unwrap_or(U256::ZERO);

        // Additional gas per active order
        let per_order_gas = U256::from(5000u32);
        let orders_gas = U256::from(self.orders.len() as u64)
            .checked_mul(per_order_gas)
            .unwrap_or(U256::ZERO);

        base_gas
            .checked_add(blocks_gas)
            .unwrap_or(U256::MAX)
            .checked_add(orders_gas)
            .unwrap_or(U256::MAX)
    }

    /// Get detailed execution statistics
    pub fn get_execution_statistics(&self, current_block: U256) -> ExecutionStatistics {
        let mut total_volume_0 = U256::ZERO;
        let mut total_volume_1 = U256::ZERO;
        let mut active_orders = 0;
        let mut completed_orders = 0;

        for order in &self.orders {
            if current_block >= order.end_block {
                completed_orders += 1;
            } else {
                active_orders += 1;
            }

            match order.direction {
                OrderDirection::SellToken0 => {
                    let executed_amount = order
                        .sell_rate
                        .checked_mul(
                            current_block
                                .checked_sub(order.start_block)
                                .unwrap_or(U256::ZERO),
                        )
                        .unwrap_or(U256::ZERO)
                        .min(order.remaining_amount);
                    total_volume_0 = total_volume_0
                        .checked_add(executed_amount)
                        .unwrap_or(total_volume_0);
                }
                OrderDirection::SellToken1 => {
                    let executed_amount = order
                        .sell_rate
                        .checked_mul(
                            current_block
                                .checked_sub(order.start_block)
                                .unwrap_or(U256::ZERO),
                        )
                        .unwrap_or(U256::ZERO)
                        .min(order.remaining_amount);
                    total_volume_1 = total_volume_1
                        .checked_add(executed_amount)
                        .unwrap_or(total_volume_1);
                }
            }
        }

        ExecutionStatistics {
            active_orders,
            completed_orders,
            total_volume_0,
            total_volume_1,
            current_sell_rate_0: self.total_sell_rate_0,
            current_sell_rate_1: self.total_sell_rate_1,
            last_virtual_execution_block: self.virtual_order_state.last_virtual_order_block,
        }
    }
}

/// Statistics for order execution
#[derive(Debug, Clone, Copy)]
pub struct ExecutionStatistics {
    pub active_orders: u32,
    pub completed_orders: u32,
    pub total_volume_0: U256,
    pub total_volume_1: U256,
    pub current_sell_rate_0: U256,
    pub current_sell_rate_1: U256,
    pub last_virtual_execution_block: U256,
}

/// Order management utilities
pub struct OrderManager;

impl OrderManager {
    /// Calculate optimal order block interval based on gas costs and execution frequency
    pub fn calculate_optimal_interval(
        avg_gas_price: U256,
        execution_frequency_target: U256, // Target executions per day
    ) -> U256 {
        // Assume ~6000 blocks per day (15 second block times)
        let blocks_per_day = U256::from(6000u32);

        let optimal_interval = blocks_per_day
            .checked_div(execution_frequency_target)
            .unwrap_or(U256::from(100u32)) // Default fallback
            .max(U256::from(10u32)) // Minimum interval
            .min(U256::from(1000u32)); // Maximum interval

        optimal_interval
    }

    /// Validate order parameters
    pub fn validate_order_params(
        sell_amount: U256,
        duration_blocks: U256,
        current_reserve: U256,
    ) -> Result<(), Vec<u8>> {
        if sell_amount == U256::ZERO {
            return Err(b"Sell amount cannot be zero".to_vec());
        }

        if duration_blocks == U256::ZERO {
            return Err(b"Duration must be positive".to_vec());
        }

        if duration_blocks < U256::from(10u32) {
            return Err(b"Duration too short (minimum 10 blocks)".to_vec());
        }

        if duration_blocks > U256::from(1000000u32) {
            return Err(b"Duration too long (maximum 1M blocks)".to_vec());
        }

        // Check if sell amount is reasonable relative to current reserves
        let max_reasonable_amount = current_reserve
            .checked_div(U256::from(10u32))
            .unwrap_or(U256::ZERO); // Max 10% of reserves

        if sell_amount > max_reasonable_amount && current_reserve > U256::ZERO {
            return Err(b"Sell amount too large relative to reserves".to_vec());
        }

        Ok(())
    }

    /// Calculate time-weighted average price impact
    pub fn calculate_twap_impact(
        sell_amount: U256,
        duration_blocks: U256,
        reserve_in: U256,
        reserve_out: U256,
    ) -> Result<U256, Vec<u8>> {
        if duration_blocks == U256::ZERO {
            return Err(b"Invalid duration".to_vec());
        }

        let sell_rate = sell_amount
            .checked_div(duration_blocks)
            .ok_or(b"Rate calculation error".to_vec())?;

        // Calculate impact using TWAMM math
        let impact = TWAMMath::calculate_unidirectional_twamm(
            sell_amount,
            duration_blocks,
            reserve_in,
            reserve_out,
        )
        .map_err(|_| b"Impact calculation failed".to_vec())?;

        // Convert to basis points (impact relative to expected amount)
        let expected_out = sell_amount
            .checked_mul(reserve_out)
            .ok_or(b"Expected calculation overflow".to_vec())?
            .checked_div(reserve_in)
            .ok_or(b"Expected calculation division error".to_vec())?;

        if expected_out <= impact {
            return Ok(U256::ZERO);
        }

        let price_impact = expected_out
            .checked_sub(impact)
            .ok_or(b"Price impact calculation error".to_vec())?
            .checked_mul(U256::from(10000u32))
            .ok_or(b"Basis points calculation overflow".to_vec())?
            .checked_div(expected_out)
            .ok_or(b"Basis points calculation division error".to_vec())?;

        Ok(price_impact)
    }
}
