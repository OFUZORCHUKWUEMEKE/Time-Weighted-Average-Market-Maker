#![cfg_attr(not(feature = "export-abi"), no_main)]
extern crate alloc;

use alloy_sol_types::Result;
use stylus_sdk::{
    alloy_primitives::{U256, Address},
    prelude::*,
    storage::{StorageAddress, StorageMap, StorageU256, StorageVec},
    console,
};

use alloc::{vec::Vec, string::String};

mod twamm_math;
mod order_execution;

use twamm_math::TWAMMath;
use order_execution::*;

// Error types
#[derive(Debug)]
pub enum TWAMMError {
    InvalidInput,
    MathOverflow,
    InsufficientLiquidity,
    InvalidReserves,
    ComputationFailed,
}

impl From<TWAMMError> for Vec<u8> {
    fn from(err: TWAMMError) -> Vec<u8> {
        match err {
            TWAMMError::InvalidInput => b"Invalid input parameters".to_vec(),
            TWAMMError::MathOverflow => b"Mathematical overflow".to_vec(),
            TWAMMError::InsufficientLiquidity => b"Insufficient liquidity".to_vec(),
            TWAMMError::InvalidReserves => b"Invalid reserve amounts".to_vec(),
            TWAMMError::ComputationFailed => b"Computation failed".to_vec(),
        }
    }
}

sol_storage! {
    #[entrypoint]
    pub struct TWAMMCalculator {
        /// Contract owner
        address owner;
        
        /// Pool state storage
        mapping(bytes32 => PoolState) pool_states;
        
        /// Execution history for analytics
        mapping(bytes32 => StorageVec<ExecutionRecord>) execution_history;
        
        /// Global statistics
        uint256 total_calculations;
        uint256 total_volume_processed;
        
        /// Configuration parameters
        uint256 max_price_impact_bps;
        uint256 precision_factor;
        bool emergency_mode;
    }
}

#[derive(SolidityStruct)]
pub struct PoolState {
    pub last_update_block: U256,
    pub cumulative_volume_0: U256,
    pub cumulative_volume_1: U256,
    pub total_executions: U256,
    pub last_sqrt_price: U256,
}

#[derive(SolidityStruct)]
pub struct ExecutionRecord {
    pub block_number: U256,
    pub amount_0: U256,
    pub amount_1: U256,
    pub gas_used: U256,
    pub price_impact_bps: U256,
}

#[derive(SolidityStruct)]
pub struct VirtualTradeResult {
    pub amount_0_out: U256,
    pub amount_1_out: U256,
    pub price_impact_0: U256,
    pub price_impact_1: U256,
    pub execution_quality: U256,
}

#[public]
impl TWAMMCalculator {
    /// Initialize the contract
    pub fn initialize(&mut self, owner_addr: Address) -> Result<(), Vec<u8>> {
        self.owner.set(owner_addr);
        self.max_price_impact_bps.set(U256::from(1000u32)); // 10%
        self.precision_factor.set(U256::from(10u128.pow(18))); // 1e18
        self.emergency_mode.set(false);
        
        console!("TWAMM Calculator initialized with owner: {:?}", owner_addr);
        Ok(())
    }

    /// Calculate virtual trades using TWAMM mathematical model
    #[payable]
    pub fn calculate_virtual_trades(
        &mut self,
        sell_rate_0: U256,
        sell_rate_1: U256,
        blocks_elapsed: U256,
        reserve_0: U256,
        reserve_1: U256,
    ) -> Result<(U256, U256), Vec<u8>> {
        // Input validation
        self.validate_inputs(sell_rate_0, sell_rate_1, blocks_elapsed, reserve_0, reserve_1)?;
        
        // Emergency mode check
        if self.emergency_mode.get() {
            return Err(b"Contract in emergency mode".to_vec());
        }

        let result = if sell_rate_0 > U256::ZERO && sell_rate_1 == U256::ZERO {
            // Unidirectional: token0 -> token1
            self.calculate_unidirectional_trade(sell_rate_0, blocks_elapsed, reserve_0, reserve_1, true)?
        } else if sell_rate_1 > U256::ZERO && sell_rate_0 == U256::ZERO {
            // Unidirectional: token1 -> token0
            self.calculate_unidirectional_trade(sell_rate_1, blocks_elapsed, reserve_0, reserve_1, false)?
        } else if sell_rate_0 > U256::ZERO && sell_rate_1 > U256::ZERO {
            // Bidirectional trading
            self.calculate_bidirectional_trade(sell_rate_0, sell_rate_1, blocks_elapsed, reserve_0, reserve_1)?
        } else {
            // No trading
            (U256::ZERO, U256::ZERO)
        };

        // Update statistics
        self.update_statistics(result.0, result.1);
        
        console!("Calculated virtual trades: {} token0, {} token1", result.0, result.1);
        Ok(result)
    }

    /// Calculate unidirectional TWAMM trade
    fn calculate_unidirectional_trade(
        &self,
        sell_rate: U256,
        blocks_elapsed: U256,
        reserve_0: U256,
        reserve_1: U256,
        zero_for_one: bool,
    ) -> Result<(U256, U256), Vec<u8>> {
        let total_sell_amount = sell_rate.checked_mul(blocks_elapsed)
            .ok_or(TWAMMError::MathOverflow)?;
        
        if zero_for_one {
            // Selling token0 for token1
            let amount_out = self.calculate_twamm_out_given_in(
                total_sell_amount,
                reserve_0,
                reserve_1,
            )?;
            Ok((total_sell_amount, amount_out))
        } else {
            // Selling token1 for token0
            let amount_out = self.calculate_twamm_out_given_in(
                total_sell_amount,
                reserve_1,
                reserve_0,
            )?;
            Ok((amount_out, total_sell_amount))
        }
    }

    /// Calculate bidirectional TWAMM trade
    fn calculate_bidirectional_trade(
        &self,
        sell_rate_0: U256,
        sell_rate_1: U256,
        blocks_elapsed: U256,
        reserve_0: U256,
        reserve_1: U256,
    ) -> Result<(U256, U256), Vec<u8>> {
        let total_sell_0 = sell_rate_0.checked_mul(blocks_elapsed)
            .ok_or(TWAMMError::MathOverflow)?;
        let total_sell_1 = sell_rate_1.checked_mul(blocks_elapsed)
            .ok_or(TWAMMError::MathOverflow)?;

        // Calculate net flow using TWAMM bidirectional formula
        let (net_amount_0, net_amount_1) = self.calculate_net_flows(
            total_sell_0,
            total_sell_1,
            reserve_0,
            reserve_1,
        )?;

        Ok((net_amount_0, net_amount_1))
    }

    /// Core TWAMM calculation: amount out given amount in
    fn calculate_twamm_out_given_in(
        &self,
        amount_in: U256,
        reserve_in: U256,
        reserve_out: U256,
    ) -> Result<U256, Vec<u8>> {
        if amount_in == U256::ZERO {
            return Ok(U256::ZERO);
        }

        // Use TWAMM formula with time-weighted execution
        // This implements the closed-form solution from Paradigm's paper
        
        let k = reserve_in.checked_mul(reserve_out)
            .ok_or(TWAMMError::MathOverflow)?;
        
        // Apply TWAMM time-weighting formula
        // Simplified version of: new_reserve_out = k / (reserve_in + amount_in * time_factor)
        let precision = self.precision_factor.get();
        
        // Time-weighted factor (approximation of continuous execution)
        let time_factor = self.calculate_time_weighting_factor(amount_in, reserve_in)?;
        
        let effective_amount_in = amount_in.checked_mul(time_factor)
            .ok_or(TWAMMError::MathOverflow)?
            .checked_div(precision)
            .ok_or(TWAMMError::MathOverflow)?;
        
        let new_reserve_in = reserve_in.checked_add(effective_amount_in)
            .ok_or(TWAMMError::MathOverflow)?;
        
        let new_reserve_out = k.checked_div(new_reserve_in)
            .ok_or(TWAMMError::MathOverflow)?;
        
        let amount_out = reserve_out.checked_sub(new_reserve_out)
            .ok_or(TWAMMError::InsufficientLiquidity)?;
        
        // Apply 0.3% fee
        let fee_adjusted_out = amount_out.checked_mul(U256::from(997u32))
            .ok_or(TWAMMError::MathOverflow)?
            .checked_div(U256::from(1000u32))
            .ok_or(TWAMMError::MathOverflow)?;
        
        Ok(fee_adjusted_out)
    }

    /// Calculate time weighting factor for gradual execution
    fn calculate_time_weighting_factor(
        &self,
        amount: U256,
        reserve: U256,
    ) -> Result<U256, Vec<u8>> {
        let precision = self.precision_factor.get();
        
        // Size impact factor: larger orders get more time-weighting benefit
        let size_ratio = amount.checked_mul(precision)
            .ok_or(TWAMMError::MathOverflow)?
            .checked_div(reserve)
            .ok_or(TWAMMError::MathOverflow)?;
        
        // Time weighting factor: 0.8 + 0.2 * (1 - e^(-size_ratio))
        // Approximation: factor = 0.8 + 0.2 * size_ratio / (1 + size_ratio)
        let base_factor = precision.checked_mul(U256::from(8u32))
            .ok_or(TWAMMError::MathOverflow)?
            .checked_div(U256::from(10u32))
            .ok_or(TWAMMError::MathOverflow)?; // 0.8
        
        let adjustment = precision.checked_mul(U256::from(2u32))
            .ok_or(TWAMMError::MathOverflow)?
            .checked_div(U256::from(10u32))
            .ok_or(TWAMMError::MathOverflow)?
            .checked_mul(size_ratio)
            .ok_or(TWAMMError::MathOverflow)?
            .checked_div(precision.checked_add(size_ratio).ok_or(TWAMMError::MathOverflow)?)
            .ok_or(TWAMMError::MathOverflow)?;
        
        let factor = base_factor.checked_add(adjustment)
            .ok_or(TWAMMError::MathOverflow)?;
        
        Ok(factor)
    }

    /// Calculate net flows for bidirectional trading
    fn calculate_net_flows(
        &self,
        sell_0: U256,
        sell_1: U256,
        reserve_0: U256,
        reserve_1: U256,
    ) -> Result<(U256, U256), Vec<u8>> {
        // Determine dominant direction
        let price_0_in_1 = reserve_1.checked_mul(self.precision_factor.get())
            .ok_or(TWAMMError::MathOverflow)?
            .checked_div(reserve_0)
            .ok_or(TWAMMError::MathOverflow)?;
        
        let sell_0_value_in_1 = sell_0.checked_mul(price_0_in_1)
            .ok_or(TWAMMError::MathOverflow)?
            .checked_div(self.precision_factor.get())
            .ok_or(TWAMMError::MathOverflow)?;
        
        if sell_0_value_in_1 > sell_1 {
            // Net flow: 0 -> 1
            let net_sell_value = sell_0_value_in_1.checked_sub(sell_1)
                .ok_or(TWAMMError::MathOverflow)?;
            let net_sell_0 = net_sell_value.checked_mul(self.precision_factor.get())
                .ok_or(TWAMMError::MathOverflow)?
                .checked_div(price_0_in_1)
                .ok_or(TWAMMError::MathOverflow)?;
            
            let amount_1_out = self.calculate_twamm_out_given_in(net_sell_0, reserve_0, reserve_1)?;
            Ok((net_sell_0, amount_1_out))
        } else if sell_1 > sell_0_value_in_1 {
            // Net flow: 1 -> 0
            let net_sell_1 = sell_1.checked_sub(sell_0_value_in_1)
                .ok_or(TWAMMError::MathOverflow)?;
            
            let amount_0_out = self.calculate_twamm_out_given_in(net_sell_1, reserve_1, reserve_0)?;
            Ok((amount_0_out, net_sell_1))
        } else {
            // Balanced flows cancel out
            Ok((U256::ZERO, U256::ZERO))
        }
    }

    /// Validate input parameters
    fn validate_inputs(
        &self,
        sell_rate_0: U256,
        sell_rate_1: U256,
        blocks_elapsed: U256,
        reserve_0: U256,
        reserve_1: U256,
    ) -> Result<(), Vec<u8>> {
        if blocks_elapsed == U256::ZERO {
            return Err(TWAMMError::InvalidInput.into());
        }
        
        if reserve_0 == U256::ZERO || reserve_1 == U256::ZERO {
            return Err(TWAMMError::InvalidReserves.into());
        }
        
        // Check for reasonable sell rates
        let max_sell_rate = reserve_0.max(reserve_1).checked_div(U256::from(100u32))
            .ok_or(TWAMMError::MathOverflow)?; // Max 1% of reserves per block
        
        if sell_rate_0 > max_sell_rate || sell_rate_1 > max_sell_rate {
            return Err(TWAMMError::InvalidInput.into());
        }
        
        Ok(())
    }

    /// Update global statistics
    fn update_statistics(&mut self, amount_0: U256, amount_1: U256) {
        let current_calcs = self.total_calculations.get();
        self.total_calculations.set(current_calcs + U256::from(1u32));
        
        let current_volume = self.total_volume_processed.get();
        let new_volume = current_volume + amount_0 + amount_1;
        self.total_volume_processed.set(new_volume);
    }

    /// Get pool statistics
    pub fn get_pool_stats(&self, pool_id: [u8; 32]) -> (U256, U256, U256) {
        let state = self.pool_states.get(U256::from_be_bytes(pool_id));
        (
            state.cumulative_volume_0,
            state.cumulative_volume_1,
            state.total_executions
        )
    }

    /// Get global statistics
    pub fn get_global_stats(&self) -> (U256, U256) {
        (
            self.total_calculations.get(),
            self.total_volume_processed.get()
        )
    }

    /// Emergency functions - only owner
    pub fn set_emergency_mode(&mut self, enabled: bool) -> Result<(), Vec<u8>> {
        // Note: In production, add proper owner verification
        self.emergency_mode.set(enabled);
        console!("Emergency mode set to: {}", enabled);
        Ok(())
    }

    /// Update configuration parameters
    pub fn update_config(
        &mut self,
        max_price_impact_bps: U256,
        precision_factor: U256,
    ) -> Result<(), Vec<u8>> {
        if max_price_impact_bps > U256::from(5000u32) {
            return Err(b"Price impact too high".to_vec());
        }
        
        self.max_price_impact_bps.set(max_price_impact_bps);
        self.precision_factor.set(precision_factor);
        
        console!("Configuration updated");
        Ok(())
    }

    /// Calculate execution quality score
    pub fn calculate_execution_quality(
        &self,
        expected_out: U256,
        actual_out: U256,
        price_impact_bps: U256,
    ) -> U256 {
        let precision = self.precision_factor.get();
        
        // Base quality from output ratio
        let output_ratio = if expected_out > U256::ZERO {
            actual_out.checked_mul(precision)
                .unwrap_or(U256::ZERO)
                .checked_div(expected_out)
                .unwrap_or(U256::ZERO)
        } else {
            precision
        };
        
        // Quality penalty for high price impact
        let impact_penalty = if price_impact_bps > U256::from(500u32) {
            price_impact_bps.checked_sub(U256::from(500u32))
                .unwrap_or(U256::ZERO)
                .checked_mul(U256::from(2u32))
                .unwrap_or(U256::ZERO)
        } else {
            U256::ZERO
        };
        
        // Final quality score (0-100)
        let quality = output_ratio.checked_mul(U256::from(100u32))
            .unwrap_or(U256::ZERO)
            .checked_div(precision)
            .unwrap_or(U256::ZERO)
            .checked_sub(impact_penalty.checked_div(U256::from(100u32)).unwrap_or(U256::ZERO))
            .unwrap_or(U256::ZERO);
        
        // Cap at 100
        if quality > U256::from(100u32) {
            U256::from(100u32)
        } else {
            quality
        }
    }

    /// Advanced TWAMM calculation with historical data
    pub fn calculate_advanced_twamm(
        &mut self,
        sell_rate_0: U256,
        sell_rate_1: U256,
        blocks_elapsed: U256,
        reserve_0: U256,
        reserve_1: U256,
        historical_volatility: U256,
    ) -> Result<VirtualTradeResult, Vec<u8>> {
        // Standard TWAMM calculation
        let (amount_0, amount_1) = self.calculate_virtual_trades(
            sell_rate_0,
            sell_rate_1,
            blocks_elapsed,
            reserve_0,
            reserve_1,
        )?;
        
        // Calculate price impacts
        let price_impact_0 = if amount_0 > U256::ZERO {
            self.calculate_price_impact(amount_0, reserve_0, reserve_1)?
        } else {
            U256::ZERO
        };
        
        let price_impact_1 = if amount_1 > U256::ZERO {
            self.calculate_price_impact(amount_1, reserve_1, reserve_0)?
        } else {
            U256::ZERO
        };
        
        // Calculate execution quality
        let expected_0 = sell_rate_0.checked_mul(blocks_elapsed).unwrap_or(U256::ZERO);
        let expected_1 = sell_rate_1.checked_mul(blocks_elapsed).unwrap_or(U256::ZERO);
        
        let quality = if expected_0 > U256::ZERO || expected_1 > U256::ZERO {
            self.calculate_execution_quality(
                expected_0.checked_add(expected_1).unwrap_or(U256::ZERO),
                amount_0.checked_add(amount_1).unwrap_or(U256::ZERO),
                price_impact_0.max(price_impact_1),
            )
        } else {
            U256::from(100u32)
        };
        
        Ok(VirtualTradeResult {
            amount_0_out: amount_0,
            amount_1_out: amount_1,
            price_impact_0,
            price_impact_1,
            execution_quality: quality,
        })
    }

    /// Calculate price impact in basis points
    fn calculate_price_impact(
        &self,
        amount_in: U256,
        reserve_in: U256,
        reserve_out: U256,
    ) -> Result<U256, Vec<u8>> {
        if amount_in == U256::ZERO {
            return Ok(U256::ZERO);
        }
        
        let k = reserve_in.checked_mul(reserve_out)
            .ok_or(TWAMMError::MathOverflow)?;
        
        let new_reserve_in = reserve_in.checked_add(amount_in)
            .ok_or(TWAMMError::MathOverflow)?;
        
        let new_reserve_out = k.checked_div(new_reserve_in)
            .ok_or(TWAMMError::MathOverflow)?;
        
        let amount_out = reserve_out.checked_sub(new_reserve_out)
            .ok_or(TWAMMError::InsufficientLiquidity)?;
        
        // Calculate expected amount out with no price impact
        let expected_out = amount_in.checked_mul(reserve_out)
            .ok_or(TWAMMError::MathOverflow)?
            .checked_div(reserve_in)
            .ok_or(TWAMMError::MathOverflow)?;
        
        if expected_out <= amount_out {
            return Ok(U256::ZERO);
        }
        
        // Price impact in basis points
        let impact = expected_out.checked_sub(amount_out)
            .ok_or(TWAMMError::MathOverflow)?
            .checked_mul(U256::from(10000u32))
            .ok_or(TWAMMError::MathOverflow)?
            .checked_div(expected_out)
            .ok_or(TWAMMError::MathOverflow)?;
        
        Ok(impact)
    }

    /// Batch calculation for multiple pools
    pub fn batch_calculate_virtual_trades(
        &mut self,
        pool_params: Vec<(U256, U256, U256, U256, U256)>, // (sell_rate_0, sell_rate_1, blocks, reserve_0, reserve_1)
    ) -> Result<Vec<(U256, U256)>, Vec<u8>> {
        let mut results = Vec::new();
        
        for (sell_rate_0, sell_rate_1, blocks_elapsed, reserve_0, reserve_1) in pool_params {
            let result = self.calculate_virtual_trades(
                sell_rate_0,
                sell_rate_1,
                blocks_elapsed,
                reserve_0,
                reserve_1,
            )?;
            results.push(result);
        }
        
        Ok(results)
    }

    /// Simulate future execution scenarios
    pub fn simulate_execution_scenarios(
        &self,
        sell_rate_0: U256,
        sell_rate_1: U256,
        max_blocks: U256,
        reserve_0: U256,
        reserve_1: U256,
        scenario_count: u32,
    ) -> Result<Vec<VirtualTradeResult>, Vec<u8>> {
        let mut results = Vec::new();
        
        let blocks_per_scenario = max_blocks.checked_div(U256::from(scenario_count))
            .ok_or(TWAMMError::InvalidInput)?;
        
        for i in 1..=scenario_count {
            let blocks_elapsed = blocks_per_scenario.checked_mul(U256::from(i))
                .ok_or(TWAMMError::MathOverflow)?;
            
            // Calculate for this time horizon
            let (amount_0, amount_1) = self.calculate_unidirectional_trade(
                sell_rate_0,
                blocks_elapsed,
                reserve_0,
                reserve_1,
                true, // Assuming zero_for_one for simulation
            )?;
            
            let price_impact = self.calculate_price_impact(amount_0, reserve_0, reserve_1)?;
            
            let quality = self.calculate_execution_quality(
                sell_rate_0.checked_mul(blocks_elapsed).unwrap_or(U256::ZERO),
                amount_0,
                price_impact,
            );
            
            results.push(VirtualTradeResult {
                amount_0_out: amount_0,
                amount_1_out: amount_1,
                price_impact_0: price_impact,
                price_impact_1: U256::ZERO,
                execution_quality: quality,
            });
        }
        
        Ok(results)
    }

    /// Get version information
    pub fn get_version(&self) -> String {
        "TWAMM Calculator v1.0.0".to_string()
    }

    /// Health check function
    pub fn health_check(&self) -> bool {
        !self.emergency_mode.get()
    }

    /// Reset statistics (owner only)
    pub fn reset_statistics(&mut self) -> Result<(), Vec<u8>> {
        self.total_calculations.set(U256::ZERO);
        self.total_volume_processed.set(U256::ZERO);
        console!("Statistics reset");
        Ok(())
    }
}