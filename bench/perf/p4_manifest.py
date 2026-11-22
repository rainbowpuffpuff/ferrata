#!/usr/bin/env python3
"""Set the `prove_command` of a P.4 owner bundle's manifests (bench/perf/p4-laptop-build.sh) to what
bench/perf/p4-laptop-owner.sh runs: r0vm 3.0.5 on the CPU, padded as LEZ's wallet pads (`--shape wallet`). The
staging scripts (lez/scripts/stage-prove-*.sh) write the GPU jobs' command. The `files` checksums are unchanged.
  python3 -I bench/perf/p4_manifest.py STAGE_DIR"""

import json
import pathlib
import sys

CMD = {
    "attest-pad": "FERRATA_EXPECT_CONSUMER_IMAGE_ID=<expected_consumer_image_id> FERRATA_ATTEST_FIXTURE_DIR=<this "
                  "directory>/<fixture> RISC0_DEV_MODE=0 RISC0_SERVER_PATH=<STAGE_DIR>/r0vm/r0vm (r0vm 3.0.5, CPU) "
                  "/usr/bin/time -v ./prove_attest --shape wallet --tx-out tx-attest.borsh; then ./prove_attest "
                  "--settle tx-attest.borsh (bench/perf/p4-laptop-owner.sh)",
    "bridge-pad": "FERRATA_EXPECT_BRIDGE_IMAGE_ID=<expected_bridge_image_id> RISC0_DEV_MODE=0 "
                  "RISC0_SERVER_PATH=<STAGE_DIR>/r0vm/r0vm (r0vm 3.0.5, CPU) /usr/bin/time -v ./prove_bridge --shape "
                  "wallet --claim-out tx-claim.borsh --burn-out tx-burn.borsh; then ./prove_bridge --settle "
                  "tx-claim.borsh tx-burn.borsh (bench/perf/p4-laptop-owner.sh)",
}

stage = pathlib.Path(sys.argv[1])
for job, cmd in CMD.items():
    p = stage / job / "MANIFEST.json"
    m = json.load(open(p))
    m["prove_command"] = cmd
    p.write_text(json.dumps(m, indent=2) + "\n")
    print(f"{p}: prove_command set")
