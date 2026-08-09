//! Cache and quality policies for layers and groups.

use serde::{Deserialize, Serialize};

/// How an expensive layer or group retains results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CachePolicy {
    /// Automatically recompute after dependencies change.
    #[default]
    Live,
    /// Reuse results until invalidated.
    Cached,
    /// Mark stale but wait for the user to refresh.
    Manual,
    /// Keep showing the current result even when dependencies change (stale badge).
    Frozen,
    /// Convert into static terrain fields / imported-data behaviour.
    Baked,
}

impl CachePolicy {
    pub fn label(self) -> &'static str {
        match self {
            CachePolicy::Live => "Live",
            CachePolicy::Cached => "Cached",
            CachePolicy::Manual => "Manual",
            CachePolicy::Frozen => "Frozen",
            CachePolicy::Baked => "Baked",
        }
    }

    /// Map legacy `LayerCommon.cached` bool.
    pub fn from_legacy_cached(cached: bool) -> Self {
        if cached {
            CachePolicy::Baked
        } else {
            CachePolicy::Live
        }
    }

    pub fn to_legacy_cached(self) -> bool {
        matches!(self, CachePolicy::Baked | CachePolicy::Cached)
    }
}

/// Runtime freshness of a cached layer output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CacheState {
    #[default]
    Fresh,
    Stale,
    Computing,
    FrozenStale,
    Baked,
}

impl CacheState {
    pub fn label(self) -> &'static str {
        match self {
            CacheState::Fresh => "Fresh",
            CacheState::Stale => "Stale",
            CacheState::Computing => "Computing",
            CacheState::FrozenStale => "Frozen (stale)",
            CacheState::Baked => "Baked",
        }
    }

    pub fn is_visually_current(self) -> bool {
        matches!(
            self,
            CacheState::Fresh | CacheState::Baked | CacheState::Computing
        )
    }
}
