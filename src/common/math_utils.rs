//! Math utilities for std/no-std compatibility

#[cfg(feature = "no-std")]
use libm;

/// Absolute value for f32
#[inline]
pub fn abs(val: f32) -> f32 {
    #[cfg(not(feature = "no-std"))]
    {
        val.abs()
    }
    #[cfg(feature = "no-std")]
    {
        libm::fabsf(val)
    }
}

/// Ceiling function for f32
#[inline]
pub fn ceil(val: f32) -> f32 {
    #[cfg(not(feature = "no-std"))]
    {
        val.ceil()
    }
    #[cfg(feature = "no-std")]
    {
        libm::ceilf(val)
    }
}

/// Power function for f32
#[inline]
pub fn powf(base: f32, exp: f32) -> f32 {
    #[cfg(not(feature = "no-std"))]
    {
        base.powf(exp)
    }
    #[cfg(feature = "no-std")]
    {
        libm::powf(base, exp)
    }
}

/// Simple e^x wrapper for f32
#[inline]
pub fn exp(exponent: f32) -> f32 {
    #[cfg(not(feature = "no-std"))]
    {
        exponent.exp()
    }
    #[cfg(feature = "no-std")]
    {
        libm::expf(exponent)
    }
}

/// Simple ln(x) wrapper for f32
#[inline]
pub fn ln(val: f32) -> f32 {
    #[cfg(not(feature = "no-std"))]
    {
        val.ln()
    }
    #[cfg(feature = "no-std")]
    {
        libm::logf(val)
    }
}

/// Round function for f32
#[inline]
pub fn round(val: f32) -> f32 {
    #[cfg(not(feature = "no-std"))]
    {
        val.round()
    }
    #[cfg(feature = "no-std")]
    {
        libm::roundf(val)
    }
}