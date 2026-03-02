#!/bin/bash
# zero benchmark script
# Usage:
#   ./benchmark.sh                    # Run default benchmarks (creates temp files)
#   ./benchmark.sh /path/to/folder    # Benchmark with existing folder
#   ./benchmark.sh --help             # Show help

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ZERO="${SCRIPT_DIR}/../target/release/zero"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Defaults
RUNS=3
VERBOSE=false
SKIP_EXTERNAL=false

usage() {
    echo "Usage: $0 [OPTIONS] [SOURCE_FOLDER]"
    echo ""
    echo "Benchmark zero against other copy methods."
    echo ""
    echo "Arguments:"
    echo "  SOURCE_FOLDER    Folder to use for benchmarking (optional)"
    echo "                   If not provided, creates temp files of various sizes"
    echo ""
    echo "Options:"
    echo "  -r, --runs N     Number of runs per benchmark (default: 3)"
    echo "  -v, --verbose    Show detailed output"
    echo "  --skip-external  Skip external tools (cp, rsync)"
    echo "  -h, --help       Show this help"
    echo ""
    echo "Examples:"
    echo "  $0                        # Benchmark with generated test files"
    echo "  $0 ../fd                  # Benchmark using fd folder"
    echo "  $0 ~/Documents -r 5       # Benchmark Documents with 5 runs"
    exit 0
}

# Parse arguments
SOURCE_FOLDER=""
while [[ $# -gt 0 ]]; do
    case $1 in
        -r|--runs)
            RUNS="$2"
            shift 2
            ;;
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        --skip-external)
            SKIP_EXTERNAL=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        -*)
            echo "Unknown option: $1"
            usage
            ;;
        *)
            SOURCE_FOLDER="$1"
            shift
            ;;
    esac
done

# Check if zero is built
if [ ! -f "$ZERO" ]; then
    echo -e "${YELLOW}Building zero (release)...${NC}"
    (cd "${SCRIPT_DIR}/.." && cargo build --release)
fi

# Benchmark function - runs command multiple times and averages
benchmark() {
    local name="$1"
    local setup_cmd="$2"
    local cmd="$3"
    local size_bytes="$4"

    local total_time=0
    local times=()

    for ((i=1; i<=RUNS; i++)); do
        # Run setup (e.g., clear destination)
        eval "$setup_cmd" 2>/dev/null || true

        # Sync filesystem
        sync 2>/dev/null || true

        # Time the command
        local start_time=$(python3 -c 'import time; print(time.time())')
        eval "$cmd" > /dev/null 2>&1 || true
        local end_time=$(python3 -c 'import time; print(time.time())')

        local duration=$(python3 -c "print(${end_time} - ${start_time})")
        times+=("$duration")
        total_time=$(python3 -c "print(${total_time} + ${duration})")

        if $VERBOSE; then
            echo "    Run $i: ${duration}s"
        fi
    done

    # Calculate average and throughput
    local avg_time=$(python3 -c "print(f'{${total_time} / ${RUNS}:.3f}')")
    local throughput=$(python3 -c "
bytes = ${size_bytes}
secs = ${total_time} / ${RUNS}
if secs > 0:
    mbps = (bytes / 1_000_000) / secs
    print(f'{mbps:.1f}')
else:
    print('N/A')
")

    printf "  %-35s %10s sec  %10s MB/s\n" "$name" "$avg_time" "$throughput"
}

# Get folder stats
get_folder_stats() {
    local folder="$1"
    local file_count=$(find "$folder" -type f 2>/dev/null | wc -l | tr -d ' ')
    local total_size=$(du -sk "$folder" 2>/dev/null | cut -f1)
    total_size=$((total_size * 1024))  # Convert to bytes
    echo "$file_count $total_size"
}

format_size() {
    local bytes=$1
    if [ "$bytes" -ge 1073741824 ]; then
        python3 -c "print(f'{${bytes} / 1073741824:.2f} GB')"
    elif [ "$bytes" -ge 1048576 ]; then
        python3 -c "print(f'{${bytes} / 1048576:.2f} MB')"
    else
        python3 -c "print(f'{${bytes} / 1024:.2f} KB')"
    fi
}

# Setup temp directory for generated tests
TEMP_DIR=""
cleanup() {
    if [ -n "$TEMP_DIR" ] && [ -d "$TEMP_DIR" ]; then
        rm -rf "$TEMP_DIR"
    fi
}
trap cleanup EXIT

echo -e "${BOLD}${BLUE}"
echo "╔═══════════════════════════════════════════════════════════════════╗"
echo "║                     ZERO BENCHMARK                              ║"
echo "╚═══════════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

# Determine source folder
if [ -n "$SOURCE_FOLDER" ]; then
    if [ ! -d "$SOURCE_FOLDER" ]; then
        echo -e "${RED}Error: Folder not found: $SOURCE_FOLDER${NC}"
        exit 1
    fi
    SRC_DIR="$SOURCE_FOLDER"
    TEMP_DIR=$(mktemp -d)
    DST_DIR="${TEMP_DIR}/dest"
    mkdir -p "$DST_DIR"

    read file_count total_bytes <<< $(get_folder_stats "$SRC_DIR")
    echo -e "${CYAN}Source:${NC} $SRC_DIR"
    echo -e "${CYAN}Files:${NC}  $file_count"
    echo -e "${CYAN}Size:${NC}   $(format_size $total_bytes)"
    echo -e "${CYAN}Runs:${NC}   $RUNS per benchmark"
    echo ""

    # Single benchmark run with the provided folder
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}COPY METHODS COMPARISON${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${BLUE}Method                                Duration      Throughput${NC}"
    echo "───────────────────────────────────────────────────────────────────"

    if ! $SKIP_EXTERNAL; then
        benchmark "cp -R" \
            "rm -rf ${DST_DIR}/*" \
            "cp -R ${SRC_DIR}/* ${DST_DIR}/" \
            "$total_bytes"

        benchmark "ditto (Finder-equivalent)" \
            "rm -rf ${DST_DIR}/*" \
            "ditto ${SRC_DIR} ${DST_DIR}" \
            "$total_bytes"

        benchmark "rsync -a" \
            "rm -rf ${DST_DIR}/*" \
            "rsync -a ${SRC_DIR}/ ${DST_DIR}/" \
            "$total_bytes"
    fi

    benchmark "zero (default)" \
        "rm -rf ${DST_DIR}/*" \
        "${ZERO} sync ${SRC_DIR} ${DST_DIR}" \
        "$total_bytes"

    benchmark "zero --no-chunked" \
        "rm -rf ${DST_DIR}/*" \
        "${ZERO} sync ${SRC_DIR} ${DST_DIR} --no-chunked" \
        "$total_bytes"

    benchmark "zero --verify" \
        "rm -rf ${DST_DIR}/*" \
        "${ZERO} sync ${SRC_DIR} ${DST_DIR} --verify" \
        "$total_bytes"

    benchmark "zero --verify --no-chunked" \
        "rm -rf ${DST_DIR}/*" \
        "${ZERO} sync ${SRC_DIR} ${DST_DIR} --verify --no-chunked" \
        "$total_bytes"

    echo ""

    # Test incremental sync (no changes)
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}INCREMENTAL SYNC (no changes)${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""

    # First, do a full sync
    rm -rf "${DST_DIR}"/*
    "${ZERO}" sync "${SRC_DIR}" "${DST_DIR}" > /dev/null 2>&1

    echo -e "${BLUE}Method                                Duration      Throughput${NC}"
    echo "───────────────────────────────────────────────────────────────────"

    if ! $SKIP_EXTERNAL; then
        benchmark "rsync -a (no changes)" \
            "" \
            "rsync -a ${SRC_DIR}/ ${DST_DIR}/" \
            "$total_bytes"
    fi

    benchmark "zero (no changes)" \
        "" \
        "${ZERO} sync ${SRC_DIR} ${DST_DIR}" \
        "$total_bytes"

    benchmark "zero --verify (no changes)" \
        "" \
        "${ZERO} sync ${SRC_DIR} ${DST_DIR} --verify" \
        "$total_bytes"

    echo ""

    # Test other commands
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}OTHER ZERO COMMANDS${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${BLUE}Command                               Duration      Throughput${NC}"
    echo "───────────────────────────────────────────────────────────────────"

    benchmark "zero scan" \
        "" \
        "${ZERO} scan ${SRC_DIR}" \
        "$total_bytes"

    benchmark "zero diff" \
        "" \
        "${ZERO} diff ${SRC_DIR} ${DST_DIR}" \
        "$total_bytes"

    benchmark "zero verify" \
        "" \
        "${ZERO} verify ${SRC_DIR} ${DST_DIR}" \
        "$total_bytes"

    benchmark "zero verify --quick" \
        "" \
        "${ZERO} verify ${SRC_DIR} ${DST_DIR} --quick" \
        "$total_bytes"

    benchmark "zero index" \
        "" \
        "${ZERO} index ${SRC_DIR}" \
        "$total_bytes"

    echo ""

    # Search index benchmarks
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}SEARCH INDEX BENCHMARKS${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${BLUE}Command                               Duration      Throughput${NC}"
    echo "───────────────────────────────────────────────────────────────────"

    # Build search index (this pre-warms for all subsequent benchmarks)
    SEARCH_INDEX_PATH="${TEMP_DIR}/search_index.bin"

    benchmark "zero search --index (build)" \
        "rm -f ${SEARCH_INDEX_PATH}" \
        "${ZERO} search --index ${SRC_DIR} --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    # Make sure index exists for search benchmarks
    "${ZERO}" search --index "${SRC_DIR}" --cache "${SEARCH_INDEX_PATH}" > /dev/null 2>&1

    benchmark "zero search --count (load + count)" \
        "" \
        "${ZERO} search --count --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    echo ""

    # Search query benchmarks
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}SEARCH QUERY BENCHMARKS${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${BLUE}Command                               Duration      Throughput${NC}"
    echo "───────────────────────────────────────────────────────────────────"

    benchmark "zero search \"fn\" (short)" \
        "" \
        "${ZERO} search fn --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search \"test\" (common)" \
        "" \
        "${ZERO} search test --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search \"mod\" (rust)" \
        "" \
        "${ZERO} search mod --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search --type code (no query)" \
        "" \
        "${ZERO} search --type code --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search --type documents" \
        "" \
        "${ZERO} search --type documents --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search \"test\" --type code" \
        "" \
        "${ZERO} search test --type code --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search --ext .rs" \
        "" \
        "${ZERO} search --ext rs --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search --ext .toml" \
        "" \
        "${ZERO} search --ext toml --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search --in (scoped)" \
        "" \
        "${ZERO} search --in ${SRC_DIR} --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search \"fn\" --in (scoped)" \
        "" \
        "${ZERO} search fn --in ${SRC_DIR} --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search --files-only" \
        "" \
        "${ZERO} search --files-only --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search -n 0 (unlimited)" \
        "" \
        "${ZERO} search test -n 0 --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search \"test\" --count" \
        "" \
        "${ZERO} search test --count --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search --recent 50" \
        "" \
        "${ZERO} search --recent 50 --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search --recent 50 --type code" \
        "" \
        "${ZERO} search --recent 50 --type code --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search --recent 100 --type images" \
        "" \
        "${ZERO} search --recent 100 --type images --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    benchmark "zero search \"test\" --recent 50" \
        "" \
        "${ZERO} search test --recent 50 --cache ${SEARCH_INDEX_PATH}" \
        "$total_bytes"

    echo ""

    # Scanner-specific benchmarks
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}SCANNER BENCHMARKS${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${BLUE}Command                               Duration      Files/sec${NC}"
    echo "───────────────────────────────────────────────────────────────────"

    # Scanner benchmark (built-in)
    benchmark "zero scan" \
        "" \
        "${ZERO} scan ${SRC_DIR}" \
        "$total_bytes"

    benchmark "zero scan --benchmark 5" \
        "" \
        "${ZERO} scan ${SRC_DIR} --benchmark 5" \
        "$total_bytes"

    # Compare with external tools
    if ! $SKIP_EXTERNAL; then
        benchmark "find (baseline)" \
            "" \
            "find ${SRC_DIR} -type f" \
            "$total_bytes"

        if command -v fd &> /dev/null; then
            benchmark "fd (rust find)" \
                "" \
                "fd . ${SRC_DIR} --type f" \
                "$total_bytes"
        fi
    fi

    echo ""

    # Dedup benchmarks (index already pre-warmed from search benchmarks above)
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}DEDUP BENCHMARKS${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${BLUE}Command                               Duration      Throughput${NC}"
    echo "───────────────────────────────────────────────────────────────────"

    # Traditional scan-based dedup
    benchmark "zero dupes (scan + hash)" \
        "" \
        "${ZERO} dupes ${SRC_DIR}" \
        "$total_bytes"

    benchmark "zero dupes --min-size 1K" \
        "" \
        "${ZERO} dupes ${SRC_DIR} --min-size 1024" \
        "$total_bytes"

    benchmark "zero dupes --min-size 10K" \
        "" \
        "${ZERO} dupes ${SRC_DIR} --min-size 10240" \
        "$total_bytes"

    benchmark "zero dupes --max-depth 3" \
        "" \
        "${ZERO} dupes ${SRC_DIR} --max-depth 3" \
        "$total_bytes"

    benchmark "zero dupes --type code" \
        "" \
        "${ZERO} dupes ${SRC_DIR} --type code" \
        "$total_bytes"

    benchmark "zero dupes --type documents" \
        "" \
        "${ZERO} dupes ${SRC_DIR} --type documents" \
        "$total_bytes"

    benchmark "zero dupes \"test\"" \
        "" \
        "${ZERO} dupes ${SRC_DIR} test" \
        "$total_bytes"

    benchmark "zero dupes --verify" \
        "" \
        "${ZERO} dupes ${SRC_DIR} --verify" \
        "$total_bytes"

    echo ""

    # Piped dedup benchmarks (uses pre-warmed index from search benchmarks)
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}PIPED DEDUP BENCHMARKS (search | dupes)${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${BLUE}Command                               Duration      Throughput${NC}"
    echo "───────────────────────────────────────────────────────────────────"

    benchmark "search | dupes (all files)" \
        "" \
        "${ZERO} search --cache ${SEARCH_INDEX_PATH} -n 0 | ${ZERO} dupes" \
        "$total_bytes"

    benchmark "search --type code | dupes" \
        "" \
        "${ZERO} search --type code --cache ${SEARCH_INDEX_PATH} | ${ZERO} dupes" \
        "$total_bytes"

    benchmark "search --type images | dupes" \
        "" \
        "${ZERO} search --type images --cache ${SEARCH_INDEX_PATH} | ${ZERO} dupes" \
        "$total_bytes"

    benchmark "search --type documents | dupes" \
        "" \
        "${ZERO} search --type documents --cache ${SEARCH_INDEX_PATH} | ${ZERO} dupes" \
        "$total_bytes"

    benchmark "search \"test\" | dupes" \
        "" \
        "${ZERO} search test --cache ${SEARCH_INDEX_PATH} | ${ZERO} dupes" \
        "$total_bytes"

    echo ""

else
    # Generated test files
    TEMP_DIR=$(mktemp -d)
    SRC_DIR="${TEMP_DIR}/source"
    DST_DIR="${TEMP_DIR}/dest"
    mkdir -p "$SRC_DIR" "$DST_DIR"

    echo -e "${YELLOW}No source folder provided. Creating test files...${NC}"
    echo ""

    # Create test files
    SIZES=(1 10 50 100 200 500)
    for size in "${SIZES[@]}"; do
        echo "  Creating ${size}MB file..."
        dd if=/dev/urandom of="${SRC_DIR}/${size}mb.bin" bs=1M count="$size" 2>/dev/null
    done

    # Also create many small files
    echo "  Creating 1000 small files (1KB each)..."
    mkdir -p "${SRC_DIR}/small_files"
    for i in $(seq 1 1000); do
        dd if=/dev/urandom of="${SRC_DIR}/small_files/file_${i}.txt" bs=1024 count=1 2>/dev/null
    done

    read file_count total_bytes <<< $(get_folder_stats "$SRC_DIR")
    echo ""
    echo -e "${CYAN}Generated:${NC} $file_count files, $(format_size $total_bytes)"
    echo -e "${CYAN}Runs:${NC}      $RUNS per benchmark"
    echo ""

    # Full benchmark suite
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}FULL COPY BENCHMARK (all files: $(format_size $total_bytes))${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${BLUE}Method                                Duration      Throughput${NC}"
    echo "───────────────────────────────────────────────────────────────────"

    if ! $SKIP_EXTERNAL; then
        benchmark "cp -R" \
            "rm -rf ${DST_DIR}/*" \
            "cp -R ${SRC_DIR}/* ${DST_DIR}/" \
            "$total_bytes"

        benchmark "ditto (Finder-equivalent)" \
            "rm -rf ${DST_DIR}/*" \
            "ditto ${SRC_DIR} ${DST_DIR}" \
            "$total_bytes"

        benchmark "rsync -a" \
            "rm -rf ${DST_DIR}/*" \
            "rsync -a ${SRC_DIR}/ ${DST_DIR}/" \
            "$total_bytes"
    fi

    benchmark "zero (default)" \
        "rm -rf ${DST_DIR}/*" \
        "${ZERO} sync ${SRC_DIR} ${DST_DIR}" \
        "$total_bytes"

    benchmark "zero --no-chunked" \
        "rm -rf ${DST_DIR}/*" \
        "${ZERO} sync ${SRC_DIR} ${DST_DIR} --no-chunked" \
        "$total_bytes"

    benchmark "zero --verify" \
        "rm -rf ${DST_DIR}/*" \
        "${ZERO} sync ${SRC_DIR} ${DST_DIR} --verify" \
        "$total_bytes"

    echo ""

    # Per-size benchmarks
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}SINGLE FILE BENCHMARKS${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""

    SINGLE_SRC="${TEMP_DIR}/single_src"
    SINGLE_DST="${TEMP_DIR}/single_dst"

    for size in "${SIZES[@]}"; do
        size_bytes=$((size * 1024 * 1024))

        mkdir -p "$SINGLE_SRC" "$SINGLE_DST"
        cp "${SRC_DIR}/${size}mb.bin" "$SINGLE_SRC/"

        echo -e "${CYAN}${size}MB file:${NC}"
        echo "───────────────────────────────────────────────────────────────────"

        if ! $SKIP_EXTERNAL; then
            benchmark "cp" \
                "rm -rf ${SINGLE_DST}/*" \
                "cp ${SINGLE_SRC}/${size}mb.bin ${SINGLE_DST}/" \
                "$size_bytes"

            benchmark "ditto" \
                "rm -rf ${SINGLE_DST}/*" \
                "ditto ${SINGLE_SRC}/${size}mb.bin ${SINGLE_DST}/${size}mb.bin" \
                "$size_bytes"
        fi

        benchmark "zero" \
            "rm -rf ${SINGLE_DST}/*" \
            "${ZERO} sync ${SINGLE_SRC} ${SINGLE_DST}" \
            "$size_bytes"

        benchmark "zero --verify" \
            "rm -rf ${SINGLE_DST}/*" \
            "${ZERO} sync ${SINGLE_SRC} ${SINGLE_DST} --verify" \
            "$size_bytes"

        rm -rf "$SINGLE_SRC" "$SINGLE_DST"
        echo ""
    done

    # Many small files
    echo -e "${CYAN}1000 small files (1KB each):${NC}"
    echo "───────────────────────────────────────────────────────────────────"

    small_bytes=$((1000 * 1024))
    mkdir -p "$SINGLE_SRC" "$SINGLE_DST"
    cp -R "${SRC_DIR}/small_files" "$SINGLE_SRC/"

    if ! $SKIP_EXTERNAL; then
        benchmark "cp -R" \
            "rm -rf ${SINGLE_DST}/*" \
            "cp -R ${SINGLE_SRC}/small_files ${SINGLE_DST}/" \
            "$small_bytes"

        benchmark "ditto (Finder-equivalent)" \
            "rm -rf ${SINGLE_DST}/*" \
            "ditto ${SINGLE_SRC}/small_files ${SINGLE_DST}/small_files" \
            "$small_bytes"
    fi

    benchmark "zero" \
        "rm -rf ${SINGLE_DST}/*" \
        "${ZERO} sync ${SINGLE_SRC} ${SINGLE_DST}" \
        "$small_bytes"

    rm -rf "$SINGLE_SRC" "$SINGLE_DST"
    echo ""

    # Resume benchmark
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}RESUME BENCHMARK (200MB file, 50% partial)${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""

    mkdir -p "$SINGLE_SRC" "$SINGLE_DST"
    cp "${SRC_DIR}/200mb.bin" "$SINGLE_SRC/"

    echo -e "${BLUE}Method                                Duration      Throughput*${NC}"
    echo "───────────────────────────────────────────────────────────────────"

    # cp overwrites entirely
    benchmark "cp (overwrites 100%)" \
        "dd if=${SINGLE_SRC}/200mb.bin of=${SINGLE_DST}/200mb.bin bs=1M count=100 2>/dev/null" \
        "cp ${SINGLE_SRC}/200mb.bin ${SINGLE_DST}/200mb.bin" \
        "$((200 * 1024 * 1024))"

    if ! $SKIP_EXTERNAL; then
        benchmark "rsync --append (resumes)" \
            "rm -f ${SINGLE_DST}/200mb.bin && dd if=${SINGLE_SRC}/200mb.bin of=${SINGLE_DST}/200mb.bin bs=1M count=100 2>/dev/null" \
            "rsync --append ${SINGLE_SRC}/200mb.bin ${SINGLE_DST}/200mb.bin" \
            "$((100 * 1024 * 1024))"
    fi

    benchmark "zero --chunked (resumes)" \
        "rm -f ${SINGLE_DST}/200mb.bin && dd if=${SINGLE_SRC}/200mb.bin of=${SINGLE_DST}/200mb.bin bs=1M count=100 2>/dev/null" \
        "${ZERO} sync ${SINGLE_SRC} ${SINGLE_DST}" \
        "$((100 * 1024 * 1024))"

    echo ""
    echo -e "${YELLOW}* Throughput for resume tests is based on bytes actually transferred${NC}"

    rm -rf "$SINGLE_SRC" "$SINGLE_DST"
fi

echo ""
echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}BENCHMARK COMPLETE${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
echo ""
echo "Notes:"
echo "  • Results vary based on disk speed, caching, and system load"
echo "  • First run may be slower due to cold cache"
echo "  • zero default includes chunked transfer + verification for large files"
echo "  • cp uses OS-level optimizations (APFS clonefile on macOS)"
echo "  • ditto is macOS Finder-equivalent copy (preserves metadata, resource forks)"
echo ""
