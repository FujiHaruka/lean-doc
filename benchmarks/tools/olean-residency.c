/*
 * olean-residency — measure how much of each file is resident in the page cache.
 *
 * Reads newline-separated file paths from stdin, and for each one maps the file
 * read-only and asks mincore(2) which of its pages are already in core. mmap
 * itself does not fault pages in, so the answer describes the page cache state
 * *before* this program ran, not a state it created.
 *
 * This exists because peak RSS is not the same quantity as "bytes of .olean read
 * from disk": RSS counts what a process kept resident, including anonymous
 * memory, and misses file pages evicted mid-run. Diffing two residency
 * snapshots taken around a run gives the file bytes that run pulled into cache.
 *
 * usage:
 *   cc -O2 -o olean-residency olean-residency.c
 *   find ... -name '*.olean*' | ./olean-residency > residency.jsonl
 *
 * Output is JSON Lines, one record per readable file:
 *   {"path":"...","bytes":N,"pages":N,"resident_pages":N,"resident_bytes":N,
 *    "resident_file_bytes":N}
 *
 * Two different quantities, because they answer different questions:
 *   resident_bytes       resident_pages * pagesize — page-cache memory occupied.
 *                        Compare this against peak RSS, which also counts whole pages.
 *   resident_file_bytes  the same pages measured in *file* bytes: a file's last page
 *                        contributes only `bytes % pagesize`. Compare this against the
 *                        apparent file size, i.e. how much of the file was read.
 * They differ by up to one page per file, which matters here: `.olean.server` files
 * average well under one page, so the page-quantized figure overstates them severalfold.
 *
 * On this machine the page size is 16384, not 4096 — do not assume 4 KiB.
 *
 * Files that are empty or cannot be opened/stat'ed/mapped are skipped and
 * counted; the counts go to stderr so stdout stays valid JSON Lines.
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

#ifndef MINCORE_INCORE
#define MINCORE_INCORE 0x1
#endif

static void print_json_string(FILE *out, const char *s) {
  fputc('"', out);
  for (const unsigned char *p = (const unsigned char *)s; *p; p++) {
    switch (*p) {
      case '"': fputs("\\\"", out); break;
      case '\\': fputs("\\\\", out); break;
      case '\n': fputs("\\n", out); break;
      case '\r': fputs("\\r", out); break;
      case '\t': fputs("\\t", out); break;
      default:
        if (*p < 0x20) fprintf(out, "\\u%04x", *p);
        else fputc(*p, out);
    }
  }
  fputc('"', out);
}

int main(void) {
  const long pagesize = sysconf(_SC_PAGESIZE);
  if (pagesize <= 0) {
    fprintf(stderr, "olean-residency: cannot determine page size\n");
    return 2;
  }

  char *line = NULL;
  size_t cap = 0;
  ssize_t len;
  uint64_t n_ok = 0, n_empty = 0, n_failed = 0;

  while ((len = getline(&line, &cap, stdin)) != -1) {
    while (len > 0 && (line[len - 1] == '\n' || line[len - 1] == '\r')) line[--len] = '\0';
    if (len == 0) continue;

    int fd = open(line, O_RDONLY);
    if (fd < 0) {
      fprintf(stderr, "skip (open %s): %s\n", strerror(errno), line);
      n_failed++;
      continue;
    }

    struct stat st;
    if (fstat(fd, &st) != 0) {
      fprintf(stderr, "skip (fstat %s): %s\n", strerror(errno), line);
      close(fd);
      n_failed++;
      continue;
    }
    if (st.st_size == 0) {
      fprintf(stderr, "skip (empty): %s\n", line);
      close(fd);
      n_empty++;
      continue;
    }

    size_t size = (size_t)st.st_size;
    /* MAP_SHARED + PROT_READ: no page is faulted in by the mapping itself. */
    void *addr = mmap(NULL, size, PROT_READ, MAP_SHARED, fd, 0);
    if (addr == MAP_FAILED) {
      fprintf(stderr, "skip (mmap %s): %s\n", strerror(errno), line);
      close(fd);
      n_failed++;
      continue;
    }

    size_t npages = (size + (size_t)pagesize - 1) / (size_t)pagesize;
    char *vec = malloc(npages);
    if (!vec) {
      fprintf(stderr, "skip (malloc %zu): %s\n", npages, line);
      munmap(addr, size);
      close(fd);
      n_failed++;
      continue;
    }

    if (mincore(addr, size, vec) != 0) {
      fprintf(stderr, "skip (mincore %s): %s\n", strerror(errno), line);
      free(vec);
      munmap(addr, size);
      close(fd);
      n_failed++;
      continue;
    }

    uint64_t resident = 0, resident_file = 0;
    for (size_t i = 0; i < npages; i++) {
      if (!(vec[i] & MINCORE_INCORE)) continue;
      resident++;
      /* The last page covers only the file's remainder, which for the many
         sub-page .olean.server files is most of the file. */
      resident_file += (i + 1 == npages) ? (size - i * (size_t)pagesize) : (size_t)pagesize;
    }

    fputs("{\"path\":", stdout);
    print_json_string(stdout, line);
    printf(",\"bytes\":%llu,\"pages\":%llu,\"resident_pages\":%llu,\"resident_bytes\":%llu,"
           "\"resident_file_bytes\":%llu}\n",
           (unsigned long long)size, (unsigned long long)npages,
           (unsigned long long)resident,
           (unsigned long long)(resident * (uint64_t)pagesize),
           (unsigned long long)resident_file);

    free(vec);
    munmap(addr, size);
    close(fd);
    n_ok++;
  }

  free(line);
  fflush(stdout);
  fprintf(stderr, "olean-residency: pagesize=%ld ok=%llu empty=%llu failed=%llu\n",
          pagesize, (unsigned long long)n_ok, (unsigned long long)n_empty,
          (unsigned long long)n_failed);
  return 0;
}
