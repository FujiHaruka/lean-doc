/*
 * olean-evict — drop the page-cache pages of the named files, without sudo.
 *
 * Reads newline-separated file paths from stdin and, for each one, maps the file
 * read-only and calls msync(MS_INVALIDATE), which discards the clean file pages
 * held for it in the unified buffer cache. `purge(8)` needs root; this does not,
 * and unlike `purge` it is *surgical* — it touches only the files it is given, so
 * the Lean binary, the extractor, dylibs and directory metadata stay warm.
 *
 * That is the point: pairing this with olean-residency isolates the cost of
 * paging in olean bytes from every other cold-start effect, and the state it
 * produces is verifiable (run olean-residency afterwards and see 0) rather than
 * assumed.
 *
 * Nothing is written: the mapping is PROT_READ / MAP_SHARED on clean pages, so
 * MS_INVALIDATE discards rather than flushes. It cannot modify the files.
 *
 * usage:
 *   cc -O2 -o olean-evict olean-evict.c
 *   find ... -name '*.olean*' | ./olean-evict
 */
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

int main(void) {
  char *line = NULL;
  size_t cap = 0;
  ssize_t len;
  uint64_t n_ok = 0, n_skipped = 0, n_failed = 0;
  uint64_t bytes = 0;

  while ((len = getline(&line, &cap, stdin)) != -1) {
    while (len > 0 && (line[len - 1] == '\n' || line[len - 1] == '\r')) line[--len] = '\0';
    if (len == 0) continue;

    int fd = open(line, O_RDONLY);
    if (fd < 0) { fprintf(stderr, "skip (open %s): %s\n", strerror(errno), line); n_failed++; continue; }

    struct stat st;
    if (fstat(fd, &st) != 0) { fprintf(stderr, "skip (fstat %s): %s\n", strerror(errno), line); close(fd); n_failed++; continue; }
    if (st.st_size == 0) { close(fd); n_skipped++; continue; }

    size_t size = (size_t)st.st_size;
    void *addr = mmap(NULL, size, PROT_READ, MAP_SHARED, fd, 0);
    if (addr == MAP_FAILED) { fprintf(stderr, "skip (mmap %s): %s\n", strerror(errno), line); close(fd); n_failed++; continue; }

    if (msync(addr, size, MS_INVALIDATE) != 0) {
      fprintf(stderr, "msync failed (%s): %s\n", strerror(errno), line);
      n_failed++;
    } else {
      n_ok++;
      bytes += size;
    }
    munmap(addr, size);
    close(fd);
  }

  free(line);
  fprintf(stderr, "olean-evict: evicted=%llu (%llu bytes) skipped=%llu failed=%llu\n",
          (unsigned long long)n_ok, (unsigned long long)bytes,
          (unsigned long long)n_skipped, (unsigned long long)n_failed);
  return 0;
}
