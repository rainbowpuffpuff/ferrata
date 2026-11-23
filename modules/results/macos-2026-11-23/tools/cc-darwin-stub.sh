#!/bin/sh
# Stand-in C compiler for aarch64-apple-darwin on a Linux box with no macOS SDK: clang for that target, every C
# source replaced by an empty one (cc-rs's -E probe goes through unchanged), so the objects are empty Mach-O files.
d=$(cd "$(dirname "$0")" && pwd)
echo "$*" >> "$d/cc-darwin-stub.log"
for a in "$@"; do case "$a" in -E) exec clang -target arm64-apple-macos11 "$@" ;; esac; done
[ -f "$d/empty.c" ] || : > "$d/empty.c"
n=$#
for a in "$@"; do
  case "$a" in *.c) a=$d/empty.c ;; esac
  set -- "$@" "$a"
done
shift "$n"
exec clang -target arm64-apple-macos11 "$@"
