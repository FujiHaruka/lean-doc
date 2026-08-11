//! Whole-site artifacts: declaration data, name map, navigation, search index.
//!
//! Filled in by milestone **M2** — see `docs/implementation-plan.md`.
//!
//! This is the cheapest of the three incremental layers and the widest net:
//! instance lists are not in any page's bytes, the browser fills them from
//! `declaration-data.bmp`, so a moved instance only shows up correctly
//! because these artifacts get rebuilt.
//!
//! The cache key discipline comes from the prototype and is worth keeping:
//! a version string for the cache format and another for the derivation rule,
//! either of which invalidates everything when bumped, plus a test that
//! deliberately staleness-checks them.
