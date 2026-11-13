# The one-command check on the published commit, 13 Nov 2026

`MODAL_PROFILE=… modal run --detach scripts/reproduce/modal_app.py` from a checkout of commit `a5fd69f` (the tree
published as `main`; later commits change documents only): **5 of 5 PASS in 32:07** on 16 CPUs, no committed file
changed: both live runs' records 13 passed each, the vault 454 passed and 1 skipped (with the review's new tests), the
core 163, 14 of 14 guest ids and 6 of 6 crown programs, the LEZ harness 56. [`summary.txt`](summary.txt) is its verdict;
[`environment.txt`](environment.txt) and [`packages.txt`](packages.txt) its toolchains and packages; `guest-ids/` the
rebuilt ids. The same check under Docker on an x86-64 machine: [`../2026-11-12-docker-aws/`](../2026-11-12-docker-aws/README.md).
