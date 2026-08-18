/-!
# ConsumerExtra

The fixture's **second** library root, and it is not in `defaultTargets`
(`lakefile.toml` says why). A `docs` script that asked Lake for one library, or
that leaned on `lake build`'s default targets, never reaches this module — so a
site that has no `ConsumerExtra` page is gate item 5 failing.
-/

/-- Present only when every `lean_lib` of the root package is documented. -/
def ConsumerExtra.marker : String := "second library root"
