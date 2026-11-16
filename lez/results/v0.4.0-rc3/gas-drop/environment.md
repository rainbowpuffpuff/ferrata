# Environment of the gas-drop session (16 November 2026)

- Machine: the Modal build box (16 physical cores, `nproc` 2 under gVisor; provers at `RAYON_NUM_THREADS=32`), no GPU.
- LEZ `v0.4.0-rc3`, commit `bafd07be0701398f9bc435f73ab05e9a7936c301`, source tree unmodified (cloned on the box,
  commit and clean tree checked); `sequencer_service` sha256 `be2ac354f4268d7c021519735667d5d5b723e5901a65e4dcd47cd686dc42ca34`
  (rustc 1.98.1), started without `RISC0_DEV_MODE` (`session/config/sequencer-process.txt`).
- Logos Blockchain node 0.3.5-rc.1 (`f6d4267fe7d1ff8ab4cb30e9a18154bc66e10c30`), binary sha256
  `2f30172903a36175606f466fa6a9d5503a26cb8f95eb9b9444677ee06520c6e2`, one validator, `POL_PROOF_DEV_MODE=true`.
- Deployed and proven: the attestation program `0x816d1fef…` (sha256 `ddfde69b…`) and the bridge `0xccde6171…`
  (sha256 `11d83248…`), the rc1 builds of docs/image-ids.md (checked by sha256 and `r0vm --id` before use). Proven, not
  deployed: the gas-drop guest `gasdrop`, `0x895804161df14b00e1cb02e5d27fe136efb82b178db511c7697c030676fc9eb4`, built on
  the box from `bridge/` (sha256 `c2119aee6f28359f383d8f1a2e14829f95bdc86e92d52790e442dd823cb43162`, as docs/image-ids.md).
  The credit runs LEZ's native token program inside rc3's privacy-preserving circuit.
- Tools built on the box from this tree: crown-driver (LEZ rc3 selected) sha256
  `1c2fe29552db571c6417945b326608049a24507de70d1935688cc638e4f7227e`; live-driver (rc3)
  `17b7502526371349f0805256bd81f0daf72bbf9f9fa74aded18449d4cb75cdf0`; `ferrata-gasdrop` (cli, LEZ rc1 paths, without
  the `lez-wallet` feature) `6f72059b07e1a090abe234b97eb43a5955e52807d162acfd74ba97c15e5eef97`; r0vm 3.0.5.
- The zone's own environment as run-live.sh wrote it: `session/environment.md`.
