/*
 * olean-prefetch — pull the named files into the page cache with N workers, then drop
 * the bytes on the floor.
 *
 * Why this exists: the cold `importModules` spends 12.7 s in 132,343 synchronous major
 * faults at 96.2 µs each (benchmarks/results/ci-residency-summary.txt §6). The disk is
 * not saturated — the same files read with read(2) sustain 679 MB/s sequentially and
 * 1,841 MB/s across 8 threads. So the cold cost is a queueing artifact of demand paging,
 * and the obvious counter-move is to issue those reads ahead of time, in parallel, while
 * the extractor is starting up. This program is that counter-move, isolated so its cost
 * and its effect can be measured separately.
 *
 * Two ways to ask the kernel for the same thing, because they are not the same thing:
 *
 *   read      open(2) + read(2) into a throwaway buffer. Goes through the unified buffer
 *             cache, so the pages it populates are the very pages a later mmap of the
 *             same vnode will find resident. This is the mode with a measured floor.
 *   madvise   mmap(PROT_READ, MAP_SHARED) + madvise(MADV_WILLNEED) + munmap. Cheaper in
 *             principle (no copy into user space), but MADV_WILLNEED is advisory and on
 *             macOS may do nothing at all. Verify with olean-residency before believing
 *             any timing taken with it — a prefetcher that loads nothing still "runs fast".
 *   touch     mmap + read one byte per page. Populates by faulting, which is what we are
 *             trying to avoid, but it is a useful control: it shows what the pages cost
 *             when fetched one fault at a time from this thread count.
 *
 * usage:
 *   cc -O2 -o olean-prefetch olean-prefetch.c
 *   ./olean-prefetch -j 8 -m read < paths.txt
 *
 * Reads newline-separated paths from stdin. Writes one JSON object to stderr:
 *   {"mode":"read","jobs":8,"files":N,"bytes":N,"elapsed_s":F,"mb_per_s":F,"failed":N}
 * Nothing is written to any file; every mapping is PROT_READ.
 */
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

enum mode { MODE_READ, MODE_MADVISE, MODE_TOUCH };

static char **paths;
static size_t n_paths;
static size_t next_index;          /* guarded by index_mu */
static pthread_mutex_t index_mu = PTHREAD_MUTEX_INITIALIZER;
static enum mode run_mode = MODE_READ;
static size_t buf_size = 1u << 20; /* 1 MiB per worker; read(2) chunk */

static uint64_t total_bytes;       /* guarded by tally_mu */
static uint64_t total_failed;
static pthread_mutex_t tally_mu = PTHREAD_MUTEX_INITIALIZER;

static double now_s(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (double)ts.tv_sec + (double)ts.tv_nsec / 1e9;
}

static size_t take_index(void) {
  pthread_mutex_lock(&index_mu);
  size_t i = next_index < n_paths ? next_index++ : (size_t)-1;
  pthread_mutex_unlock(&index_mu);
  return i;
}

static void *worker(void *arg) {
  (void)arg;
  const long pagesize = sysconf(_SC_PAGESIZE);
  char *buf = NULL;
  uint64_t bytes = 0, failed = 0;

  if (run_mode == MODE_READ) {
    buf = malloc(buf_size);
    if (!buf) { fprintf(stderr, "olean-prefetch: malloc failed\n"); return NULL; }
  }

  for (;;) {
    size_t i = take_index();
    if (i == (size_t)-1) break;
    const char *path = paths[i];

    int fd = open(path, O_RDONLY);
    if (fd < 0) { failed++; continue; }

    if (run_mode == MODE_READ) {
      ssize_t n;
      while ((n = read(fd, buf, buf_size)) > 0) bytes += (uint64_t)n;
      if (n < 0) failed++;
      close(fd);
      continue;
    }

    struct stat st;
    if (fstat(fd, &st) != 0 || st.st_size == 0) { close(fd); failed++; continue; }
    size_t size = (size_t)st.st_size;
    void *addr = mmap(NULL, size, PROT_READ, MAP_SHARED, fd, 0);
    if (addr == MAP_FAILED) { close(fd); failed++; continue; }

    if (run_mode == MODE_MADVISE) {
      if (madvise(addr, size, MADV_WILLNEED) != 0) failed++;
    } else { /* MODE_TOUCH */
      volatile const char *p = (const char *)addr;
      char sink = 0;
      for (size_t off = 0; off < size; off += (size_t)pagesize) sink ^= p[off];
      (void)sink;
    }
    bytes += (uint64_t)size;
    munmap(addr, size);
    close(fd);
  }

  free(buf);
  pthread_mutex_lock(&tally_mu);
  total_bytes += bytes;
  total_failed += failed;
  pthread_mutex_unlock(&tally_mu);
  return NULL;
}

int main(int argc, char **argv) {
  int jobs = 8;
  int opt;
  while ((opt = getopt(argc, argv, "j:m:b:")) != -1) {
    switch (opt) {
      case 'j': jobs = atoi(optarg); break;
      case 'b': buf_size = (size_t)strtoull(optarg, NULL, 10); break;
      case 'm':
        if (!strcmp(optarg, "read")) run_mode = MODE_READ;
        else if (!strcmp(optarg, "madvise")) run_mode = MODE_MADVISE;
        else if (!strcmp(optarg, "touch")) run_mode = MODE_TOUCH;
        else { fprintf(stderr, "unknown mode: %s\n", optarg); return 2; }
        break;
      default:
        fprintf(stderr, "usage: %s [-j jobs] [-m read|madvise|touch] [-b bufbytes] < paths\n", argv[0]);
        return 2;
    }
  }
  if (jobs < 1) jobs = 1;
  if (buf_size < 4096) buf_size = 4096;

  /* Read the whole list before starting the clock: list parsing is not prefetch. */
  size_t cap_paths = 4096;
  paths = malloc(cap_paths * sizeof(char *));
  if (!paths) { fprintf(stderr, "olean-prefetch: malloc failed\n"); return 2; }
  char *line = NULL;
  size_t cap = 0;
  ssize_t len;
  while ((len = getline(&line, &cap, stdin)) != -1) {
    while (len > 0 && (line[len - 1] == '\n' || line[len - 1] == '\r')) line[--len] = '\0';
    if (len == 0) continue;
    if (n_paths == cap_paths) {
      cap_paths *= 2;
      char **grown = realloc(paths, cap_paths * sizeof(char *));
      if (!grown) { fprintf(stderr, "olean-prefetch: realloc failed\n"); return 2; }
      paths = grown;
    }
    paths[n_paths] = strdup(line);
    if (!paths[n_paths]) { fprintf(stderr, "olean-prefetch: strdup failed\n"); return 2; }
    n_paths++;
  }
  free(line);

  double t0 = now_s();
  pthread_t *tids = malloc((size_t)jobs * sizeof(pthread_t));
  if (!tids) { fprintf(stderr, "olean-prefetch: malloc failed\n"); return 2; }
  int started = 0;
  for (int i = 0; i < jobs; i++) {
    if (pthread_create(&tids[i], NULL, worker, NULL) == 0) started++;
    else break;
  }
  if (started == 0) { worker(NULL); }
  for (int i = 0; i < started; i++) pthread_join(tids[i], NULL);
  double elapsed = now_s() - t0;

  const char *mname = run_mode == MODE_READ ? "read" : (run_mode == MODE_MADVISE ? "madvise" : "touch");
  fprintf(stderr,
          "{\"mode\":\"%s\",\"jobs\":%d,\"files\":%llu,\"bytes\":%llu,"
          "\"elapsed_s\":%.3f,\"mb_per_s\":%.1f,\"failed\":%llu}\n",
          mname, started ? started : 1, (unsigned long long)n_paths,
          (unsigned long long)total_bytes, elapsed,
          elapsed > 0 ? (double)total_bytes / elapsed / 1e6 : 0.0,
          (unsigned long long)total_failed);

  for (size_t i = 0; i < n_paths; i++) free(paths[i]);
  free(paths);
  free(tids);
  return 0;
}
