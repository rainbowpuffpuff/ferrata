#!/usr/bin/env bash
# Runs inside the transient user service (working directory: the worktree): one P4_JOBS job of the owner script, its
# output and exit code to files.
#   part-service.sh STAGE_DIR OUT_DIR LOG
bash bench/perf/p4-laptop-owner.sh "$1" "$2" > "$3" 2>&1
echo $? > "$3.exit"
