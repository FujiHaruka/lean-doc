#!/usr/bin/env bash
# Record which machine a measurement ran on, in enough detail to tell two
# `ubuntu-latest` runners apart.
#
# WHY THIS EXISTS
#   The four CI measurement workflows before this one recorded date, `uname`,
#   `nproc`, page size, image version, `/proc/meminfo` and `df` — and **all four
#   env files came out byte-identical** in every one of those fields (kernel
#   6.17.0-1020-azure, nproc 2, page size 4096, image ubuntu24 20260720.247.2).
#   Yet the runs behind them split into two clearly different machines: the same
#   cold import took 20.4 s on one and 63-89 s on the other, at 5.3 ms per major
#   fault against 0.67 ms 【実測 → docs/verification-log.md「CI 軸」】. Nothing
#   recorded identified which machine a run had landed on; the split could only
#   be inferred from the numbers it was supposed to explain.
#
#   Worse for an A/B: a pure-CPU phase of the same work varied **2.19x** across
#   runner instances and did **not** line up with the I/O split — the runner with
#   the fastest disk had the slowest CPU 【実測, same runs】. Two arms on two
#   runners can therefore differ by 2x for reasons that have nothing to do with
#   what is being tested.
#
#   So this records the fields that name the hardware (CPU model, `lscpu`,
#   readahead, block devices) and, because none of them is guaranteed to be
#   truthful on a VM, a **CPU calibrator**: a fixed amount of pure-CPU work,
#   timed. A calibrator that differs between two runners is the warning that
#   their other numbers are not directly comparable.
#
# usage:
#   record-runner.sh <outfile> [<lean package dir>]
set -euo pipefail

OUT="${1:-runner.txt}"
TARGET="${2:-}"

have () { command -v "$1" > /dev/null 2>&1; }

{
  echo "## runner"
  echo "date        $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "kernel      $(uname -srm)"
  echo "nproc       $( (nproc 2> /dev/null || sysctl -n hw.ncpu) 2> /dev/null || echo '?')"
  echo "pagesize    $(getconf PAGESIZE)"
  echo "runner      ${RUNNER_OS:-?} / ${RUNNER_ARCH:-?} / image ${ImageOS:-?} ${ImageVersion:-?}"

  # ---------------------------------------------------------------- the machine
  echo
  echo "## cpu"
  if [ -r /proc/cpuinfo ]; then
    grep -m1 -E '^model name' /proc/cpuinfo || true
    grep -m1 -E '^cpu MHz' /proc/cpuinfo || true
    grep -m1 -E '^flags' /proc/cpuinfo | tr ' ' '\n' | grep -cE '^(sha_ni|avx512f|avx2)$' \
      | sed 's/^/accel flags matched: /' || true
  fi
  if have lscpu; then lscpu | grep -E 'Model name|Vendor|Socket|Core|Thread|MHz|Hypervisor|Flags' || true; fi

  echo
  echo "## storage"
  if have lsblk; then lsblk -o NAME,SIZE,MODEL,ROTA,MOUNTPOINT 2> /dev/null || true; fi
  # Through sysfs, not `blockdev --getra`: the latter opens the device and needs
  # root, so on a runner it answers `?` — and readahead is the one setting the
  # two `ubuntu-latest` I/O realities are suspected to differ by 【実測: 5.3 ms
  # vs 0.67 ms per major fault at nearly equal sequential bandwidth】. A field
  # that is always `?` would leave that suspicion untestable for a fourth time.
  if have findmnt; then
    for mp in / /mnt; do
      src="$(findmnt -no SOURCE --target "$mp" 2> /dev/null || true)"
      [ -n "$src" ] || continue
      base="$( (lsblk -no PKNAME "$src" 2> /dev/null || true) | head -1 | tr -d ' ')"
      [ -n "$base" ] || base="$(basename "$src")"
      q="/sys/block/$base/queue"
      printf 'readahead %s (%s -> %s): %s kB, rotational %s, scheduler %s\n' \
        "$mp" "$src" "$base" \
        "$(cat "$q/read_ahead_kb" 2> /dev/null || echo '?')" \
        "$(cat "$q/rotational" 2> /dev/null || echo '?')" \
        "$( (cat "$q/scheduler" 2> /dev/null || echo '?') | tr -d '\n')"
    done
  fi

  echo
  echo "## memory"
  if [ -r /proc/meminfo ]; then
    grep -E '^(MemTotal|MemFree|MemAvailable|Buffers|Cached|SwapTotal)' /proc/meminfo
  else
    vm_stat 2> /dev/null | head -5 || true
  fi

  echo
  echo "## df"
  df -h / /mnt 2> /dev/null || df -h /

  # ------------------------------------------------------------- the calibrator
  #
  # A fixed amount of CPU work with no disk in it: 1 GiB of zeroes from the
  # kernel through sha256. Three times, because the first one on a fresh VM is
  # not representative; the minimum is the machine's floor. This is a *relative*
  # number — its job is to say whether two runners are the same speed, not how
  # fast either is.
  echo
  echo "## cpu calibrator (1 GiB sha256, 3 runs, seconds)"
  for _ in 1 2 3; do
    s="$(date +%s.%N 2> /dev/null || date +%s)"
    dd if=/dev/zero bs=1048576 count=1024 2> /dev/null | sha256sum > /dev/null
    e="$(date +%s.%N 2> /dev/null || date +%s)"
    awk -v a="$s" -v b="$e" 'BEGIN{printf "%.3f\n", b-a}'
  done

  # ----------------------------------------------------------------- the target
  if [ -n "$TARGET" ] && [ -d "$TARGET" ]; then
    echo
    echo "## target"
    echo "path        $TARGET"
    echo "HEAD        $(git -C "$TARGET" rev-parse HEAD 2> /dev/null || echo '?')"
    echo "toolchain   $(tr -d '\n' < "$TARGET/lean-toolchain" 2> /dev/null || echo '?')"
  fi
} | tee "$OUT"
