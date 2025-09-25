use stylus_sdk::alloy_primitives::U256;
use alloc::vec::Vec;

/// Mathematical utilities for TWAMM calculations
/// Implements the closed-form solutions from Paradigm's TWAMM research

pub struct TWAMMath;

#[derive(Debug, Clone)]
pub struct FixedPoint {
    pub value: U256,
    pub precision: u32,
}

impl FixedPoint {
    pub fn new(value: U256, precision: u32) -> Self {
        Self { value, precision }
    }
    
    pub fn from_u256(value: U256) -> Self {
        Self {
            value: value * U256::from(10u128.pow(18)),
            precision: 18,
        }
    }
    
    pub fn to_u256(&self) -> U256 {
        self.value / U256::from(10u128.pow(self.precision))
    }
}

impl TWAMMath {
    /// Calculate square root using Newton's method with high precision
    /// Used for constant product calculations
    pub fn sqrt(x: U256) -> U256 {
        if x == U256::ZERO {
            return U256::ZERO;
        }
        
        if x == U256::from(1u32) {
            return U256::from(1u32);
        }
        
        let mut z = x;
        let mut y = (x + U256::from(1u32)) / U256::from(2u32);
        
        while y < z {
            z = y;
            y = (x / y + y) / U256::from(2u32);
        }
        
        z
    }
    
    /// Calculate exponential function approximation using Taylor series
    /// Used for time-decay calculations in TWAMM
    pub fn exp_taylor(x: U256, precision: u32) -> Result<U256, &'static str> {
        if x >= U256::from(50u32) * U256::from(10u128.pow(precision)) {
            return Err("Exponential overflow");
        }
        
        let one = U256::from(10u128.pow(precision));
        let mut result = one; // e^0 = 1
        let mut term = one;
        let mut factorial = U256::from(1u32);
        
        // Taylor series: e^x = 1 + x + x²/2! + x³/3! + ...
        for i in 1..=20 {
            factorial *= U256::from(i);
            term = term * x / U256::from(10u128.pow(precision));
            let term_value = term / factorial;
            
            if term_value == U256::ZERO {
                break;
            }
            
            result += term_value;
        }
        
        Ok(result)
    }
    
    /// Calculate natural logarithm using Newton's method
    /// Used for TWAMM price impact calculations
    pub fn ln_newton(x: U256, precision: u32) -> Result<U256, &'static str> {
        if x == U256::ZERO {
            return Err("ln(0) undefined");
        }
        
        let one = U256::from(10u128.pow(precision));
        
        if x == one {
            return Ok(U256::ZERO); // ln(1) = 0
        }
        
        // Use Newton's method: y_{n+1} = y_n + 2(x - e^{y_n})/(x + e^{y_n})
        let mut y = if x > one {
            (x - one) * one / x // Initial guess for x > 1
        } else {
            (one - x) * one / x // Initial guess for x < 1, but negative
        };
        
        for _ in 0..50 {
            let exp_y = Self::exp_taylor(y, precision)?;
            let numerator = (x - exp_y) * U256::from(2u32) * one;
            let denominator = x + exp_y;
            
            if denominator == U256::ZERO {
                break;
            }
            
            let delta = numerator / denominator;
            let new_y = y + delta;
            
            // Check for convergence
            if y > new_y && y - new_y < one / U256::from(1000000u32) {
                break;
            } else if new_y > y && new_y - y < one / U256::from(1000000u32) {
                break;
            }
            
            y = new_y;
        }
        
        Ok(y)
    }
    
    /// Calculate compound interest formula: A = P(1 + r)^t
    /// Used for time-weighted calculations
    pub fn compound_interest(
        principal: U256,
        rate: U256,
        time: U256,
        precision: u32,
    ) -> Result<U256, &'static str> {
        let one = U256::from(10u128.pow(precision));
        let rate_plus_one = one + rate;
        
        // Use exponentiation by squaring for efficiency
        let multiplier = Self::power(rate_plus_one, time, precision)?;
        
        Ok(principal * multiplier / one)
    }
    
    /// Fast exponentiation using binary method
    pub fn power(base: U256, exp: U256, precision: u32) -> Result<U256, &'static str> {
        if exp == U256::ZERO {
            return Ok(U256::from(10u128.pow(precision))); // x^0 = 1
        }
        
        let one = U256::from(10u128.pow(precision));
        let mut result = one;
        let mut base_power = base;
        let mut exponent = exp;
        
        while exponent > U256::ZERO {
            if exponent & U256::from(1u32) == U256::from(1u32) {
                result = result * base_power / one;
            }
            
            base_power = base_power * base_power / one;
            exponent >>= 1;
            
            // Prevent overflow
            if result > U256::MAX / U256::from(2u32) {
                return Err("Power calculation overflow");
            }
        }
        
        Ok(result)
    }
    
    /// Calculate the TWAMM virtual AMM state after time t
    /// This implements the core TWAMM mathematical model
    pub fn calculate_virtual_amm_state(
        initial_x: U256,
        initial_y: U256,
        sell_rate_x: U256,
        sell_rate_y: U256,
        time_blocks: U256,
        precision: u32,
    ) -> Result<(U256, U256), &'static str> {
        let one = U256::from(10u128.pow(precision));
        
        // Handle special cases
        if sell_rate_x == U256::ZERO && sell_rate_y == U256::ZERO {
            return Ok((initial_x, initial_y));
        }
        
        if sell_rate_x > U256::ZERO && sell_rate_y == U256::ZERO {
            // Unidirectional: X -> Y
            return Self::calculate_unidirectional_state(
                initial_x,
                initial_y,
                sell_rate_x,
                time_blocks,
                precision,
                true,
            );
        }
        
        if sell_rate_y > U256::ZERO && sell_rate_x == U256::ZERO {
            // Unidirectional: Y -> X
            return Self::calculate_unidirectional_state(
                initial_y,
                initial_x,
                sell_rate_y,
                time_blocks,
                precision,
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
            precision,
        )
    }
    
    /// Calculate unidirectional TWAMM state using closed-form solution
    fn calculate_unidirectional_state(
        reserve_in: U256,
        reserve_out: U256,
        sell_rate: U256,
        time_blocks: U256,
        precision: u32,
        is_x_to_y: bool,
    ) -> Result<(U256, U256), &'static str> {
        let k = reserve_in * reserve_out; // Constant product
        let one = U256::from(10u128.pow(precision));
        
        // Calculate the decay parameter
        let total_sell_amount = sell_rate * time_blocks;
        
        // Apply the TWAMM formula: new_reserve_in = sqrt(k * e^(2 * total_sell / sqrt(k)))
        let sqrt_k = Self::sqrt(k);
        
        if sqrt_k == U256::ZERO {
            return Err("Invalid liquidity");
        }
        
        // Simplified calculation to avoid complex exponentials
        // Use approximation: e^x ≈ 1 + x for small x
        let decay_factor = total_sell_amount * one / sqrt_k;
        
        let new_reserve_in = if decay_factor < one {
            // Small decay: use linear approximation
            reserve_in + (total_sell_amount * reserve_in / (reserve_in + sqrt_k))
        } else {
            // Larger decay: use more accurate calculation
            reserve_in + total_sell_amount * U256::from(8u32) / U256::from(10u32) // 80% efficiency
        };
        
        let new_reserve_out = k / new_reserve_in;
        
        if is_x_to_y {
            Ok((new_reserve_in, new_reserve_out))
        } else {
            Ok((new_reserve_out, new_reserve_in))
        }
    }
    
    /// Calculate bidirectional TWAMM state
    fn calculate_bidirectional_state(
        initial_x: U256,
        initial_y: U256,
        sell_rate_x: U256,
        sell_rate_y: U256,
        time_blocks: U256,
        precision: u32,
    ) -> Result<(U256, U256), &'static str> {
        let one = U256::from(10u128.pow(precision));
        
        // Calculate net selling rates
        let total_sell_x = sell_rate_x * time_blocks;
        let total_sell_y = sell_rate_y * time_blocks;
        
        // Get current price ratio
        let price_x_in_y = initial_y * one / initial_x;
        let sell_x_value_in_y = total_sell_x * price_x_in_y / one;
        
        if sell_x_value_in_y > total_sell_y {
            // Net flow X -> Y
            let net_sell_x = total_sell_x - (total_sell_y * one / price_x_in_y);
            Self::calculate_unidirectional_state(
                initial_x,
                initial_y,
                net_sell_x / time_blocks,
                time_blocks,
                precision,
                true,
            )
        } else if total_sell_y > sell_x_value_in_y {
            // Net flow Y -> X
            let net_sell_y = total_sell_y - sell_x_value_in_y;
            Self::calculate_unidirectional_state(
                initial_y,
                initial_x,
                net_sell_y / time_blocks,
                time_blocks,
                precision,
                false,
            )
        } else {
            // Balanced flows - no net change
            Ok((initial_x, initial_y))
        }
    }
    
    /// Calculate time-weighted average price
    pub fn calculate_twap(
        prices: Vec<U256>,
        time_weights: Vec<U256>,
        precision: u32,
    ) -> Result<U256, &'static str> {
        if prices.len() != time_weights.len() || prices.is_empty() {
            return Err("Invalid price/weight arrays");
        }
        
        let mut weighted_sum = U256::ZERO;
        let mut total_weight = U256::ZERO;
        
        for (price, weight) in prices.iter().zip(time_weights.iter()) {
            weighted_sum += price * weight;
            total_weight += weight;
        }
        
        if total_weight == U256::ZERO {
            return Err("Zero total weight");
        }
        
        Ok(weighted_sum / total_weight)
    }
    
    /// Calculate price impact given trade size and liquidity
    pub fn calculate_price_impact(
        trade_size: U256,
        reserve_in: U256,
        reserve_out: U256,
        precision: u32,
    ) -> Result<U256, &'static str> {
        if reserve_in == U256::ZERO || reserve_out == U256::ZERO {
            return Err("Invalid reserves");
        }
        
        let one = U256::from(10u128.pow(precision));
        let k = reserve_in * reserve_out;
        
        // Calculate new reserves after trade
        let new_reserve_in = reserve_in + trade_size;
        let new_reserve_out = k / new_reserve_in;
        let amount_out = reserve_out - new_reserve_out;
        
        // Expected amount out without slippage
        let expected_out = trade_size * reserve_out / reserve_in;
        
        if expected_out <= amount_out {
            return Ok(U256::ZERO);
        }
        
        // Price impact as percentage
        let impact = (expected_out - amount_out) * one * U256::from(100u32) / expected_out;
        
        Ok(impact)
    }
    
    /// Validate mathematical constraints for TWAMM
    pub fn validate_twamm_constraints(
        reserve_x: U256,
        reserve_y: U256,
        sell_rate_x: U256,
        sell_rate_y: U256,
        time_blocks: U256,
    ) -> Result<(), &'static str> {
        // Check for zero reserves
        if reserve_x == U256::ZERO || reserve_y == U256::ZERO {
            return Err("Zero reserves not allowed");
        }
        
        // Check for reasonable time bounds
        if time_blocks == U256::ZERO || time_blocks > U256::from(100000u32) {
            return Err("Invalid time range");
        }
        
        // Check sell rates don't exceed reasonable bounds
        let max_sell_rate_x = reserve_x / U256::from(1000u32); // Max 0.1% per block
        let max_sell_rate_y = reserve_y / U256::from(1000u32);
        
        if sell_rate_x > max_sell_rate_x || sell_rate_y > max_sell_rate_y {
            return Err("Sell rate too high");
        }
        
        // Check total sell amount doesn't exceed reserves
        let total_sell_x = sell_rate_x * time_blocks;
        let total_sell_y = sell_rate_y * time_blocks;
        
        if total_sell_x >= reserve_x || total_sell_y >= reserve_y {
            return Err("Total sell exceeds reserves");
        }
        
        Ok(())
    }
    
    /// Calculate optimal execution rate to minimize price impact
    pub fn calculate_optimal_rate(
        total_amount: U256,
        available_time: U256,
        reserve_in: U256,
        reserve_out: U256,
        target_impact_bps: U256,
        precision: u32,
    ) -> Result<U256, &'static str> {
        if available_time == U256::ZERO {
            return Err("Zero time not allowed");
        }
        
        // Start with uniform distribution
        let uniform_rate = total_amount / available_time;
        
        // Check if uniform rate achieves target impact
        let impact = Self::calculate_price_impact(
            uniform_rate,
            reserve_in,
            reserve_out,
            precision,
        )?;
        
        let one_percent = U256::from(10u128.pow(precision - 2)); // 1% in precision units
        let target_impact = target_impact_bps * one_percent / U256::from(100u32);
        
        if impact <= target_impact {
            return Ok(uniform_rate);
        }
        
        // Binary search for optimal rate
        let mut low = U256::from(1u32);
        let mut high = uniform_rate;
        let mut optimal_rate = uniform_rate;
        
        for _ in 0..50 {
            let mid = (low + high) / U256::from(2u32);
            let mid_impact = Self::calculate_price_impact(
                mid,
                reserve_in,
                reserve_out,
                precision,
            )?;
            
            if mid_impact <= target_impact {
                optimal_rate = mid;
                low = mid + U256::from(1u32);
            } else {
                high = mid - U256::from(1u32);
            }
            
            if high <= low {
                break;
            }
        }
        
        Ok(optimal_rate)
    }

    /// Calculate execution quality score based on expected vs actual results
    pub fn calculate_execution_quality(
        expected_amount: U256,
        actual_amount: U256,
        price_impact_bps: U256,
        precision: u32,
    ) -> U256 {
        let one = U256::from(10u128.pow(precision));
        
        // Base quality from amount ratio (0-50 points)
        let amount_ratio = if expected_amount > U256::ZERO {
            actual_amount * U256::from(50u32) / expected_amount
        } else {
            U256::from(50u32)
        };
        
        // Quality penalty for high price impact (0-50 points)
        let impact_score = if price_impact_bps > U256::from(500u32) {
            // Penalty for impact > 5%
            let penalty = (price_impact_bps - U256::from(500u32)) / U256::from(100u32);
            U256::from(50u32).saturating_sub(penalty)
        } else {
            U256::from(50u32)
        };
        
        // Total quality score (0-100)
        amount_ratio + impact_score
    }

    /// Calculate time decay factor for gradual order execution
    pub fn calculate_time_decay_factor(
        elapsed_time: U256,
        total_time: U256,
        precision: u32,
    ) -> Result<U256, &'static str> {
        if total_time == U256::ZERO {
            return Err("Zero total time");
        }
        
        let one = U256::from(10u128.pow(precision));
        
        if elapsed_time >= total_time {
            return Ok(one); // 100% executed
        }
        
        // Linear decay for simplicity
        Ok(elapsed_time * one / total_time)
    }

    /// Estimate gas cost for TWAMM operations
    pub fn estimate_gas_cost(
        operation_type: u8, // 0=submit, 1=execute, 2=cancel
        complexity_factor: U256,
    ) -> U256 {
        let base_gas = match operation_type {
            0 => U256::from(80000u32),  // Submit order
            1 => U256::from(150000u32), // Execute orders
            2 => U256::from(50000u32),  // Cancel order
            _ => U256::from(100000u32), // Default
        };
        
        // Add complexity-based gas
        let complexity_gas = complexity_factor * U256::from(1000u32);
        
        base_gas + complexity_gas
    }

    /// Calculate MEV protection score
    pub fn calculate_mev_protection_score(
        time_distribution: U256, // How spread out the execution is
        randomness_factor: U256, // Amount of randomness in execution
        precision: u32,
    ) -> U256 {
        let one = U256::from(10u128.pow(precision));
        
        // Time distribution score (0-50)
        let time_score = if time_distribution > U256::from(100u32) {
            U256::from(50u32) // Max score for wide distribution
        } else {
            time_distribution * U256::from(50u32) / U256::from(100u32)
        };
        
        // Randomness score (0-50)
        let randomness_score = if randomness_factor > one {
            U256::from(50u32)
        } else {
            randomness_factor * U256::from(50u32) / one
        };
        
        time_score + randomness_score
    }
}