// Why does render.ts's IR read cost more than benchmarks/tools/read-ir.ts's?
// The one structural difference is that render.ts KEEPS every parsed module
// (`modules.push(mod)`) while read-ir.ts tallies and drops it. This isolates
// that: same files, same parser, one fresh process per measurement, the only
// difference being whether the parsed object stays reachable.
//   usage: read-retain-probe.ts <ir> retain|drop
const IR = Deno.args[0];
const RETAIN = Deno.args[1] === "retain";
const t0 = performance.now();
const index = JSON.parse(await Deno.readTextFile(`${IR}/index.json`));
const kept: unknown[] = [];
let touched = 0;
for (const dep of index.dependencyMaps) {
  const m = JSON.parse(await Deno.readTextFile(`${IR}/${dep.file}`));
  touched += Object.keys(m.declarations).length;
  if (RETAIN) kept.push(m);
}
for (const e of index.modules) {
  const mod = JSON.parse(await Deno.readTextFile(`${IR}/${e.file}`));
  touched += mod.declarations.length;
  if (RETAIN) kept.push(mod);
}
const t1 = performance.now();
console.log(`${((t1 - t0) / 1000).toFixed(4)} ${touched} ${kept.length}`);
