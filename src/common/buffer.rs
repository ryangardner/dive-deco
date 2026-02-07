// Stage buffer abstraction for heapless/alloc compatibility
//
// This module provides a trait-based abstraction over Vec-like containers
// to enable compile-time selection between alloc::vec::Vec and heapless::Vec.

use super::deco::DecoStage;

/// Trait for Vec-like containers that store DecoStage items.
///
/// This abstraction allows the library to work with either:
/// - `alloc::vec::Vec<DecoStage>` (with `alloc` feature)
/// - `heapless::Vec<DecoStage, N>` (with `heapless` feature)
pub trait DecoStageContainer: Default + Clone {
    /// Push a stage to the container. Returns Err with the stage if full.
    fn push(&mut self, stage: DecoStage) -> Result<(), DecoStage>;
    
    /// Number of stages currently stored.
    fn len(&self) -> usize;
    
    /// Whether the container is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Mutable reference to the last stage, if any.
    fn last_mut(&mut self) -> Option<&mut DecoStage>;
    
    /// View stages as a slice.
    fn as_slice(&self) -> &[DecoStage];
    
    /// Number of slots remaining before the buffer is full.
    /// For alloc::Vec, returns usize::MAX (effectively unlimited).
    fn remaining_capacity(&self) -> usize;
    
    /// Clear all stages.
    fn clear(&mut self);
}

// ============ ALLOC IMPLEMENTATION ============

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

#[cfg(feature = "alloc")]
impl DecoStageContainer for Vec<DecoStage> {
    fn push(&mut self, stage: DecoStage) -> Result<(), DecoStage> {
        Vec::push(self, stage);
        Ok(()) // Vec never fails (OOM panics, not returns Err)
    }
    
    fn len(&self) -> usize {
        Vec::len(self)
    }
    
    fn last_mut(&mut self) -> Option<&mut DecoStage> {
        self.as_mut_slice().last_mut()
    }
    
    fn as_slice(&self) -> &[DecoStage] {
        self.as_slice()
    }
    
    fn remaining_capacity(&self) -> usize {
        usize::MAX // Effectively unlimited
    }
    
    fn clear(&mut self) {
        Vec::clear(self);
    }
}

// ============ HEAPLESS IMPLEMENTATION ============

#[cfg(feature = "heapless")]
use heapless::Vec as HVec;

/// Default buffer size for heapless builds.
/// 128 stages covers 99% of recreational and tech dives.
/// For extreme dives (21+ hour TTS), consider 192 or 256.
#[cfg(feature = "heapless")]
pub const MAX_DECO_STAGES: usize = 128;

#[cfg(feature = "heapless")]
impl DecoStageContainer for HVec<DecoStage, MAX_DECO_STAGES> {
    fn push(&mut self, stage: DecoStage) -> Result<(), DecoStage> {
        HVec::push(self, stage)
    }
    
    fn len(&self) -> usize {
        // Use fully qualified syntax to avoid recursion
        self.as_slice().len()
    }
    
    fn last_mut(&mut self) -> Option<&mut DecoStage> {
        self.as_mut_slice().last_mut()
    }
    
    fn as_slice(&self) -> &[DecoStage] {
        heapless::Vec::as_slice(self)
    }
    
    fn remaining_capacity(&self) -> usize {
        MAX_DECO_STAGES.saturating_sub(self.len())
    }
    
    fn clear(&mut self) {
        HVec::clear(self);
    }
}

// ============ TYPE ALIASES ============

/// Default stage container based on enabled features.
/// 
/// - With `alloc` feature: uses `alloc::vec::Vec<DecoStage>`
/// - With `heapless` feature: uses `heapless::Vec<DecoStage, 128>`
#[cfg(feature = "alloc")]
pub type DefaultStageContainer = Vec<DecoStage>;

#[cfg(all(feature = "heapless", not(feature = "alloc")))]
pub type DefaultStageContainer = HVec<DecoStage, MAX_DECO_STAGES>;


