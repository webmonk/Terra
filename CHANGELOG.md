# Changelog

All notable changes to Terra will be documented in this file.

## [Unreleased]

### Added
- Phase 11 geological integration: shared typed AuxMaps (bedrock / sediment / soil /
  lithology), `ScaleBand::MultiScale` + `LayerKind::scale_band`, field invalidation
  graph with iterative guard, `TerrainStatistics`, `LandscapeStyle` presets, and
  realism benchmark worlds (Alpine, Desert Mesa, Badlands, Old/Young Mountains,
  Dune Field, Coastal)
- Rebuilt New World templates from scratch via `WorldTemplate` + shared
  cause→effect process chain (materials → evolution → hydro → meso → detail)
- MIT licensing and CONTRIBUTING guide
- Open-source packaging pass (README accuracy, user-facing docs for workflow / creating terrain)
- Drainage-conditioned multi-scale terrain amplification (`analyze::amplify_terrain` /
  Geomorphic Detail): macro/meso/micro metre bands, cascaded flow-aligned patterns,
  outputs for fine flow / micro-channel / ridge breakup / fine erosion

### Changed
- Split `terra-app` shell into lifecycle / eval / project / paint / shapes / actions modules
- Split UI `PanelAction` into `terra-ui::actions`; inspector and hierarchy live under module directories
- Split `LayerKind` param families under `terra-core::layer::kinds`
- Split `TerrainDocument` into document / migrate / session modules
- Shared geometry helpers in `terra-core::mask::geom`
- Layer type registry documented as catalog source of truth; tool catalog points contributors there
- Geomorphic Detail upgraded from isotropic-aligned polish to anti-soup structured amplification
