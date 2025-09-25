// Time-Weighted Average Market Maker (TWAMM) Calculator
// This is a demonstration of the mathematical concepts behind TWAMM

/// Mathematical utilities for TWAMM calculations
pub struct TWAMMath;

impl TWAMMath {
    /// Calculate the TWAMM virtual AMM state after time t
    pub fn calculate_virtual_amm_state(
        initial_x: u128,
        initial_y: u128,
        sell_rate_x: u128,
        sell_rate_y: u128,
        time_blocks: u128,
    ) -> Result<(u128, u128), &'static str> {
        // Handle special cases
        if sell_rate_x == 0 && sell_rate_y == 0 {
            return Ok((initial_x, initial_y));
        }

        if sell_rate_x > 0 && sell_rate_y == 0 {
            // Unidirectional: X -> Y
            return Self::calculate_unidirectional_state(
                initial_x,
                initial_y,
                sell_rate_x,
                time_blocks,
                true,
            );
        }

        if sell_rate_y > 0 && sell_rate_x == 0 {
            // Unidirectional: Y -> X
            return Self::calculate_unidirectional_state(
                initial_y,
                initial_x,
                sell_rate_y,
                time_blocks,
                false,
            );
        }

        // Bidirectional case - use net flow calculation
        Self::calculate_bidirectional_state(
            initial_x,
            initial_y,
            sell_rate_x,
            sell_rate_y,
            time_blocks,
        )
    }

    /// Calculate unidirectional TWAMM state using closed-form solution
    fn calculate_unidirectional_state(
        reserve_in: u128,
        reserve_out: u128,
        sell_rate: u128,
        time_blocks: u128,
        is_x_to_y: bool,
    ) -> Result<(u128, u128), &'static str> {
        let k = reserve_in.checked_mul(reserve_out).ok_or("Overflow")?;

        // Calculate the total amount to sell
        let total_sell_amount = sell_rate.checked_mul(time_blocks).ok_or("Overflow")?;

        // Apply the TWAMM formula with time-weighting
        let time_factor = 8; // 0.8 as integer (simplified)
        let effective_amount_in = total_sell_amount
            .checked_mul(time_factor)
            .ok_or("Overflow")?
            .checked_div(10)
            .ok_or("Division by zero")?;

        let new_reserve_in = reserve_in
            .checked_add(effective_amount_in)
            .ok_or("Overflow")?;

        let new_reserve_out = k.checked_div(new_reserve_in).ok_or("Division by zero")?;

        if is_x_to_y {
            Ok((new_reserve_in, new_reserve_out))
        } else {
            Ok((new_reserve_out, new_reserve_in))
        }
    }

    /// Calculate bidirectional TWAMM state
    fn calculate_bidirectional_state(
        initial_x: u128,
        initial_y: u128,
        sell_rate_x: u128,
        sell_rate_y: u128,
        time_blocks: u128,
    ) -> Result<(u128, u128), &'static str> {
        // Calculate net selling rates
        let total_sell_x = sell_rate_x.checked_mul(time_blocks).ok_or("Overflow")?;
        let total_sell_y = sell_rate_y.checked_mul(time_blocks).ok_or("Overflow")?;

        // Get current price ratio
        let price_x_in_y = initial_y
            .checked_mul(1000)
            .ok_or("Overflow")?
            .checked_div(initial_x)
            .ok_or("Division by zero")?;
        let sell_x_value_in_y = total_sell_x
            .checked_mul(price_x_in_y)
            .ok_or("Overflow")?
            .checked_div(1000)
            .ok_or("Division by zero")?;

        if sell_x_value_in_y > total_sell_y {
            // Net flow X -> Y
            let net_sell_x = total_sell_x
                .checked_sub(
                    total_sell_y
                        .checked_mul(1000)
                        .ok_or("Overflow")?
                        .checked_div(price_x_in_y)
                        .ok_or("Division by zero")?,
                )
                .ok_or("Overflow")?;
            Self::calculate_unidirectional_state(
                initial_x,
                initial_y,
                net_sell_x
                    .checked_div(time_blocks)
                    .ok_or("Division by zero")?,
                time_blocks,
                true,
            )
        } else if total_sell_y > sell_x_value_in_y {
            // Net flow Y -> X
            let net_sell_y = total_sell_y
                .checked_sub(sell_x_value_in_y)
                .ok_or("Overflow")?;
            Self::calculate_unidirectional_state(
                initial_y,
                initial_x,
                net_sell_y
                    .checked_div(time_blocks)
                    .ok_or("Division by zero")?,
                time_blocks,
                false,
            )
        } else {
            // Balanced flows - no net change
            Ok((initial_x, initial_y))
        }
    }

    /// Calculate price impact given trade size and liquidity
    pub fn calculate_price_impact(
        trade_size: u128,
        reserve_in: u128,
        reserve_out: u128,
    ) -> Result<u128, &'static str> {
        if reserve_in == 0 || reserve_out == 0 {
            return Err("Invalid reserves");
        }

        let k = reserve_in.checked_mul(reserve_out).ok_or("Overflow")?;

        // Calculate new reserves after trade
        let new_reserve_in = reserve_in.checked_add(trade_size).ok_or("Overflow")?;
        let new_reserve_out = k.checked_div(new_reserve_in).ok_or("Division by zero")?;
        let amount_out = reserve_out.checked_sub(new_reserve_out).ok_or("Overflow")?;

        // Expected amount out without slippage
        let expected_out = trade_size
            .checked_mul(reserve_out)
            .ok_or("Overflow")?
            .checked_div(reserve_in)
            .ok_or("Division by zero")?;

        if expected_out <= amount_out {
            return Ok(0);
        }

        // Price impact as percentage (in basis points)
        let impact = expected_out
            .checked_sub(amount_out)
            .ok_or("Overflow")?
            .checked_mul(10000)
            .ok_or("Overflow")?
            .checked_div(expected_out)
            .ok_or("Division by zero")?;

        Ok(impact)
    }
}

/// TWAMM Calculator - Main contract logic
pub struct TWAMMCalculator {
    pub total_calculations: u128,
    pub total_volume_processed: u128,
    pub emergency_mode: bool,
}

impl TWAMMCalculator {
    /// Initialize the contract
    pub fn new() -> Self {
        Self {
            total_calculations: 0,
            total_volume_processed: 0,
            emergency_mode: false,
        }
    }

    /// Calculate virtual trades using TWAMM mathematical model
    pub fn calculate_virtual_trades(
        &mut self,
        sell_rate_0: u128,
        sell_rate_1: u128,
        blocks_elapsed: u128,
        reserve_0: u128,
        reserve_1: u128,
    ) -> Result<(u128, u128), &'static str> {
        // Input validation
        if blocks_elapsed == 0 {
            return Err("Invalid blocks elapsed");
        }

        if reserve_0 == 0 || reserve_1 == 0 {
            return Err("Invalid reserves");
        }

        // Emergency mode check
        if self.emergency_mode {
            return Err("Contract in emergency mode");
        }

        let result = if sell_rate_0 > 0 && sell_rate_1 == 0 {
            // Unidirectional: token0 -> token1
            self.calculate_unidirectional_trade(
                sell_rate_0,
                blocks_elapsed,
                reserve_0,
                reserve_1,
                true,
            )?
        } else if sell_rate_1 > 0 && sell_rate_0 == 0 {
            // Unidirectional: token1 -> token0
            self.calculate_unidirectional_trade(
                sell_rate_1,
                blocks_elapsed,
                reserve_0,
                reserve_1,
                false,
            )?
        } else if sell_rate_0 > 0 && sell_rate_1 > 0 {
            // Bidirectional trading
            self.calculate_bidirectional_trade(
                sell_rate_0,
                sell_rate_1,
                blocks_elapsed,
                reserve_0,
                reserve_1,
            )?
        } else {
            // No trading
            (0, 0)
        };

        // Update statistics
        self.update_statistics(result.0, result.1);

        Ok(result)
    }

    /// Calculate unidirectional TWAMM trade
    fn calculate_unidirectional_trade(
        &self,
        sell_rate: u128,
        blocks_elapsed: u128,
        reserve_0: u128,
        reserve_1: u128,
        zero_for_one: bool,
    ) -> Result<(u128, u128), &'static str> {
        let total_sell_amount = sell_rate.checked_mul(blocks_elapsed).ok_or("Overflow")?;

        if zero_for_one {
            // Selling token0 for token1
            let amount_out =
                self.calculate_twamm_out_given_in(total_sell_amount, reserve_0, reserve_1)?;
            Ok((total_sell_amount, amount_out))
        } else {
            // Selling token1 for token0
            let amount_out =
                self.calculate_twamm_out_given_in(total_sell_amount, reserve_1, reserve_0)?;
            Ok((amount_out, total_sell_amount))
        }
    }

    /// Calculate bidirectional TWAMM trade
    fn calculate_bidirectional_trade(
        &self,
        sell_rate_0: u128,
        sell_rate_1: u128,
        blocks_elapsed: u128,
        reserve_0: u128,
        reserve_1: u128,
    ) -> Result<(u128, u128), &'static str> {
        let total_sell_0 = sell_rate_0.checked_mul(blocks_elapsed).ok_or("Overflow")?;
        let total_sell_1 = sell_rate_1.checked_mul(blocks_elapsed).ok_or("Overflow")?;

        // Calculate net flow using TWAMM bidirectional formula
        let (net_amount_0, net_amount_1) =
            self.calculate_net_flows(total_sell_0, total_sell_1, reserve_0, reserve_1)?;

        Ok((net_amount_0, net_amount_1))
    }

    /// Core TWAMM calculation: amount out given amount in
    fn calculate_twamm_out_given_in(
        &self,
        amount_in: u128,
        reserve_in: u128,
        reserve_out: u128,
    ) -> Result<u128, &'static str> {
        if amount_in == 0 {
            return Ok(0);
        }

        // Use TWAMM formula with time-weighted execution
        let k = reserve_in.checked_mul(reserve_out).ok_or("Overflow")?;

        // Apply TWAMM time-weighting formula (simplified)
        let time_factor = 8; // 0.8 as integer
        let effective_amount_in = amount_in
            .checked_mul(time_factor)
            .ok_or("Overflow")?
            .checked_div(10)
            .ok_or("Division by zero")?;

        let new_reserve_in = reserve_in
            .checked_add(effective_amount_in)
            .ok_or("Overflow")?;

        let new_reserve_out = k.checked_div(new_reserve_in).ok_or("Division by zero")?;

        let amount_out = reserve_out.checked_sub(new_reserve_out).ok_or("Overflow")?;

        // Apply 0.3% fee
        let fee_adjusted_out = amount_out
            .checked_mul(997)
            .ok_or("Overflow")?
            .checked_div(1000)
            .ok_or("Division by zero")?;

        Ok(fee_adjusted_out)
    }

    /// Calculate net flows for bidirectional trading
    fn calculate_net_flows(
        &self,
        sell_0: u128,
        sell_1: u128,
        reserve_0: u128,
        reserve_1: u128,
    ) -> Result<(u128, u128), &'static str> {
        // Determine dominant direction
        let price_0_in_1 = reserve_1
            .checked_mul(1000)
            .ok_or("Overflow")?
            .checked_div(reserve_0)
            .ok_or("Division by zero")?;

        let sell_0_value_in_1 = sell_0
            .checked_mul(price_0_in_1)
            .ok_or("Overflow")?
            .checked_div(1000)
            .ok_or("Division by zero")?;

        if sell_0_value_in_1 > sell_1 {
            // Net flow: 0 -> 1
            let net_sell_value = sell_0_value_in_1.checked_sub(sell_1).ok_or("Overflow")?;
            let net_sell_0 = net_sell_value
                .checked_mul(1000)
                .ok_or("Overflow")?
                .checked_div(price_0_in_1)
                .ok_or("Division by zero")?;

            let amount_1_out =
                self.calculate_twamm_out_given_in(net_sell_0, reserve_0, reserve_1)?;
            Ok((net_sell_0, amount_1_out))
        } else if sell_1 > sell_0_value_in_1 {
            // Net flow: 1 -> 0
            let net_sell_1 = sell_1.checked_sub(sell_0_value_in_1).ok_or("Overflow")?;

            let amount_0_out =
                self.calculate_twamm_out_given_in(net_sell_1, reserve_1, reserve_0)?;
            Ok((amount_0_out, net_sell_1))
        } else {
            // Balanced flows cancel out
            Ok((0, 0))
        }
    }

    /// Update global statistics
    fn update_statistics(&mut self, amount_0: u128, amount_1: u128) {
        self.total_calculations += 1;
        self.total_volume_processed += amount_0 + amount_1;
    }

    /// Get global statistics
    pub fn get_global_stats(&self) -> (u128, u128) {
        (self.total_calculations, self.total_volume_processed)
    }

    /// Emergency functions
    pub fn set_emergency_mode(&mut self, enabled: bool) {
        self.emergency_mode = enabled;
    }

    /// Get version information
    pub fn get_version(&self) -> &'static str {
        "TWAMM Calculator v1.0.0"
    }

    /// Health check function
    pub fn health_check(&self) -> bool {
        !self.emergency_mode
    }

    /// Reset statistics
    pub fn reset_statistics(&mut self) {
        self.total_calculations = 0;
        self.total_volume_processed = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_twamm_calculation() {
        let mut calculator = TWAMMCalculator::new();

        // Test unidirectional trade
        let result = calculator.calculate_virtual_trades(
            1000,    // sell_rate_0
            0,       // sell_rate_1
            100,     // blocks_elapsed
            1000000, // reserve_0
            1000000, // reserve_1
        );

        assert!(result.is_ok());
        let (amount_0, amount_1) = result.unwrap();
        assert!(amount_0 > 0);
        assert!(amount_1 > 0);
    }

    #[test]
    fn test_price_impact_calculation() {
        let impact = TWAMMath::calculate_price_impact(
            1000,   // trade_size
            100000, // reserve_in
            100000, // reserve_out
        );

        assert!(impact.is_ok());
        let impact_value = impact.unwrap();
        assert!(impact_value < 10000); // Less than 100% impact
    }
}
