/**
 * One name at a time, reused for the life of the page.
 *
 * Front coding means the shared prefix of the previous name is already in
 * place, so a step writes only the suffix — which is why these cannot be
 * allocated per declaration, and why [`room`] grows them by copying rather
 * than by replacing.
 *
 * Exported as `let` on purpose: the two readers ([`findNames`] and the search
 * walk) have to see the array [`room`] grew to, and ES module bindings are
 * live. A getter would be an extra call in the hottest loop on the page.
 */

export let scratch = new Uint8Array(512);
export let folded = new Uint8Array(512);

/** Make room for a name of `need` bytes, keeping what is already there. */
export function room(need: number): void {
  if (need <= scratch.length) return;
  let size = scratch.length;
  while (size < need) size *= 2;
  // Copied rather than replaced: the bytes already here are the prefix the
  // next name shares.
  const grownScratch = new Uint8Array(size);
  grownScratch.set(scratch);
  const grownFolded = new Uint8Array(size);
  grownFolded.set(folded);
  scratch = grownScratch;
  folded = grownFolded;
}
