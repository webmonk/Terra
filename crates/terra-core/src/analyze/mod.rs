//! Terrain analysis masks and CPU erosion references.

mod erosion;
mod level_step;
mod metrics;

pub use erosion::{hydraulic_erode, thermal_erode, HydraulicResult};
pub use level_step::{
    default_sim_levels, downsample_height, draft_sim_levels, hydraulic_erode_leveled,
    thermal_erode_leveled, SimLevel,
};
pub use metrics::*;
